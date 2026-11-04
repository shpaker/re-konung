//! Просмотрщик карт «Князя»: состояние, перестроение сцены и интерфейс.
//!
//! Состояние ([Viewer]) и команды интерфейса отделены от отрисовки: обработчики
//! виджетов правят только состояние и взводят [Rebuild], а сцену перестраивает
//! одна система. CLI parsing and the headless check live in the `kn-viewer`
//! app crate.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    feathers::{
        FeathersPlugins,
        controls::{
            FeathersButton, FeathersCheckbox, FeathersSelect, OptionIndex, list_rows_from_strings,
        },
        display::caption,
        theme::ThemeBackgroundColor,
        tokens,
    },
    input::mouse::{AccumulatedMouseMotion, MouseWheel},
    picking::hover::Hovered,
    prelude::*,
    ui::Checked,
    ui_widgets::{Activate, ValueChange},
};
use kn_game::simulation::{CreatureState, Simulation, WalkState};
use kn_game::{
    Animation, CreatureAnimation, Game, MapRef, MapSource, Piece, PieceGroup, SceneOptions,
    TICK_MS, group_is_shown,
};
use kn_input::{ZoomScroll, drag_delta, pan_direction, wheel_steps};
use kn_render::{
    AnimPlugin, Animator, CameraApply, CameraBounds, CameraInput, CameraState, Clock, CreatureData,
    CreatureNode, CreatureRoot, DepthMap, GameResource, LayerAnimation, LayerData, MapCamera,
    MapCameraPlugin, PAN_SPEED, PendingFrame, PlayState, SceneGroup, ScenePiece, Sequence,
    TextureCache, Z_STEP, clear_scene, load_scene, ortho_scale, piece_sequence, piece_size,
    spawn_creature, update_creature_depth,
};
use kn_ui::{dark_theme, pointer_over_ui, sync_checked};

/// Отладочный оверлей, который включает просмотрщик.
///
/// `Clone` и `Default` нужны макросу `bsn!`: компоненты сцены шаблона
/// создаются из значения по умолчанию.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Overlay {
    #[default]
    Foot,
    Lights,
    Markers,
    Characters,
}

/// Оверлеи, включённые при запуске: персонажей видно сразу.
///
/// Значения должны совпадать с `Checked` у соответствующих чекбоксов в [panel]:
/// виджет хранит состояние сам, а не читает его из [Viewer].
pub const DEFAULT_OVERLAYS: [bool; 4] = [false, false, false, true];

/// Идёт ли симуляция при запуске. Согласовано с `Checked` чекбокса `Play` в
/// [panel]: виджет хранит состояние сам, а не читает его из [Viewer].
pub const DEFAULT_PLAYBACK: PlayState = PlayState::Playing;

/// Что показывает просмотрщик. Правится интерфейсом, читается системами.
#[derive(Resource)]
pub struct Viewer {
    pub maps: Vec<MapRef>,
    pub selected: usize,
    /// Включённые оверлеи в порядке [Overlay]
    pub overlays: [bool; 4],
    /// Ход симуляции: `Playing` — существа двигаются, `Paused` — стоят. Это
    /// единственный переключатель движения; выключение визуальной анимации
    /// движение не останавливает (одного `Play`/`Pause` вместо прежних двух).
    pub playback: PlayState,
}

/// Часы под единственный переключатель `Play`: снятая галочка — пауза, а не
/// выключение. В [`PlayState::Off`] часы не уходят ни одна ветка интерфейса,
/// поэтому движение существ никогда не глохнет молча вместе с анимацией.
pub fn playback_for(playing: bool) -> PlayState {
    if playing {
        PlayState::Playing
    } else {
        PlayState::Paused
    }
}

/// Клавиша `Space`: пауза и продолжение одним нажатием.
pub fn toggle_playback(playback: PlayState) -> PlayState {
    match playback {
        PlayState::Playing => PlayState::Paused,
        PlayState::Paused | PlayState::Off => PlayState::Playing,
    }
}

impl Viewer {
    pub fn is_on(&self, overlay: Overlay) -> bool {
        self.overlays[overlay as usize]
    }

    pub fn set(&mut self, overlay: Overlay, on: bool) {
        self.overlays[overlay as usize] = on;
    }

    /// Включённые слои как опции сцены: по ним [`sync_groups`] выбирает, какие
    /// группы кусков показывать.
    ///
    /// Это не состав сцены — карта собирается сразу со всеми кусками
    /// ([`SceneOptions::ALL`]), иначе переключение слоя стоило бы пересборки
    /// тысяч спрайтов.
    fn options(&self) -> SceneOptions {
        SceneOptions {
            foot: self.is_on(Overlay::Foot),
            lights: self.is_on(Overlay::Lights),
            markers: self.is_on(Overlay::Markers),
            characters: self.is_on(Overlay::Characters),
            // Героя выбирает фасад по источнику карты: у обучения своё TGame.N,
            // и один Game.N на все карты расставлял бы по tutorial-картам
            // персонажей основной игры
            hero: None,
        }
    }
}

/// Сцену надо собрать заново: выбрана другая карта.
///
/// Оверлеи флаг не взводят: карта собирается сразу со всеми кусками, а слой
/// переключается видимостью группы ([`sync_groups`]) — иначе клик по чекбоксу
/// сносил тысячи спрайтов ради нескольких десятков.
#[derive(Resource, Default)]
pub struct Rebuild {
    pub map: bool,
}

/// Корень спрайтов текущей карты.
#[derive(Resource, Default)]
pub struct ActiveRoot(pub Option<Entity>);

/// Статистика последней построенной сцены — для панели диагностики.
///
/// Сколько кусков видно и сколько собрано считает [`diagnostics`] по спрайтам в
/// мире: переключение слоя не пересобирает сцену и не оставляет здесь устаревшего
/// числа.
#[derive(Resource, Default)]
pub struct Stats {
    pub map: Option<MapRef>,
    pub unique_textures: usize,
    pub cached_textures: usize,
    pub built_ms: u128,
    /// Куски с анимацией и все их кадры: кадры тоже декодируются при сборке
    pub animated_pieces: usize,
    pub animation_frames: usize,
    /// Ключи, по которым картинка не собралась: карта показана, но не целиком
    pub problems: Vec<String>,
    pub error: Option<String>,
}

/// Seed of the creature simulation: fixed so "Reset positions" reproduces the
/// exact spawn of the first build (same seed, same map → same initial state).
const SIM_SEED: u64 = 42;

/// Simulation driving creature positions; recreated on map rebuild and on reset.
#[derive(Resource, Default)]
struct SimulationResource {
    sim: Option<Simulation>,
    initial_pixels: Vec<(usize, (i32, i32))>,
    label_root: Option<Entity>,
    labels_spawned: bool,
    /// Root of the creature nodes, despawned on map rebuild.
    creature_root: Option<Entity>,
    /// Whether this map's creature nodes have been spawned; reset on rebuild and
    /// after a reset so nodes are rebuilt once.
    creatures_spawned: bool,
}

/// Depth map built from scene stack boundaries, used for dynamic z on creature move.
#[derive(Resource)]
struct SceneDepth {
    map: DepthMap,
}

impl Default for SceneDepth {
    fn default() -> Self {
        Self {
            map: DepthMap::build(Vec::new().into_iter(), 0.0),
        }
    }
}

/// Clock tick at which the simulation was last advanced; drives catch-up ticking.
#[derive(Resource, Default)]
struct SimTickTracker {
    last_clock_ticks: u64,
}

/// Idle and walk [`Sequence`]s per creature for the `WalkState` swap; rebuilt on
/// map change together with [`SimulationResource`].
#[derive(Resource, Default)]
struct CreatureAnim {
    creatures: HashMap<usize, CreatureSequences>,
}

/// Sequences for one creature's pieces: `body` by Characters-group layer index,
/// `shadow` for its ground shadow.
struct CreatureSequences {
    body: HashMap<usize, LayerSequences>,
    shadow: LayerSequences,
}

/// Resting sequence and one per walk direction for a single layer.
struct LayerSequences {
    rest: Option<Clip>,
    walk: Vec<Option<Clip>>,
}

/// A built sequence and the frame it should show when play restarts.
struct Clip {
    sequence: Arc<Sequence>,
    base: usize,
}

/// One [`CreatureData`] per creature to spawn, rebuilt with the scene. Kept as a
/// resource so the spawn system runs it once per map, after the depth map exists.
#[derive(Resource, Default)]
struct CreatureSpawns {
    plans: Vec<CreatureData>,
    /// Body height in pixels per creature (`CreatureData::bottom - origin.y`),
    /// used to turn a live cell into an occlusion bottom without a scene lookup.
    heights: HashMap<usize, i32>,
}

/// A sprite under a [`CreatureNode`]: which creature it belongs to and which of
/// its layers it is (or the shadow), so the animation system swaps its sequence.
#[derive(Component)]
struct CreatureSprite {
    creature_id: usize,
    layer: usize,
    is_shadow: bool,
}

