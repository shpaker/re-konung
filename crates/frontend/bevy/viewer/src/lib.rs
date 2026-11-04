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
    CreatureAnimation, CreatureClips, CreatureLayer, Game, MapRef, MapSource, Piece, PieceGroup,
    SceneOptions, TICK_MS, group_is_shown,
};
use kn_input::{ZoomScroll, drag_delta, pan_direction, wheel_steps};
use kn_render::{
    AnimPlugin, AnimSet, Animator, CameraApply, CameraBounds, CameraInput, CameraState, Clock,
    CreatureData, CreatureNode, CreatureRoot, CreatureShadow, DepthMap, GameResource,
    LayerAnimation, LayerData, MapCamera, MapCameraPlugin, PAN_SPEED, PendingFrame, PlayState,
    SceneGroup, ScenePiece, Sequence, TextureCache, Z_STEP, clear_scene, load_scene, ortho_scale,
    piece_sequence, piece_size, spawn_creature, update_creature_depth,
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
    /// Why this map's simulation failed to build, if it did; surfaced in the panel
    /// instead of silently showing a scene with no moving creatures (review #68).
    error: Option<String>,
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

/// Simulation clock, relative to an epoch: the clock tick at which the current
/// simulation's tick 0 sits, and the ticks it has advanced since. A reset or a map
/// change moves the epoch to the clock so the fresh simulation neither replays the
/// ticks the old one already ran nor rewinds to absolute clock time on a backward
/// step (review #68). The simulation's clock position is `epoch + sim_ticks`.
#[derive(Resource, Default)]
struct SimTickTracker {
    epoch: u64,
    sim_ticks: u64,
}

/// Idle and walk [`Sequence`]s per creature for the `WalkState` swap; rebuilt on
/// map change together with [`SimulationResource`].
#[derive(Resource, Default)]
struct CreatureAnim {
    creatures: HashMap<usize, CreatureSequences>,
    /// Last observed `(from, to)` and `step_elapsed` per creature: the first
    /// frame a step is seen on re-anchors its clips, so the walk cycle runs
    /// step-relative like the original's `Frame` counter (Slavik
    /// `game_play.cpp:3727-3740`) instead of free-running against the global clock.
    locked: HashMap<usize, (StepKey, u32)>,
}

/// What identifies the step a creature is on, for [`CreatureAnim::locked`].
type StepKey = ((i32, i32), Option<(i32, i32)>);

impl CreatureAnim {
    /// Creatures whose step is new or restarted since the last observation —
    /// their clips must open on the step's base frame. The lock updates here, so
    /// a step re-anchors exactly once, on its first observation.
    ///
    /// A falling `step_elapsed` under an unchanged key is a restart: the world
    /// can cancel a step whose destination became impassable (`cancel_step`) and
    /// order the same `(from, to)` again, and the key alone cannot tell that
    /// from a step still running — without re-anchoring the cycle would keep
    /// running past the halfway point and wrap a cell behind the creature
    /// (review #68).
    fn observe_steps(&mut self, states: &[CreatureState]) -> HashSet<usize> {
        let mut restep = HashSet::new();
        for state in states {
            let observed = ((state.pixel, state.step_to_pixel), state.step_elapsed);
            let restarted = match self.locked.get(&state.id) {
                Some(&(last_key, last_elapsed)) => {
                    last_key != observed.0 || observed.1 < last_elapsed
                }
                None => true,
            };
            if restarted {
                restep.insert(state.id);
            }
            // Recorded every observation, not only on a re-anchor: the restart
            // test compares against the highest tick the step was last seen on.
            self.locked.insert(state.id, observed);
        }
        restep
    }
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

/// A built sequence and the frame it should show when play restarts. `corner`
/// is the clip's base-frame corner in absolute map pixels: a swap re-anchors
/// the animator on `corner` minus the creature's origin, because the clip's
/// frame offsets are relative to that corner, not to the resting piece's.
struct Clip {
    sequence: Arc<Sequence>,
    base: usize,
    corner: (i32, i32),
}

/// One [`CreatureData`] per creature to spawn, rebuilt with the scene. Kept as a
/// resource so the spawn system runs it once per map, after the depth map exists.
#[derive(Resource, Default)]
struct CreatureSpawns {
    plans: Vec<CreatureData>,
    /// Body height in pixels per creature (`CreatureData::bottom - origin.y`),
    /// used to turn a live cell into an occlusion bottom without a scene lookup.
    heights: HashMap<usize, i32>,
    /// Spawn pixel per creature, the origin every clip corner is reduced
    /// against when a swap re-anchors its animator.
    origins: HashMap<usize, (i32, i32)>,
}

/// A sprite under a [`CreatureNode`]: which creature it belongs to and which of
/// its layers it is (or the shadow), so the animation system swaps its sequence.
#[derive(Component)]
struct CreatureSprite {
    creature_id: usize,
    layer: usize,
    is_shadow: bool,
}

/// Slots pinning the creature pipeline around [`kn_render::AnimSet::Sprites`]:
/// clips are chosen before the frame write and nodes placed after it.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum CreatureSet {
    Swap,
    Nodes,
}

/// Decodes the walk frames the scene does not carry and builds the swap
/// sequences; a creature whose walk cannot be decoded simply keeps its record
/// pose. Nothing about that is silent: every reason a walk is missing or
/// partial is pushed into `problems` for the diagnostics panel (review #68) —
/// a creature walking on its rest pose is a rendering defect, not an absence
/// of creatures to explain away.
fn build_creature_anim(
    game: &Game,
    map_ref: MapRef,
    cache: &mut TextureCache,
    images: &mut Assets<Image>,
    problems: &mut Vec<String>,
) -> CreatureAnim {
    let animations = match game.creature_animations(map_ref) {
        Ok(animations) => animations,
        Err(error) => {
            problems.push(format!(
                "walk animations for {map_ref:?}: {error}; every creature keeps its record pose"
            ));
            return CreatureAnim::default();
        }
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
        problems.push(format!(
            "walk textures for {map_ref:?}: {error}; undecoded walk frames draw the rest pose"
        ));
    }
    let without_walk = animations
        .iter()
        .filter(|(_, anim)| anim.walk.iter().all(Option::is_none))
        .count();
    let partial_walk = animations
        .iter()
        .filter(|(_, anim)| {
            anim.walk.iter().any(Option::is_some) && anim.walk.iter().any(Option::is_none)
        })
        .count();
    if without_walk > 0 {
        problems.push(format!(
            "{without_walk} creature(s) of {} have no walk frames at all: they draw \
             the rest pose while the simulation walks them",
            animations.len()
        ));
    }
    if partial_walk > 0 {
        problems.push(format!(
            "{partial_walk} creature(s) of {} lack walk frames in some directions: \
             those steps draw the rest pose",
            animations.len()
        ));
    }
    CreatureAnim {
        creatures: animations
            .into_iter()
            .map(|(id, anim)| (id, build_sequences(&anim, cache)))
            .collect(),
        locked: HashMap::new(),
    }
}

/// One creature's clips → sequences, aligned to the scene's layers by
/// [`kn_game::LayerKey`], never by position: see [`pose_matched_layers`].
fn build_sequences(anim: &CreatureAnimation, cache: &TextureCache) -> CreatureSequences {
    let matched = pose_matched_layers(&anim.rest.layers, &anim.walk);
    let body = anim
        .rest
        .layers
        .iter()
        .enumerate()
        .map(|(index, rest_layer)| {
            let rest = clip_seq(Some(rest_layer), cache);
            let walk = matched[index]
                .iter()
                .map(|layer| clip_seq(*layer, cache))
                .collect();
            (index, LayerSequences { rest, walk })
        })
        .collect();
    let rest = clip_seq(anim.rest.shadow.as_ref(), cache);
    let walk = anim
        .walk
        .iter()
        .map(|dir| {
            dir.as_ref()
                .and_then(|clips| clips.shadow.as_ref())
                .and_then(|layer| clip_seq(Some(layer), cache))
        })
        .collect();
    CreatureSequences {
        body,
        shadow: LayerSequences { rest, walk },
    }
}

/// For each rest layer, the layer of every walk pose that draws the same part.
///
/// The match is by [`kn_game::LayerKey`]: pose tracks are ordered by first
/// appearance within one pose, so a walk pose may list the same layers in
/// another order, and an index join would bolt one part's walk clip onto
/// another's piece — equipment sliding across the sprite on every step
/// (review #68).
fn pose_matched_layers<'a>(
    rest: &'a [CreatureLayer],
    walk: &'a [Option<CreatureClips>],
) -> Vec<Vec<Option<&'a CreatureLayer>>> {
    rest.iter()
        .map(|rest_layer| {
            walk.iter()
                .map(|dir| {
                    dir.as_ref().and_then(|clips| {
                        clips
                            .layers
                            .iter()
                            .find(|layer| layer.key == rest_layer.key)
                    })
                })
                .collect()
        })
        .collect()
}

