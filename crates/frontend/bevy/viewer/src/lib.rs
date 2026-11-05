//! Просмотрщик карт «Князя»: состояние, перестроение сцены и интерфейс.
//!
//! Состояние ([Viewer]) и команды интерфейса отделены от отрисовки: обработчики
//! виджетов правят только состояние и взводят [Rebuild], а сцену перестраивает
//! одна система. CLI parsing and the headless check live in the `kn-viewer`
//! app crate.

use std::collections::HashMap;
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
use kn_game::{Game, MapRef, MapSource, Piece, SceneOptions, TICK_MS, group_is_shown};
use kn_input::{ZoomScroll, drag_delta, pan_direction, wheel_steps};
use kn_render::{
    AnimPlugin, AnimSet, Animator, CameraApply, CameraBounds, CameraInput, CameraState, Clock,
    CreatureAnim, CreatureAnimReport, CreatureAnimationPlugin, CreatureMotion, CreatureNode,
    CreatureNodesVisible, CreatureRoot, CreatureSet, CreatureShadow, CreatureSim, CreatureSprite,
    GameResource, MapCamera, MapCameraPlugin, PAN_SPEED, PendingFrame, PlayState, SceneGroup,
    ScenePiece, Sequence, TextureCache, clear_scene, load_scene, ortho_scale, reset_creature_sim,
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

/// Per-creature id labels the viewer draws above the creatures. The creature
/// nodes themselves belong to the shared subsystem: `kn_render` spawns and
/// animates them, the viewer only puts its diagnostics on top (issue #70).
#[derive(Resource, Default)]
struct CreatureLabels {
    label_root: Option<Entity>,
    labels_spawned: bool,
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

/// Кнопка "Reset animation" (issue #70 AC4): brings the clock and the
/// creature state machine back to what the file records, so the next Play
/// replays the state-4 special from frame 0. Distinct from "Reset positions",
/// which returns creatures to their cell pixels but leaves the clock alone.
#[derive(Component, Clone, Copy, Default)]
struct ResetAnimation;

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
            // Составные спрайты существ, их клипы и синхронизация кадров: та же
            // подсистема, что поднимает существ в проигрывателе (#70)
            CreatureAnimationPlugin,
            FrameTimeDiagnosticsPlugin::default(),
        ))
        .insert_resource(dark_theme())
        .insert_resource(GameResource(config.game))
        .init_resource::<TextureCache>()
        .init_resource::<ActiveRoot>()
        .init_resource::<Stats>()
        .init_resource::<CreatureLabels>()
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
                sync_creature_visibility,
                spawn_creature_labels,
                diagnostics,
            )
                .chain()
                // The frame's clock position must be settled before the simulation
                // catches up to it and before the creature systems read it, and the
                // map a rebuild asks for must be requested before the shared
                // subsystem builds it — never left to the scheduler's placement of
                // two unconstrained chains (review #68).
                .after(AnimSet::Clock)
                .after(CameraInput)
                .before(CreatureSet::Prepare)
                .before(CameraApply),
        )
        // The viewer's diagnostics on top of the shared pipeline: the id rides the
        // body image, so it can only be placed once the subsystem has written the
        // node and the frame's sprite offsets.
        .add_systems(
            Update,
            sync_creature_labels
                .after(CreatureSet::Swap)
                .after(AnimSet::Sprites)
                .after(CreatureSet::Nodes),
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
            @FeathersButton {
                @caption: bsn! { @caption("Reset animation") }
            }
            ResetAnimation
            on(reset_animation)
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
    mut labels: ResMut<CreatureLabels>,
    mut motion: ResMut<CreatureMotion>,
    mut visible: ResMut<CreatureNodesVisible>,
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
    // The scene's creatures belong to the shared subsystem: it builds the
    // simulation, the clips, the depth map and the nodes from the scene handed
    // over here. The viewer only retires its own labels and decides whether the
    // creatures are born visible, so a map change with characters off shows
    // nothing for the first frame (review #68).
    labels.labels_spawned = false;
    if let Some(root) = labels.label_root.take() {
        commands.entity(root).despawn();
    }
    visible.0 = viewer.is_on(Overlay::Characters);
    motion.request(map_ref, SIM_SEED, scene);
}

/// Видимость куска по включённым слоям: границы сцены считают только по тому,
/// что действительно на экране.
fn shown(options: &SceneOptions) -> impl Fn(&Piece) -> bool + '_ {
    move |piece| group_is_shown(piece.group, options)
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