/// Decodes the walk frames the scene does not carry and builds the swap
/// sequences; a creature whose walk cannot be decoded simply keeps its record pose.
fn build_creature_anim(
    game: &Game,
    map_ref: MapRef,
    cache: &mut TextureCache,
    images: &mut Assets<Image>,
) -> CreatureAnim {
    let Ok(animations) = game.creature_animations(map_ref) else {
        return CreatureAnim::default();
    };
    let mut seen = HashSet::new();
    for (_, anim) in &animations {
        for clips in std::iter::once(&anim.rest).chain(anim.walk.iter().flatten()) {
            // Both the layer's static base and every animation frame are drawn.
            seen.extend(clips.layers.iter().filter_map(|layer| layer.base));
            seen.extend(clips.shadow.as_ref().and_then(|layer| layer.base));
            let frames = clips
                .layers
                .iter()
                .map(|layer| layer.animation.as_ref())
                .chain(std::iter::once(
                    clips
                        .shadow
                        .as_ref()
                        .and_then(|layer| layer.animation.as_ref()),
                ));
            for animation in frames.flatten() {
                seen.extend(animation.frames.iter().filter_map(|frame| frame.texture));
            }
        }
    }
    if let Err(error) = cache.load(game, &seen.into_iter().collect::<Vec<_>>(), images) {
        warn!("walk textures for {map_ref:?}: {error}");
    }
    CreatureAnim {
        creatures: animations
            .into_iter()
            .map(|(id, anim)| (id, build_sequences(&anim, cache)))
            .collect(),
    }
}

/// One creature's clips → sequences, aligned by layer position with the scene.
fn build_sequences(anim: &CreatureAnimation, cache: &TextureCache) -> CreatureSequences {
    let body = anim
        .rest
        .layers
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let rest = clip_seq(
                anim.rest
                    .layers
                    .get(index)
                    .and_then(|l| l.animation.as_ref()),
                cache,
            );
            let walk = anim
                .walk
                .iter()
                .map(|dir| {
                    dir.as_ref()
                        .and_then(|clips| {
                            clips.layers.get(index).and_then(|l| l.animation.as_ref())
                        })
                        .and_then(|a| clip_seq(Some(a), cache))
                })
                .collect();
            (index, LayerSequences { rest, walk })
        })
        .collect();
    let rest = clip_seq(
        anim.rest.shadow.as_ref().and_then(|l| l.animation.as_ref()),
        cache,
    );
    let walk = anim
        .walk
        .iter()
        .map(|dir| {
            dir.as_ref()
                .and_then(|clips| clips.shadow.as_ref().and_then(|l| l.animation.as_ref()))
                .and_then(|a| clip_seq(Some(a), cache))
        })
        .collect();
    CreatureSequences {
        body,
        shadow: LayerSequences { rest, walk },
    }
}

/// Frames of one layer → a swap clip; `None` when no frame decoded.
fn clip_seq(animation: Option<&Animation>, cache: &TextureCache) -> Option<Clip> {
    let animation = animation?;
    let sequence = piece_sequence(animation, cache)?;
    Some(Clip {
        sequence,
        base: animation.base,
    })
}

/// Builds every creature's [`CreatureData`] from its scene pieces.
///
/// A creature's body/equipment pieces (group `Characters`) and shadow piece
/// (`CharacterShadows`) carry their own geometry in the scene; each is placed
/// relative to the creature's origin cell, so the whole set rides the node as it
/// moves. Only creatures that actually have scene pieces get a plan; roster
/// order keeps the result deterministic.
fn build_creature_spawns(
    scene: &kn_game::Scene,
    cache: &TextureCache,
    anim: &CreatureAnim,
    initial_pixels: &[(usize, (i32, i32))],
) -> CreatureSpawns {
    let mut plans = Vec::new();
    let mut heights = HashMap::new();
    for &(id, origin) in initial_pixels {
        let pieces: Vec<&Piece> = scene
            .pieces
            .iter()
            .filter(|piece| piece.creature_id == Some(id))
            .collect();
        if pieces.is_empty() {
            continue;
        }
        let (plan, height) = build_creature_plan(pieces, cache, anim, id, origin);
        plans.push(plan);
        heights.insert(id, height);
    }
    CreatureSpawns { plans, heights }
}

/// One creature's pieces → spawn data, with body height reported for depth.
///
/// `bottom` is the lowest body edge (the shadow never sets the occlusion line).
/// Body layers keep their scene order, which indexes [`CreatureSequences::body`].
fn build_creature_plan(
    pieces: Vec<&Piece>,
    cache: &TextureCache,
    anim: &CreatureAnim,
    id: usize,
    origin: (i32, i32),
) -> (CreatureData, i32) {
    let mut layers: Vec<LayerData> = Vec::new();
    let mut shadow: Option<LayerData> = None;
    let mut bottom = i32::MIN;
    for piece in pieces {
        let entry = piece.texture.as_ref().and_then(|id| cache.get(id));
        let size = piece_size(piece, entry.map_or(UVec2::ZERO, |entry| entry.size));
        let layer = LayerData {
            image: entry.map_or_else(Handle::default, |entry| entry.handle.clone()),
            size,
            offset: Vec2::new((piece.x - origin.0) as f32, (piece.y - origin.1) as f32),
        };
        if piece.group == PieceGroup::CharacterShadows {
            shadow = Some(layer);
        } else {
            bottom = bottom.max(piece.y + size.y as i32);
            layers.push(layer);
        }
    }
    let bottom = if bottom == i32::MIN { origin.1 } else { bottom };
    let sequences = anim.creatures.get(&id);
    let animations = layers
        .iter()
        .enumerate()
        .map(|(index, _)| rest_animation(sequences.and_then(|c| c.body.get(&index))))
        .collect();
    let shadow_animation = sequences.and_then(|c| rest_animation(Some(&c.shadow)));
    (
        CreatureData {
            id: id as u32,
            x: origin.0,
            y: origin.1,
            bottom,
            layers,
            animations,
            shadow,
            shadow_animation,
        },
        bottom - origin.1,
    )
}

/// The resting [`LayerAnimation`] of a layer, if it has a resting sequence.
fn rest_animation(layer: Option<&LayerSequences>) -> Option<LayerAnimation> {
    let clip = layer?.rest.as_ref()?;
    Some(LayerAnimation {
        sequence: Arc::clone(&clip.sequence),
        base: clip.base,
    })
}

/// World-space text label above a creature, showing `#ID`.
#[derive(Component)]
struct CreatureLabel {
    creature_id: usize,
}

/// Root entity for all creature labels; despawned on map rebuild.
#[derive(Component)]
struct LabelRoot;

/// Единственный переключатель движения симуляции (`Play`): [sync_playboxes]
/// находит по нему чекбокс, чтобы зеркалить в него [Viewer::playback].
#[derive(Component, Clone, Copy, Default)]
struct PlaybackToggle;

/// Кнопка "Reset positions": по ней [reset_positions] отличает свой `Activate`
/// от активаций других кнопок (у выпадющего списка карт они тоже бывают).
#[derive(Component, Clone, Copy, Default)]
struct ResetPositions;

/// Подпись элемента интерфейса, включающего оверлей.
#[derive(Component, Clone, Copy, Default)]
struct OverlayToggle(Overlay);

/// Текст панели диагностики.
#[derive(Component, Clone, Copy, Default)]
struct Diagnostics;

/// Названия карт для списка: номер и источник (тексты из EXE не перенесены).
pub fn map_labels(maps: &[MapRef]) -> Vec<String> {
    maps.iter()
        .map(|map| match map.source {
            MapSource::Game => format!("Map {}", map.number),
            MapSource::Tutorial => format!("Tutorial {}", map.number),
        })
        .collect()
}

/// Everything the viewer window needs. CLI types never cross into the frontend:
/// the app resolves the arguments and hands over an opened game.
pub struct ViewerConfig {
    pub game: Game,
    pub game_dir: PathBuf,
    pub maps: Vec<MapRef>,
    pub selected: usize,
}

/// The viewer window: runs until the user closes it.
pub fn run_viewer(config: ViewerConfig) {
    let labels = map_labels(&config.maps);
    let selected = config.selected;

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: format!("re-konung viewer — {}", config.game_dir.display()),
                        resolution: (1280, 800).into(),
                        ..default()
                    }),
                    ..default()
                })
                // Пиксельная графика: без сглаживания при дробном масштабе
                .set(ImagePlugin::default_nearest()),
        )
        .add_plugins((
            FeathersPlugins,
            MapCameraPlugin,
            // Тик игры: на нём меняются кадры объектов, персонажей и огней
            AnimPlugin::new(TICK_MS as f32),
            FrameTimeDiagnosticsPlugin::default(),
        ))
        .insert_resource(dark_theme())
        .insert_resource(GameResource(config.game))
        .init_resource::<TextureCache>()
        .init_resource::<ActiveRoot>()
        .init_resource::<Stats>()
        .init_resource::<SimulationResource>()
        .init_resource::<SimTickTracker>()
        .init_resource::<SceneDepth>()
        .init_resource::<CreatureAnim>()
        .init_resource::<CreatureSpawns>()
        .insert_resource(Rebuild { map: true })
        .insert_resource(Viewer {
            maps: config.maps.clone(),
            selected,
            overlays: DEFAULT_OVERLAYS,
            playback: DEFAULT_PLAYBACK,
        })
        .add_systems(Startup, move |world: &mut World| {
            if let Err(error) = world.spawn_scene_list(panel_list(labels.clone(), selected)) {
                error!("не собрался интерфейс: {error}");
            }
        })
        .add_systems(Update, camera_control.in_set(CameraInput))
        .add_systems(
            Update,
            (
                playback_hotkeys,
                apply_playback,
                sync_playboxes,
                rebuild_scene,
                sync_groups,
                sync_labels,
                advance_simulation,
                spawn_creature_labels,
                spawn_creatures,
                sync_creature_nodes,
                sync_creature_animation,
                diagnostics,
            )
                .chain()
                .after(CameraInput)
                .before(CameraApply),
        )
        .run();
}