/// Frames of one layer → a swap clip; `None` when no frame decoded.
fn clip_seq(layer: Option<&CreatureLayer>, cache: &TextureCache) -> Option<Clip> {
    let layer = layer?;
    let animation = layer.animation.as_ref()?;
    let sequence = piece_sequence(animation, cache)?;
    Some(Clip {
        sequence,
        base: animation.base,
        corner: layer.corner,
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
    let mut origins = HashMap::new();
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
        origins.insert(id, origin);
    }
    CreatureSpawns {
        plans,
        heights,
        origins,
    }
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
        .map(|(index, _)| rest_animation(sequences.and_then(|c| c.body.get(&index)), origin))
        .collect();
    let shadow_animation = sequences.and_then(|c| rest_animation(Some(&c.shadow), origin));
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

/// The resting [`LayerAnimation`] of a layer, if it has a resting sequence,
/// anchored on the clip's own base corner relative to the creature's origin —
/// the same coordinates the node lives in.
fn rest_animation(layer: Option<&LayerSequences>, origin: (i32, i32)) -> Option<LayerAnimation> {
    let clip = layer?.rest.as_ref()?;
    Some(LayerAnimation {
        sequence: Arc::clone(&clip.sequence),
        base: clip.base,
        anchor: (clip.corner.0 - origin.0, clip.corner.1 - origin.1),
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
                sync_creature_visibility,
                advance_simulation,
                spawn_creature_labels,
                spawn_creatures,
                sync_creature_animation.in_set(CreatureSet::Swap),
                sync_creature_nodes.in_set(CreatureSet::Nodes),
                diagnostics,
            )
                .chain()
                // The frame's clock position must be settled before the simulation
                // catches up to it and before the creature systems read it, and
                // clips must be chosen before `apply_animators` writes sprites and
                // the nodes placed after them — never left to the scheduler's
                // placement of two unconstrained chains (review #68).
                .after(AnimSet::Clock)
                .after(CameraInput)
                .before(CameraApply),
        )
        .configure_sets(
            Update,
            (
                AnimSet::Sprites.after(CreatureSet::Swap),
                CreatureSet::Nodes.after(AnimSet::Sprites),
            ),
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
    match game.0.simulation(map_ref, SIM_SEED) {
        Ok(simulation) => {
            sim_resource.error = None;
            sim_resource.initial_pixels = simulation.initial_pixels();
            sim_resource.sim = Some(simulation);
        }
        Err(error) => {
            sim_resource.error = Some(format!("simulation for {map_ref:?}: {error}"));
            warn!("map {map_ref:?}: simulation failed to build: {error}");
            sim_resource.initial_pixels = Vec::new();
            sim_resource.sim = None;
        }
    }
    sim_resource.labels_spawned = false;
    if let Some(root) = sim_resource.label_root.take() {
        commands.entity(root).despawn();
    }
    if let Some(root) = sim_resource.creature_root.take() {
        commands.entity(root).despawn();
    }
    sim_resource.creatures_spawned = false;
    // The fresh simulation starts at tick 0 from the current clock: set the epoch so
    // `advance_simulation` neither replays the accumulated history as one burst nor
    // rewinds to absolute clock time on a later backward step (review #68).
    sim_tracker.epoch = clock.ticks;
    sim_tracker.sim_ticks = 0;
    scene_depth.map = build_scene_depth(&scene, &cache);
    *creature_anim = build_creature_anim(
        &game.0,
        map_ref,
        &mut cache,
        &mut images,
        &mut stats.problems,
    );
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

/// Advances the simulation to the clock, one logical tick per clock tick.
///
/// Time is simulation-relative: the clock tick that maps to the simulation's tick 0
/// is the tracker's `epoch`, moved to the clock by a reset or a map change. Forward
/// is a plain catch-up. A backward `Step` rewinds the clock, but the engine has no
/// reverse tick, so we rebuild the map's simulation and replay it forward to the
/// *epoch-relative* target — a reset is the zero point, so stepping back just after
/// one replays nothing instead of the whole clock (review #68). Replaying from seed
/// is deterministic, so the rewound state equals what forward stepping reached there.
fn advance_simulation(
    clock: Res<Clock>,
    mut tracker: ResMut<SimTickTracker>,
    mut sim: ResMut<SimulationResource>,
    game: Res<GameResource>,
    viewer: Res<Viewer>,
) {
    let Some(&map_ref) = viewer.maps.get(viewer.selected) else {
        tracker.epoch = clock.ticks;
        tracker.sim_ticks = 0;
        return;
    };
    if sim.sim.is_none() {
        tracker.epoch = clock.ticks;
        tracker.sim_ticks = 0;
        return;
    }
    let target = clock.ticks.saturating_sub(tracker.epoch);
    if target < tracker.sim_ticks {
        // Backward: rebuild and replay forward to the epoch-relative target. This
        // runs only when the clock actually moved behind the simulation, not every
        // frame, so a shallow rewind after a reset (target near 0) is cheap.
        let Some(replayed) = replay_simulation(&game.0, map_ref, target) else {
            return;
        };
        sim.initial_pixels = replayed.initial_pixels();
        sim.sim = Some(replayed);
        tracker.sim_ticks = target;
        return;
    }
    while tracker.sim_ticks < target {
        let Some(simulation) = sim.sim.as_mut() else {
            break;
        };
        simulation.tick();
        tracker.sim_ticks += 1;
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

/// Rebuilds the map's simulation and pins the tracker's epoch to the clock.
///
/// The fresh simulation is at tick 0 from this point: `epoch = clock_ticks` keeps it
/// from replaying ticks the old one already ran and makes a later backward Step
/// rewind against the reset, not absolute clock time. Creature nodes are not
/// recreated — `sync_creature_nodes` repositions them from the state each frame.
fn reset_simulation(
    game: &Game,
    map_ref: MapRef,
    clock_ticks: u64,
    sim: &mut SimulationResource,
    tracker: &mut SimTickTracker,
) {
    match game.simulation(map_ref, SIM_SEED) {
        Ok(simulation) => {
            sim.error = None;
            sim.initial_pixels = simulation.initial_pixels();
            sim.sim = Some(simulation);
        }
        Err(error) => {
            sim.error = Some(format!("simulation for {map_ref:?}: {error}"));
            sim.sim = None;
        }
    }
    tracker.epoch = clock_ticks;
    tracker.sim_ticks = 0;
}

const LABEL_OFFSET_Y: f32 = 50.0;

/// Temporary per-creature diagnostics for the movement/animation review: set
/// `KN_CREATURE_DEBUG` in the environment to log, per creature id, the simulation
/// tick, cell, step from/to, `step_elapsed`, the cell pixel the node sits on, the
/// node position and z as this frame writes them, and the simulation's own
/// facing direction; [`sync_creature_animation`] adds each sprite's clip, anchor,
/// phase and shown frame with its local offset and the world position as it will
/// be drawn (the frame's `GlobalTransform`s are only propagated at the end of the
/// update, so reading them would log the previous frame). Comparing
/// `sim_dir`/`from`/`to` (which change only at a logical tick) with the sprite's
/// world position separates a node jump from an image jump: the node only ever
/// sits on cell pixels, the glide to the next cell lives in the frame offsets.
static CREATURE_DEBUG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

fn creature_debug() -> bool {
    *CREATURE_DEBUG.get_or_init(|| std::env::var_os("KN_CREATURE_DEBUG").is_some())
}

fn spawn_creature_labels(
    mut sim: ResMut<SimulationResource>,
    viewer: Res<Viewer>,
    mut commands: Commands,
) {
    if sim.labels_spawned || sim.sim.is_none() {
        return;
    }
    let states = sim.sim.as_ref().unwrap().states();
    sim.labels_spawned = true;
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
    viewer: Res<Viewer>,
) {
    if sim.creatures_spawned || sim.sim.is_none() {
        return;
    }
    sim.creatures_spawned = true;
    // Same rule as the label root: born visible only when the Characters
    // overlay is on, so a map change with characters off shows nothing for a
    // frame (review #68).
    let target = if viewer.is_on(Overlay::Characters) {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    let root = commands
        .spawn((CreatureRoot, Transform::IDENTITY, target))
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
/// body, equipment and shadow stay in lockstep (review #68). The node sits on the
/// simulation cell's pixel for the whole step: the glide to the next cell lives in
/// the walk frames' own offsets (docs/k1/movement.md, "Step duration"), so sliding
/// the node along with them would double the travel and snap the image back by a
/// cell at every cycle wrap.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn sync_creature_nodes(
    sim: Res<SimulationResource>,
    scene_depth: Res<SceneDepth>,
    spawns: Res<CreatureSpawns>,
    clock: Res<Clock>,
    // `Without<CreatureSprite>` on top of `Without<CreatureShadow>`: the node
    // now writes the `Transform` the `sprites` query reads, and without the
    // filter Bevy cannot prove an entity never carries both (B0001).
    mut nodes: Query<
        (&mut CreatureNode, &mut Transform),
        (Without<CreatureShadow>, Without<CreatureSprite>),
    >,
    // `Without<CreatureShadow>` keeps shadows out: they carry `CreatureSprite`
    // too, and this query reads the `Transform` the `shadows` query writes.
    sprites: Query<(&CreatureSprite, &Transform), Without<CreatureShadow>>,
    // `CreatureShadow` marks only the shadow children, disjoint from the nodes.
    mut shadows: Query<(&CreatureShadow, &ChildOf, &mut Transform)>,
    mut labels: Query<
        (&CreatureLabel, &mut Transform),
        (
            Without<CreatureNode>,
            Without<CreatureShadow>,
            Without<CreatureSprite>,
        ),
    >,
) {
    let Some(ref simulation) = sim.sim else {
        return;
    };
    let states = simulation.states();
    let drawn: HashMap<usize, (i32, i32)> =
        states.iter().map(|state| (state.id, state.pixel)).collect();
    // A label rides above the body image, not the cell: during a step the image
    // is a cell away from the node, carried there by its walk frames. The world
    // position is the node's — this frame's cell, which the loop below writes —
    // plus the sprite's local offset, which `apply_animators` has just updated;
    // the sprite's own `GlobalTransform` would still carry the previous frame
    // (propagation runs at the end of the update, review #68).
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
    for (mut node, mut transform) in &mut nodes {
        let id = node.id as usize;
        let Some(&(x, y)) = drawn.get(&id) else {
            continue;
        };
        transform.translation.x = x as f32;
        transform.translation.y = -(y as f32);
        // Occlusion bottom from the live cell and this creature's body height.
        let height = spawns.heights.get(&id).copied().unwrap_or(0);
        let bottom = (y + height).max(0);
        update_creature_depth(&mut node, &mut transform, bottom, &scene_depth.map);
        if creature_debug()
            && let Some(state) = states.iter().find(|state| state.id == id)
        {
            // The world position is the `Transform` just written: the node's
            // parent is the identity root, and this frame's `GlobalTransform`
            // is only propagated at the end of the update, so reading it here
            // would log the previous frame (review #68).
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
                x,
                y,
                transform.translation.x,
                transform.translation.y,
                transform.translation.z,
                state.direction,
                state.walk_state,
            );
        }
    }
    // The shadow rides the node but must hold the fixed shadow band in world z:
    // its local z cancels the node's stack z, which `update_creature_depth`
    // rewrites whenever the bottom edge moves (review #68).
    let shadow_z = scene_depth.map.shadow_z();
    for (_, parent, mut transform) in &mut shadows {
        let Ok(node) = nodes.get(parent.0) else {
            continue;
        };
        let target = shadow_z - node.1.translation.z;
        if transform.translation.z != target {
            transform.translation.z = target;
        }
    }
    for (label, mut transform) in &mut labels {
        let Some(&(x, y)) = label_at.get(&label.creature_id) else {
            continue;
        };
        transform.translation.x = x;
        transform.translation.y = y - LABEL_OFFSET_Y;
    }
}

/// Swaps each creature sprite's [`Animator`] between the resting and the walking
/// sequence when the simulation's [`WalkState`] or facing direction changes,
/// keeping body, equipment and shadow of one creature in step.
///
/// The swap re-anchors the animator on the clip's own base corner: every clip's
/// frame offsets are relative to that corner, so an animator left on the
/// resting corner would render the new clip displaced by the corner difference
/// — the periodic image jump of review #68. A step seen for the first time
/// re-anchors too, opening the cycle `(elapsed - 1)` frames in: the walk frames
/// carry the travel across a step, so their index must run with the step like the
/// original's `Frame` counter (Slavik `game_play.cpp:3727-3740`), not free-run
/// against the global clock and wrap to the first frame mid-step.
fn sync_creature_animation(
    sim: Res<SimulationResource>,
    mut anim: ResMut<CreatureAnim>,
    spawns: Res<CreatureSpawns>,
    clock: Res<Clock>,
    mut sprites: Query<(&CreatureSprite, &mut Animator, &Transform)>,
) {
    let Some(ref simulation) = sim.sim else {
        return;
    };
    let states = simulation.states();
    let restep = anim.observe_steps(&states);
    let facing: HashMap<usize, (bool, u8, u32)> = states
        .iter()
        .map(|state| {
            (
                state.id,
                (
                    state.walk_state == WalkState::Walk,
                    state.direction.unwrap_or(0),
                    state.step_elapsed,
                ),
            )
        })
        .collect();
    for (tag, mut animator, transform) in &mut sprites {
        let Some(&(walking, direction, elapsed)) = facing.get(&tag.creature_id) else {
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
            let local = transform.translation;
            // The world position as it will be drawn: the current tick's cell
            // pixel, which `sync_creature_nodes` writes to the node this frame,
            // plus this frame's local offset. The sprite's own `GlobalTransform`
            // still holds the previous frame here — propagation runs at the end
            // of the update (review #68).
            let (world_x, world_y) = states
                .iter()
                .find(|state| state.id == tag.creature_id)
                .map_or((local.x, local.y), |state| {
                    (
                        state.pixel.0 as f32 + local.x,
                        -(state.pixel.1 as f32) + local.y,
                    )
                });
            info!(
                "creature #{} {} base={} corner={:?} shown={:?} anchor=({},{}) phase={} \
                 local=({:.1},{:.1}) world=({:.1},{:.1}) walking={walking} dir={direction}",
                tag.creature_id,
                if tag.is_shadow { "shadow" } else { "body" },
                clip.base,
                clip.corner,
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
        if Arc::ptr_eq(&animator.sequence, &clip.sequence) && !restep.contains(&tag.creature_id) {
            continue;
        }
        let Some(&origin) = spawns.origins.get(&tag.creature_id) else {
            continue;
        };
        // The step began `elapsed - 1` ticks ago: opening the clip on its base
        // frame there puts the shown frame at base + elapsed - 1 now — the
        // step-relative index, correct even when catch-up first saw the step late.
        let step_start = clock
            .ticks
            .saturating_sub(u64::from(elapsed.saturating_sub(1)));
        apply_clip(&mut animator, clip, origin, step_start);
    }
}

/// Point one animator at one clip: open it on the clip's base frame at the
/// current clock tick and re-anchor on the clip's own corner, reduced to the
/// creature's local coordinates.
///
/// Both halves are load-bearing (review #68): the shared clock never returns to
/// zero, so without a per-clip phase the swap would show a foreign frame; and
/// every clip's frame offsets are relative to its own base corner, so an
/// animator left on the previous corner would render the image displaced by the
/// corner difference — the periodic one-cell jump.
fn apply_clip(animator: &mut Animator, clip: &Clip, origin: (i32, i32), at_tick: u64) {
    animator.play_at(
        Arc::clone(&clip.sequence),
        clip.base,
        at_tick,
        (clip.corner.0 - origin.0, clip.corner.1 - origin.1),
    );
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

#[allow(clippy::too_many_arguments)]
fn diagnostics(
    stats: Res<Stats>,
    sim: Res<SimulationResource>,
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
                "{} {} · {pieces} · {} textures · {} ms{problems}{sim_error}\nzoom ×{} · {} fps{playback}",
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
    use kn_game::{LayerKey, PieceGroup};
    use kn_render::Frame;

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

    /// The walk cycle is opened at the tick the step started, not the tick it was
    /// first observed: `elapsed - 1` ticks after the base frame, so the shown
    /// frame runs with the step like the original's per-tick `Frame` counter.
    #[test]
    fn apply_clip_opens_the_frame_the_step_has_reached() {
        let frames = (0..12)
            .map(|_| Frame::new(Handle::default(), Vec2::new(8.0, 8.0), Vec2::ZERO))
            .collect();
        let clip = Clip {
            sequence: Arc::new(Sequence::new(frames)),
            base: 0,
            corner: (0, 0),
        };
        let mut animator = Animator::new(Arc::clone(&clip.sequence), 0, 0);
        let clock_ticks = 100u64;
        let elapsed = 3u32;
        apply_clip(
            &mut animator,
            &clip,
            (0, 0),
            clock_ticks.saturating_sub(u64::from(elapsed - 1)),
        );
        assert_eq!(
            clip.sequence.frame_at(clock_ticks, animator.phase),
            (elapsed - 1) as usize,
            "three ticks into a step shows its third frame"
        );
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

    fn data_layer(key: LayerKey, corner: (i32, i32)) -> CreatureLayer {
        CreatureLayer {
            base: None,
            animation: None,
            corner,
            key,
        }
    }

    /// A walk pose may list the same layers in another order than the resting
    /// one (tracks are ordered by first appearance per pose), so a join by
    /// position would bolt the gear's walk clip onto the body's piece and the
    /// body's onto the gear's — equipment sliding across the sprite on every
    /// step. The join must follow [`LayerKey`].
    #[test]
    fn walk_layers_join_to_their_part_not_their_position() {
        let rest = [
            data_layer(LayerKey::Body, (0, 0)),
            data_layer(LayerKey::Gear { slot: 1 }, (4, 40)),
        ];
        // Direction 0 lists the layers reversed, direction 1 has no gear at all.
        let reversed = CreatureClips {
            layers: vec![
                data_layer(LayerKey::Gear { slot: 1 }, (6, 44)),
                data_layer(LayerKey::Body, (2, 1)),
            ],
            shadow: None,
        };
        let gearless = CreatureClips {
            layers: vec![data_layer(LayerKey::Body, (3, 2))],
            shadow: None,
        };
        let walk = [Some(reversed), Some(gearless), None];

        let matched = pose_matched_layers(&rest, &walk);
        assert_eq!(matched.len(), 2);
        assert_eq!(
            matched[0][0].map(|layer| layer.key),
            Some(LayerKey::Body),
            "the body's walk clip must be the body's, not the first listed"
        );
        assert_eq!(
            matched[0][0].map(|layer| layer.corner),
            Some((2, 1)),
            "the body's clip keeps the walk pose's own corner"
        );
        assert_eq!(
            matched[1][0].map(|layer| layer.key),
            Some(LayerKey::Gear { slot: 1 }),
            "the gear's walk clip must be the gear's, not the second listed"
        );
        assert!(
            matched[1][1].is_none(),
            "a pose without the part contributes no clip"
        );
        assert!(
            matched[0][2].is_none(),
            "a missing pose contributes nothing"
        );
    }

    /// A swap re-anchors the animator on the clip's own corner reduced to the
    /// creature's origin: leaving the resting anchor there displaced the image
    /// by the corner difference — the periodic jump review #68 is about.
    #[test]
    fn apply_clip_reanchors_on_the_clips_own_corner() {
        let rest_sequence = Arc::new(Sequence::new(Vec::new()));
        let walk_sequence = Arc::new(Sequence::new(Vec::new()));
        let mut animator = Animator::with_base(Arc::clone(&rest_sequence), 4, 40, 0);
        let walk = Clip {
            sequence: Arc::clone(&walk_sequence),
            base: 0,
            corner: (62, 26),
        };

        apply_clip(&mut animator, &walk, (4, 10), 9);

        assert_eq!(
            (animator.x, animator.y),
            (58, 16),
            "the anchor is the clip corner in the creature's local coordinates"
        );
        assert!(Arc::ptr_eq(&animator.sequence, &walk_sequence));
        assert_eq!(animator.shown(), None, "the swap forces a sprite rewrite");
        assert_eq!(
            walk_sequence.frame_at(9, animator.phase),
            walk.base,
            "the clip opens on its base frame at the current tick"
        );
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
            epoch: 0,
            sim_ticks: 0,
        };
        // The old tracker lags behind the clock: a reset must pin its epoch to the
        // current tick, otherwise the fresh simulation replays the backlog in one jump
        reset_simulation(&game, map_ref, 517, &mut sim, &mut tracker);
        assert_eq!(tracker.epoch, 517, "the tracker is pinned to the clock");
        assert_eq!(
            tracker.sim_ticks, 0,
            "a reset simulation starts its own timeline at zero"
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
                epoch: 0,
                sim_ticks: 0,
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
            app.world().resource::<SimTickTracker>().epoch,
            300,
            "the tracker is pinned to the clock without a jump"
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
                epoch: 900,
                sim_ticks: 0,
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

    /// A backward `Step` rewinds the clock below the simulation's own timeline;
    /// `advance_simulation` sees `target < sim_ticks`, replays the map from tick 0 to
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
                epoch: 0,
                sim_ticks: 200,
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
        let mut sim_resource = SimulationResource::default();
        sim_resource.sim = game.simulation(map_ref, SIM_SEED).ok();
        sim_resource.initial_pixels = sim_resource.sim.as_ref().unwrap().initial_pixels();
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
            // A reset at clock tick 400 left a fresh simulation at its own tick 0.
            .insert_resource(SimTickTracker {
                epoch: 400,
                sim_ticks: 0,
            })
            .add_systems(Update, advance_simulation);
        // A backward Step rewinds the clock below the reset epoch.
        app.world_mut().resource_mut::<Clock>().ticks = 395;
        app.update();

        let states = app
            .world()
            .resource::<SimulationResource>()
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
    /// `.ok()`: `reset_simulation` records the reason on the resource and leaves the
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
        let mut sim = SimulationResource::default();
        let mut tracker = SimTickTracker {
            epoch: 123,
            sim_ticks: 45,
        };
        reset_simulation(&game, bogus, 123, &mut sim, &mut tracker);
        assert!(sim.sim.is_none(), "no simulation was built");
        let reason = sim
            .error
            .expect("the build failure is surfaced, not swallowed by .ok()");
        assert!(
            reason.contains("simulation for"),
            "the reason names the map that failed: {reason}"
        );
        assert_eq!(tracker.epoch, 123, "the epoch is pinned even on failure");
        assert_eq!(tracker.sim_ticks, 0, "the timeline restarts at the reset");
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
        let sim_resource = SimulationResource {
            sim: game.simulation(map_ref, SIM_SEED).ok(),
            ..Default::default()
        };
        assert!(sim_resource.sim.is_some(), "the map must give a simulation");

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(sim_resource)
            .init_resource::<CreatureSpawns>()
            .init_resource::<SceneDepth>()
            .insert_resource(viewer([false, false, false, false]))
            .add_systems(
                Update,
                (
                    spawn_creature_labels,
                    spawn_creatures,
                    sync_creature_visibility,
                )
                    .chain(),
            );
        app.update();

        let sim = app.world().resource::<SimulationResource>();
        let creature_root = sim.creature_root.expect("the creature root spawned");
        let label_root = sim.label_root.expect("the label root spawned");
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
            .init_resource::<SimulationResource>()
            .init_resource::<SceneDepth>()
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
        let mut anim_problems = Vec::new();
        let anim = build_creature_anim(&game, map_ref, &mut cache, &mut images, &mut anim_problems);
        for problem in &anim_problems {
            println!("walk diagnostics: {problem}");
        }
        let scene = game.scene(map_ref, &SceneOptions::ALL).unwrap();
        let mut sim_resource = SimulationResource::default();
        sim_resource.sim = game.simulation(map_ref, SIM_SEED).ok();
        sim_resource.initial_pixels = sim_resource.sim.as_ref().unwrap().initial_pixels();
        let spawns = build_creature_spawns(&scene, &cache, &anim, &sim_resource.initial_pixels);
        assert!(
            !spawns.plans.is_empty(),
            "карта обязана дать существ со сценой"
        );
        let depth = build_scene_depth(&scene, &cache);

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::transform::TransformPlugin,
            AnimPlugin::new(TICK_MS as f32),
        ))
        .insert_resource(GameResource(game))
        .insert_resource(Viewer {
            maps: vec![map_ref],
            selected: 0,
            overlays: DEFAULT_OVERLAYS,
            playback: PlayState::Paused,
        })
        .insert_resource(images)
        .insert_resource(sim_resource)
        .insert_resource(SimTickTracker::default())
        .insert_resource(SceneDepth { map: depth })
        .insert_resource(anim)
        .insert_resource(spawns)
        .add_systems(
            Update,
            (
                advance_simulation,
                spawn_creatures,
                sync_creature_animation.in_set(CreatureSet::Swap),
                sync_creature_nodes.in_set(CreatureSet::Nodes),
            )
                .chain()
                .after(AnimSet::Clock),
        )
        .configure_sets(
            Update,
            (
                AnimSet::Sprites.after(CreatureSet::Swap),
                CreatureSet::Nodes.after(AnimSet::Sprites),
            ),
        );
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
                .resource::<SimulationResource>()
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
        let anim = build_creature_anim(&game, map_ref, &mut cache, &mut images, &mut Vec::new());
        let scene = game.scene(map_ref, &SceneOptions::ALL).unwrap();
        let simulation = game.simulation(map_ref, SIM_SEED).ok();
        let initial = home(simulation.as_ref().unwrap());
        let initial_by_id: HashMap<usize, (i32, i32)> = initial.iter().copied().collect();
        let sim_resource = SimulationResource {
            initial_pixels: simulation.as_ref().unwrap().initial_pixels(),
            sim: simulation,
            ..Default::default()
        };
        let spawns = build_creature_spawns(&scene, &cache, &anim, &sim_resource.initial_pixels);
        assert!(!spawns.plans.is_empty(), "the map must give creatures");
        let plan_count = spawns.plans.len();
        let plan_ids: HashSet<usize> = spawns.plans.iter().map(|plan| plan.id as usize).collect();
        let depth = build_scene_depth(&scene, &cache);
        let shadow_z = depth.shadow_z();

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::transform::TransformPlugin,
            AnimPlugin::new(TICK_MS as f32),
        ))
        .insert_resource(GameResource(game))
        .insert_resource(Viewer {
            maps: vec![map_ref],
            selected: 0,
            overlays: DEFAULT_OVERLAYS,
            playback: PlayState::Paused,
        })
        .insert_resource(images)
        .insert_resource(sim_resource)
        .insert_resource(SimTickTracker::default())
        .insert_resource(SceneDepth { map: depth })
        .insert_resource(anim)
        .insert_resource(spawns)
        .add_systems(
            Update,
            (
                advance_simulation,
                spawn_creature_labels,
                spawn_creatures,
                sync_creature_animation.in_set(CreatureSet::Swap),
                sync_creature_nodes.in_set(CreatureSet::Nodes),
            )
                .chain()
                .after(AnimSet::Clock),
        )
        .add_observer(reset_positions)
        .configure_sets(
            Update,
            (
                AnimSet::Sprites.after(CreatureSet::Swap),
                CreatureSet::Nodes.after(AnimSet::Sprites),
            ),
        );
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
            let sim = app.world().resource::<SimulationResource>();
            let states = sim.sim.as_ref().unwrap().states();
            left_home |= states
                .iter()
                .any(|state| initial_by_id.get(&state.id) != Some(&state.pixel));
        }
        assert!(left_home, "200 ticks must move at least one creature");
        let (root_before, label_root_before) = {
            let sim = app.world().resource::<SimulationResource>();
            (sim.creature_root, sim.label_root)
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
        let sim = world.resource::<SimulationResource>();
        assert_eq!(
            sim.creature_root, root_before,
            "the nodes are not respawned"
        );
        assert_eq!(
            sim.label_root, label_root_before,
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

        // The shadow band survives the depth recompute a reset triggers.
        let mut shadow_count = 0;
        for (_, parent, shadow) in shadows.iter(world) {
            let Some(node) = world.get::<Transform>(parent.0) else {
                continue;
            };
            assert!(
                (shadow.translation.z + node.translation.z - shadow_z).abs() < 1e-3,
                "shadow of a reset creature left the band: local {} + node {}",
                shadow.translation.z,
                node.translation.z
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
        let replayed = replay_simulation(&game, map_ref, TICKS).expect("replay builds");
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

    fn fire_reset(mut commands: Commands, buttons: Query<Entity, With<ResetPositions>>) {
        for button in &buttons {
            commands.trigger(Activate { entity: button });
        }
    }
}