/// Shows creature nodes and their id labels together with the Characters overlay.
///
/// The nodes live under their own [`CreatureRoot`] and the labels under a
/// [`LabelRoot`], neither inside the Characters scene group, so turning characters
/// off would otherwise leave sprites and `#ID` texts floating over an empty map
/// (review #68). `Inherited` so a map hidden on `MapRoot` stays hidden. One query over
/// both roots so the shared `&mut Visibility` has no split-query conflict (B0001).
#[allow(clippy::type_complexity)]
fn sync_creature_visibility(
    viewer: Res<Viewer>,
    mut roots: Query<&mut Visibility, Or<(With<CreatureRoot>, With<LabelRoot>)>>,
) {
    let target = if viewer.is_on(Overlay::Characters) {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut visibility in &mut roots {
        if *visibility != target {
            *visibility = target;
        }
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
    mut sim: ResMut<CreatureSim>,
) {
    if markers.get(activate.entity).is_err() {
        return;
    }
    let Some(&map_ref) = viewer.maps.get(viewer.selected) else {
        return;
    };
    reset_creature_sim(&game.0, map_ref, SIM_SEED, clock.ticks, &mut sim);
}

/// Кнопка "Reset animation" (issue #70 AC4): the viewer's `Play → Off → Play`
/// path. Zeroes the clock, puts `Viewer::playback` into `Off` and rebuilds the
/// simulation so the fresh state machine reseeds every state-4 creature to
/// `Special { frame: 0 }`. Nothing writes map data — the fresh simulation is
/// what `Game::simulation` produces from the file. A later `Play` on the
/// checkbox flips the state back to `Playing` and the special plays again.
#[allow(clippy::too_many_arguments)]
fn reset_animation(
    activate: On<Activate>,
    markers: Query<&ResetAnimation>,
    game: Res<GameResource>,
    viewer: Res<Viewer>,
    mut viewer_state: ResMut<Viewer>,
    mut clock: ResMut<Clock>,
    mut sim: ResMut<CreatureSim>,
) {
    if markers.get(activate.entity).is_err() {
        return;
    }
    let Some(&map_ref) = viewer.maps.get(viewer.selected) else {
        return;
    };
    // `goto(Off)` zeroes the clock and its sub-tick remainder, so the fresh
    // simulation starts on tick 0 (the machine's frame 0), matching what the
    // file records. The simulation's epoch follows the clock.
    clock.goto(PlayState::Off);
    viewer_state.playback = PlayState::Off;
    reset_creature_sim(&game.0, map_ref, SIM_SEED, clock.ticks, &mut sim);
}

const LABEL_OFFSET_Y: f32 = 50.0;

/// Temporary per-creature diagnostics for the movement/animation review: set
/// `KN_CREATURE_DEBUG` in the environment to log, per creature id, the simulation
/// tick, cell, step from/to, `step_elapsed`, the cell pixel the node sits on, the
/// node position and z as this frame writes them, and the simulation's own
/// facing direction; the same pass ([`sync_creature_labels`]) adds each sprite's
/// chosen clip — named by [`clip_identity`] against the clips the subsystem built
/// — with its anchor, phase and shown frame, its local offset and the world
/// position as it will be drawn (the frame's `GlobalTransform`s are only
/// propagated at the end of the update, so reading them would log the previous
/// frame). Comparing `sim_dir`/`from`/`to` (which change only at a logical tick)
/// with the sprite's world position separates a node jump from an image jump: the
/// node only ever sits on cell pixels, the glide to the next cell lives in the
/// frame offsets. The logging stays here, in the viewer: the shared subsystem in
/// `kn-render` writes no diagnostics of its own.
static CREATURE_DEBUG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

fn creature_debug() -> bool {
    *CREATURE_DEBUG.get_or_init(|| std::env::var_os("KN_CREATURE_DEBUG").is_some())
}

fn spawn_creature_labels(
    sim: Res<CreatureSim>,
    mut labels: ResMut<CreatureLabels>,
    viewer: Res<Viewer>,
    mut commands: Commands,
) {
    if labels.labels_spawned || sim.sim.is_none() {
        return;
    }
    let states = sim.sim.as_ref().unwrap().states();
    labels.labels_spawned = true;
    // Born with the Characters overlay's visibility, not a fixed one: a root
    // that were `Visible` would render for the frame before
    // [`sync_creature_visibility`] hides it, leaking creature ids over an
    // empty map (it hangs off its own root, not the Characters group).
    let target = if viewer.is_on(Overlay::Characters) {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    let root = commands
        .spawn((LabelRoot, Transform::IDENTITY, target))
        .id();
    labels.label_root = Some(root);
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

/// Puts each id label above its creature's body image and logs the frame's
/// creature state under `KN_CREATURE_DEBUG`.
///
/// The label rides the image, not the cell: during a step the image is a cell
/// away from the node, carried there by its walk frames. The world position is
/// the node's — the shared subsystem wrote this frame's cell into it — plus the
/// sprite's local offset, which `apply_animators` has just updated; the sprite's
/// own `GlobalTransform` would still carry the previous frame, because
/// propagation runs at the end of the update (review #68).
#[allow(clippy::type_complexity)]
fn sync_creature_labels(
    sim: Res<CreatureSim>,
    clock: Res<Clock>,
    anim: Res<CreatureAnim>,
    sprites: Query<(&CreatureSprite, &Transform), Without<CreatureShadow>>,
    nodes: Query<(&CreatureNode, &Transform)>,
    // Read-only over the sprites, so it overlaps the queries above freely.
    clips: Query<(&CreatureSprite, &Animator, &Transform)>,
    // `Without<CreatureNode>` on top of `Without<CreatureSprite>`: this pass
    // writes the `Transform` the two queries above read, and Bevy cannot prove
    // an entity never carries both (B0001).
    mut labels: Query<
        (&CreatureLabel, &mut Transform),
        (Without<CreatureSprite>, Without<CreatureNode>),
    >,
) {
    let Some(ref simulation) = sim.sim else {
        return;
    };
    let states = simulation.states();
    let drawn: HashMap<usize, (i32, i32)> =
        states.iter().map(|state| (state.id, state.pixel)).collect();
    let mut label_at: HashMap<usize, (f32, f32, usize)> = HashMap::new();
    for (tag, transform) in &sprites {
        if tag.is_shadow {
            continue;
        }
        let Some(&(x, y)) = drawn.get(&tag.creature_id) else {
            continue;
        };
        let translation = transform.translation;
        let (world_x, world_y) = (x as f32 + translation.x, -(y as f32) + translation.y);
        label_at
            .entry(tag.creature_id)
            .and_modify(|existing| {
                // Keep the lowest layer index: the body, not the equipment.
                if tag.layer < existing.2 {
                    *existing = (world_x, world_y, tag.layer);
                }
            })
            .or_insert((world_x, world_y, tag.layer));
    }
    let label_at: HashMap<usize, (f32, f32)> = label_at
        .into_iter()
        .map(|(id, (x, y, _))| (id, (x, y)))
        .collect();
    for (label, mut transform) in &mut labels {
        let Some(&(x, y)) = label_at.get(&label.creature_id) else {
            continue;
        };
        transform.translation.x = x;
        transform.translation.y = y - LABEL_OFFSET_Y;
    }
    if !creature_debug() {
        return;
    }
    for (node, transform) in &nodes {
        let id = node.id as usize;
        let Some(state) = states.iter().find(|state| state.id == id) else {
            continue;
        };
        // The world position is the `Transform` the subsystem just wrote: the
        // node's parent is the identity root, and this frame's `GlobalTransform`
        // is only propagated at the end of the update, so reading it here would
        // log the previous frame (review #68).
        info!(
            "creature #{id} tick={} cell=({},{}) from=({},{}) to={:?} elapsed={}/{} \
             pixel=({},{}) render=({:.1},{:.1}) z={:.4} \
             sim_dir={:?} walk={:?}",
            clock.ticks,
            state.cell.0,
            state.cell.1,
            state.pixel.0,
            state.pixel.1,
            state.step_to_pixel,
            state.step_elapsed,
            state.step_ticks,
            state.pixel.0,
            state.pixel.1,
            transform.translation.x,
            transform.translation.y,
            transform.translation.z,
            state.direction,
            state.walk_state,
        );
    }
    for (tag, animator, transform) in &clips {
        let local = transform.translation;
        // The world position as it will be drawn: this frame's cell pixel plus
        // this sprite's local offset. The sprite's own `GlobalTransform` still
        // holds the previous frame — propagation runs at the end of the update.
        let (world_x, world_y) = drawn
            .get(&tag.creature_id)
            .map_or((local.x, local.y), |&(x, y)| {
                (x as f32 + local.x, -(y as f32) + local.y)
            });
        let chosen = clip_identity(&anim, tag, &animator.sequence);
        info!(
            "creature #{} {} clip={} base={} corner={:?} shown={:?} anchor=({},{}) phase={} \
             local=({:.1},{:.1}) world=({:.1},{:.1})",
            tag.creature_id,
            if tag.is_shadow { "shadow" } else { "body" },
            chosen
                .as_ref()
                .map_or("foreign", |(kind, _, _)| kind.as_str()),
            chosen.as_ref().map_or(0, |(_, base, _)| *base),
            chosen.as_ref().map(|(_, _, corner)| *corner),
            animator.shown(),
            animator.x,
            animator.y,
            animator.phase,
            local.x,
            local.y,
            world_x,
            world_y,
        );
    }
}

/// Names the clip an animator holds — `rest`, `walk[dir]` or `special` — by
/// matching its `Arc` against the clips the subsystem built for that layer, so
/// the debug line says what is on screen without the viewer choosing a clip
/// itself. `None` when the animator carries a sequence of its own.
fn clip_identity(
    anim: &CreatureAnim,
    tag: &CreatureSprite,
    sequence: &Arc<Sequence>,
) -> Option<(String, usize, (i32, i32))> {
    let sequences = anim.creatures.get(&tag.creature_id)?;
    let layer = if tag.is_shadow {
        &sequences.shadow
    } else {
        sequences.body.get(&tag.layer)?
    };
    std::iter::once(("rest".to_string(), layer.rest.as_ref()))
        .chain(
            layer
                .walk
                .iter()
                .enumerate()
                .map(|(dir, clip)| (format!("walk[{dir}]"), clip.as_ref())),
        )
        .chain(std::iter::once((
            "special".to_string(),
            layer.special.as_ref(),
        )))
        .find_map(|(kind, clip)| {
            clip.filter(|clip| Arc::ptr_eq(&clip.sequence, sequence))
                .map(|clip| (kind, clip.base, clip.corner))
        })
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

#[allow(clippy::too_many_arguments)]
fn diagnostics(
    stats: Res<Stats>,
    sim: Res<CreatureSim>,
    creatures: Res<CreatureAnimReport>,
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
            // A simulation that failed to build shows its reason, not a silent still map.
            let sim_error = sim
                .error
                .as_ref()
                .map_or(String::new(), |error| format!(" · {error}"));
            let creatures = creature_report(&creatures);
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
                "{} {} · {pieces} · {} textures · {} ms{problems}{sim_error}{creatures}\nzoom ×{} · {} fps{playback}",
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

/// How the shared subsystem's creature findings read in this viewer's panel. A
/// creature walking on its rest pose is a rendering defect, not an absence of
/// creatures to explain away, so every reason the subsystem reported shows
/// (review #68). The wording is the viewer's; the numbers come from kn-render.
fn creature_report(report: &CreatureAnimReport) -> String {
    if let Some(error) = &report.error {
        return format!(" · creatures: {error}");
    }
    let mut parts = Vec::new();
    if report.total != 0 {
        parts.push(format!("{} creatures", report.total));
    }
    if report.without_walk != 0 {
        parts.push(format!("{} without walk", report.without_walk));
    }
    if report.partial_walk != 0 {
        parts.push(format!("{} partial walk", report.partial_walk));
    }
    if let Some(error) = &report.texture_error {
        parts.push(format!("walk: {error}"));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" · {}", parts.join(" · "))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use super::*;
    use kn_game::simulation::{CreatureState, Simulation, WalkState};
    use kn_game::{LayerKey, PieceGroup};
    use kn_render::{
        Animator, Clip, CreatureAnim, CreatureDepth, CreatureSpawns, LayerSequences, Sequence,
        advance_creature_sim, build_creature_anim, build_creature_spawns, build_depth_map,
        select_clip, spawn_creature_nodes, sync_creature_nodes,
    };

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
    fn a_new_step_asks_for_reanchoring_once() {
        let mut anim = CreatureAnim::default();
        let walking = [state((58, 16), Some((116, 32)), 1, 12)];
        assert_eq!(
            anim.observe_steps(&walking),
            [0].into_iter().collect::<HashSet<_>>()
        );
        // The same step seen on its later ticks must not re-anchor again: the
        // cycle keeps running from where the step opened it.
        let later = [state((58, 16), Some((116, 32)), 7, 12)];
        assert!(anim.observe_steps(&later).is_empty());
    }

    #[test]
    fn a_restarted_step_asks_for_reanchoring_too() {
        // `World::cancel_step` rolls a step back and the same `(from, to)` may
        // be ordered again: the key is unchanged, but a falling `step_elapsed`
        // is a new step and must re-anchor — otherwise the cycle keeps running
        // mid-step and the image wraps a cell behind the creature (review #68).
        let mut anim = CreatureAnim::default();
        anim.observe_steps(&[state((58, 16), Some((116, 32)), 1, 12)]);
        anim.observe_steps(&[state((58, 16), Some((116, 32)), 8, 12)]);
        assert_eq!(
            anim.observe_steps(&[state((58, 16), Some((116, 32)), 1, 12)]),
            [0].into_iter().collect::<HashSet<_>>(),
            "elapsed falling under an unchanged key is a restarted step"
        );
        assert!(
            anim.observe_steps(&[state((58, 16), Some((116, 32)), 2, 12)])
                .is_empty(),
            "the restarted step anchors once"
        );
    }

    #[test]
    fn a_turn_or_a_new_step_reanchors() {
        let mut anim = CreatureAnim::default();
        anim.observe_steps(&[state((0, 0), Some((58, 0)), 1, 12)]);
        // A different target — the next step of a straight walk or a turn.
        assert_eq!(
            anim.observe_steps(&[state((58, 0), Some((58, -32)), 1, 12)]),
            [0].into_iter().collect::<HashSet<_>>()
        );
        // Coming to rest is a step change too: the rest clip opens on its base.
        assert_eq!(
            anim.observe_steps(&[state((58, 0), None, 0, 0)]),
            [0].into_iter().collect::<HashSet<_>>()
        );
        assert!(anim.observe_steps(&[state((58, 0), None, 0, 0)]).is_empty());
    }

    /// The measurement behind the display model (docs/k1/movement.md, "Step
    /// duration"): a walk sequence carries the cell travel in its own frame
    /// offsets — from the first frame to the last it slides to within ~10 px of
    /// the step vector in every direction. The node must not interpolate along
    /// that vector on top, or the travel is counted twice (data test, skips
    /// without `original/`).
    #[test]
    fn walk_frames_carry_the_step() {
        // Pixel step per direction index, mirroring kn_game::movement::grid.
        let step: [(i32, i32); 8] = [
            (-58, 0),
            (-29, -16),
            (0, -32),
            (29, -16),
            (58, 0),
            (29, 16),
            (0, 32),
            (-29, 16),
        ];
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../original/k1");
        for edition in ["rus", "eng"] {
            let dir = root.join(edition).join("game");
            let Ok(game) = Game::open(&dir) else {
                continue;
            };
            let map_ref = *game.maps().unwrap().first().unwrap();
            let animations = game.creature_animations(map_ref).unwrap();
            assert!(!animations.is_empty(), "{edition}: no creatures");
            let mut checked = 0;
            for (_, anim) in &animations {
                for (direction, walk) in anim.walk.iter().enumerate() {
                    let Some(clips) = walk else { continue };
                    let Some(body) = clips
                        .layers
                        .iter()
                        .find(|layer| layer.key == LayerKey::Body)
                        .and_then(|layer| layer.animation.as_ref())
                    else {
                        continue;
                    };
                    let (Some(first), Some(last)) = (body.frames.first(), body.frames.last())
                    else {
                        continue;
                    };
                    let drift = (last.dx - first.dx, last.dy - first.dy);
                    let (want_x, want_y) = step[direction];
                    assert!(
                        (drift.0 - want_x).abs() <= 12 && (drift.1 - want_y).abs() <= 12,
                        "{edition} dir={direction} walk frames drift {drift:?}, expected ≈ \
                         ({want_x},{want_y}) — the travel is not in the frames"
                    );
                    checked += 1;
                }
            }
            assert!(checked > 0, "{edition}: no walk sequences to check");
        }
    }

    fn clip_of(base: usize) -> Clip {
        Clip {
            sequence: Arc::new(Sequence::new(Vec::new())),
            base,
            corner: (0, 0),
        }
    }

    /// The resting clip must be anchored exactly where the scene draws the
    /// piece, or a creature would be born displaced and only the swaps would
    /// read correct. Both come from one pose build, so they must agree on
    /// every real creature (data test, skips without `original/`).
    #[test]
    fn the_rest_clips_are_anchored_where_the_scene_draws_the_layer() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();
        let scene = game.scene(map_ref, &SceneOptions::ALL).unwrap();
        let mut checked = 0;
        for (slot, anim) in game.creature_animations(map_ref).unwrap() {
            let pieces: Vec<&Piece> = scene
                .pieces
                .iter()
                .filter(|piece| {
                    piece.creature_id == Some(slot) && piece.group != PieceGroup::CharacterShadows
                })
                .collect();
            assert_eq!(
                pieces.len(),
                anim.rest.layers.len(),
                "creature {slot}: the scene and the resting clips list the same layers"
            );
            for (piece, layer) in pieces.iter().zip(&anim.rest.layers) {
                assert_eq!(
                    (piece.x, piece.y),
                    layer.corner,
                    "creature {slot} layer {:?}: the resting clip anchors on the scene piece",
                    layer.key
                );
                checked += 1;
            }
        }
        assert!(checked > 0, "the map has creatures to check");
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
            special: None,
        };
        select_clip(&layer, walking, direction, false)
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
            special: None,
        };
        let chosen = select_clip(&layer, true, 7, false).unwrap();
        assert_eq!(chosen.base, 0);
    }

    #[test]
    fn an_out_of_range_direction_falls_back_to_rest() {
        let layer = LayerSequences {
            rest: Some(clip_of(0)),
            walk: Vec::new(),
            special: None,
        };
        let chosen = select_clip(&layer, true, 200, false).unwrap();
        assert_eq!(chosen.base, 0, "направление вне таблицы не роняет выбор");
    }

    /// The state machine's Special stage picks the state-4 clip even while the
    /// simulation reports the creature as walking (issue #70 AC2/AC5).
    #[test]
    fn in_special_picks_the_state_four_clip_winning_over_walk() {
        let layer = LayerSequences {
            rest: Some(clip_of(0)),
            walk: (0..8).map(|_| Some(clip_of(1))).collect(),
            special: Some(clip_of(2)),
        };
        let chosen = select_clip(&layer, true, 3, true).unwrap();
        assert_eq!(chosen.base, 2, "the special wins over walk");
    }

    /// A creature the state machine has put in Idle returns to the resting or
    /// walk clip on the next frame (issue #70 AC2: the transition into idle is
    /// continuous — the machine hands back the pair without a hold).
    #[test]
    fn after_special_idle_picks_the_rest_clip() {
        let layer = LayerSequences {
            rest: Some(clip_of(0)),
            walk: (0..8).map(|_| Some(clip_of(1))).collect(),
            special: Some(clip_of(2)),
        };
        let chosen = select_clip(&layer, false, 3, false).unwrap();
        assert_eq!(
            chosen.base, 0,
            "the idle pose after the special is the rest clip"
        );
    }

    /// A missing special clip falls back to rest so a state-4 creature whose
    /// sequence did not decode still draws something (never a blank).
    #[test]
    fn missing_special_falls_back_to_rest() {
        let layer = LayerSequences {
            rest: Some(clip_of(0)),
            walk: Vec::new(),
            special: None,
        };
        let chosen = select_clip(&layer, false, 3, true).unwrap();
        assert_eq!(chosen.base, 0);
    }

    /// AC4 at the viewer's rebuild-gate: the stage snapshot reports a pose
    /// rebuild only when a creature's animation-state kind flips. `Play →
    /// Paused → Play` hands the same stage back (the clock stopped, so no tick
    /// moved the machine) and must not rebuild; `Play → Off → Play` recreates
    /// the machine, which reseeds a finished special back to Special, and that
    /// re-entry must be reported so the clip is swapped back (issue #70).
    #[test]
    fn the_stage_snapshot_rebuilds_only_on_a_state_kind_flip() {
        let mut anim = CreatureAnim::default();
        // World load: the state-4 creature enters Special — reported once.
        assert_eq!(
            anim.observe_stages(&[(7, true)]),
            [7].into_iter().collect::<HashSet<_>>(),
            "entering the special is a rebuild"
        );
        // Play → Paused → Play: the clock did not advance, the caller feeds the
        // same stage again; a resume must not pay a second rebuild.
        assert!(
            anim.observe_stages(&[(7, true)]).is_empty(),
            "resuming an unchanged special is not a change"
        );
        // The sequence finishes into idle — reported once, then quiet.
        assert_eq!(
            anim.observe_stages(&[(7, false)]),
            [7].into_iter().collect::<HashSet<_>>(),
            "the Special → Idle edge is one rebuild"
        );
        assert!(
            anim.observe_stages(&[(7, false)]).is_empty(),
            "idle does not advance, so it is not a change"
        );
        // Play → Off → Play: `reset_animation` recreates the simulation, which
        // reseeds the creature to `Special { frame: 0 }`; the first observation
        // after the reset is a change so the special clip is chosen again.
        assert_eq!(
            anim.observe_stages(&[(7, true)]),
            [7].into_iter().collect::<HashSet<_>>(),
            "the replay after Off re-enters the special and rebuilds once"
        );
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
        let map = build_depth_map(&scene, &TextureCache::default());
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
    fn reset_returns_creatures_home_and_resyncs_the_epoch() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();

        let mut sim = CreatureSim::default();
        sim.sim = game.simulation(map_ref, SIM_SEED).ok();
        sim.initial_pixels = sim.sim.as_ref().unwrap().initial_pixels();
        let root = Entity::PLACEHOLDER;
        let labels = CreatureLabels {
            label_root: Some(root),
            labels_spawned: true,
        };
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

        // The old epoch lags behind the clock: a reset must pin it to the current
        // tick, otherwise the fresh simulation replays the backlog in one jump
        reset_creature_sim(&game, map_ref, SIM_SEED, 517, &mut sim);
        assert_eq!(sim.epoch, 517, "the epoch is pinned to the clock");
        assert_eq!(
            sim.sim_ticks, 0,
            "a reset simulation starts its own timeline at zero"
        );
        assert_eq!(
            home(sim.sim.as_ref().unwrap()),
            initial,
            "существа вернулись домой"
        );
        assert!(labels.labels_spawned, "метки не пересоздаются");
        assert_eq!(labels.label_root, Some(root), "корень меток сохранён");

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

        let mut sim = CreatureSim::default();
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
            .add_observer(reset_positions)
            .add_systems(Update, fire_reset);
        app.world_mut().spawn(ResetPositions);

        app.update();
        app.update();

        let sim = app.world().resource::<CreatureSim>();
        assert_eq!(
            home(sim.sim.as_ref().unwrap()),
            initial,
            "кнопка вернула существ домой"
        );
        assert_eq!(
            sim.epoch, 300,
            "the epoch is pinned to the clock without a jump"
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
    fn rewinding_the_clock_replays_to_the_positions_a_forward_run_reached() {
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

        // The subsystem ran ahead to 200; the clock steps back to 150. There is no
        // reverse tick, so it rebuilds and replays — and must land on what the
        // forward run showed at 150, which is what a rewinded frame shows.
        let sim = CreatureSim {
            sim: game.simulation(map_ref, SIM_SEED).ok(),
            ..Default::default()
        };
        let motion = CreatureMotion {
            map: Some(map_ref),
            seed: SIM_SEED,
            ..Default::default()
        };
        let mut clock = Clock::new(TICK_MS as f32);
        clock.ticks = 200;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(GameResource(game))
            .insert_resource(clock)
            .insert_resource(motion)
            .insert_resource(sim)
            .add_systems(Update, advance_creature_sim);
        app.update();
        *app.world_mut().resource_mut::<Clock>() = Clock::new(TICK_MS as f32);
        app.world_mut().resource_mut::<Clock>().ticks = 150;
        app.update();

        let sim = app.world().resource::<CreatureSim>();
        assert_eq!(
            sim.sim_ticks, 150,
            "the simulation sits on the selected tick"
        );
        let got: Vec<(usize, (i32, i32))> = sim
            .sim
            .as_ref()
            .unwrap()
            .states()
            .iter()
            .map(|s| (s.id, s.pixel))
            .collect();
        assert_eq!(
            got, expected,
            "перемотка к тику 150 совпадает с прямым прогоном"
        );
    }

    /// After a map change the epoch is pinned to the clock, so the fresh
    /// simulation does not replay the ticks the previous map had already run — the
    /// catch-up burst that used to fast-forward every creature in one frame
    /// (review #68). A forward run from a behind epoch still advances normally.
    #[test]
    fn map_change_pins_the_epoch_so_history_is_not_replayed() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();
        let sim = game.simulation(map_ref, SIM_SEED).ok();
        let initial = sim.as_ref().unwrap().initial_pixels();
        let sim_resource = CreatureSim {
            sim,
            initial_pixels: initial.clone(),
            // The clock has been running for 900 ticks when this map is opened:
            // the epoch is pinned there, so the fresh simulation stays at tick 0.
            epoch: 900,
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
            .init_resource::<CreatureMotion>()
            .add_systems(Update, advance_creature_sim);
        app.world_mut().resource_mut::<Clock>().ticks = 900;
        app.update();

        let states = app
            .world()
            .resource::<CreatureSim>()
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

    /// A backward `Step` rewinds the clock below the simulation's own timeline;
    /// `advance_creature_sim` sees `target < sim_ticks`, replays the map from tick 0 to
    /// the rewound target, and pins `sim_ticks` there — so creatures show the same
    /// positions a forward run would. Pinning it back to 0 would let the catch-up loop
    /// tick the fresh simulation a second `target` times and overshoot (review #68).
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
        // A forward run reached tick 200; a backward Step rewinds the clock to 150.
        let sim_resource = CreatureSim {
            sim: game.simulation(map_ref, SIM_SEED).ok(),
            sim_ticks: 200,
            ..Default::default()
        };
        let motion = CreatureMotion {
            map: Some(map_ref),
            seed: SIM_SEED,
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
            .insert_resource(motion)
            .insert_resource(Clock::new(TICK_MS as f32))
            .add_systems(Update, advance_creature_sim);
        app.world_mut().resource_mut::<Clock>().ticks = 150;
        app.update();

        let got: Vec<(usize, (i32, i32))> = app
            .world()
            .resource::<CreatureSim>()
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

    /// A reset pins the simulation's epoch to the clock; a backward `Step` just after
    /// it rewinds the clock *below* that epoch. Because time is simulation-relative,
    /// the epoch-relative target saturates to 0 and the fresh simulation stays at its
    /// initial positions — it does not replay the clock to absolute tick time, which
    /// is what the old absolute-clock tracker did (review #68).
    #[test]
    fn a_backward_step_after_a_reset_rewinds_to_the_reset_not_the_absolute_clock() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();
        let mut sim_resource = CreatureSim::default();
        sim_resource.sim = game.simulation(map_ref, SIM_SEED).ok();
        sim_resource.initial_pixels = sim_resource.sim.as_ref().unwrap().initial_pixels();
        // A reset at clock tick 400 left a fresh simulation at its own tick 0.
        sim_resource.epoch = 400;
        let initial = sim_resource.initial_pixels.clone();
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
            .init_resource::<CreatureMotion>()
            .add_systems(Update, advance_creature_sim);
        // A backward Step rewinds the clock below the reset epoch.
        app.world_mut().resource_mut::<Clock>().ticks = 395;
        app.update();

        let states = app
            .world()
            .resource::<CreatureSim>()
            .sim
            .as_ref()
            .unwrap()
            .states();
        assert!(!states.is_empty(), "the map must have creatures");
        for state in &states {
            assert_eq!(
                state.pixel, by_id[&state.id],
                "a Step back after a reset rewinds to the reset, not to absolute clock tick 395"
            );
        }
    }

    /// Turning the Characters overlay off hides the creature nodes and their `#ID`
    /// labels together. They hang off their own [`CreatureRoot`] / [`LabelRoot`],
    /// outside the Characters scene group, so without this they would keep sprites and
    /// texts floating over an empty map. A shown overlay leaves them `Inherited` so a
    /// map hidden on `MapRoot` stays hidden; a plain [`CreatureNode`] is never touched
    /// (review #68).
    #[test]
    fn the_characters_overlay_hides_the_creature_and_label_roots_together() {
        for (characters_on, expected) in
            [(true, Visibility::Inherited), (false, Visibility::Hidden)]
        {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .insert_resource(viewer([false, false, false, characters_on]))
                .add_systems(Update, sync_creature_visibility);
            let creature_root = app
                .world_mut()
                .spawn((CreatureRoot, Visibility::Inherited))
                .id();
            let label_root = app
                .world_mut()
                .spawn((LabelRoot, Visibility::Inherited))
                .id();
            let node = app
                .world_mut()
                .spawn((CreatureNode { id: 1, bottom: 0 }, Visibility::Inherited))
                .id();
            app.update();
            assert_eq!(
                *app.world().get::<Visibility>(creature_root).unwrap(),
                expected,
                "creature root follows the overlay (on: {characters_on})"
            );
            assert_eq!(
                *app.world().get::<Visibility>(label_root).unwrap(),
                expected,
                "label root follows the overlay (on: {characters_on})"
            );
            assert_eq!(
                *app.world().get::<Visibility>(node).unwrap(),
                Visibility::Inherited,
                "a node is hidden only through its root, never pinned directly"
            );
        }
    }

    /// A simulation that cannot be built reports why instead of being dropped through
    /// `.ok()`: `reset_creature_sim` records the reason on the resource and leaves the
    /// simulation empty, while still pinning the epoch so a later backward Step has a
    /// zero point (review #68).
    #[test]
    fn a_simulation_that_fails_to_build_records_its_reason() {
        let Some((dir, _)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();
        // A map number that has no files on disk: the simulation cannot be built.
        let bogus = MapRef {
            source: MapSource::Game,
            number: u32::MAX,
        };
        let mut sim = CreatureSim {
            epoch: 45,
            ..Default::default()
        };
        reset_creature_sim(&game, bogus, SIM_SEED, 123, &mut sim);
        assert!(sim.sim.is_none(), "no simulation was built");
        let reason = sim
            .error
            .expect("the build failure is surfaced, not swallowed by .ok()");
        assert!(
            reason.contains("simulation for"),
            "the reason names the map that failed: {reason}"
        );
        assert_eq!(sim.epoch, 123, "the epoch is pinned even on failure");
        assert_eq!(sim.sim_ticks, 0, "the timeline restarts at the reset");
    }

    /// Rebuilding a map hands its creatures to the shared subsystem and mirrors
    /// the Characters overlay into the subsystem's visibility resource: the map a
    /// rebuild asks for is the selected one, and a map opened with characters off
    /// shows no creature for the first frame (issue #70 AC5, review #68).
    #[test]
    fn rebuild_asks_the_subsystem_and_mirrors_the_characters_overlay() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        for (characters_on, expected) in [(true, true), (false, false)] {
            let state = Viewer {
                maps: vec![map_ref],
                selected: 0,
                overlays: [false, false, false, characters_on],
                playback: PlayState::Paused,
            };
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .insert_resource(GameResource(Game::open(&dir).unwrap()))
                .insert_resource(state)
                .insert_resource(Rebuild { map: true })
                .init_resource::<TextureCache>()
                .insert_resource(Assets::<Image>::default())
                .init_resource::<ActiveRoot>()
                .init_resource::<Stats>()
                .init_resource::<PendingFrame>()
                .init_resource::<CreatureLabels>()
                .init_resource::<CreatureMotion>()
                // Opposite of what the overlay says: a rebuild must overwrite it.
                .insert_resource(CreatureNodesVisible(!characters_on))
                .add_systems(Update, rebuild_scene);
            app.update();

            let motion = app.world().resource::<CreatureMotion>();
            assert_eq!(motion.map, Some(map_ref), "the selected map is requested");
            assert!(motion.rebuild, "the subsystem is asked to build it");
            assert_eq!(
                app.world().resource::<CreatureNodesVisible>().0,
                expected,
                "creatures are born visible exactly when the overlay is on"
            );
        }
    }

    /// The panel's own wording of what the subsystem found: every finding shows,
    /// a clean map adds nothing, and a map whose animation data did not decode
    /// says so (review #68 — a creature walking on its rest pose is a defect).
    #[test]
    fn the_panel_words_the_creature_findings() {
        assert_eq!(creature_report(&CreatureAnimReport::default()), "");
        let report = CreatureAnimReport {
            total: 5,
            without_walk: 2,
            partial_walk: 1,
            ..Default::default()
        };
        let shown = creature_report(&report);
        for want in ["5 creatures", "2 without walk", "1 partial walk"] {
            assert!(shown.contains(want), "{shown} must mention {want:?}");
        }
        let broken = CreatureAnimReport {
            error: Some("no animation data".to_owned()),
            ..Default::default()
        };
        assert_eq!(
            creature_report(&broken),
            " · creatures: no animation data",
            "a map that did not decode says why instead of showing fewer creatures"
        );
    }

    /// A map built with the Characters overlay off must show neither creatures
    /// nor labels for a frame: both roots are born hidden, not fixed-visible
    /// for the sync system to catch up a frame late (review #68). Switching
    /// the overlay on afterwards shows them through the usual sync.
    #[test]
    fn roots_are_born_hidden_when_characters_are_off() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();
        let sim_resource = CreatureSim {
            sim: game.simulation(map_ref, SIM_SEED).ok(),
            ..Default::default()
        };
        assert!(sim_resource.sim.is_some(), "the map must give a simulation");

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(sim_resource)
            .init_resource::<CreatureSpawns>()
            .init_resource::<CreatureDepth>()
            .init_resource::<CreatureLabels>()
            // What `rebuild_scene` derives from the Characters overlay.
            .insert_resource(CreatureNodesVisible(false))
            .insert_resource(viewer([false, false, false, false]))
            .add_systems(
                Update,
                (
                    spawn_creature_labels,
                    spawn_creature_nodes,
                    sync_creature_visibility,
                )
                    .chain(),
            );
        app.update();

        let creature_root = app
            .world()
            .resource::<CreatureSim>()
            .creature_root
            .expect("the creature root spawned");
        let label_root = app
            .world()
            .resource::<CreatureLabels>()
            .label_root
            .expect("the label root spawned");
        assert_eq!(
            *app.world().get::<Visibility>(creature_root).unwrap(),
            Visibility::Hidden,
            "nodes born visible would flash for a frame over a characters-off map"
        );
        assert_eq!(
            *app.world().get::<Visibility>(label_root).unwrap(),
            Visibility::Hidden,
            "labels born visible would leak ids over a characters-off map"
        );

        app.world_mut()
            .resource_mut::<Viewer>()
            .set(Overlay::Characters, true);
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(creature_root).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(
            *app.world().get::<Visibility>(label_root).unwrap(),
            Visibility::Inherited
        );
    }

    /// `sync_creature_nodes` reads two `&mut Transform` queries in one system.
    /// Bevy panics (`B0001`) unless they are proven disjoint, and that panic used
    /// to crash the running viewer the first frame the system ran — the movement
    /// bug review #68 surfaced. Building the queries trips the conflict at param
    /// init, so a plain `update` locks the `Without<CreatureNode>` guard.
    #[test]
    fn creature_node_and_label_queries_are_disjoint() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<CreatureSim>()
            .init_resource::<CreatureDepth>()
            .init_resource::<CreatureSpawns>()
            .insert_resource(Clock::new(TICK_MS as f32))
            .add_systems(Update, sync_creature_nodes);
        app.world_mut()
            .spawn((CreatureNode { id: 1, bottom: 0 }, Transform::default()));
        app.world_mut()
            .spawn((CreatureLabel { creature_id: 1 }, Transform::default()));
        app.update();
    }

    /// What one rendered frame looked like for one creature: where its body and
    /// shadow's top-left corner ended up in map pixels, and the simulation state
    /// behind them.
    ///
    /// Corners, not sprite centers: Bevy centers a sprite on `corner + size/2`,
    /// and walk frames have different bounding boxes, so a center-to-center
    /// distance misreads a legitimate size change as (missing) travel. The
    /// original positions its sprites by corner (`compose` adds the frame's
    /// `image_offset` to the cell pixel), so that is the space where the
    /// step-travel assertion means what it says.
    #[derive(Debug)]
    struct VisualSample {
        body: (f32, f32),
        shadow: Option<(f32, f32)>,
        from: (i32, i32),
        to: Option<(i32, i32)>,
        elapsed: u32,
        ticks: u32,
        walking: bool,
    }

    /// End-to-end visual continuity on real data: the whole pipeline runs —
    /// simulation, clip swap, [`apply_animators`], node sync — and the body sprite's
    /// world position is read through the real transform hierarchy. A walking
    /// creature must never jump back further than the gait (the double travel of
    /// node interpolation stacked on walk-frame offsets made the image leap a cell
    /// back every 12-frame cycle), a full step must land it on the next cell, and
    /// the shadow must move with the body.
    #[test]
    fn a_walking_creature_image_never_jumps_back() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();
        let mut cache = TextureCache::default();
        let mut images = Assets::<Image>::default();
        let (anim, report) = build_creature_anim(&game, map_ref, &mut cache, &mut images);
        println!("walk diagnostics: {report:?}");
        let scene = game.scene(map_ref, &SceneOptions::ALL).unwrap();
        let mut sim_resource = CreatureSim::default();
        sim_resource.sim = game.simulation(map_ref, SIM_SEED).ok();
        sim_resource.initial_pixels = sim_resource.sim.as_ref().unwrap().initial_pixels();
        let spawns = build_creature_spawns(&scene, &cache, &anim, &sim_resource.initial_pixels);
        assert!(
            !spawns.plans.is_empty(),
            "карта обязана дать существ со сценой"
        );
        let depth = build_depth_map(&scene, &cache);

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::transform::TransformPlugin,
            AnimPlugin::new(TICK_MS as f32),
            // The very subsystem the viewer window runs — the test is about it
            // keeping the image continuous, not about a hand-assembled copy of it.
            CreatureAnimationPlugin,
        ))
        .insert_resource(GameResource(game))
        .insert_resource(Viewer {
            maps: vec![map_ref],
            selected: 0,
            overlays: DEFAULT_OVERLAYS,
            playback: PlayState::Paused,
        })
        .insert_resource(images)
        // The subsystem's prepare system validates against these even when no
        // rebuild is pending, exactly as a window that owns them provides them.
        .init_resource::<TextureCache>()
        .insert_resource(sim_resource)
        .insert_resource(CreatureDepth { map: depth })
        .insert_resource(CreatureMotion {
            map: Some(map_ref),
            seed: SIM_SEED,
            ..Default::default()
        })
        .insert_resource(anim)
        .insert_resource(spawns);
        // The clock moves only through this test: real-time advance would tick
        // the animation independently of the simulation stepping below.
        app.world_mut()
            .resource_mut::<Clock>()
            .goto(PlayState::Paused);

        let mut history: HashMap<usize, Vec<VisualSample>> = HashMap::new();
        let mut sprite_query = app
            .world_mut()
            .query::<(&CreatureSprite, &GlobalTransform, &Sprite)>();
        for tick in 0..600u64 {
            app.world_mut().resource_mut::<Clock>().ticks = tick;
            app.update();
            let world = app.world();
            let states: HashMap<usize, CreatureState> = world
                .resource::<CreatureSim>()
                .sim
                .as_ref()
                .unwrap()
                .states()
                .into_iter()
                .map(|state| (state.id, state))
                .collect();
            let sprites: Vec<(usize, usize, bool, f32, f32)> = sprite_query
                .iter(world)
                .map(|(tag, global, sprite)| {
                    // From the Bevy center back to the map-pixel corner the
                    // original positions sprites by.
                    let center = global.translation();
                    let size = sprite.custom_size.unwrap_or_default();
                    (
                        tag.creature_id,
                        tag.layer,
                        tag.is_shadow,
                        center.x - size.x / 2.0,
                        -center.y - size.y / 2.0,
                    )
                })
                .collect();
            for (creature_id, layer, is_shadow, x, y) in &sprites {
                if *is_shadow || *layer != 0 {
                    continue;
                }
                let Some(state) = states.get(creature_id) else {
                    continue;
                };
                let shadow = sprites
                    .iter()
                    .find(|(other, _, tagged_shadow, _, _)| {
                        *other == *creature_id && *tagged_shadow
                    })
                    .map(|&(_, _, _, sx, sy)| (sx, sy));
                history.entry(*creature_id).or_default().push(VisualSample {
                    body: (*x, *y),
                    shadow,
                    from: state.pixel,
                    to: state.step_to_pixel,
                    elapsed: state.step_elapsed,
                    ticks: state.step_ticks,
                    walking: state.walk_state == WalkState::Walk,
                });
            }
        }

        let mut walking_samples = 0usize;
        let mut judged_steps = 0usize;
        let mut tick_distribution: std::collections::BTreeMap<u32, usize> =
            std::collections::BTreeMap::new();
        for (id, samples) in &history {
            let mut index = 0;
            while index < samples.len() {
                if !samples[index].walking {
                    index += 1;
                    continue;
                }
                let start = index;
                while index < samples.len() && samples[index].walking {
                    index += 1;
                }
                let segment = &samples[start..index];
                if segment.len() < 2 {
                    continue; // a lone sample says nothing about boundaries
                }
                walking_samples += segment.len();
                for sample in segment {
                    *tick_distribution.entry(sample.ticks).or_default() += 1;
                }
                for window in segment.windows(2) {
                    let (first, second) = (&window[0], &window[1]);
                    let jump = ((second.body.0 - first.body.0).powi(2)
                        + (second.body.1 - first.body.1).powi(2))
                    .sqrt();
                    assert!(
                        jump <= 30.0,
                        "creature #{id} image jumped {jump:.1} px in one tick: \
                         {first:?} -> {second:?}"
                    );
                    if let (Some(shadow_before), Some(shadow_after)) = (first.shadow, second.shadow)
                    {
                        let jump = ((shadow_after.0 - shadow_before.0).powi(2)
                            + (shadow_after.1 - shadow_before.1).powi(2))
                        .sqrt();
                        assert!(
                            jump <= 30.0,
                            "creature #{id} shadow jumped {jump:.1} px in one tick: \
                             {first:?} -> {second:?}"
                        );
                    }
                }
                // Each complete step must land the image on the next cell: the
                // frames carry the travel, so last minus first ≈ the step vector
                // (walk frames stop 1-11 px short of the cell, docs/k1/movement.md).
                let mut step_start = 0;
                for position in 1..=segment.len() {
                    let ends_step = position == segment.len()
                        || segment[position].from != segment[step_start].from
                        || segment[position].to != segment[step_start].to;
                    if !ends_step {
                        continue;
                    }
                    let first = &segment[step_start];
                    let last = &segment[position - 1];
                    // Only a step seen from its first tick to its last can be
                    // judged on total travel; a partial view measures nothing.
                    // Short-cycle gaits (1-2 tick steps, `movement::pace`) carry
                    // no glide in their frames at all — the image snaps cell to
                    // cell there exactly as in the original — so travel is judged
                    // only for gaits long enough to have it.
                    if let Some(to) = first
                        .to
                        .filter(|_| first.elapsed == 1 && last.elapsed == last.ticks)
                        .filter(|_| first.ticks >= 6)
                    {
                        judged_steps += 1;
                        // Corners are already map space: a complete step must
                        // land the image corner on the next cell.
                        let travelled = (last.body.0 - first.body.0, last.body.1 - first.body.1);
                        let wanted = ((to.0 - first.from.0) as f32, (to.1 - first.from.1) as f32);
                        assert!(
                            (travelled.0 - wanted.0).abs() <= 20.0
                                && (travelled.1 - wanted.1).abs() <= 20.0,
                            "creature #{id} step {:?}->{:?} travelled {travelled:?}, \
                             expected ≈ {wanted:?}\nsegment: {segment:#?} \
                             [{step_start}..{position}]",
                            first.from,
                            to,
                        );
                        // The shadow rides the same step: judged over the whole
                        // step, not per tick — body and shadow are different
                        // sprites whose frame bounding boxes change differently,
                        // so per-tick corner deltas legitimately diverge while
                        // the step's total drift must not.
                        if let (Some(shadow_first), Some(shadow_last)) = (first.shadow, last.shadow)
                        {
                            let shadow_travelled = (
                                shadow_last.0 - shadow_first.0,
                                shadow_last.1 - shadow_first.1,
                            );
                            assert!(
                                (shadow_travelled.0 - travelled.0).abs() <= 8.0
                                    && (shadow_travelled.1 - travelled.1).abs() <= 8.0,
                                "creature #{id} shadow travelled {shadow_travelled:?} \
                                 while the body travelled {travelled:?} on step \
                                 {:?}->{:?}",
                                first.from,
                                to,
                            );
                        }
                    }
                    step_start = position;
                }
            }
        }
        assert!(
            walking_samples >= 60,
            "no creature walked a step's length in 600 ticks: {walking_samples} samples"
        );
        // Anti-vacuous-green: the travel assertions above only run on complete
        // steps (elapsed 1..ticks); a counter that never completed would leave
        // the test silently unchecked.
        assert!(
            judged_steps >= 5,
            "only {judged_steps} complete steps judged - the travel assertions \
             barely ran; step tick distribution: {tick_distribution:?}"
        );
        println!(
            "continuity: judged {judged_steps} complete steps, {walking_samples} \
             walking samples, step tick distribution {tick_distribution:?}"
        );
    }

    /// A reset through the real button must leave nothing half-stepped: every
    /// node on its home cell pixel, every body animator back on the resting
    /// sequence, every shadow holding the shadow band around its node, every
    /// label above its body — all without the static scene being rebuilt (the
    /// roots and sprites are the same entities, review #68).
    #[test]
    fn a_reset_returns_nodes_animators_shadows_and_labels_home() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();
        let mut cache = TextureCache::default();
        let mut images = Assets::<Image>::default();
        let (anim, _report) = build_creature_anim(&game, map_ref, &mut cache, &mut images);
        let scene = game.scene(map_ref, &SceneOptions::ALL).unwrap();
        let simulation = game.simulation(map_ref, SIM_SEED).ok();
        let initial = home(simulation.as_ref().unwrap());
        let initial_by_id: HashMap<usize, (i32, i32)> = initial.iter().copied().collect();
        let sim_resource = CreatureSim {
            initial_pixels: simulation.as_ref().unwrap().initial_pixels(),
            sim: simulation,
            ..Default::default()
        };
        let spawns = build_creature_spawns(&scene, &cache, &anim, &sim_resource.initial_pixels);
        assert!(!spawns.plans.is_empty(), "the map must give creatures");
        let plan_count = spawns.plans.len();
        let plan_ids: HashSet<usize> = spawns.plans.iter().map(|plan| plan.id as usize).collect();
        let depth = build_depth_map(&scene, &cache);
        let depth_map = depth.clone();

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::transform::TransformPlugin,
            AnimPlugin::new(TICK_MS as f32),
            // The pipeline the button must reset is the shared subsystem's.
            CreatureAnimationPlugin,
        ))
        .insert_resource(GameResource(game))
        .insert_resource(Viewer {
            maps: vec![map_ref],
            selected: 0,
            overlays: DEFAULT_OVERLAYS,
            playback: PlayState::Paused,
        })
        .insert_resource(images)
        .init_resource::<TextureCache>()
        .insert_resource(sim_resource)
        .insert_resource(CreatureDepth { map: depth })
        .insert_resource(CreatureMotion {
            map: Some(map_ref),
            seed: SIM_SEED,
            ..Default::default()
        })
        .insert_resource(anim)
        .insert_resource(spawns)
        .init_resource::<CreatureLabels>()
        .add_systems(
            Update,
            (
                spawn_creature_labels.after(CreatureSet::Spawn),
                // The viewer's own pass over the finished frame, exactly as the
                // window runs it: ids above the body images.
                sync_creature_labels
                    .after(CreatureSet::Swap)
                    .after(AnimSet::Sprites)
                    .after(CreatureSet::Nodes),
            ),
        )
        .add_observer(reset_positions);
        let button = app.world_mut().spawn(ResetPositions).id();
        app.world_mut()
            .resource_mut::<Clock>()
            .goto(PlayState::Paused);

        // Walk somebody off its cell first: resetting a standing world proves
        // nothing about half-finished steps.
        let mut left_home = false;
        for tick in 0..200u64 {
            app.world_mut().resource_mut::<Clock>().ticks = tick;
            app.update();
            let sim = app.world().resource::<CreatureSim>();
            let states = sim.sim.as_ref().unwrap().states();
            left_home |= states
                .iter()
                .any(|state| initial_by_id.get(&state.id) != Some(&state.pixel));
        }
        assert!(left_home, "200 ticks must move at least one creature");
        let (root_before, label_root_before) = {
            let sim = app.world().resource::<CreatureSim>();
            let labels = app.world().resource::<CreatureLabels>();
            (sim.creature_root, labels.label_root)
        };

        // The button, pressed once: the observer rebuilds the simulation and a
        // following frame re-reads it. The clock stands still across the press,
        // so the fresh simulation is judged at its own tick 0.
        app.world_mut().resource_mut::<Clock>().ticks = 200;
        app.world_mut().trigger(Activate { entity: button });
        app.update();

        let mut nodes = app.world_mut().query::<(&CreatureNode, &Transform)>();
        let mut shadows = app
            .world_mut()
            .query::<(&CreatureShadow, &ChildOf, &Transform)>();
        let mut sprites = app.world_mut().query::<(&CreatureSprite, &Animator)>();
        let mut labels = app.world_mut().query::<(&CreatureLabel, &Transform)>();
        let mut bodies = app
            .world_mut()
            .query::<(&CreatureSprite, &GlobalTransform)>();
        let world = app.world();
        let sim = world.resource::<CreatureSim>();
        assert_eq!(
            sim.creature_root, root_before,
            "the nodes are not respawned"
        );
        assert_eq!(
            world.resource::<CreatureLabels>().label_root,
            label_root_before,
            "the labels are not respawned"
        );
        let states = sim.sim.as_ref().unwrap().states();
        assert!(!states.is_empty());
        for state in &states {
            assert!(!state.moving, "creature #{} still walking", state.id);
            assert_eq!(
                initial_by_id[&state.id], state.pixel,
                "creature #{} not home",
                state.id
            );
        }

        // Every node sits on its initial pixel: live creatures on the home cell
        // of the fresh simulation, and creatures the world refused to place on
        // their record cell — the documented fallback, which a reset must not
        // disturb either. A state without a node (no scene pieces) and a node
        // without a state (unplaced) are both legitimate, so the count is
        // judged against the plans, not against `states`.
        let pixels: HashMap<usize, (i32, i32)> = sim.initial_pixels.iter().copied().collect();
        let node_ids: Vec<usize> = nodes
            .iter(world)
            .map(|(node, _)| node.id as usize)
            .collect();
        assert_eq!(
            node_ids.len(),
            plan_count,
            "the reset neither spawns nor drops creature nodes"
        );
        for (node, transform) in nodes.iter(world) {
            let id = node.id as usize;
            let Some(&(x, y)) = pixels.get(&id) else {
                continue;
            };
            assert_eq!(
                (transform.translation.x, transform.translation.y),
                (x as f32, -(y as f32)),
                "node #{id} off its initial pixel after the reset",
            );
        }
        for state in &states {
            if plan_ids.contains(&state.id) {
                assert!(
                    node_ids.contains(&state.id),
                    "live creature #{} lost its node",
                    state.id
                );
            }
        }

        // The shadows ride their slots through the depth recompute a reset
        // triggers: over the statics under their creature, under the body
        // (review #68).
        let mut shadow_count = 0;
        for (_, parent, shadow) in shadows.iter(world) {
            let (Some(node), Some(creature)) = (
                world.get::<Transform>(parent.0),
                world.get::<CreatureNode>(parent.0),
            ) else {
                continue;
            };
            let slot = depth_map.creature_slot(creature.bottom, creature.id);
            let world_z = shadow.translation.z + node.translation.z;
            assert!(
                (world_z - slot.shadow).abs() < 1e-3
                    && slot.base < slot.shadow
                    && slot.shadow < node.translation.z,
                "shadow of a reset creature left its slot: world {world_z}, \
                 node {}, slot base {} shadow {}",
                node.translation.z,
                slot.base,
                slot.shadow
            );
            shadow_count += 1;
        }
        assert!(shadow_count > 0, "the map must give shadows to judge");

        // Every creature stands after the reset, so every body sits on its
        // resting sequence again: a stale walk clip would keep the cycle of a
        // step that no longer exists.
        let creature_anim = world.resource::<CreatureAnim>();
        let mut judged = 0;
        for (tag, animator) in sprites.iter(world) {
            let Some(sequences) = creature_anim.creatures.get(&tag.creature_id) else {
                continue;
            };
            let layer = if tag.is_shadow {
                &sequences.shadow
            } else {
                let Some(layer) = sequences.body.get(&tag.layer) else {
                    continue;
                };
                layer
            };
            let Some(rest) = &layer.rest else { continue };
            assert!(
                Arc::ptr_eq(&animator.sequence, &rest.sequence),
                "creature #{} layer {} (shadow: {}) is on a foreign sequence after the reset",
                tag.creature_id,
                tag.layer,
                tag.is_shadow
            );
            judged += 1;
        }
        assert!(judged > 0, "the map must give animated sprites to judge");

        // Labels ride the body image again, in the same propagated frame the
        // sprite itself draws at.
        let mut label_count = 0;
        for (label, transform) in labels.iter(world) {
            let Some((_, global)) = bodies.iter(world).find(|(tag, _)| {
                tag.creature_id == label.creature_id && !tag.is_shadow && tag.layer == 0
            }) else {
                continue;
            };
            let body = global.translation();
            assert!(
                (transform.translation.x - body.x).abs() < 1e-3
                    && (transform.translation.y - (body.y - LABEL_OFFSET_Y)).abs() < 1e-3,
                "label #{} left its body: label {:?}, body {:?}",
                label.creature_id,
                transform.translation,
                body
            );
            label_count += 1;
        }
        assert!(label_count > 0, "the map must give labels to judge");
    }

    /// The cost of the backward `Step` after a long session: replaying from
    /// seed to an epoch-relative target is the only way back (no reverse tick),
    /// so a 30-minute session's rewind must stay sub-second in the shipped
    /// (release) build — measured 306 ms, 10 µs/tick, on an Apple M-series;
    /// the same test build runs ~4.3 s, hence the debug-sized bound (review
    /// #68; numbers also in docs/rust.md, "Decision: no reverse tick — replay").
    #[test]
    fn replaying_a_long_session_stays_a_bargain() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        const TICKS: u64 = 30_000; // 30 minutes at the 60 ms tick
        let game = Game::open(&dir).unwrap();
        let started = std::time::Instant::now();
        // What a backward Step costs: the subsystem rebuilds the map's simulation
        // and replays it to the selected tick (`advance_creature_sim`).
        let mut replayed = game.simulation(map_ref, SIM_SEED).expect("replay builds");
        for _ in 0..TICKS {
            replayed.tick();
        }
        let elapsed = started.elapsed();
        println!(
            "replay: {TICKS} ticks in {} ms ({} ns/tick)",
            elapsed.as_millis(),
            elapsed.as_nanos() / u128::from(TICKS)
        );
        assert!(!replayed.states().is_empty());
        assert!(
            elapsed.as_millis() < 10_000,
            "a 30 000-tick replay took {elapsed:?} — the backward Step \
             would stutter even in a debug build"
        );
    }

    #[test]
    fn depth_boundaries_stay_monotonic_on_every_map() {
        // The flame on rus map 1 used to invert a boundary: the fire's bottom
        // sits above its object's while its draw z is higher, and the search
        // plus interpolation assume z grows with bottom (review #68). The fix
        // is twofold — flames are not depth contributors, and `build` refuses
        // drops — so every edition map must produce monotonic boundaries.
        let Some((dir, _)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();
        let mut cache = TextureCache::default();
        let mut images = Assets::<Image>::default();
        let mut judged = 0;
        for map_ref in game.maps().unwrap() {
            let scene = game.scene(map_ref, &SceneOptions::ALL).unwrap();
            let ids: Vec<kn_game::TextureId> = {
                let mut seen = std::collections::HashSet::new();
                for piece in &scene.pieces {
                    if let Some(id) = &piece.texture {
                        seen.insert(*id);
                    }
                    if let Some(anim) = &piece.animation {
                        for frame in &anim.frames {
                            if let Some(id) = &frame.texture {
                                seen.insert(*id);
                            }
                        }
                    }
                }
                seen.into_iter().collect()
            };
            let problems = cache.load(&game, &ids, &mut images).unwrap();
            assert!(
                problems.is_empty(),
                "map {} textures: {problems:?}",
                map_ref.number
            );
            let map = build_depth_map(&scene, &cache);
            // Sample every whole bottom the stacks can reach: a drop only
            // needs one pixel to hide in.
            let previous = map.z_for_bottom(0);
            let mut bottom_z = (previous, 0);
            for bottom in 1..4000 {
                let z = map.z_for_bottom(bottom);
                assert!(
                    z >= bottom_z.0,
                    "map {} boundaries drop at bottom {bottom}: {z} after {} \
                     from bottom {}",
                    map_ref.number,
                    bottom_z.0,
                    bottom_z.1
                );
                if z > bottom_z.0 {
                    bottom_z = (z, bottom);
                }
            }
            judged += 1;
        }
        assert!(judged > 0, "the edition must give maps to judge");
    }

    fn fire_reset(mut commands: Commands, buttons: Query<Entity, With<ResetPositions>>) {
        for button in &buttons {
            commands.trigger(Activate { entity: button });
        }
    }
}