/// Camera-control binding: applies the kn-input mechanisms to this app's camera.
/// All computation lives in kn-input and kn-render; only the wiring is here.
#[allow(clippy::too_many_arguments)]
fn camera_control(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    over_ui_nodes: Query<&Hovered, With<Node>>,
    time: Res<Time>,
    mut state: ResMut<CameraState>,
    camera: Single<(&mut Transform, &Projection), With<MapCamera>>,
    mut scroll: Local<ZoomScroll>,
) {
    let over_ui = pointer_over_ui(over_ui_nodes.iter().map(|hovered| hovered.0));
    let direction = pan_direction(&keys);
    let drag = if over_ui {
        None
    } else {
        drag_delta(&buttons, &motion)
    };
    let steps = wheel_steps(&mut scroll, &mut wheel, over_ui);
    let (mut transform, projection) = camera.into_inner();
    let scale = ortho_scale(projection);
    if direction != Vec2::ZERO {
        transform.translation.x += direction.x * PAN_SPEED * time.delta_secs() * scale;
        transform.translation.y += direction.y * PAN_SPEED * time.delta_secs() * scale;
    }
    if let Some(delta) = drag {
        transform.translation.x -= delta.x * scale;
        // Screen y points down, world y points up
        transform.translation.y += delta.y * scale;
    }
    if steps != 0 {
        state.step(steps);
    }
}

/// Панель интерфейса: список карт, переключатели оверлеев, диагностика.
fn panel_list(labels: Vec<String>, selected: usize) -> impl SceneList {
    bsn_list! {
        @panel(labels, selected)
    }
}

fn panel(labels: Vec<String>, selected: usize) -> impl Scene {
    bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: px(8.0),
            top: px(8.0),
            width: px(240.0),
            flex_direction: FlexDirection::Column,
            row_gap: px(6.0),
            padding: UiRect::axes(px(10.0), px(10.0)),
        }
        // Панель блокирует ввод камеры целиком, а не только над виджетами:
        // `Hovered` Bevy обновляет лишь у тех, у кого он уже есть, а Feathers
        // кладёт его на свои контролы — пустые промежутки панели иначе остались
        // бы «не над интерфейсом»
        Hovered
        ThemeBackgroundColor(tokens::WINDOW_BG)
        Children[
            @caption("Map")
            --
            @FeathersSelect {
                @options: list_rows_from_strings(labels, Some(selected)),
                @max_visible: 12,
            }
            Node {
                width: percent(100),
            }
            on(select_map)
            --
            @caption("Overlays")
            --
            @FeathersCheckbox {
                @caption: bsn! { @caption("Walkability") }
            }
            OverlayToggle(Overlay::Foot)
            on(toggle_overlay)
            --
            @FeathersCheckbox {
                @caption: bsn! { @caption("Lights") }
            }
            OverlayToggle(Overlay::Lights)
            on(toggle_overlay)
            --
            @FeathersCheckbox {
                @caption: bsn! { @caption("Waypoints") }
            }
            OverlayToggle(Overlay::Markers)
            on(toggle_overlay)
            --
            @FeathersCheckbox {
                @caption: bsn! { @caption("Characters") }
            }
            Checked
            OverlayToggle(Overlay::Characters)
            on(toggle_overlay)
            --
            @caption("Simulation")
            --
            @FeathersCheckbox {
                @caption: bsn! { @caption("Play") }
            }
            Checked
            PlaybackToggle
            on(change_playback)
            --
            @FeathersButton {
                @caption: bsn! { @caption("Reset positions") }
            }
            ResetPositions
            on(reset_positions)
            --
            @caption("") Diagnostics
        ]
    }
}

fn select_map(
    change: On<ValueChange<Entity>>,
    options: Query<&OptionIndex>,
    mut viewer: ResMut<Viewer>,
    mut rebuild: ResMut<Rebuild>,
) {
    let Ok(option) = options.get(change.value) else {
        return;
    };
    if viewer.selected != option.0 {
        viewer.selected = option.0;
        rebuild.map = true;
    }
}

/// Переключатель оверлея правит только состояние: спрайты той же карты остаются
/// на месте, а [`sync_groups`] прячет и показывает группу. Виджет подписывает
/// [sync_checked] — иначе каждый клик присылал бы `true`.
fn toggle_overlay(
    change: On<ValueChange<bool>>,
    toggles: Query<&OverlayToggle>,
    mut viewer: ResMut<Viewer>,
    mut commands: Commands,
) {
    let Ok(kind) = toggles.get(change.source) else {
        return;
    };
    viewer.set(kind.0, change.value);
    sync_checked(&mut commands, change.source, change.value);
}

/// Переключатель `Play` правит только состояние: сцена той же карты не
/// перестраивается, меняются лишь часы симуляции. Галочку зеркалит
/// [sync_playboxes] — иначе `Space` не отразился бы на панели, а пауза,
/// поставленная шагом, оставила бы галочку отмеченной.
fn change_playback(
    change: On<ValueChange<bool>>,
    toggles: Query<&PlaybackToggle>,
    mut viewer: ResMut<Viewer>,
) {
    if toggles.get(change.source).is_err() {
        return;
    }
    viewer.playback = playback_for(change.value);
}

/// Отмечен ли единственный переключатель при этом состоянии: галочка стоит,
/// пока симуляция идёт.
fn playback_checked(viewer: &Viewer) -> bool {
    viewer.playback == PlayState::Playing
}

/// Зеркалит ход симуляции в чекбокс `Play`.
///
/// `Checkbox` в Bevy сам `Checked` не пишет (см. [sync_checked]), и пишет его
/// только обработчик клика — а состояние правят и клавиша `Space`, и шаг по
/// кадру. Поэтому признак берётся из `Viewer`, а не из события чекбокса.
fn sync_playboxes(
    viewer: Res<Viewer>,
    boxes: Query<(Entity, &PlaybackToggle, Has<Checked>)>,
    mut commands: Commands,
) {
    for (entity, _, checked) in &boxes {
        let want = playback_checked(&viewer);
        if want != checked {
            sync_checked(&mut commands, entity, want);
        }
    }
}

/// Клавиши: `Space` — пауза и продолжение, `.` и `,` — шаг на тик вперёд и
/// назад. Панорамирование камеры этих клавиш не использует.
fn playback_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    mut viewer: ResMut<Viewer>,
    mut clock: ResMut<Clock>,
) {
    if keys.just_pressed(KeyCode::Space) {
        viewer.playback = toggle_playback(viewer.playback);
    }
    let steps = i64::from(keys.just_pressed(KeyCode::Period))
        - i64::from(keys.just_pressed(KeyCode::Comma));
    if steps != 0 {
        // Шаг при идущем воспроизведении часы тут же увели бы кадр обратно,
        // поэтому он ставит паузу
        viewer.playback = PlayState::Paused;
        clock.step(steps);
    }
}

/// Состояние интерфейса переносится в часы анимации.
fn apply_playback(viewer: Res<Viewer>, mut clock: ResMut<Clock>) {
    if clock.state != viewer.playback {
        clock.goto(viewer.playback);
    }
}

/// Перестраивает сцену, когда выбрана другая карта.
///
/// Карта собирается со всеми кусками ([`SceneOptions::ALL`]): переключать слой
/// пересборкой значит сносить тысячи спрайтов ради нескольких десятков — см.
/// [`sync_groups`].
#[allow(clippy::too_many_arguments)]
fn rebuild_scene(
    mut rebuild: ResMut<Rebuild>,
    viewer: Res<Viewer>,
    game: Res<GameResource>,
    mut cache: ResMut<TextureCache>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    mut active: ResMut<ActiveRoot>,
    mut stats: ResMut<Stats>,
    mut pending: ResMut<PendingFrame>,
    mut sim_resource: ResMut<SimulationResource>,
    mut sim_tracker: ResMut<SimTickTracker>,
    mut scene_depth: ResMut<SceneDepth>,
    mut creature_anim: ResMut<CreatureAnim>,
    mut creature_spawns: ResMut<CreatureSpawns>,
    clock: Res<Clock>,
) {
    if !rebuild.map {
        return;
    }
    rebuild.map = false;
    let Some(&map_ref) = viewer.maps.get(viewer.selected) else {
        return;
    };
    let started = Instant::now();
    let scene = match game.0.scene(map_ref, &SceneOptions::ALL) {
        Ok(scene) => scene,
        Err(error) => {
            *stats = failed(map_ref, cache.len(), error.to_string(), started.elapsed());
            return;
        }
    };
    let animation = scene.animation_stats();
    let loaded = match load_scene(
        &game.0,
        &scene,
        shown(&viewer.options()),
        &mut cache,
        &mut images,
        &mut commands,
        &mut pending,
    ) {
        Ok(loaded) => loaded,
        Err(error) => {
            *stats = failed(map_ref, cache.len(), error.to_string(), started.elapsed());
            return;
        }
    };
    for problem in &loaded.problems {
        warn!("карта {map_ref:?}: {problem}");
    }
    if let Some(old) = active.0 {
        clear_scene(&mut commands, old);
    }
    active.0 = Some(loaded.root);
    *stats = Stats {
        map: Some(map_ref),
        unique_textures: loaded.unique_textures,
        cached_textures: cache.len(),
        built_ms: started.elapsed().as_millis(),
        animated_pieces: animation.pieces,
        animation_frames: animation.frames,
        problems: loaded.problems,
        error: None,
    };
    sim_resource.sim = game.0.simulation(map_ref, SIM_SEED).ok();
    sim_resource.initial_pixels = sim_resource
        .sim
        .as_ref()
        .map_or_else(Vec::new, |s| s.initial_pixels());
    sim_resource.labels_spawned = false;
    if let Some(root) = sim_resource.label_root.take() {
        commands.entity(root).despawn();
    }
    if let Some(root) = sim_resource.creature_root.take() {
        commands.entity(root).despawn();
    }
    sim_resource.creatures_spawned = false;
    // The fresh simulation counts ticks from 0 while the scene clock keeps
    // running: pin the tracker to the current tick so `advance_simulation` never
    // replays the whole accumulated history as one catch-up burst (review #68).
    sim_tracker.last_clock_ticks = clock.ticks;
    scene_depth.map = build_scene_depth(&scene, &cache);
    *creature_anim = build_creature_anim(&game.0, map_ref, &mut cache, &mut images);
    *creature_spawns =
        build_creature_spawns(&scene, &cache, &creature_anim, &sim_resource.initial_pixels);
}

/// Видимость куска по включённым слоям: границы сцены считают только по тому,
/// что действительно на экране.
fn shown(options: &SceneOptions) -> impl Fn(&Piece) -> bool + '_ {
    move |piece| group_is_shown(piece.group, options)
}

/// Builds a depth map from the scene's sorted pieces for creature z-interpolation.
fn build_scene_depth(scene: &kn_game::Scene, cache: &TextureCache) -> DepthMap {
    let mut stacks: Vec<(i32, f32)> = scene
        .pieces
        .iter()
        .enumerate()
        .filter(|(_, p)| is_depth_relevant(p.group))
        .map(|(order, piece)| {
            let height = piece.h.unwrap_or_else(|| {
                piece
                    .texture
                    .as_ref()
                    .and_then(|id| cache.get(id))
                    .map_or(0, |entry| entry.size.y as i32)
            });
            (piece.y + height, order as f32 * Z_STEP)
        })
        .collect();
    stacks.sort_by_key(|&(bottom, _)| bottom);
    DepthMap::build(stacks.into_iter(), -Z_STEP)
}

/// Only static stacks participate in depth ordering.
///
/// Characters and their shadows are drawn by their own [`CreatureNode`]s and move
/// every frame; putting them in the map would let a creature depth-sort against
/// another creature rather than against the static objects it should hide behind
/// (review #68). Terrain and the ground layers sit below everything by scene order
/// and never occlude a creature, so they are excluded too.
fn is_depth_relevant(group: PieceGroup) -> bool {
    matches!(group, PieceGroup::Objects)
}

/// Переносит переключатели слоёв в видимость групп кусков.
///
/// Узел группы — единственный, чего касается переключение: дети наследуют её
/// видимость ([`kn_render::SceneGroup`]), поэтому кадр не обходится в тысячи
/// despawn/spawn. Включённая группа получает [`Visibility::Inherited`], а не
/// `Visible`: `Visible` перебивает наследование от [`kn_render::MapRoot`], и карту,
/// спрятанную целиком, было бы видно. Система идёт сразу за [`rebuild_scene`] и
/// успевает спрятать свежие группы до извлечения кадра.
///
/// Смена видимости пересчитывает и границы камеры: слой может выходить за
/// содержимое карты (ромбы проходимости идут по всей сетке), и иначе включённый
/// оверлей оказался бы там, куда камера не пускает. Положение и масштаб при этом
/// не трогаются — меняется только рамка, а `clamp_to_bounds` сам вернёт камеру,
/// если она вылезла за сузившуюся рамку.
fn sync_groups(
    viewer: Res<Viewer>,
    mut groups: Query<(&SceneGroup, &mut Visibility)>,
    mut bounds: ResMut<CameraBounds>,
) {
    let options = viewer.options();
    let mut changed = false;
    for (group, mut visibility) in &mut groups {
        let target = if group_is_shown(group.group, &options) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != target {
            *visibility = target;
            changed = true;
        }
    }
    if !changed {
        return;
    }
    let mut shown: Option<(Vec2, Vec2)> = None;
    for (group, visibility) in &groups {
        if *visibility == Visibility::Hidden {
            continue;
        }
        let Some((min, max)) = group.bounds else {
            continue;
        };
        shown = Some(match shown {
            Some((lo, hi)) => (lo.min(min), hi.max(max)),
            None => (min, max),
        });
    }
    if let Some((min, max)) = shown {
        bounds.min = min;
        bounds.max = max;
    }
}

/// Shows the creature-id labels with the Characters overlay.
///
/// They hang off their own [`LabelRoot`], not the Characters group, so hiding
/// characters would otherwise leave the `#ID` texts floating over an empty map
/// (review #68). `Inherited` so a map hidden on `MapRoot` stays hidden.
fn sync_labels(
    sim: Res<SimulationResource>,
    viewer: Res<Viewer>,
    mut labels: Query<&mut Visibility, With<LabelRoot>>,
) {
    let Some(root) = sim.label_root else {
        return;
    };
    let Ok(mut visibility) = labels.get_mut(root) else {
        return;
    };
    let target = if viewer.is_on(Overlay::Characters) {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if *visibility != target {
        *visibility = target;
    }
}

fn failed(map: MapRef, cached: usize, error: String, elapsed: Duration) -> Stats {
    Stats {
        map: Some(map),
        unique_textures: 0,
        cached_textures: cached,
        built_ms: elapsed.as_millis(),
        animated_pieces: 0,
        animation_frames: 0,
        problems: Vec::new(),
        error: Some(error),
    }
}

/// Advances the simulation to the clock, one logical tick per clock tick.
///
/// Forward is a plain catch-up loop. A backward `Step` (`.`/`,`) rewinds the clock
/// but the engine has no reverse tick, so we rebuild the map's simulation and
/// replay it forward to the rewound tick: sprites and creature positions then both
/// show that tick, instead of sprites animating backwards while creatures stand
/// still (review #68). Replaying from seed is deterministic, so the state is the
/// same one forward stepping reached at that tick.
fn advance_simulation(
    clock: Res<Clock>,
    mut tracker: ResMut<SimTickTracker>,
    mut sim: ResMut<SimulationResource>,
    game: Res<GameResource>,
    viewer: Res<Viewer>,
) {
    let Some(&map_ref) = viewer.maps.get(viewer.selected) else {
        tracker.last_clock_ticks = clock.ticks;
        return;
    };
    if sim.sim.is_none() {
        tracker.last_clock_ticks = clock.ticks;
        return;
    }
    if clock.ticks < tracker.last_clock_ticks
        && let Some(replayed) = replay_simulation(&game.0, map_ref, clock.ticks)
    {
        sim.initial_pixels = replayed.initial_pixels();
        sim.sim = Some(replayed);
        // The replay left the fresh simulation at exactly `clock.ticks`, so pin the
        // tracker there: pinning to 0 would make the catch-up loop below tick it a
        // second `clock.ticks` times and every creature would overshoot the target.
        tracker.last_clock_ticks = clock.ticks;
    }
    while let Some(simulation) = sim.sim.as_mut() {
        if tracker.last_clock_ticks >= clock.ticks {
            break;
        }
        simulation.tick();
        tracker.last_clock_ticks += 1;
    }
}

/// A fresh map simulation advanced to `ticks`, matching forward stepping exactly.
fn replay_simulation(game: &Game, map_ref: MapRef, ticks: u64) -> Option<Simulation> {
    let mut simulation = game.simulation(map_ref, SIM_SEED).ok()?;
    for _ in 0..ticks {
        simulation.tick();
    }
    Some(simulation)
}

/// Кнопка "Reset positions" возвращает существ в исходные позиции текущей карты.
///
/// Симуляция пересоздаётся с тем же зерном ([`SIM_SEED`]), поэтому маршруты,
/// состояния походки и ГСЧ свежие. Карта, камера, масштаб, выбранный слой и
/// пауза не трогаются: сцену не пересобираем и текстуры не грузим заново.
fn reset_positions(
    activate: On<Activate>,
    markers: Query<&ResetPositions>,
    game: Res<GameResource>,
    viewer: Res<Viewer>,
    clock: Res<Clock>,
    mut sim: ResMut<SimulationResource>,
    mut tracker: ResMut<SimTickTracker>,
) {
    if markers.get(activate.entity).is_err() {
        return;
    }
    let Some(&map_ref) = viewer.maps.get(viewer.selected) else {
        return;
    };
    reset_simulation(&game.0, map_ref, clock.ticks, &mut sim, &mut tracker);
}

/// Пересоздаёт симуляцию карты и ставит трекер в текущий тик часов.
///
/// Трекер синхронизируется с часами: свежая симуляция не должна отыграть тики,
/// которые уже прошла старая, иначе существа мгновенно перематываются вперёд.
/// Узлы существ не пересоздаются — [`sync_creature_nodes`] каждый кадр ставит их
/// по состоянию, поэтому после сброса они сами встанут над исходными точками.
fn reset_simulation(
    game: &Game,
    map_ref: MapRef,
    clock_ticks: u64,
    sim: &mut SimulationResource,
    tracker: &mut SimTickTracker,
) {
    let Ok(simulation) = game.simulation(map_ref, SIM_SEED) else {
        return;
    };
    sim.initial_pixels = simulation.initial_pixels();
    sim.sim = Some(simulation);
    tracker.last_clock_ticks = clock_ticks;
}

const LABEL_OFFSET_Y: f32 = 50.0;

/// Temporary per-creature diagnostics for the movement/animation review: set
/// `KN_CREATURE_DEBUG` in the environment to log, per creature id, the simulation
/// tick, cell, step from/to, `step_elapsed`, the interpolated pixel, the rendered
/// node position and z, and the simulation's own facing direction. Comparing
/// `sim_dir`/`from`/`to` (which change only at a logical tick) with `render` shows
/// whether the simulation itself moves or only the display shifts.
static CREATURE_DEBUG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

fn creature_debug() -> bool {
    *CREATURE_DEBUG.get_or_init(|| std::env::var_os("KN_CREATURE_DEBUG").is_some())
}

fn spawn_creature_labels(mut sim: ResMut<SimulationResource>, mut commands: Commands) {
    if sim.labels_spawned || sim.sim.is_none() {
        return;
    }
    let states = sim.sim.as_ref().unwrap().states();
    sim.labels_spawned = true;
    // Hidden until [`sync_labels`] shows it with the Characters overlay: a label
    // that were `Visible` would survive Characters off (it hangs off its own root,
    // not the Characters group) and leak creature ids over an empty map.
    let root = commands
        .spawn((LabelRoot, Transform::IDENTITY, Visibility::Hidden))
        .id();
    sim.label_root = Some(root);
    let mut children = Vec::new();
    for state in &states {
        let (px, py) = state.pixel;
        let text = format!("#{}", state.id);
        let label = commands
            .spawn((
                CreatureLabel {
                    creature_id: state.id,
                },
                Text2d(text),
                TextFont {
                    font_size: FontSize::Px(11.0),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 1.0, 0.6)),
                Transform::from_xyz(px as f32, -(py as f32) - LABEL_OFFSET_Y, 9999.0),
                Visibility::Inherited,
            ))
            .id();
        children.push(label);
    }
    commands.entity(root).add_children(&children);
}

/// Spawns the creature nodes once per map, tagging each child sprite so the
/// animation system can find its layer. Runs after [`rebuild_scene`] laid out the
/// depth map, so every node is born at its correct depth.
fn spawn_creatures(
    mut commands: Commands,
    mut sim: ResMut<SimulationResource>,
    spawns: Res<CreatureSpawns>,
    scene_depth: Res<SceneDepth>,
) {
    if sim.creatures_spawned || sim.sim.is_none() {
        return;
    }
    sim.creatures_spawned = true;
    let root = commands
        .spawn((CreatureRoot, Transform::IDENTITY, Visibility::Visible))
        .id();
    sim.creature_root = Some(root);
    for plan in &spawns.plans {
        let spawned = spawn_creature(&mut commands, root, plan, &scene_depth.map);
        for (layer, child) in spawned.layers.iter().enumerate() {
            commands.entity(*child).insert(CreatureSprite {
                creature_id: plan.id as usize,
                layer,
                is_shadow: false,
            });
        }
        if let Some(shadow) = spawned.shadow {
            commands.entity(shadow).insert(CreatureSprite {
                creature_id: plan.id as usize,
                layer: 0,
                is_shadow: true,
            });
        }
    }
}

/// Puts each creature node at its live cell and depth, and its label above it.
///
/// The node owns the creature's world position and z; its sprite children carry
/// only local offsets and per-frame deltas, so they follow the node untouched and
/// body, equipment and shadow stay in lockstep (review #68).
fn sync_creature_nodes(
    sim: Res<SimulationResource>,
    scene_depth: Res<SceneDepth>,
    spawns: Res<CreatureSpawns>,
    clock: Res<Clock>,
    mut nodes: Query<(&mut CreatureNode, &mut Transform)>,
    mut labels: Query<(&CreatureLabel, &mut Transform)>,
) {
    let Some(ref simulation) = sim.sim else {
        return;
    };
    let states = simulation.states();
    let sub = clock.tick_fraction();
    let drawn: HashMap<usize, (f32, f32)> = states
        .iter()
        .map(|state| (state.id, smooth_pixel(state, sub)))
        .collect();
    for (mut node, mut transform) in &mut nodes {
        let id = node.id as usize;
        let Some(&(x, y)) = drawn.get(&id) else {
            continue;
        };
        transform.translation.x = x;
        transform.translation.y = -y;
        // Occlusion bottom from the live cell and this creature's body height.
        let height = spawns.heights.get(&id).copied().unwrap_or(0);
        let bottom = (y.round() + height as f32).max(0.0) as i32;
        update_creature_depth(&mut node, &mut transform, bottom, &scene_depth.map);
        if creature_debug()
            && let Some(state) = states.iter().find(|state| state.id == id)
        {
            info!(
                "creature #{id} tick={} cell=({},{}) from=({},{}) to={:?} elapsed={}/{} \
                 interp=({x:.1},{y:.1}) render=({:.1},{:.1}) z={:.4} sim_dir={:?} walk={:?}",
                clock.ticks,
                state.cell.0,
                state.cell.1,
                state.pixel.0,
                state.pixel.1,
                state.step_to_pixel,
                state.step_elapsed,
                state.step_ticks,
                transform.translation.x,
                transform.translation.y,
                transform.translation.z,
                state.direction,
                state.walk_state,
            );
        }
    }
    for (label, mut transform) in &mut labels {
        let Some(&(x, y)) = drawn.get(&label.creature_id) else {
            continue;
        };
        transform.translation.x = x;
        transform.translation.y = -y - LABEL_OFFSET_Y;
    }
}

/// Where a creature is drawn this frame: the cell centre slid towards its step
/// target by the fraction of the step already elapsed.
///
/// `step_elapsed` starts at 1 (the logical tick a walk step commits on, kept for
/// the engine's tick contract), so the visible fraction is `(elapsed - 1 + sub)`
/// over the step length: a creature leaves its cell at 0 progress and reaches the
/// target as the step's last tick ends, continuous at both step boundaries instead
/// of jumping one frame-slice forward the instant it steps (review #68).
fn smooth_pixel(state: &CreatureState, sub: f32) -> (f32, f32) {
    let Some((tx, ty)) = state.step_to_pixel else {
        return (state.pixel.0 as f32, state.pixel.1 as f32);
    };
    if state.step_ticks == 0 {
        return (state.pixel.0 as f32, state.pixel.1 as f32);
    }
    let elapsed = (state.step_elapsed as f32 - 1.0 + sub).clamp(0.0, state.step_ticks as f32);
    let t = (elapsed / state.step_ticks as f32).min(1.0);
    (
        state.pixel.0 as f32 + (tx - state.pixel.0) as f32 * t,
        state.pixel.1 as f32 + (ty - state.pixel.1) as f32 * t,
    )
}

/// Swaps each creature sprite's [`Animator`] between the resting and the walking
/// sequence when the simulation's [`WalkState`] or facing direction changes,
/// keeping body, equipment and shadow of one creature in step.
fn sync_creature_animation(
    sim: Res<SimulationResource>,
    anim: Res<CreatureAnim>,
    mut sprites: Query<(&CreatureSprite, &mut Animator)>,
) {
    let Some(ref simulation) = sim.sim else {
        return;
    };
    let facing: HashMap<usize, (bool, u8)> = simulation
        .states()
        .iter()
        .map(|state| {
            (
                state.id,
                (
                    state.walk_state == WalkState::Walk,
                    state.direction.unwrap_or(0),
                ),
            )
        })
        .collect();
    for (tag, mut animator) in &mut sprites {
        let Some(&(walking, direction)) = facing.get(&tag.creature_id) else {
            continue;
        };
        let Some(creature) = anim.creatures.get(&tag.creature_id) else {
            continue;
        };
        let layer = if tag.is_shadow {
            &creature.shadow
        } else {
            match creature.body.get(&tag.layer) {
                Some(layer) => layer,
                None => continue,
            }
        };
        let clip = select_clip(layer, walking, direction);
        let Some(clip) = clip else { continue };
        if creature_debug() {
            info!(
                "creature #{} {} base={} walking={walking} dir={direction} shown={:?}",
                tag.creature_id,
                if tag.is_shadow { "shadow" } else { "body" },
                clip.base,
                animator.shown(),
            );
        }
        if Arc::ptr_eq(&animator.sequence, &clip.sequence) {
            continue;
        }
        animator.play(Arc::clone(&clip.sequence), clip.base);
    }
}

/// Which clip a piece shows: the walk sequence for its facing direction while
/// walking, otherwise the resting one. A missing walk direction falls back to
/// resting, so an unmapped direction never blanks the creature.
fn select_clip(layer: &LayerSequences, walking: bool, direction: u8) -> Option<&Clip> {
    if walking && let Some(walk) = layer.walk.get(direction as usize).and_then(Option::as_ref) {
        return Some(walk);
    }
    layer.rest.as_ref()
}

/// Как состояние воспроизведения подписать в диагностике.
fn playback_label(playback: PlayState) -> &'static str {
    match playback {
        PlayState::Playing => "playing",
        PlayState::Paused => "paused",
        PlayState::Off => "off",
    }
}

/// Сколько спрайтов видно и сколько собрано.
///
/// `InheritedVisibility` пишет Bevy, и в неё уже входит и скрытая группа, и кадр
/// анимации без картинки — счёт по детям групп завышал бы первое. Это линейный
/// обход, а не сумма по девяти узлам: он на три порядка дешевле переключения,
/// которое за ним следует (замеры — «Цена переключения слоёв» в docs/rust.md).
fn piece_counts(pieces: &Query<&InheritedVisibility, With<ScenePiece>>) -> (usize, usize) {
    pieces.iter().fold((0, 0), |(shown, total), visibility| {
        (shown + usize::from(visibility.get()), total + 1)
    })
}

fn diagnostics(
    stats: Res<Stats>,
    viewer: Res<Viewer>,
    clock: Res<Clock>,
    zoom: Res<CameraState>,
    frames: Res<DiagnosticsStore>,
    pieces: Query<&InheritedVisibility, With<ScenePiece>>,
    mut text: Single<&mut Text, With<Diagnostics>>,
) {
    let fps = frames
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|fps| fps.smoothed())
        .map_or_else(|| "—".to_owned(), |fps| format!("{fps:.0}"));
    let next = match (&stats.map, &stats.error) {
        (_, Some(error)) => format!("error: {error}"),
        (Some(map), None) => {
            let (shown, built) = piece_counts(&pieces);
            let pieces = if shown == built {
                format!("{built} pieces")
            } else {
                format!("{shown}/{built} pieces")
            };
            let problems = if stats.problems.is_empty() {
                String::new()
            } else {
                format!(" · {} textures missing", stats.problems.len())
            };
            // Карта без огней и движущихся объектов — без этой строки
            let playback = if stats.animated_pieces == 0 {
                String::new()
            } else {
                format!(
                    "\nanim {} · {} frames · tick {} · {}",
                    stats.animated_pieces,
                    stats.animation_frames,
                    clock.ticks,
                    playback_label(viewer.playback),
                )
            };
            format!(
                "{} {} · {pieces} · {} textures · {} ms{problems}\nzoom ×{} · {} fps{playback}",
                match map.source {
                    MapSource::Game => "map",
                    MapSource::Tutorial => "tutorial",
                },
                map.number,
                stats.unique_textures,
                stats.built_ms,
                zoom.scale(),
                fps,
            )
        }
        (None, None) => String::new(),
    };
    if text.0 != next {
        text.0 = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kn_game::PieceGroup;

    fn state(
        pixel: (i32, i32),
        step_to: Option<(i32, i32)>,
        elapsed: u32,
        ticks: u32,
    ) -> CreatureState {
        CreatureState {
            id: 0,
            cell: (0, 0),
            pixel,
            step_to_pixel: step_to,
            direction: None,
            moving: step_to.is_some(),
            step_elapsed: elapsed,
            step_ticks: ticks,
            walk_state: if step_to.is_some() {
                WalkState::Walk
            } else {
                WalkState::Rest
            },
        }
    }

    #[test]
    fn a_step_begins_at_the_cell_it_left() {
        // A walk step commits with elapsed = 1 but shows no progress yet, so the
        // creature leaves its cell at 0, not already a frame-slice across.
        assert_eq!(
            smooth_pixel(&state((58, 16), Some((116, 32)), 1, 4), 0.0),
            (58.0, 16.0)
        );
    }

    #[test]
    fn one_tick_into_a_four_tick_step_sits_a_quarter_across() {
        // The first tick shows nothing, the second shows one tick of four done.
        assert_eq!(
            smooth_pixel(&state((0, 0), Some((100, 40)), 2, 4), 0.0),
            (25.0, 10.0)
        );
    }

    #[test]
    fn the_sub_tick_fraction_pushes_the_interpolation_forward() {
        // One whole tick done plus half the next: t = 1.5/4
        assert_eq!(
            smooth_pixel(&state((0, 0), Some((100, 40)), 2, 4), 0.5),
            (37.5, 15.0)
        );
    }

    #[test]
    fn a_completed_step_clamps_at_the_target() {
        let landed = smooth_pixel(&state((0, 0), Some((100, 40)), 4, 4), 1.0);
        assert_eq!(landed, (100.0, 40.0), "t never passes 1");
    }

    #[test]
    fn a_creature_without_a_step_stays_put() {
        assert_eq!(
            smooth_pixel(&state((174, 80), None, 0, 0), 0.7),
            (174.0, 80.0)
        );
        // step_ticks 0 with a target still must not divide by zero
        assert_eq!(
            smooth_pixel(&state((0, 0), Some((10, 10)), 0, 0), 0.0),
            (0.0, 0.0)
        );
    }

    /// A step of any length must slide smoothly from the cell it left to the
    /// target: monotonic in x, starting at 0 progress and ending exactly on the
    /// target, so movement is continuous across every step boundary (1, 2 and 4
    /// tick durations, the values walk can produce).
    #[test]
    fn movement_is_continuous_across_step_boundaries() {
        for ticks in [1u32, 2, 4] {
            let xs: Vec<f32> = (1..=ticks)
                .flat_map(|elapsed| [0.0_f32, 0.5, 1.0].map(|sub| (elapsed, sub)))
                .map(|(elapsed, sub)| {
                    smooth_pixel(&state((0, 0), Some((100, 40)), elapsed, ticks), sub).0
                })
                .collect();
            assert_eq!(
                xs.first(),
                Some(&0.0),
                "{ticks}-tick step starts at its cell"
            );
            assert_eq!(
                xs.last(),
                Some(&100.0),
                "{ticks}-tick step lands on its target"
            );
            assert!(
                xs.windows(2).all(|w| w[1] >= w[0]),
                "{ticks}-tick step must never go backwards: {xs:?}"
            );
        }
    }

    fn clip_of(base: usize) -> Clip {
        Clip {
            sequence: Arc::new(Sequence::new(Vec::new())),
            base,
        }
    }

    /// Base frame of the clip `select_clip` picks for one walking state: the rest
    /// clip is base 0, the walk clip of the facing direction is base 1.
    fn chosen_base(walking: bool, direction: u8) -> usize {
        let layer = LayerSequences {
            rest: Some(clip_of(0)),
            walk: (0..8)
                .map(|d| {
                    if walking && d == direction {
                        Some(clip_of(1))
                    } else {
                        None
                    }
                })
                .collect(),
        };
        select_clip(&layer, walking, direction)
            .map(|clip| clip.base)
            .unwrap()
    }

    #[test]
    fn standing_shows_the_resting_sequence() {
        assert_eq!(chosen_base(false, 3), 0, "покой — базовый кадр покоя");
    }

    #[test]
    fn walking_shows_the_facing_direction() {
        assert_eq!(
            chosen_base(true, 5),
            1,
            "ходьба — кадр шага в направлении движения"
        );
    }

    #[test]
    fn an_unmapped_walk_direction_falls_back_to_rest() {
        // Only direction 5 has a walk clip; walking west (7) must fall back to rest.
        let layer = LayerSequences {
            rest: Some(clip_of(0)),
            walk: (0..8)
                .map(|d| if d == 5 { Some(clip_of(1)) } else { None })
                .collect(),
        };
        let chosen = select_clip(&layer, true, 7).unwrap();
        assert_eq!(chosen.base, 0);
    }

    #[test]
    fn an_out_of_range_direction_falls_back_to_rest() {
        let layer = LayerSequences {
            rest: Some(clip_of(0)),
            walk: Vec::new(),
        };
        let chosen = select_clip(&layer, true, 200).unwrap();
        assert_eq!(chosen.base, 0, "направление вне таблицы не роняет выбор");
    }

    #[test]
    fn depth_map_uses_only_static_object_stacks() {
        // Objects carry the occlusion; Characters (drawn by CreatureNodes) and the
        // ground Terrain layer must not be stacks, or a creature would depth-sort
        // against a moving sprite or the floor instead of the object it hides
        // behind (review #68).
        let piece = |y: i32, group: PieceGroup| Piece {
            texture: None,
            x: 0,
            y,
            w: None,
            h: Some(0),
            animation: None,
            group,
            creature_id: None,
        };
        let scene = kn_game::Scene {
            pieces: vec![
                piece(100, PieceGroup::Objects),    // order 0 → z 0
                piece(500, PieceGroup::Objects),    // order 1 → z 0.01
                piece(200, PieceGroup::Characters), // order 2 → excluded
                piece(50, PieceGroup::Terrain),     // order 3 → excluded
            ],
        };
        let map = build_scene_depth(&scene, &TextureCache::default());
        // Object bottoms are the only boundaries; between them the z interpolates.
        assert_eq!(map.z_for_bottom(100), 0.0);
        assert_eq!(map.z_for_bottom(500), 0.01);
        // Bottom 200 is the Characters piece: excluded, so it interpolates between
        // the object stacks rather than becoming its own stack at z 0.02.
        let at_200 = map.z_for_bottom(200);
        assert!(
            (at_200 - 0.0025).abs() < 1e-5,
            "characters must not set a boundary, got {at_200}"
        );
        // Terrain (bottom 50) is excluded too: below the first object stack it
        // reads the first stack's z, not its own higher draw z.
        assert_eq!(map.z_for_bottom(50), map.z_for_bottom(100));
    }

    #[test]
    fn labels_show_source_and_number() {
        assert_eq!(
            map_labels(&[
                MapRef {
                    source: MapSource::Game,
                    number: 12
                },
                MapRef {
                    source: MapSource::Tutorial,
                    number: 4
                },
            ]),
            ["Map 12", "Tutorial 4"]
        );
    }

    fn viewer(overlays: [bool; 4]) -> Viewer {
        Viewer {
            maps: vec![],
            selected: 0,
            overlays,
            playback: DEFAULT_PLAYBACK,
        }
    }

    #[test]
    fn overlay_flags_round_trip() {
        let mut viewer = viewer([false; 4]);
        assert!(!viewer.is_on(Overlay::Markers));
        viewer.set(Overlay::Markers, true);
        assert!(viewer.is_on(Overlay::Markers));
        assert!(!viewer.is_on(Overlay::Foot));
        assert!(viewer.options().markers && !viewer.options().foot);
    }

    #[test]
    fn default_overlays_show_characters_only() {
        // Совпадает с `Checked` у чекбокса Characters в panel()
        let viewer = viewer(DEFAULT_OVERLAYS);
        assert!(viewer.is_on(Overlay::Characters));
        assert!(!viewer.is_on(Overlay::Foot));
        assert!(!viewer.is_on(Overlay::Lights));
        assert!(!viewer.is_on(Overlay::Markers));
        let options = viewer.options();
        assert!(options.characters && !options.foot && !options.lights && !options.markers);
        // Просмотрщик героя не выбирает: фасад делает это по источнику карты,
        // иначе tutorial-карты описывались бы состоянием основной игры
        assert!(options.hero.is_none());
    }

    #[test]
    fn play_toggle_never_stops_the_animation_behind_the_simulation_s_back() {
        // Единственный переключатель движения: снятая галочка — пауза, а не
        // выключение. Ни одна ветка интерфейса не доводит часы до `Off`, иначе
        // отключение визуальной анимации молча остановило бы движение существ.
        assert_eq!(playback_for(true), PlayState::Playing);
        assert_eq!(playback_for(false), PlayState::Paused);
        assert_ne!(playback_for(false), PlayState::Off);
        // Из любого рабочего состояния переключение остаётся в паре play/pause
        for from in [PlayState::Playing, PlayState::Paused] {
            assert_ne!(toggle_playback(from), PlayState::Off, "{from:?}");
        }
    }

    #[test]
    fn space_flips_between_playing_and_paused() {
        assert_eq!(toggle_playback(PlayState::Playing), PlayState::Paused);
        assert_eq!(toggle_playback(PlayState::Paused), PlayState::Playing);
    }

    #[test]
    fn the_play_checkbox_mirrors_playing() {
        for (playback, checked) in [
            (PlayState::Playing, true),
            (PlayState::Paused, false),
            // `Off` интерфейсом не достижим, но если и возникнет — галочка снята
            (PlayState::Off, false),
        ] {
            let mut viewer = viewer([false; 4]);
            viewer.playback = playback;
            assert_eq!(playback_checked(&viewer), checked, "{playback:?}");
        }
    }

    #[test]
    fn default_playback_is_playing() {
        // Совпадает с `Checked` у чекбокса Play в panel()
        let viewer = viewer(DEFAULT_OVERLAYS);
        assert_eq!(viewer.playback, PlayState::Playing);
        assert!(playback_checked(&viewer));
    }

    /// Узлы групп и их видимость после [`sync_groups`] при этих переключателях.
    ///
    /// Узлы спавнятся руками с `Inherited`, как их спавнит `spawn_scene`: важно,
    /// что система делает со скрытым слоем, а не то, сколько спрайтов под узлом.
    fn group_visibility(overlays: [bool; 4]) -> Vec<(PieceGroup, Visibility)> {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(viewer(overlays))
            .init_resource::<CameraBounds>()
            .add_systems(Update, sync_groups);
        let nodes: Vec<(PieceGroup, Entity)> = PieceGroup::ALL
            .iter()
            .map(|&group| {
                (
                    group,
                    app.world_mut()
                        .spawn((SceneGroup::new(group, None), Visibility::Inherited))
                        .id(),
                )
            })
            .collect();
        app.update();
        nodes
            .iter()
            .map(|&(group, node)| {
                (
                    group,
                    *app.world()
                        .get::<Visibility>(node)
                        .expect("узел группы жив"),
                )
            })
            .collect()
    }

    #[test]
    fn a_switch_hides_exactly_the_groups_the_model_hides() {
        for group in PieceGroup::ALL {
            for index in 0..4 {
                let mut overlays = [false; 4];
                overlays[index] = true;
                let options = SceneOptions {
                    foot: index == 0,
                    lights: index == 1,
                    markers: index == 2,
                    characters: index == 3,
                    hero: None,
                };
                let visible = group_visibility(overlays)
                    .into_iter()
                    .find(|(g, _)| *g == group)
                    .expect("группа под своим узлом")
                    .1
                    == Visibility::Inherited;
                assert_eq!(
                    visible,
                    group_is_shown(group, &options),
                    "{group:?} при переключателе {index}"
                );
            }
        }
    }

    #[test]
    fn a_shown_group_follows_the_map_root_instead_of_overriding_it() {
        // Включённая группа — `Inherited`, иначе она перебивает наследование и
        // карту, спрятанную целиком на `MapRoot`, было бы видно
        let characters = group_visibility(DEFAULT_OVERLAYS)
            .into_iter()
            .find(|(group, _)| *group == PieceGroup::Characters)
            .expect("персонажи включены");
        assert_eq!(characters.1, Visibility::Inherited);
    }

    #[test]
    fn a_layer_outside_the_map_widens_the_camera_bounds() {
        let terrain = (Vec2::new(0.0, -200.0), Vec2::new(1000.0, 0.0));
        // Ромбы проходимости идут по всей сетке и дальше тайлов карты — с обеих
        // сторон, чтобы рамка с ними сравнивалась целиком
        let foot = (Vec2::new(-50.0, -260.0), Vec2::new(1050.0, 20.0));
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(viewer([false; 4]))
            .init_resource::<CameraBounds>()
            .add_systems(Update, sync_groups);
        let map_node = app
            .world_mut()
            .spawn((
                SceneGroup::new(PieceGroup::Terrain, Some(terrain)),
                Visibility::Inherited,
            ))
            .id();
        let walkability = app
            .world_mut()
            .spawn((
                SceneGroup::new(PieceGroup::Foot, Some(foot)),
                Visibility::Inherited,
            ))
            .id();
        app.update();
        assert_eq!(app.world().resource::<CameraBounds>().min, terrain.0);
        assert_eq!(app.world().resource::<CameraBounds>().max, terrain.1);

        // Слой включили — рамка расширилась, отступ и остальное остались своими
        app.world_mut()
            .resource_mut::<Viewer>()
            .set(Overlay::Foot, true);
        app.update();
        let bounds = *app.world().resource::<CameraBounds>();
        assert_eq!(bounds.min, foot.0, "границы расширились по x и y");
        assert_eq!(bounds.max, foot.1);
        assert_eq!(bounds.margin, CameraBounds::default().margin);

        // Слой выключили — рамка сузилась обратно: за неё камеру вернёт
        // ограничение в PostUpdate, а не этот расчёт
        app.world_mut()
            .resource_mut::<Viewer>()
            .set(Overlay::Foot, false);
        app.update();
        assert_eq!(app.world().resource::<CameraBounds>().min, terrain.0);
        assert_eq!(app.world().resource::<CameraBounds>().max, terrain.1);
        assert_eq!(
            *app.world().get::<Visibility>(map_node).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(
            *app.world().get::<Visibility>(walkability).unwrap(),
            Visibility::Hidden
        );
    }

    #[test]
    fn map_groups_survive_every_switch() {
        let all_on = group_visibility([true; 4]);
        assert_eq!(all_on.len(), PieceGroup::ALL.len(), "узел на каждую группу");
        assert!(
            all_on
                .iter()
                .all(|(_, visibility)| *visibility == Visibility::Inherited),
            "все слои включены"
        );
        // Выключенный слой не убирает узлы: карта остаётся собранной, и её можно
        // показать обратно без пересборки
        assert_eq!(group_visibility([false; 4]).len(), all_on.len());
    }

    // --- Reset positions -----------------------------------------------------

    /// First game directory with data and its first map, or `None` to skip.
    fn game_and_first_map() -> Option<(PathBuf, MapRef)> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../original/k1");
        for edition in ["rus", "eng"] {
            let dir = root.join(edition).join("game");
            if dir.is_dir() {
                let game = Game::open(&dir).ok()?;
                let map_ref = *game.maps().ok()?.first()?;
                return Some((dir, map_ref));
            }
        }
        eprintln!("пропуск: нет original/k1/{{rus,eng}}/game (docs/k1/editions.md)");
        None
    }

    fn home(sim: &Simulation) -> Vec<(usize, (i32, i32))> {
        sim.states().iter().map(|s| (s.id, s.pixel)).collect()
    }

    #[test]
    fn reset_returns_creatures_home_and_resyncs_the_tracker() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();

        let mut sim = SimulationResource::default();
        sim.sim = game.simulation(map_ref, SIM_SEED).ok();
        sim.initial_pixels = sim.sim.as_ref().unwrap().initial_pixels();
        sim.labels_spawned = true;
        let root = Entity::PLACEHOLDER;
        sim.label_root = Some(root);
        let initial = home(sim.sim.as_ref().unwrap());

        // Уводим существ: без движения проверка сброса ничего не доказала бы
        for _ in 0..200 {
            sim.sim.as_mut().unwrap().tick();
        }
        let initial_by_id: HashMap<usize, (i32, i32)> = initial.iter().copied().collect();
        assert!(
            home(sim.sim.as_ref().unwrap())
                .iter()
                .any(|(id, pixel)| initial_by_id.get(id) != Some(pixel)),
            "за 200 тиков хотя бы один персонаж должен был сойти с места"
        );

        let mut tracker = SimTickTracker {
            last_clock_ticks: 0,
        };
        // Старый трекер отстаёт от часов: сброс обязан подтянуть его к текущему
        // тику, иначе свежая симуляция отыграет накопленное рывком
        reset_simulation(&game, map_ref, 517, &mut sim, &mut tracker);
        assert_eq!(
            tracker.last_clock_ticks, 517,
            "трекер синхронизирован с часами"
        );
        assert_eq!(
            home(sim.sim.as_ref().unwrap()),
            initial,
            "существа вернулись домой"
        );
        assert!(sim.labels_spawned, "метки не пересоздаются");
        assert_eq!(sim.label_root, Some(root), "корень меток сохранён");

        // ГСЧ свежий: сброшенная симуляция повторяет только что созданную один в один
        let mut reference = game.simulation(map_ref, SIM_SEED).unwrap();
        for _ in 0..120 {
            reference.tick();
        }
        let expected = home(&reference);
        for _ in 0..120 {
            sim.sim.as_mut().unwrap().tick();
        }
        assert_eq!(
            home(sim.sim.as_ref().unwrap()),
            expected,
            "сброс заново сеет ГСЧ"
        );
    }

    /// The reset button leaves the panel settings and the pause state alone and
    /// does not flag a map rebuild.
    #[test]
    fn reset_keeps_the_panel_settings() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();

        let mut sim = SimulationResource::default();
        sim.sim = game.simulation(map_ref, SIM_SEED).ok();
        sim.initial_pixels = sim.sim.as_ref().unwrap().initial_pixels();
        let initial = home(sim.sim.as_ref().unwrap());
        for _ in 0..200 {
            sim.sim.as_mut().unwrap().tick();
        }

        let overlays = [true, false, true, false];
        let mut clock = Clock::new(TICK_MS as f32);
        clock.goto(PlayState::Paused);
        clock.ticks = 300;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(GameResource(game))
            .insert_resource(Viewer {
                maps: vec![map_ref],
                selected: 0,
                overlays,
                playback: PlayState::Paused,
            })
            .insert_resource(Rebuild { map: false })
            .insert_resource(clock)
            .insert_resource(sim)
            .insert_resource(SimTickTracker {
                last_clock_ticks: 0,
            })
            .add_observer(reset_positions)
            .add_systems(Update, fire_reset);
        app.world_mut().spawn(ResetPositions);

        app.update();
        app.update();

        let sim = app.world().resource::<SimulationResource>();
        assert_eq!(
            home(sim.sim.as_ref().unwrap()),
            initial,
            "кнопка вернула существ домой"
        );
        assert_eq!(
            app.world().resource::<SimTickTracker>().last_clock_ticks,
            300,
            "трекер подтянут к часам без рывка"
        );
        let viewer = app.world().resource::<Viewer>();
        assert_eq!(viewer.overlays, overlays, "оверлеи сохранены");
        assert_eq!(viewer.selected, 0, "карта выбрана та же");
        assert_eq!(viewer.playback, PlayState::Paused, "пауза сохранена");
        assert_eq!(
            app.world().resource::<Clock>().state,
            PlayState::Paused,
            "часы на паузе"
        );
        assert!(
            !app.world().resource::<Rebuild>().map,
            "карта не пересобирается"
        );
    }

    /// Rebuilding and replaying a map to tick `n` gives exactly the state forward
    /// stepping reached at tick `n`, so the backward `Step` (which replays) shows
    /// the same creature positions a forward run would (review #68, no reverse tick).
    #[test]
    fn replay_simulation_matches_forward_stepping() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();
        let mut reference = game.simulation(map_ref, SIM_SEED).unwrap();
        for _ in 0..150 {
            reference.tick();
        }
        let expected: Vec<(usize, (i32, i32))> =
            reference.states().iter().map(|s| (s.id, s.pixel)).collect();

        let replayed = replay_simulation(&game, map_ref, 150).expect("replay builds");
        let got: Vec<(usize, (i32, i32))> =
            replayed.states().iter().map(|s| (s.id, s.pixel)).collect();
        assert_eq!(
            got, expected,
            "перемотка к тику 150 совпадает с прямым прогоном"
        );
    }

    /// After a map change the tracker is pinned to the clock, so the fresh
    /// simulation does not replay the ticks the previous map had already run — the
    /// catch-up burst that used to fast-forward every creature in one frame
    /// (review #68). A forward run from a behind tracker still advances normally.
    #[test]
    fn map_change_pins_the_tracker_so_history_is_not_replayed() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();
        let sim = game.simulation(map_ref, SIM_SEED).ok();
        let initial = sim.as_ref().unwrap().initial_pixels();
        let sim_resource = SimulationResource {
            sim,
            initial_pixels: initial.clone(),
            ..Default::default()
        };
        let by_id: HashMap<usize, (i32, i32)> = initial.into_iter().collect();

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(GameResource(game))
            .insert_resource(Viewer {
                maps: vec![map_ref],
                selected: 0,
                overlays: DEFAULT_OVERLAYS,
                playback: PlayState::Paused,
            })
            .insert_resource(sim_resource)
            .insert_resource(Clock::new(TICK_MS as f32))
            .insert_resource(SimTickTracker {
                last_clock_ticks: 900,
            })
            .add_systems(Update, advance_simulation);
        app.world_mut().resource_mut::<Clock>().ticks = 900;
        app.update();

        let states = app
            .world()
            .resource::<SimulationResource>()
            .sim
            .as_ref()
            .unwrap()
            .states();
        assert!(!states.is_empty(), "на карте должны быть существа");
        for state in &states {
            assert_eq!(
                state.pixel, by_id[&state.id],
                "смена карты не должна проматывать накопленную историю"
            );
        }
    }

    /// A backward `Step` rewinds the clock; `advance_simulation` rebuilds the map's
    /// simulation and replays it to the rewound tick, so creatures show the same
    /// positions a forward run would. The replay already leaves the fresh simulation
    /// at that tick, so the tracker must be pinned there — pinning it to 0 let the
    /// catch-up loop tick the simulation a second `clock.ticks` times and every
    /// creature overshot the rewound state (review #68, reverse step).
    #[test]
    fn a_backward_step_rewinds_the_simulation_to_the_clock_not_beyond_it() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let reference_game = Game::open(&dir).unwrap();
        let mut reference = reference_game.simulation(map_ref, SIM_SEED).unwrap();
        for _ in 0..150 {
            reference.tick();
        }
        let expected: Vec<(usize, (i32, i32))> =
            reference.states().iter().map(|s| (s.id, s.pixel)).collect();

        let game = Game::open(&dir).unwrap();
        let sim_resource = SimulationResource {
            sim: game.simulation(map_ref, SIM_SEED).ok(),
            ..Default::default()
        };

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(GameResource(game))
            .insert_resource(Viewer {
                maps: vec![map_ref],
                selected: 0,
                overlays: DEFAULT_OVERLAYS,
                playback: PlayState::Paused,
            })
            .insert_resource(sim_resource)
            .insert_resource(Clock::new(TICK_MS as f32))
            // A forward run reached tick 200; a backward Step rewinds the clock to 150.
            .insert_resource(SimTickTracker {
                last_clock_ticks: 200,
            })
            .add_systems(Update, advance_simulation);
        app.world_mut().resource_mut::<Clock>().ticks = 150;
        app.update();

        let got: Vec<(usize, (i32, i32))> = app
            .world()
            .resource::<SimulationResource>()
            .sim
            .as_ref()
            .unwrap()
            .states()
            .iter()
            .map(|s| (s.id, s.pixel))
            .collect();
        assert_eq!(
            got, expected,
            "a backward Step must show the forward run's state at the rewound tick"
        );
    }

    fn fire_reset(mut commands: Commands, buttons: Query<Entity, With<ResetPositions>>) {
        for button in &buttons {
            commands.trigger(Activate { entity: button });
        }
    }
}
