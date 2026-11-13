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
    ecs::schedule::common_conditions::resource_exists,
    feathers::{
        controls::{
            FeathersButton, FeathersCheckbox, FeathersSelect, FeathersSlider, OptionIndex,
            list_rows_from_strings,
        },
        display::caption,
        theme::ThemeBackgroundColor,
        tokens,
    },
    input::mouse::{AccumulatedMouseMotion, MouseWheel},
    picking::hover::Hovered,
    prelude::*,
    render::view::screenshot::{Capturing, Screenshot, save_to_disk},
    sprite_render::SpriteMaterial,
    ui::{Checked, Pressed},
    ui_widgets::{Activate, SliderStep, SliderValue, ValueChange},
    window::PresentMode,
};
// Re-exported: `run_viewer` hands the app's exit back and the windowed
// binary must be able to name the type to turn it into a process exit code.
pub use bevy::app::AppExit;
use k1_data::lights_res;
use k1_data::map;
use k1_data::repo::{GAME_FILES, GameDir, TUTORIAL_FILES};
use k1_game::lighting;
use k1_game::time_of_day::CYCLE_TICKS;
use kn_game::{
    DayPhase, Game, MapRef, MapSource, Piece, PieceGroup, SceneOptions, TICK_MS, group_is_shown,
    simulation::{CreatureState, Simulation, WalkState},
};
use kn_input::{ZoomScroll, drag_delta, pan_direction, wheel_steps};
use kn_render::{
    AnimPlugin, AnimSet, Animator, CameraApply, CameraBounds, CameraInput, CameraState, Clock,
    CreatureAnim, CreatureAnimReport, CreatureAnimationPlugin, CreatureMotion, CreatureNode,
    CreatureNodesVisible, CreatureRoot, CreatureSet, CreatureShadow, CreatureSim, CreatureSprite,
    FineLightGrid, GameResource, KonungLightMaterial, KonungLightingPlugin, LightingMaterials,
    LightingState, MapCamera, MapCameraPlugin, PAN_SPEED, PendingFrame, PlayState, SceneGroup,
    ScenePiece, Sequence, TextureCache, clear_scene, load_scene, ortho_scale, piece_center,
    piece_size, reset_creature_animations, reset_creature_positions, update_lighting_uniforms,
};
use kn_ui::{KonungFonts, KonungUiPlugin, pointer_over_ui, sync_checked};

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

/// The day/night cycle runs at startup (`false` — "constant day"). Agreed
/// with the missing `Checked` on the `Constant day` checkbox in [panel]:
/// the default is the cycle, because the original's saved-options default is
/// unconfirmed (a `.bss` global whose writers copy from the settings table;
/// see `docs/k1/time-of-day.md` and the divergence note below).
pub const DEFAULT_CONSTANT_DAY: bool = false;

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
    /// "Constant day": `true` holds the lighting input at its constant day
    /// value (`k1_game::lighting::params_gated`) while time keeps running —
    /// the phase advances exactly as with the cycle on, as in the original,
    /// whose counter no option ever gates. A viewer session setting, not
    /// simulation state: it survives a map switch (which rebuilds the
    /// simulation) and returns to the cycle-on default only on app restart.
    pub constant_day: bool,
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

/// Hotkey `N`: the day/night cycle in one keypress — constant day on/off.
/// Toggling never touches time: the phase keeps advancing, only the
/// lighting input is gated.
pub fn toggle_day_night(constant_day: bool) -> bool {
    !constant_day
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
    pub problems: Vec<kn_render::TextureProblem>,
    pub error: Option<String>,
}

/// Seed of the creature simulation: fixed so "Reset positions" reproduces the
/// exact spawn of the first build (same seed, same map → same initial state).
const SIM_SEED: u64 = 42;

/// Benchmark knobs of the "Viewer costs" measurements (docs/rust.md): a var
/// set to anything but `0` turns its factor on, a var set to `0` or absent
/// keeps the shipped default. They exist so a benchmark run can hold one
/// factor at a time — the id labels (`KN_VIEWER_LABELS`) or the playback
/// start (`KN_VIEWER_PAUSE`) — while an interactive run never sees either.
/// Read through [`knob_value`], so the numbers reproduce from the recorded
/// recipe without a build flag.
fn knob_value(value: Option<&str>, default: bool) -> bool {
    match value {
        Some("0") => false,
        Some(_) => true,
        None => default,
    }
}

fn knob(name: &str, default: bool) -> bool {
    knob_value(std::env::var(name).ok().as_deref(), default)
}

/// Whether the creature id labels spawn at all (`KN_VIEWER_LABELS`, default
/// on). The labels measurement needs the with-and-without pair on one map;
/// off, the label systems run over an empty query.
fn labels_enabled() -> bool {
    knob("KN_VIEWER_LABELS", true)
}

/// The playback a run starts in (`KN_VIEWER_PAUSE`, default playing): the
/// paused benchmark holds the simulation and the walk clips out and leaves
/// the settled scene — the display floor the moving runs compare against.
fn playback_start() -> PlayState {
    if knob("KN_VIEWER_PAUSE", false) {
        PlayState::Paused
    } else {
        DEFAULT_PLAYBACK
    }
}

/// The lighting inputs of the opened game: the LIGHTS.RES tile masks the pool
/// is built from at startup, and optionally the tick the lighting is pinned to
/// (`--light-tick`) instead of following the scene clock each frame.
#[derive(Resource)]
struct LightingSetup {
    tick: Option<u64>,
    masks: Vec<(Vec<u8>, u32, u32)>,
}

/// The LIGHTS.RES tile masks of the opened game: raw brightness bytes with
/// their pixel size, ready for [`LightingMaterials::new`]. A game without a
/// readable LIGHTS.RES gets an empty pool — the scene still lights, the tiles
/// just carry no tile mask. Every way the file can fail warns with its path
/// and reason: an empty pool reached silently reads as a broken night scene
/// (review #106).
fn lights_res_masks(game: &Game) -> Vec<(Vec<u8>, u32, u32)> {
    let Some(path) = k1_game::resources::find_named(game.root(), "lights.res") else {
        warn!(
            "LIGHTS.RES not found in {}; the scene lights without tile masks",
            game.root().display()
        );
        return Vec::new();
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            warn!(
                "LIGHTS.RES unreadable at {}: {error}; the scene lights without tile masks",
                path.display()
            );
            return Vec::new();
        }
    };
    let res = match lights_res::parse(&bytes) {
        Ok(res) => res,
        Err(error) => {
            warn!(
                "LIGHTS.RES unparsable at {}: {error}; the scene lights without tile masks",
                path.display()
            );
            return Vec::new();
        }
    };
    res.tiles
        .into_iter()
        .map(|mask| {
            (
                mask,
                lights_res::TILE_MASK_WIDTH as u32,
                lights_res::TILE_MASK_HEIGHT as u32,
            )
        })
        .collect()
}

/// The map's LIGHTS.N fine grid — the tile-light index per fine cell, what the
/// creature lighting reads to tell a lit patch from a dark one. `None` when
/// the map has no LIGHTS.N: every cell counts as dark, exactly as on a map
/// without lights.
fn map_fine_lights(game: &Game, map_ref: MapRef) -> Option<Vec<Vec<u8>>> {
    let files = match map_ref.source {
        MapSource::Game => GAME_FILES,
        MapSource::Tutorial => TUTORIAL_FILES,
    };
    let bytes = GameDir::new(game.root())
        .lights_bytes(map_ref.number, files)
        .ok()??;
    map::parse_lights(&bytes).ok().map(|lights| lights.fine)
}

/// The k1 reading of `k1_game::lighting::params` into the render contract:
/// terrain and decorations get the ground ambient only inside the ground-light
/// window (outside it a zero blend is the shader's no-op), objects and
/// characters the plain ambient always, plus the ground tile-light colour.
fn lighting_state(params: lighting::LightingParams) -> LightingState {
    LightingState {
        terrain_blend: Vec3::from(params.ambient),
        ambient_blend: Vec3::from(params.ambient),
        light_color: Vec3::from(params.ground_light),
        light_gain: if params.ground_window { 1.0 } else { 0.0 },
    }
}

/// Writes the lighting state for the scene clock — the single time source the
/// scrub moves and the simulation is caught up to — or for the tick pinned by
/// `--light-tick`, which holds a night scene up for screenshots and
/// inspection. The switch `Constant day` (`Viewer::constant_day`) gates only
/// the lighting input (`params_gated`): the ambient holds its day value while
/// the tick — pinned or the clock's — keeps moving through the cycle. Writes
/// only on change: a rewritten state re-extracts the uniforms for the GPU, and
/// the day cycle shifts a few times a minute.
fn update_lighting_state(
    setup: Option<Res<LightingSetup>>,
    viewer: Res<Viewer>,
    clock: Res<Clock>,
    mut state: ResMut<LightingState>,
) {
    let tick = match setup.and_then(|setup| setup.tick) {
        Some(tick) => tick,
        None => clock.ticks,
    };
    let next = lighting_state(lighting::params_gated(tick, SIM_SEED, !viewer.constant_day));
    if *state != next {
        *state = next;
    }
}

/// Builds the material pool from the game's LIGHTS.RES at startup: lighting is
/// part of the scene, not an option — the uniforms arrive every frame through
/// [`update_lighting_state`].
fn setup_lighting(
    setup: Res<LightingSetup>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<KonungLightMaterial>>,
    mut commands: Commands,
) {
    let pool = LightingMaterials::new(&mut images, &mut materials, &setup.masks);
    info!(
        "lighting: {} materials ({} tile masks from LIGHTS.RES)",
        pool.len(),
        setup.masks.len(),
    );
    commands.insert_resource(pool);
}

// --- Time-of-day scrub -----------------------------------------------------

/// Marker of the cycle-position slider: [`change_scrub`] tells this very
/// slider's value changes from the other widgets'.
#[derive(Component, Clone, Copy, Default)]
struct ScrubSlider;

/// A time-of-day preset button carrying its cycle position
/// (`docs/k1/time-of-day.md`, "Camera presets" — places to stand in the cycle,
/// not boundaries). `Default` exists for the template macro; every button in
/// the panel names its own tick.
#[derive(Component, Clone, Copy, Default)]
struct ScrubPreset(u64);

/// One scrub session: the slider drives the Clock while the simulation waits.
///
/// The Clock is the single time source; the widget writes nothing itself —
/// like the `Play` checkbox, its thumb is mirrored from the state by
/// [`sync_scrub_slider`] whenever no drag is in progress.
#[derive(Resource, Default)]
struct ScrubState {
    /// The session is open: the slider is pressed, a preset fired, or a value
    /// change is still unapplied. While it stands, the thumb mirror is held
    /// off and the simulation stays where it is.
    active: bool,
    /// The absolute day (`ticks / CYCLE_TICKS`) captured once at the session's
    /// start: a scrub moves the cycle position only, so the night palette —
    /// `night_palette(day, seed)` over the absolute day — stays the day's.
    day: u64,
    /// The cycle position the widget asked for, `0..CYCLE_TICKS`.
    target: u64,
    /// `target` has not reached the Clock yet.
    pending: bool,
    /// The interaction is over (drag released, keyboard step, preset): apply
    /// the last target and let the simulation catch up to the Clock.
    release: bool,
    /// The Clock moved during the session: a bare click on the track changes
    /// nothing and must not settle the simulation.
    moved: bool,
}

/// The cycle position a slider value names: whole ticks inside `0..CYCLE_TICKS`.
fn scrub_position(value: f32) -> u64 {
    value.round().clamp(0.0, (CYCLE_TICKS - 1) as f32) as u64
}

/// The slider's value change: it moves the Clock at once while the simulation
/// stays where it is; the release (`is_final`) settles both. The day is
/// captured at the session's start — the slider writes
/// `day * CYCLE_TICKS + target`, never the day itself.
fn change_scrub(
    change: On<ValueChange<f32>>,
    sliders: Query<&ScrubSlider>,
    clock: Res<Clock>,
    mut scrub: ResMut<ScrubState>,
    mut commands: Commands,
) {
    if sliders.get(change.source).is_err() {
        return;
    }
    if !scrub.active {
        scrub.active = true;
        scrub.day = clock.ticks / CYCLE_TICKS;
    }
    let position = scrub_position(change.value);
    scrub.target = position;
    scrub.pending = true;
    if change.is_final {
        // A keyboard step is a jump, not a drag: one change, settled at once.
        scrub.release = true;
    }
    // The thumb and its label follow the pointer like a checkbox's tick
    // follows the click ([kn_ui::sync_checked]): the value is the widget's own
    // view state; the real one is the Clock, and [apply_scrub] moves that.
    commands
        .entity(change.source)
        .insert(SliderValue(position as f32));
}

/// A preset button: the jump the slider makes without a drag — applied and
/// settled right away.
fn jump_to_preset(
    activate: On<Activate>,
    presets: Query<&ScrubPreset>,
    clock: Res<Clock>,
    mut scrub: ResMut<ScrubState>,
) {
    let Ok(preset) = presets.get(activate.entity) else {
        return;
    };
    if !scrub.active {
        scrub.active = true;
        scrub.day = clock.ticks / CYCLE_TICKS;
    }
    scrub.target = preset.0;
    scrub.pending = true;
    scrub.release = true;
}

/// Applies the scrub session to the single time source and decides when the
/// simulation may follow.
///
/// While the slider is held, the Clock is pinned to the target every frame and
/// the simulation stays where it stands (`epoch = clock.ticks,
/// sim_ticks = 0` leaves the shared catch-up nothing to chase) — so a drag
/// lights the scrubbed tick without replaying any movement. The pin is not a
/// one-shot: `advance_clock` keeps running under the held slider while
/// playback is on, and without rewriting `clock.ticks` every frame the light
/// would drift a tick per frame from the scrubbed position until the next
/// value change restored it (review #106). A bare press opens the session at
/// the Clock's own position — the thumb has not moved, so a hold to
/// `day * CYCLE_TICKS + target` with the target read from the Clock moves
/// nothing. On release the hold is lifted by re-basing the epoch at zero,
/// which is the whole settle: the next pass of `advance_creature_sim` walks
/// the simulation to the Clock — forward by plain catch-up, backward by its
/// own seed replay — and the creatures land exactly on the scrubbed tick.
/// `reset_creature_positions` would teleport them home instead (its tested
/// purpose), and nothing else can satisfy `tick_number == clock.ticks`.
fn apply_scrub(
    slider: Option<Single<Has<Pressed>, With<ScrubSlider>>>,
    mut scrub: ResMut<ScrubState>,
    mut clock: ResMut<Clock>,
    mut sim: ResMut<CreatureSim>,
) {
    let pressed = slider.is_some_and(|slider| *slider);
    if pressed && !scrub.active {
        scrub.active = true;
        scrub.day = clock.ticks / CYCLE_TICKS;
        // The hold starts where the clock stands: until the first value change
        // the session owns the Clock but moves it nowhere.
        scrub.target = clock.ticks % CYCLE_TICKS;
    }
    // The pointer left the slider without a final value change (cancel): the
    // session is over all the same.
    if scrub.active && !pressed && !scrub.release {
        scrub.release = true;
    }
    if scrub.pending {
        clock.ticks = scrub.day * CYCLE_TICKS + scrub.target;
        // A scrub is a discrete jump: the partial tick of the old position
        // means nothing about the new one.
        clock.sub_tick = 0.0;
        scrub.moved = true;
        scrub.pending = false;
    }
    if !scrub.active {
        return;
    }
    if !scrub.release {
        // Hold the Clock at the target for the whole frame: the same formula
        // the value change applied, rewritten after `advance_clock` moved it.
        clock.ticks = scrub.day * CYCLE_TICKS + scrub.target;
        clock.sub_tick = 0.0;
        sim.epoch = clock.ticks;
        sim.sim_ticks = 0;
        return;
    }
    if scrub.moved
        && let Some(ticks) = sim.sim.as_ref().map(|simulation| simulation.tick_number())
    {
        sim.epoch = 0;
        sim.sim_ticks = ticks;
    }
    *scrub = ScrubState::default();
}

/// Mirrors the Clock into the slider whenever no drag is in progress — the
/// same direction as [sync_playboxes]: while the simulation plays, the thumb
/// rides the cycle position (`clock.ticks % CYCLE_TICKS`) and the day line of
/// the diagnostics moves with it; the widget never writes the state itself
/// outside its own interaction.
fn sync_scrub_slider(
    mut commands: Commands,
    scrub: Res<ScrubState>,
    clock: Res<Clock>,
    sliders: Query<(Entity, &SliderValue), With<ScrubSlider>>,
) {
    if scrub.active {
        return;
    }
    let want = (clock.ticks % CYCLE_TICKS) as f32;
    for (entity, value) in &sliders {
        if value.0 != want {
            // A new component, not a field write: `SliderValue` is immutable,
            // and the widget docs name insertion as the way to set it.
            // Written only on change — a rewritten value re-renders the
            // widget's bar and label for nothing.
            commands.entity(entity).insert(SliderValue(want));
        }
    }
}

/// Where the capture camera aims — the first lit terrain piece of the current
/// map, in world coordinates.
#[derive(Resource, Default)]
struct CaptureTarget(Option<Vec2>);

/// Capture path and counters for the exit report, set by `--screenshot`.
#[derive(Resource, Default)]
struct CaptureRequest {
    path: Option<PathBuf>,
    terrain_pieces: usize,
}

/// Capture phases — settle the pipelines, save the shot, report and exit.
#[derive(Clone, Copy, Default, PartialEq)]
enum CapturePhase {
    #[default]
    Settle,
    Shot,
    Done,
}

/// How many frames the pipelines get to specialize the light material before
/// the screenshot.
const SETTLE_FRAMES: u32 = 40;
/// Camera zoom step while capturing: 1:1, a 114-px tile spans 114 screen px.
const CAPTURE_ZOOM: usize = 3;

/// Drives the `--screenshot` capture — aims the camera at the first lit tile,
/// settles the pipelines, saves the shot, logs the run's measurements and
/// exits. The lighting on the shot is the session's: pinned by `--light-tick`
/// or the simulation's current tick.
///
/// Runs after [`CameraApply`]: the zoom it picks must reach the projection
/// before the shot, and the map must already be built.
#[allow(clippy::too_many_arguments)]
fn capture(
    mut commands: Commands,
    request: Res<CaptureRequest>,
    target: Res<CaptureTarget>,
    pool: Res<LightingMaterials>,
    state: Res<LightingState>,
    mut zoom: ResMut<CameraState>,
    mut camera: Single<&mut Transform, With<MapCamera>>,
    lit: Query<(), With<SpriteMaterial<KonungLightMaterial>>>,
    capturing: Query<(), With<Capturing>>,
    time: Res<Time>,
    mut phase: Local<CapturePhase>,
    mut frames: Local<u32>,
    // (seconds sum, frame count) over the whole run — the CPU frame-time report
    mut frame_time: Local<(f32, u32)>,
) {
    frame_time.0 += time.delta_secs();
    frame_time.1 += 1;
    match *phase {
        CapturePhase::Settle => {
            if let Some(center) = target.0 {
                camera.translation = Vec3::new(center.x, center.y, 0.0);
            }
            zoom.zoom = CAPTURE_ZOOM;
            if *frames >= SETTLE_FRAMES {
                *frames = 0;
                *phase = CapturePhase::Shot;
                if let Some(path) = request.path.clone() {
                    commands
                        .spawn(Screenshot::primary_window())
                        .observe(save_to_disk(path));
                }
            }
        }
        CapturePhase::Shot => {
            if capturing.is_empty() && *frames > 1 {
                let (seconds, count) = *frame_time;
                info!(
                    "capture: avg frame time {:.2} ms over {} frames · terrain blend {:?} · \
                     terrain pieces {} · sprites with the light material {} · materials {}",
                    seconds * 1000.0 / count.max(1) as f32,
                    count,
                    state.terrain_blend,
                    request.terrain_pieces,
                    lit.iter().count(),
                    pool.len(),
                );
                commands.write_message(AppExit::Success);
                *phase = CapturePhase::Done;
            }
        }
        CapturePhase::Done => {}
    }
    *frames += 1;
}

/// Benchmark request from `--benchmark N` / `--benchmark-secs S`: after the
/// same settle window the screenshot uses, collect per-frame times until the
/// bound is reached and print the `BENCH …` line. A benchmark run never
/// captures — the CLI rejects the benchmark flags together with
/// `--screenshot` up front.
#[derive(Resource)]
struct BenchmarkRequest {
    bound: BenchmarkBound,
}

/// What closes the measured window. `Frames` is the old count bound;
/// `Seconds` accumulates frame time until the sum crosses the target — the
/// 60-second gate protocol (#98) reads the same recipe on any hardware, the
/// frame count becomes an output of the run.
#[derive(Clone, Copy, Debug)]
enum BenchmarkBound {
    Frames(u32),
    Seconds(u64),
}

/// The frame-time gate threshold (#98): a pass needs average and p95 at or
/// under 30 FPS (one frame every 33.3 ms); p99 and the worst frame are
/// reported with their cause and never gate.
const FRAME_GATE_LIMIT_MS: f64 = 33.3;

/// The window's statistics, as the `BENCH` line and the gate read them:
/// nearest-rank percentiles over the measured samples plus the worst frame.
#[derive(Clone, Copy, Debug, PartialEq)]
struct FrameStats {
    average: f64,
    p95: f64,
    p99: f64,
    worst: f64,
}

/// Nearest-rank percentiles over the measured frame times: the smallest
/// value covering the fraction of the sorted samples. Empty input yields
/// zeros — a window that closed before any sample cannot print a number.
fn frame_stats(samples: &[f64]) -> FrameStats {
    if samples.is_empty() {
        return FrameStats {
            average: 0.0,
            p95: 0.0,
            p99: 0.0,
            worst: 0.0,
        };
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let percentile = |fraction: f64| {
        let rank = ((sorted.len() as f64) * fraction).ceil() as usize;
        sorted[rank.clamp(1, sorted.len()) - 1]
    };
    FrameStats {
        average: samples.iter().sum::<f64>() / samples.len() as f64,
        p95: percentile(0.95),
        p99: percentile(0.99),
        worst: sorted[sorted.len() - 1],
    }
}

/// The first index of the largest sample — where `max_at` points in the
/// `BENCH` line (1-based within the measured window). Strict comparison
/// keeps the first of equal maxima, unlike `Iterator::max_by`, which keeps
/// the last.
fn worst_frame_index(samples: &[f64]) -> usize {
    let mut worst = 0usize;
    for (index, value) in samples.iter().enumerate() {
        if value.total_cmp(&samples[worst]) == std::cmp::Ordering::Greater {
            worst = index;
        }
    }
    worst
}

/// Whether a simulation tick executed inside the wall-clock interval the
/// worst sample measured. The frame-time delta is written in `Update` from
/// `Time<Real>`, whose value `time_system` (in `First`) computed as the span
/// between the previous iteration's anchor and this one — so a sample always
/// carries the *previous* iteration's wall time, while the tick count read
/// beside it belongs to this one. Pairing a sample at index `i` with
/// `tick_deltas[i - 1]` puts both on the same interval: the ticks the
/// creature chain executed in iteration `i - 1` are exactly the ticks whose
/// execution cost that interval includes. The first sample of a window
/// measures a settle frame's interval and pairs with nothing, so it reads
/// off-tick.
fn worst_frame_on_tick(tick_deltas: &[u32], worst_index: usize) -> bool {
    worst_index > 0 && tick_deltas[worst_index - 1] > 0
}

/// The gate verdict of one run: pass = average and p95 both at or under
/// [`FRAME_GATE_LIMIT_MS`]. p99 and the worst frame are reported, never gate
/// — a number that shows a problem opens its own optimization ticket (#98).
fn frame_gate_passes(stats: &FrameStats) -> bool {
    stats.average <= FRAME_GATE_LIMIT_MS && stats.p95 <= FRAME_GATE_LIMIT_MS
}

/// The exit a benchmark run asks for: `Success` on a pass, `Error(1)` on a
/// fail — the verdict must reach the process exit code, or `cargo run` and
/// `just viewer-gate` would succeed through a failed gate.
fn gate_exit(stats: &FrameStats) -> AppExit {
    if frame_gate_passes(stats) {
        AppExit::Success
    } else {
        AppExit::error()
    }
}

/// Benchmark phases — settle the pipelines and the camera, measure, exit.
#[derive(Clone, Copy, Default, PartialEq)]
enum BenchmarkPhase {
    #[default]
    Settle,
    Measure,
    Done,
}

/// Drives `--benchmark N` / `--benchmark-secs S` — reuses the screenshot
/// path's settle window, then samples the frame time until the bound closes
/// the window and prints two machine-readable lines:
/// `BENCH map=17 frames=600 avg_ms=.. p95_ms=.. p99_ms=.. max_ms=..
/// max_at=.. max_on_tick=.. ticks=.. creatures=.. walking=.. labels=..
/// sprites=.. light_materials=..` and `GATE map=.. pass=.. avg_ms=..
/// p95_ms=.. limit_ms=33.3`, followed by the verdict's exit —
/// `AppExit::Success` on a pass, `AppExit::Error(1)` on a fail, so `cargo
/// run` and `just viewer-gate` carry the verdict to their own exit codes. No
/// screenshot is taken. `max_ms` is the worst measured frame (the "worst
/// frame" measurement of docs/rust.md); `max_at` is its 1-based position in
/// the measured window and `max_on_tick` says whether a simulation tick
/// executed inside the wall-clock interval that sample measured — the two
/// attributes the frame-time gate's cause attribution reads. `ticks` is how
/// far the simulation advanced across the measured window — snapshotted when
/// the window opens, so the settle frames' clock does not leak in — and
/// `creatures` / `walking` are the population it ended on; together they say
/// how much movement the window covered. `labels` echoes
/// [`labels_enabled`], so a recorded line carries the knob state it ran
/// with.
///
/// The samples are read from the `DiagnosticsStore` value
/// [`FrameTimeDiagnosticsPlugin`] writes each Update — the latest raw
/// wall-clock delta, not the plugin's smoothed value; the same wall-clock
/// frame time the diagnostics panel shows. That delta is `Time<Real>`'s
/// value, which `time_system` computes in `First` as the span between the
/// previous iteration's anchor and this one — a sample therefore always
/// carries the *previous* iteration's wall time, and the tick counter read
/// in the same iteration belongs to *this* one. The system is ordered after
/// the plugin's `diagnostic_system` and after the creature chain, so the
/// sample and the counter are both complete for the iteration that reads
/// them; the print pairs a sample with the previous iteration's counter
/// ([`worst_frame_on_tick`]), which puts both sides of `max_on_tick` on the
/// same interval — no ±1 ambiguity on either side. The percentiles are
/// nearest-rank over the samples. A `Seconds` bound closes the window when
/// the sum of the samples crosses the target, so the run is 60 seconds of
/// measured frame time whatever the hardware delivers.
#[allow(clippy::too_many_arguments)]
fn benchmark(
    request: Res<BenchmarkRequest>,
    stats: Res<Stats>,
    viewer: Res<Viewer>,
    sim: Res<CreatureSim>,
    pool: Option<Res<LightingMaterials>>,
    diagnostics: Res<DiagnosticsStore>,
    pieces: Query<(), With<ScenePiece>>,
    mut commands: Commands,
    mut phase: Local<BenchmarkPhase>,
    mut frames: Local<u32>,
    mut samples: Local<Vec<f64>>,
    mut tick_deltas: Local<Vec<u32>>,
    mut elapsed_ms: Local<f64>,
    mut ticks_at_open: Local<u64>,
    mut last_ticks: Local<u64>,
) {
    match *phase {
        BenchmarkPhase::Settle => {
            if *frames >= SETTLE_FRAMES {
                samples.clear();
                tick_deltas.clear();
                *elapsed_ms = 0.0;
                *ticks_at_open = sim.sim_ticks;
                *last_ticks = sim.sim_ticks;
                *frames = 0;
                *phase = BenchmarkPhase::Measure;
            }
        }
        BenchmarkPhase::Measure => {
            if let Some(value) = diagnostics
                .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
                .and_then(|frame_time| frame_time.value())
            {
                samples.push(value);
                // The ticks the creature chain executed during this
                // iteration. A rebuild would reset `sim_ticks`, but a
                // benchmark run has no input and no rebuild — the counter is
                // monotonic within the window. The print pairs a sample with
                // the previous iteration's count: the sample's delta carries
                // the previous iteration's wall time (`worst_frame_on_tick`).
                let delta = sim.sim_ticks.saturating_sub(*last_ticks);
                tick_deltas.push(delta.min(u32::MAX as u64) as u32);
                *last_ticks = sim.sim_ticks;
                *elapsed_ms += value;
            }
            let window_full = match request.bound {
                BenchmarkBound::Frames(count) => samples.len() >= count as usize,
                BenchmarkBound::Seconds(secs) => *elapsed_ms >= secs as f64 * 1000.0,
            };
            if window_full && !samples.is_empty() {
                let measured = frame_stats(&samples);
                let worst_index = worst_frame_index(&samples);
                let walking_states = sim.sim.as_ref().map(Simulation::states);
                let walking = walking_states
                    .as_deref()
                    .map(|states| {
                        states
                            .iter()
                            .filter(|state| state.walk_state == WalkState::Walk)
                            .count()
                    })
                    .unwrap_or(0);
                let map = stats
                    .map
                    .or_else(|| viewer.maps.get(viewer.selected).copied())
                    .map_or_else(
                        || "?".to_owned(),
                        |map| match map.source {
                            MapSource::Game => map.number.to_string(),
                            MapSource::Tutorial => format!("t{}", map.number),
                        },
                    );
                println!(
                    "BENCH map={map} frames={} avg_ms={:.2} p95_ms={:.2} p99_ms={:.2} \
                     max_ms={:.2} max_at={} max_on_tick={} ticks={} creatures={} walking={} \
                     labels={} sprites={} light_materials={}",
                    samples.len(),
                    measured.average,
                    measured.p95,
                    measured.p99,
                    measured.worst,
                    worst_index + 1,
                    u32::from(worst_frame_on_tick(&tick_deltas, worst_index)),
                    sim.sim_ticks - *ticks_at_open,
                    walking_states.as_deref().map_or(0, <[CreatureState]>::len),
                    walking,
                    labels_enabled() as u8,
                    pieces.iter().count(),
                    pool.as_ref().map_or(0, |pool| pool.len()),
                );
                println!(
                    "GATE map={map} pass={} avg_ms={:.2} p95_ms={:.2} limit_ms={FRAME_GATE_LIMIT_MS}",
                    frame_gate_passes(&measured),
                    measured.average,
                    measured.p95,
                );
                commands.write_message(gate_exit(&measured));
                *phase = BenchmarkPhase::Done;
            }
        }
        BenchmarkPhase::Done => {}
    }
    *frames += 1;
}

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

/// The single switch of the simulation's movement (`Play`): [sync_playboxes]
/// finds the checkbox by it to mirror [Viewer::playback] into.
#[derive(Component, Clone, Copy, Default)]
struct PlaybackToggle;

/// The `Constant day` switch: [sync_daynight_boxes] finds the checkbox by it
/// to mirror [Viewer::constant_day] into.
#[derive(Component, Clone, Copy, Default)]
struct DayNightToggle;

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
    /// Tick the lighting is pinned to (`--light-tick`); without it the lighting
    /// follows the scene clock every frame.
    pub lighting_tick: Option<u64>,
    /// Open with the day/night cycle held at its day value (`--constant-day`);
    /// the panel switch and `N` flip it live.
    pub constant_day: bool,
    /// Capture a screenshot to this path and exit.
    pub screenshot: Option<PathBuf>,
    /// Run this many measured frames after the settle window, print one
    /// `BENCH …` line to stdout and exit (`--benchmark`).
    pub benchmark: Option<u32>,
    /// Run the measured window until this many seconds of frame time have
    /// accumulated, print one `BENCH …` line to stdout and exit
    /// (`--benchmark-secs`); the frame count becomes an output. One of the
    /// two bounds, never both — the CLI rejects the pair up front.
    pub benchmark_secs: Option<u64>,
}

/// The viewer window: runs until the user closes it. Returns the exit the
/// app asked for — a benchmark run's gate verdict rides on it.
pub fn run_viewer(config: ViewerConfig) -> AppExit {
    let labels = map_labels(&config.maps);
    let selected = config.selected;
    let capture_request = config
        .screenshot
        .as_ref()
        .map(|path| CaptureRequest {
            path: Some(path.clone()),
            terrain_pieces: 0,
        })
        .unwrap_or_default();
    let benchmark_request = config
        .benchmark_secs
        .map(|secs| BenchmarkRequest {
            bound: BenchmarkBound::Seconds(secs),
        })
        .or_else(|| {
            config.benchmark.map(|frames| BenchmarkRequest {
                bound: BenchmarkBound::Frames(frames),
            })
        });
    // A benchmark run only: the measurement must not be quantized to the
    // monitor's refresh quantum — with the default Fifo a frame sits on the
    // next vblank, so day and night both read as ~16.67 ms and the meter
    // reports the display's pace, not the scene's cost. Interactive runs keep
    // the default.
    let present_mode = benchmark_request
        .as_ref()
        .map(|_| PresentMode::AutoNoVsync)
        .unwrap_or_default();
    // LIGHTS.RES is read before the game moves into its resource
    let setup = LightingSetup {
        tick: config.lighting_tick,
        masks: lights_res_masks(&config.game),
    };
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: format!("re-konung viewer — {}", config.game_dir.display()),
                    resolution: (1280, 800).into(),
                    present_mode,
                    ..default()
                }),
                ..default()
            })
            // Пиксельная графика: без сглаживания при дробном масштабе
            .set(ImagePlugin::default_nearest()),
    )
    .add_plugins((
        KonungUiPlugin,
        MapCameraPlugin,
        // Тик игры: на нём меняются кадры объектов, персонажей и огней
        AnimPlugin::new(TICK_MS as f32),
        // Составные спрайты существ, их клипы и синхронизация кадров: та же
        // подсистема, что поднимает существ в проигрывателе (#70)
        CreatureAnimationPlugin,
        // Подсветка сцены: материалы с масками LIGHTS.RES и контракт времени
        KonungLightingPlugin,
        FrameTimeDiagnosticsPlugin::default(),
    ))
    .insert_resource(GameResource(config.game))
    .init_resource::<TextureCache>()
    .init_resource::<ActiveRoot>()
    .init_resource::<Stats>()
    .init_resource::<CreatureLabels>()
    .init_resource::<CaptureTarget>()
    .init_resource::<ScrubState>()
    .insert_resource(Rebuild { map: true })
    .insert_resource(Viewer {
        maps: config.maps.clone(),
        selected,
        overlays: DEFAULT_OVERLAYS,
        playback: playback_start(),
        constant_day: config.constant_day,
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
            day_night_hotkey,
            apply_playback,
            sync_playboxes,
            sync_daynight_boxes,
            apply_scrub,
            sync_scrub_slider,
            update_lighting_state,
            rebuild_scene,
            sync_groups,
            sync_creature_visibility,
            spawn_creature_labels,
            diagnostics,
        )
            .chain()
            // The frame's clock position must be settled — by the scrub as by
            // anything else — before the lighting reads it, before the
            // simulation catches up to it and before the creature systems read
            // it, and the map a rebuild asks for must be requested before the
            // shared subsystem builds it — never left to the scheduler's
            // placement of two unconstrained chains (review #68). The lighting
            // state itself is written before the plugin's uniform writer reads
            // it, or a day change would reach the GPU a frame late (review
            // #106).
            .after(AnimSet::Clock)
            .after(CameraInput)
            .before(CreatureSet::Prepare)
            .before(CameraApply)
            .before(update_lighting_uniforms),
    )
    // The scrub slider's label as clock hours: after the feathers value writer
    // (PreUpdate) and after the frame's clock position has settled, so the
    // face never lags a drag by a frame.
    .add_systems(PostUpdate, scrub_clock_label)
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
    // The lighting pool at startup; the per-frame state runs in the chained
    // systems above, after the scrub has settled the clock. The capture
    // controller runs after the camera write-out so the zoom it picks reaches
    // the projection on the next frame's apply.
    .add_systems(Startup, setup_lighting)
    .add_systems(
        Update,
        (
            capture
                .run_if(resource_exists::<CaptureRequest>)
                .run_if(resource_exists::<LightingMaterials>)
                .after(CameraApply),
            benchmark
                .run_if(resource_exists::<BenchmarkRequest>)
                // The sample and the tick counter must both be complete for
                // this iteration when they are read: after the diagnostics
                // plugin has written this frame's delta and after the
                // creature chain has advanced the tick counter. The delta
                // itself carries the previous iteration's wall time, so the
                // print pairs a sample with the previous iteration's count
                // (`worst_frame_on_tick`).
                .after(FrameTimeDiagnosticsPlugin::diagnostic_system)
                .after(CreatureSet::Nodes),
        ),
    );
    app.insert_resource(setup);
    if capture_request.path.is_some() {
        app.insert_resource(capture_request);
    }
    if let Some(request) = benchmark_request {
        app.insert_resource(request);
    }
    app.run()
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
            @FeathersCheckbox {
                @caption: bsn! { @caption("Constant day") }
            }
            DayNightToggle
            on(change_day_night)
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
            @caption("Time of day")
            --
            @FeathersSlider {
                @min: 0.0,
                @max: {(CYCLE_TICKS - 1) as f32},
            }
            // Keyboard and track steps of one displayed minute; the drag itself
            // rounds to whole ticks (`SliderPrecision` defaults to 0)
            SliderStep(15.0)
            ScrubSlider
            on(change_scrub)
            --
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(6.0),
            }
            Children[
                @FeathersButton {
                    @caption: bsn! { @caption("Light 07:00") }
                }
                ScrubPreset(900)
                on(jump_to_preset)
                --
                @FeathersButton {
                    @caption: bsn! { @caption("Warm 09:00") }
                }
                ScrubPreset(2700)
                on(jump_to_preset)
            ]
            --
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(6.0),
            }
            Children[
                @FeathersButton {
                    @caption: bsn! { @caption("Ground 18:13") }
                }
                ScrubPreset(11000)
                on(jump_to_preset)
                --
                @FeathersButton {
                    @caption: bsn! { @caption("Ramp 03:04") }
                }
                ScrubPreset(18960)
                on(jump_to_preset)
            ]
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

/// The `Constant day` switch writes state only — like `Play`: the scene is
/// not rebuilt (`Rebuild` stays down), the clock does not move, time runs.
/// The checkbox is mirrored by [sync_daynight_boxes] — otherwise the `N`
/// hotkey would not reach the panel.
fn change_day_night(
    change: On<ValueChange<bool>>,
    toggles: Query<&DayNightToggle>,
    mut viewer: ResMut<Viewer>,
) {
    if toggles.get(change.source).is_err() {
        return;
    }
    viewer.constant_day = change.value;
}

/// Отмечен ли единственный переключатель при этом состоянии: галочка стоит,
/// пока симуляция идёт.
fn playback_checked(viewer: &Viewer) -> bool {
    viewer.playback == PlayState::Playing
}

/// Whether the `Constant day` switch is checked at this state: the box is
/// checked while the cycle is held off.
fn constant_day_checked(viewer: &Viewer) -> bool {
    viewer.constant_day
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

/// Mirrors the constant-day flag into the `Constant day` checkbox — the same
/// pattern as [sync_playboxes]: a `Checkbox` never writes `Checked` itself,
/// and both the click and the `N` hotkey write the state.
fn sync_daynight_boxes(
    viewer: Res<Viewer>,
    boxes: Query<(Entity, &DayNightToggle, Has<Checked>)>,
    mut commands: Commands,
) {
    for (entity, _, checked) in &boxes {
        let want = constant_day_checked(&viewer);
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

/// Key `N` — constant day on/off. Panning (WASD and the arrows), `Space`,
/// `.` and `,` never use it.
fn day_night_hotkey(keys: Res<ButtonInput<KeyCode>>, mut viewer: ResMut<Viewer>) {
    if keys.just_pressed(KeyCode::KeyN) {
        viewer.constant_day = toggle_day_night(viewer.constant_day);
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
    lighting: Option<Res<LightingMaterials>>,
    mut commands: Commands,
    mut active: ResMut<ActiveRoot>,
    mut stats: ResMut<Stats>,
    mut pending: ResMut<PendingFrame>,
    mut labels: ResMut<CreatureLabels>,
    mut motion: ResMut<CreatureMotion>,
    mut visible: ResMut<CreatureNodesVisible>,
    target: Option<ResMut<CaptureTarget>>,
    capture: Option<ResMut<CaptureRequest>>,
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
        lighting.as_deref(),
        &mut commands,
        &mut pending,
    ) {
        Ok(loaded) => loaded,
        Err(error) => {
            *stats = failed(map_ref, cache.len(), error.to_string(), started.elapsed());
            return;
        }
    };
    // The fine grid of the map just loaded: what the creature lighting reads.
    // Maps without LIGHTS.N insert `None` — every cell dark.
    commands.insert_resource(FineLightGrid(map_fine_lights(&game.0, map_ref)));
    // aim the capture camera at the first lit terrain tile and count the
    // terrain pieces for the exit report
    if let (Some(mut capture), Some(mut target)) = (capture, target)
        && capture.path.is_some()
    {
        capture.terrain_pieces = scene
            .pieces
            .iter()
            .filter(|piece| piece.group == PieceGroup::Terrain && piece.creature_id.is_none())
            .count();
        let lit = scene.pieces.iter().find(|piece| {
            piece.group == PieceGroup::Terrain
                && piece.light_mask_index.is_some()
                && piece.creature_id.is_none()
        });
        if let Some(piece) = lit {
            let size = piece
                .texture
                .as_ref()
                .and_then(|id| cache.get(id))
                .map_or(UVec2::ZERO, |entry| entry.size);
            let size = piece_size(piece, size);
            let center = piece_center(piece.x as f32, piece.y as f32, size.x, size.y);
            target.0 = Some(center);
            // The whole-map framing stays in bounds; only the center moves
            pending.center = Some(center);
        }
    }
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
    // The viewer has no hero picker, so it asks for the first hero (`None`), the
    // same choice its `SceneOptions::ALL` scene resolved to (review #101, P2-2).
    motion.request(map_ref, SIM_SEED, None, scene);
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
/// Движение пересоздаётся с тем же зерном ([`SIM_SEED`]): маршруты, состояния
/// походки и ГСЧ свежие. Машина анимации сохраняется — спец-анимация state-4,
/// идущая в момент сброса, продолжается с текущего кадра, а не начинается
/// заново (обзор #101, P2-#3): сброс позиций не есть сброс анимации. Карта,
/// камера, масштаб, выбранный слой и пауза не трогаются.
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
    reset_creature_positions(&game.0, map_ref, SIM_SEED, clock.ticks, &mut sim);
}

/// Кнопка "Reset animation" (обзор #101, P2-#3): заново запускает анимации с
/// нулевого кадра, не трогая ни позиции существ, ни игровые часы. Отличается от
/// полного сброса (`Play → Off → Play`, issue #70 AC4): тот пересобирает
/// симуляцию и возвращает всех в клетки загрузки, а эта кнопка лишь отматывает
/// машину анимации — спец state-4 начинается с кадра 0, а координаты и
/// накопленное время симуляции остаются. Карта, камера, часы и пауза не
/// трогаются.
fn reset_animation(
    activate: On<Activate>,
    markers: Query<&ResetAnimation>,
    mut sim: ResMut<CreatureSim>,
) {
    if markers.get(activate.entity).is_err() {
        return;
    }
    // Only the machine rewinds: positions and the simulation clock are kept, and
    // the animator re-anchors on the same clip through the special-frame
    // regression the subsystem's `observe_specials` reports (review of #101, P2-#3).
    reset_creature_animations(&mut sim);
}

const LABEL_OFFSET_Y: f32 = 50.0;

/// The label z: above every map piece (`z = draw order · kn_render::Z_STEP`,
/// at most ~172 on the largest shipped map — 17 148 pieces on RUS 37) yet
/// inside the 2D camera's clip range (±1000, `OrthographicProjection::default_2d`).
/// The old 9999 sat beyond the far plane, so the whole label layer was
/// silently clipped away — no label ever reached a pixel (issue #84).
const LABEL_Z: f32 = 500.0;

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
    fonts: Res<KonungFonts>,
    mut labels: ResMut<CreatureLabels>,
    viewer: Res<Viewer>,
    mut commands: Commands,
) {
    if !labels_enabled() || labels.labels_spawned || sim.sim.is_none() {
        return;
    }
    let simulation = sim.sim.as_ref().unwrap();
    let states = simulation.states();
    // Finished strings from the game layer (review #116): the names were built
    // at simulation load out of the K1 texts; the frontend never reads them.
    let names = simulation.labels();
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
        let text = names
            .get(&state.id)
            .cloned()
            .unwrap_or_else(|| format!("#{}", state.id));
        let label = commands
            .spawn((
                CreatureLabel {
                    creature_id: state.id,
                },
                Text2d(text),
                // The bundled PT Sans, not Bevy's embedded default: RUS names
                // are Cyrillic and the default face draws boxes for them. The
                // same face the panel renders with (issue #84 AC1).
                TextFont {
                    font: fonts.regular.clone().into(),
                    font_size: FontSize::Px(11.0),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 1.0, 0.6)),
                Transform::from_xyz(px as f32, -(py as f32) + LABEL_OFFSET_Y, LABEL_Z),
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
        // World y points up: the offset lifts the label over the body image
        // on screen. A subtraction dropped it below the creature's feet.
        transform.translation.y = y + LABEL_OFFSET_Y;
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

/// The game-time readout: clock face, phase, day index and progress through the cycle.
///
/// Every part of it is a function of the scene clock's tick — the single time
/// source the lighting follows and the simulation is caught up to on a scrub
/// release — so the panel cannot disagree with the picture. The simulation's
/// own tick is not read here: mid-drag it lags the clock by design. `None`
/// (a map whose simulation failed to build) shows nothing: a clock of its own
/// would read 06:00 over a frozen scene and look like a running day. The four
/// phase words are our reading of the cycle, not a property of the original;
/// the derivation and the 06:00 offset are recorded in `docs/k1/time-of-day.md`.
fn game_time_line(tick: Option<u64>) -> String {
    let Some(tick) = tick else {
        return String::new();
    };
    let time = k1_game::time_of_day::game_time(tick);
    format!(
        "\nclock {:02}:{:02} · {} · day {} of the cycle · {}%",
        time.clock_hour,
        time.clock_minute,
        phase_label(time.phase),
        time.day,
        time.cycle_progress_percent,
    )
}

/// The scrub slider's value label, rewritten as clock hours.
///
/// `bevy_feathers` prints a slider's value as the bare number — it has no value
/// formatter (0.20.1, `controls/slider.rs:356-370`) — which for the time scrub
/// is a raw tick. The adapter below owns that widget knowledge in one place: it
/// finds the first `Text` descendant of the scrub slider and rewrites it to
/// `HH:MM` of the scene clock — the same source the lighting follows, so the
/// label cannot disagree with the picture or the presets (`Warm 09:00`). The
/// write goes one way: the slider's own `SliderValue` is never touched, and no
/// text outside the slider is considered.
///
/// PostUpdate: the feathers writer runs in PreUpdate (`update_slider_pos`), so
/// the clock face wins every frame; the write happens only on a change, so an
/// untouched frame costs one comparison.
fn scrub_clock_label(
    slider: Single<Entity, With<ScrubSlider>>,
    children: Query<&Children>,
    mut texts: Query<&mut Text>,
    clock: Res<Clock>,
) {
    let Some(entity) = slider_value_text(*slider, &children, &texts) else {
        return;
    };
    let Ok(mut text) = texts.get_mut(entity) else {
        return;
    };
    let time = k1_game::time_of_day::game_time(clock.ticks);
    let label = format!("{:02}:{:02}", time.clock_hour, time.clock_minute);
    if text.0 != label {
        text.0 = label;
    }
}

/// The first `Text` entity under the slider — the widget nests exactly one
/// value label. A seam of its own so a widget change touches one function.
/// The search reads through the write query's shared borrow, so the system
/// stays a single-query one.
fn slider_value_text(
    root: Entity,
    children: &Query<&Children>,
    texts: &Query<&mut Text>,
) -> Option<Entity> {
    for child in children.get(root).ok()?.iter() {
        if texts.get(child).is_ok() {
            return Some(child);
        }
        if let Some(found) = slider_value_text(child, children, texts) {
            return Some(found);
        }
    }
    None
}

/// The phase word shown to the player — a display concern: `k1-game` keeps ticks and
/// intervals, the words live here.
fn phase_label(phase: DayPhase) -> &'static str {
    match phase {
        DayPhase::Day => "day",
        DayPhase::Dusk => "dusk",
        DayPhase::Night => "night",
        DayPhase::Dawn => "dawn",
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
            // The readout follows the Clock; the simulation's presence only
            // gates it ([game_time_line]).
            let time = game_time_line(sim.sim.is_some().then_some(clock.ticks));
            format!(
                "{} {} · {pieces} · {} textures · {} ms{problems}{sim_error}{creatures}\nzoom ×{} · {} fps{playback}{time}",
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
    use std::num::NonZero;
    use std::sync::Arc;

    use super::*;
    use kn_game::simulation::{CreatureState, Simulation, WalkState};
    use kn_game::{GameTime, LayerKey, PieceGroup};
    use kn_render::{
        Animator, Clip, CreatureAnim, CreatureDepth, CreatureSpawns, LayerSequences, Sequence,
        advance_creature_sim, build_creature_anim, build_creature_spawns, build_depth_map,
        reset_creature_sim, select_clip, spawn_creature_nodes, sync_creature_nodes,
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

    /// The resting clip must be anchored exactly where the node draws the
    /// piece, or a creature would be born displaced and only the swaps would
    /// read correct. Both come from one roster pose build — the clips from
    /// `creature_animations`, the node geometry from `Scene::creature_pieces` —
    /// so they must agree on every real creature, including a roaming member the
    /// static draw list omits (data test, skips without `original/`).
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
                .creature_pieces
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
            depth_bottom: None,
            light_mask_index: None,
            self_lit: false,
            fine_light_value: 0,
        };
        let scene = kn_game::Scene {
            creature_pieces: Vec::new(),
            creature_origins: std::collections::HashMap::new(),
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

    #[test]
    fn frame_stats_are_nearest_rank_over_the_samples() {
        let samples: Vec<f64> = (1..=100).map(f64::from).collect();
        let measured = frame_stats(&samples);
        assert_eq!(measured.average, 50.5);
        // Nearest-rank: ceil(100 * fraction) picks the 95th and 99th sample
        assert_eq!(measured.p95, 95.0);
        assert_eq!(measured.p99, 99.0);
        assert_eq!(measured.worst, 100.0);
        // The average is over the window, not the sorted tail
        let spike = [3.0, 1.0, 2.0, 40.0];
        let measured = frame_stats(&spike);
        assert!((measured.average - 11.5).abs() < 1e-9);
        assert_eq!(measured.worst, 40.0);
    }

    #[test]
    fn frame_stats_of_an_empty_window_are_zeros() {
        // A window that closed before any sample cannot print a number
        assert_eq!(
            frame_stats(&[]),
            FrameStats {
                average: 0.0,
                p95: 0.0,
                p99: 0.0,
                worst: 0.0,
            }
        );
    }

    #[test]
    fn worst_frame_index_points_at_the_first_maximum() {
        let spike = [1.0, 7.5, 2.0, 7.5, 3.0];
        assert_eq!(worst_frame_index(&spike), 1, "first of the equal maxima");
        assert_eq!(worst_frame_index(&[4.2]), 0);
        assert_eq!(worst_frame_index(&[]), 0);
    }

    #[test]
    fn the_gate_passes_only_when_average_and_p95_fit_the_limit() {
        // 30 FPS: one frame every 33.3 ms; the boundary is inclusive
        let fits = FrameStats {
            average: 33.3,
            p95: 33.3,
            p99: 90.0,
            worst: 120.0,
        };
        assert!(
            frame_gate_passes(&fits),
            "p99 and the worst frame are reported, never gate"
        );
        let slow_average = FrameStats {
            average: 33.31,
            p95: 30.0,
            ..fits
        };
        assert!(!frame_gate_passes(&slow_average));
        let slow_p95 = FrameStats {
            average: 30.0,
            p95: 33.4,
            ..fits
        };
        assert!(!frame_gate_passes(&slow_p95));
    }

    #[test]
    fn a_failed_gate_exits_the_process_with_an_error() {
        // The printed GATE line is not enough: `just viewer-gate` and `cargo
        // run` read the verdict from the exit code
        let fits = FrameStats {
            average: 10.0,
            p95: 12.0,
            p99: 15.0,
            worst: 20.0,
        };
        assert_eq!(gate_exit(&fits), AppExit::Success);
        let fails = FrameStats {
            average: 40.0,
            ..fits
        };
        assert_eq!(gate_exit(&fails), AppExit::Error(NonZero::new(1).unwrap()));
    }

    #[test]
    fn the_worst_frame_pairs_with_the_previous_iterations_ticks() {
        // A sample carries the previous iteration's wall time, so the flag
        // reads the tick count of the iteration before it: a tick executed
        // in iteration i is seen by the sample at i + 1, not at i
        let ticks = [0, 0, 1, 0];
        assert!(
            worst_frame_on_tick(&ticks, 3),
            "the tick of iteration 2 lands in the sample at 3"
        );
        assert!(
            !worst_frame_on_tick(&ticks, 2),
            "the sample at 2 measures iteration 1, which ran no tick"
        );
        assert!(
            !worst_frame_on_tick(&ticks, 0),
            "the first sample measures a settle frame's interval and pairs with nothing"
        );
        assert!(!worst_frame_on_tick(&[1, 1], 0));
    }

    fn viewer(overlays: [bool; 4]) -> Viewer {
        Viewer {
            maps: vec![],
            selected: 0,
            overlays,
            playback: DEFAULT_PLAYBACK,
            constant_day: DEFAULT_CONSTANT_DAY,
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
    fn the_benchmark_knobs_default_to_the_shipped_behavior() {
        // The "Viewer costs" knobs (docs/rust.md) change a run only when the
        // var is set, and `0` is the only off value: absent means shipped
        // behavior, and any other value means the factor is deliberately on.
        assert!(knob_value(None, true), "absent keeps the default");
        assert!(!knob_value(None, false), "absent keeps the default");
        assert!(!knob_value(Some("0"), true), "`0` opts out");
        assert!(!knob_value(Some("0"), false));
        assert!(knob_value(Some("1"), false), "any other value opts in");
        assert!(knob_value(Some("on"), false));
        // The factors themselves: labels on, playback playing, unless asked.
        assert!(labels_enabled() || std::env::var("KN_VIEWER_LABELS").as_deref() == Ok("0"));
        assert_eq!(
            playback_start(),
            if std::env::var("KN_VIEWER_PAUSE").as_deref() == Ok("1") {
                PlayState::Paused
            } else {
                DEFAULT_PLAYBACK
            },
            "the run starts playing unless KN_VIEWER_PAUSE=1"
        );
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

    // --- Constant day (#97) ---------------------------------------------------

    #[test]
    fn default_constant_day_is_off() {
        // Agreed with the missing `Checked` on the Constant day checkbox in
        // panel(): the default is the cycle on, the original's saved-options
        // default is unconfirmed (docs/k1/time-of-day.md, "The four globals").
        let state = viewer(DEFAULT_OVERLAYS);
        assert!(!state.constant_day);
        assert!(!constant_day_checked(&state));
        let mut on = viewer(DEFAULT_OVERLAYS);
        on.constant_day = true;
        assert!(constant_day_checked(&on));
    }

    #[test]
    fn n_toggles_the_day_night_cycle_both_ways() {
        assert!(toggle_day_night(false));
        assert!(!toggle_day_night(true));
    }

    /// The checkbox is written by the state mirror, not by the widget or the
    /// handler: a Bevy `Checkbox` never sets `Checked` itself, and both the
    /// click and the `N` hotkey write the state — the `Play` checkbox's
    /// pattern.
    #[test]
    fn the_constant_day_checkbox_mirrors_the_state() {
        let mut app = App::new();
        app.insert_resource(viewer([false; 4]));
        let box_entity = app.world_mut().spawn(DayNightToggle).id();
        app.add_systems(Update, sync_daynight_boxes);

        app.update();
        assert!(
            app.world().entity(box_entity).get::<Checked>().is_none(),
            "the default is the cycle: no check"
        );

        app.world_mut().resource_mut::<Viewer>().constant_day = true;
        app.update();
        assert!(app.world().entity(box_entity).get::<Checked>().is_some());

        app.world_mut().resource_mut::<Viewer>().constant_day = false;
        app.update();
        assert!(app.world().entity(box_entity).get::<Checked>().is_none());
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
                constant_day: false,
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

    // --- Game time -----------------------------------------------------------

    /// The simulation is driven by the clock only — the system under test is the
    /// one the playback controls feed, so nothing can move game time by hand.
    fn time_app(game: Game, map_ref: MapRef, clock_ticks: u64) -> App {
        let simulation = game.simulation(map_ref, SIM_SEED).ok();
        let initial_pixels = simulation.as_ref().unwrap().initial_pixels();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(GameResource(game))
            .insert_resource(Viewer {
                maps: vec![map_ref],
                selected: 0,
                overlays: DEFAULT_OVERLAYS,
                playback: PlayState::Paused,
                constant_day: false,
            })
            .insert_resource(CreatureSim {
                sim: simulation,
                initial_pixels,
                ..Default::default()
            })
            .insert_resource(CreatureMotion {
                map: Some(map_ref),
                seed: SIM_SEED,
                ..Default::default()
            })
            .insert_resource(Clock::new(TICK_MS as f32))
            .add_systems(Update, advance_creature_sim);
        app.world_mut().resource_mut::<Clock>().ticks = clock_ticks;
        app
    }

    fn time_of(app: &App) -> GameTime {
        app.world()
            .resource::<CreatureSim>()
            .sim
            .as_ref()
            .unwrap()
            .game_time()
    }

    /// The readout is the model's answer for the scene clock's tick — the single
    /// time source — so day and position always agree with the lighting and the
    /// scrubbed thumb: 317 ticks are window 1 of `docs/k1/time-of-day.md` —
    /// neutral ambient, no ground light, 21 minutes past the 06:00 our clock
    /// face starts the cycle at; a full day on the clock shows as day 1.
    #[test]
    fn the_time_line_reads_the_clock_tick() {
        assert_eq!(
            game_time_line(Some(317)),
            "\nclock 06:21 · day · day 0 of the cycle · 1%"
        );
        assert_eq!(
            game_time_line(Some(CYCLE_TICKS + 317)),
            "\nclock 06:21 · day · day 1 of the cycle · 1%"
        );
    }

    /// A map whose simulation failed to build shows no time at all: a clock of
    /// its own would read 06:00 over a frozen scene and look like a running day.
    #[test]
    fn a_map_without_a_simulation_shows_no_time() {
        assert_eq!(game_time_line(None), "");
    }

    /// The four words are ours, not the original's (`docs/k1/time-of-day.md`), so
    /// the panel is pinned to exactly the phases the model derives.
    #[test]
    fn the_panel_names_the_four_phases_we_derived() {
        let words = [
            DayPhase::Day,
            DayPhase::Dusk,
            DayPhase::Night,
            DayPhase::Dawn,
        ]
        .map(phase_label);
        assert_eq!(words, ["day", "dusk", "night", "dawn"]);
    }

    /// `Space` pauses phase and movement together and `.`/`,` step both, because
    /// there is one thing to move: the simulation. With the clock still, an idle
    /// frame changes neither positions nor game time; the ticks the clock gains
    /// move both by the same amount.
    #[test]
    fn game_time_moves_only_with_the_clock_that_moves_the_creatures() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let mut app = time_app(Game::open(&dir).unwrap(), map_ref, 200);
        app.update();

        let before = time_of(&app);
        let homes = {
            let sim = app.world().resource::<CreatureSim>();
            home(sim.sim.as_ref().unwrap())
        };
        app.update();
        app.update();
        assert_eq!(time_of(&app), before, "an idle frame advances no game time");
        let sim = app.world().resource::<CreatureSim>();
        assert_eq!(
            home(sim.sim.as_ref().unwrap()),
            homes,
            "an idle frame advances no movement either"
        );

        app.world_mut().resource_mut::<Clock>().ticks = 203;
        app.update();
        assert_eq!(
            time_of(&app).cycle_tick,
            before.cycle_tick + 3,
            "the same three ticks moved the creatures and the day"
        );
    }

    /// After a map change the tracker is pinned to the clock, so the fresh
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
                constant_day: false,
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

    /// The viewer's own contract: switching to another map installs a fresh
    /// simulation and its day begins with it — game time is derived from that
    /// simulation's own tick, so there is no second accumulator on the tracker or
    /// in the viewer to carry a running day across. This is a tool convention for
    /// the viewer's independent scenes, not a game rule: a game-side map
    /// transition in the Player keeps the session's day (docs/k1/time-of-day.md,
    /// "The day travels through a game transition").
    #[test]
    fn switching_to_another_map_recreates_the_day_with_the_new_simulation() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();
        let mut app = time_app(game, map_ref, 400);
        app.update();
        assert_eq!(time_of(&app).cycle_tick, 400, "the first map runs a day");

        // The map-change path: a fresh simulation, the epoch pinned to the clock.
        let fresh = app
            .world()
            .resource::<GameResource>()
            .0
            .simulation(map_ref, SIM_SEED)
            .unwrap();
        app.world_mut().resource_mut::<CreatureSim>().sim = Some(fresh);
        let clock_ticks = app.world().resource::<Clock>().ticks;
        app.world_mut().resource_mut::<CreatureSim>().epoch = clock_ticks;
        app.world_mut().resource_mut::<CreatureSim>().sim_ticks = 0;
        app.update();

        let time = time_of(&app);
        assert_eq!(
            (time.day, time.cycle_tick),
            (0, 0),
            "the new map starts its own day at the epoch"
        );
        app.world_mut().resource_mut::<Clock>().ticks = clock_ticks + 5;
        app.update();
        assert_eq!(
            time_of(&app).cycle_tick,
            5,
            "and continues from there alone"
        );
    }

    /// A map change keeps the ambient correct: the state the lighting pool is
    /// driven from stays `params(clock.ticks, seed)` — the Clock never resets
    /// on a switch, and the fresh simulation's day begins at the epoch the
    /// switch pinned (the day-restart itself is
    /// [`switching_to_another_map_recreates_the_day_with_the_new_simulation`]
    /// ). The pool is game-wide (LIGHTS.RES), built once at startup, so what a
    /// switch could strand stale is the state, not the materials.
    #[test]
    fn switching_to_another_map_keeps_the_ambient_on_the_new_simulation_s_tick() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();
        let mut app = scrub_app(Some((game, map_ref)));
        app.world_mut().resource_mut::<Clock>().ticks = 2 * CYCLE_TICKS + 11000;
        app.update();
        assert_eq!(
            scrub_lighting(&app),
            lighting_state(lighting::params(2 * CYCLE_TICKS + 11000, SIM_SEED)),
            "the first map lights from the clock"
        );

        // The map-change path: a fresh simulation, the epoch pinned to the
        // clock; the ambient must not move off the clock's tick.
        let clock_ticks = app.world().resource::<Clock>().ticks;
        let fresh = app
            .world()
            .resource::<GameResource>()
            .0
            .simulation(map_ref, SIM_SEED)
            .unwrap();
        {
            let mut sim = app.world_mut().resource_mut::<CreatureSim>();
            sim.sim = Some(fresh);
            sim.epoch = clock_ticks;
            sim.sim_ticks = 0;
        }
        app.update();
        assert_eq!(
            scrub_lighting(&app),
            lighting_state(lighting::params(clock_ticks, SIM_SEED)),
            "the ambient stays on the clock across the switch"
        );

        app.world_mut().resource_mut::<Clock>().ticks = clock_ticks + 5;
        app.update();
        assert_eq!(
            scrub_lighting(&app),
            lighting_state(lighting::params(clock_ticks + 5, SIM_SEED)),
            "and follows it with the new simulation"
        );
    }

    // --- Constant day (#97) ---------------------------------------------------

    /// The click path: the observer writes the state only — no `Rebuild`, no
    /// Clock move; the lighting input flips on the next read (#97 AC1: no
    /// rebuild, so the map's entity count is unchanged), and the checkbox
    /// mirrors the new state through the sync system.
    #[test]
    fn the_day_night_click_writes_state_without_a_rebuild() {
        let mut app = scrub_app(None);
        app.world_mut().resource_mut::<Clock>().ticks = 11000;
        let box_entity = app.world_mut().spawn(DayNightToggle).id();
        app.update();

        app.world_mut().trigger(ValueChange {
            source: box_entity,
            value: true,
            is_final: true,
        });
        app.update();
        assert!(
            app.world().resource::<Viewer>().constant_day,
            "the click wrote the state"
        );
        assert!(
            app.world().entity(box_entity).get::<Checked>().is_some(),
            "the checkbox mirrors the state it wrote"
        );
        assert_eq!(
            scrub_lighting(&app),
            LightingState::default(),
            "the ambient holds the day value at a night tick"
        );
        assert_eq!(scrub_clock(&app), 11000, "the clock did not move");
        assert!(!app.world().resource::<Rebuild>().map, "no rebuild asked");
    }

    /// The hotkey path: `N` flips the same state the click writes, and the
    /// checkbox follows through the sync system, not the widget.
    #[test]
    fn the_n_hotkey_flips_the_switch_and_the_checkbox_follows() {
        let mut app = scrub_app(None);
        let box_entity = app.world_mut().spawn(DayNightToggle).id();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.add_systems(Update, day_night_hotkey.before(sync_daynight_boxes));

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyN);
        app.update();
        assert!(app.world().resource::<Viewer>().constant_day);
        assert!(app.world().entity(box_entity).get::<Checked>().is_some());

        // MinimalPlugins runs no input clearing: release the key by hand, the
        // next frame must not toggle it again.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.update();
        assert!(
            app.world().resource::<Viewer>().constant_day,
            "a held key toggles once, not every frame"
        );
    }

    /// #97 AC2 headless: with the switch on the ambient snapshot at a night
    /// boundary equals the one at a day boundary — one constant-day state —
    /// and switching back off restores the phase value at the same tick. The
    /// tick itself never moved: the flag holds only the lighting input.
    #[test]
    fn constant_day_holds_the_ambient_at_the_phase_boundaries() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();
        let mut app = scrub_app(Some((game, map_ref)));
        app.world_mut().resource_mut::<Viewer>().constant_day = true;

        // Night boundary (the ground-lit plateau) and day boundary (the light
        // span), under the switch.
        app.world_mut().resource_mut::<Clock>().ticks = 11000;
        app.update();
        let night = scrub_lighting(&app);
        app.world_mut().resource_mut::<Clock>().ticks = 900;
        app.update();
        assert_eq!(
            scrub_lighting(&app),
            night,
            "one lighting input through the whole cycle"
        );
        assert_eq!(
            night,
            LightingState::default(),
            "the constant day value is the neutral state: no ambient, no tile light"
        );

        // Switch back on at the same night tick: the visible cycle returns.
        app.world_mut().resource_mut::<Viewer>().constant_day = false;
        app.world_mut().resource_mut::<Clock>().ticks = 11000;
        app.update();
        assert_eq!(
            scrub_lighting(&app),
            lighting_state(lighting::params(11000, SIM_SEED)),
            "the cycle resumes from the tick where it stood"
        );
        assert_ne!(
            scrub_lighting(&app),
            night,
            "the night phase differs from the held day value"
        );
    }

    /// #97 AC2: the phase trace with the switch on equals the flag-on run —
    /// time keeps running. The simulation is caught up to the clock whatever
    /// the flag is, so both runs report the same day, cycle position, phase
    /// word and ground window at every tick across the ground window's edges.
    #[test]
    fn the_constant_day_run_keeps_the_phase_trace_of_the_cycle_on_run() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let phase_trace = |constant_day: bool, dir: &PathBuf, map_ref: MapRef| {
            let game = Game::open(dir).unwrap();
            let mut app = scrub_app(Some((game, map_ref)));
            app.world_mut().resource_mut::<Viewer>().constant_day = constant_day;
            // Install the simulation the catch-up walks: the clock is the
            // single time source, the sim follows it tick for tick.
            let simulation = app
                .world()
                .resource::<GameResource>()
                .0
                .simulation(map_ref, SIM_SEED)
                .unwrap();
            let initial_pixels = simulation.initial_pixels();
            app.insert_resource(CreatureSim {
                sim: Some(simulation),
                initial_pixels,
                ..Default::default()
            });
            let mut trace = Vec::new();
            for tick in 8090..=8110 {
                app.world_mut().resource_mut::<Clock>().ticks = tick;
                app.update();
                let time = app
                    .world()
                    .resource::<CreatureSim>()
                    .sim
                    .as_ref()
                    .unwrap()
                    .game_time();
                trace.push((time.day, time.cycle_tick, time.phase, time.ground_window));
            }
            trace
        };
        assert_eq!(
            phase_trace(true, &dir, map_ref),
            phase_trace(false, &dir, map_ref),
            "the switch must not move, stall or rewind the day"
        );
    }

    /// #97 AC3, the reset semantics: the switch is a viewer session setting,
    /// not simulation state. A map switch installs a fresh simulation, and the
    /// switch survives it — still holding the lighting input on the new map's
    /// standing tick; it resets only when the app restarts
    /// ([`DEFAULT_CONSTANT_DAY`]).
    #[test]
    fn the_constant_day_switch_survives_a_map_switch() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();
        let mut app = scrub_app(Some((game, map_ref)));
        app.world_mut().resource_mut::<Viewer>().constant_day = true;
        app.world_mut().resource_mut::<Clock>().ticks = 11000;
        app.update();

        // The map-change path: a fresh simulation, the epoch pinned to the
        // clock — `Viewer` is no part of it.
        let clock_ticks = app.world().resource::<Clock>().ticks;
        let fresh = app
            .world()
            .resource::<GameResource>()
            .0
            .simulation(map_ref, SIM_SEED)
            .unwrap();
        {
            let mut sim = app.world_mut().resource_mut::<CreatureSim>();
            sim.sim = Some(fresh);
            sim.epoch = clock_ticks;
            sim.sim_ticks = 0;
        }
        app.update();
        assert!(
            app.world().resource::<Viewer>().constant_day,
            "the switch outlives the simulation it does not belong to"
        );
        assert_eq!(
            scrub_lighting(&app),
            LightingState::default(),
            "the held lighting input stands on the new map too"
        );
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
                constant_day: false,
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
                constant_day: false,
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
                constant_day: false,
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
            // Assets for the bundled face: `init_asset` needs the asset
            // machinery, which MinimalPlugins alone does not carry (the
            // kn-ui headless setup does the same).
            .add_plugins(bevy::asset::AssetPlugin::default())
            .insert_resource(sim_resource)
            // `spawn_creature_labels` draws with the bundled face, like the
            // window does.
            .init_asset::<Font>();
        let fonts = {
            let mut font_assets = app.world_mut().resource_mut::<Assets<Font>>();
            kn_ui::konung_fonts(&mut font_assets)
        };
        app.insert_resource(fonts)
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
        let scene = game.scene(map_ref, &SceneOptions::ALL).unwrap();
        let (anim, report) = build_creature_anim(
            &game,
            map_ref,
            None,
            &scene.creature_pieces,
            &mut cache,
            &mut images,
        );
        println!("walk diagnostics: {report:?}");
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
            constant_day: false,
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
        let scene = game.scene(map_ref, &SceneOptions::ALL).unwrap();
        let (anim, _report) = build_creature_anim(
            &game,
            map_ref,
            None,
            &scene.creature_pieces,
            &mut cache,
            &mut images,
        );
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
            // Assets for the bundled face the labels spawn with: `init_asset`
            // needs the asset machinery (the kn-ui headless setup, ditto).
            bevy::asset::AssetPlugin::default(),
            bevy::transform::TransformPlugin,
            AnimPlugin::new(TICK_MS as f32),
            // The pipeline the button must reset is the shared subsystem's.
            CreatureAnimationPlugin,
        ))
        .insert_resource(GameResource(game))
        .init_asset::<Font>();
        let fonts = {
            let mut font_assets = app.world_mut().resource_mut::<Assets<Font>>();
            kn_ui::konung_fonts(&mut font_assets)
        };
        app.insert_resource(fonts)
            .insert_resource(Viewer {
                maps: vec![map_ref],
                selected: 0,
                overlays: DEFAULT_OVERLAYS,
                playback: PlayState::Paused,
                constant_day: false,
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
                    && (transform.translation.y - (body.y + LABEL_OFFSET_Y)).abs() < 1e-3,
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

    /// The cost of a scrub release's settle on the busiest map — the one whose
    /// simulation carries the most creatures. Forward `0 -> 18960` is plain
    /// catch-up; backward `18960 -> 900` is the seed replay, the only way
    /// back. Measured in release and printed; no bound on the numbers and no
    /// optimization — the measurement says only whether a release can stutter
    /// (review #106).
    ///
    /// ```bash
    /// cargo test --release --manifest-path crates/Cargo.toml -p kn-fe-viewer \
    ///     --lib benchmark_scrub_catchup_cost -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "cost measurement: release only, and only when run explicitly"]
    fn benchmark_scrub_catchup_cost() {
        let Some((dir, _)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();
        // The map whose simulation carries the most creatures: the settle
        // walks exactly that population.
        let (busiest, creatures) = game
            .maps()
            .unwrap()
            .into_iter()
            .filter_map(|map_ref| {
                game.simulation(map_ref, SIM_SEED)
                    .ok()
                    .map(|simulation| (map_ref, simulation.states().len()))
            })
            .max_by_key(|&(_, count)| count)
            .expect("the edition gives maps with simulations");
        println!("scrub catch-up cost, busiest map {busiest:?}: {creatures} creatures");

        for (label, from, to) in [
            ("forward   0 -> 18960", 0, 18960),
            ("backward 18960 -> 900", 18960, 900),
        ] {
            let mut runs = [Duration::ZERO; 3];
            for run in &mut runs {
                let mut app = catch_up_app(&dir, busiest, from);
                let slider = spawn_scrub_slider(&mut app);
                app.world_mut().entity_mut(slider).remove::<Pressed>();
                app.world_mut().trigger(ValueChange {
                    source: slider,
                    value: to as f32,
                    is_final: true,
                });
                let started = Instant::now();
                app.update();
                *run = started.elapsed();
                // The settle really landed on the scrubbed tick.
                let simulation = app.world().resource::<CreatureSim>().sim.as_ref().unwrap();
                assert_eq!(simulation.tick_number(), to, "{label}");
            }
            let ms: Vec<String> = runs
                .iter()
                .map(|run| format!("{:.1}", run.as_secs_f64() * 1000.0))
                .collect();
            println!("{label}: {} ms", ms.join(", "));
        }
    }

    /// The scrub app as the drag tests build it, standing at `clock_ticks`
    /// with its simulation caught up to it: exactly what the window holds
    /// between a drag move and the release. The catch-up of the warm-up frame
    /// (the whole `clock_ticks` for a backward case) stays outside the timed
    /// update.
    fn catch_up_app(dir: &std::path::Path, map_ref: MapRef, clock_ticks: u64) -> App {
        let game = Game::open(dir).unwrap();
        let mut sim = CreatureSim::default();
        sim.sim = game.simulation(map_ref, SIM_SEED).ok();
        sim.initial_pixels = sim.sim.as_ref().unwrap().initial_pixels();
        let mut app = scrub_app(Some((game, map_ref)));
        *app.world_mut().resource_mut::<CreatureSim>() = sim;
        app.world_mut().resource_mut::<Clock>().ticks = clock_ticks;
        app.update();
        app
    }

    /// The edition directory the cost measurements run on: `GAME_DIR` when
    /// set, otherwise the first edition directory with data (rus, then eng) —
    /// the same order [`game_and_first_map`] walks. The measurements name the
    /// edition they ran on, so the recipe passes `GAME_DIR` explicitly.
    fn bench_game_dir() -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os("GAME_DIR") {
            let dir = PathBuf::from(dir);
            if dir.is_dir() {
                return Some(dir);
            }
            eprintln!("skip: GAME_DIR set but not a directory: {}", dir.display());
            return None;
        }
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../original/k1");
        for edition in ["rus", "eng"] {
            let dir = root.join(edition).join("game");
            if dir.is_dir() {
                return Some(dir);
            }
        }
        eprintln!("skip: no original/k1/{{rus,eng}}/game and no GAME_DIR (docs/k1/editions.md)");
        None
    }

    /// The game map of `number`, or `None` when the edition lacks it.
    fn map_by_number(game: &Game, number: u32) -> Option<MapRef> {
        game.maps()
            .ok()?
            .into_iter()
            .find(|map| map.source == MapSource::Game && map.number == number)
    }

    /// Which sequence every creature sprite shows: the pose the sprite pass
    /// has installed. The key carries `is_shadow`, because the shadow and
    /// body layer 0 are different sprites of one creature.
    fn pose_snapshot(app: &mut App) -> HashMap<(usize, bool, usize), usize> {
        let mut sprites = app.world_mut().query::<(&CreatureSprite, &Animator)>();
        sprites
            .iter(app.world())
            .map(|(tag, animator)| {
                (
                    (tag.creature_id, tag.is_shadow, tag.layer),
                    Arc::as_ptr(&animator.sequence) as usize,
                )
            })
            .collect()
    }

    /// Creatures whose pose changed between the snapshots and the sprite
    /// clips they re-installed: a pose change is one creature with any of its
    /// sprites on another sequence.
    fn pose_changes(
        before: &HashMap<(usize, bool, usize), usize>,
        after: &HashMap<(usize, bool, usize), usize>,
    ) -> (HashSet<usize>, usize) {
        let mut creatures = HashSet::new();
        let mut clips = 0;
        for (key, sequence) in after {
            if before.get(key) != Some(sequence) {
                creatures.insert(key.0);
                clips += 1;
            }
        }
        (creatures, clips)
    }

    /// Median of an unsorted copy.
    fn median(values: &mut [f64]) -> f64 {
        values.sort_by(f64::total_cmp);
        values[values.len() / 2]
    }

    /// The cost of one pose change — a creature switching its state —
    /// isolated in the frame stream. The shared subsystem runs headless on
    /// the walking map (19), the clock advances exactly one tick per update,
    /// and every update is timed against the creatures whose sprite
    /// sequences swapped on it. A steady frame (the simulation ticks, no
    /// creature changes pose) is the baseline; frames where exactly one
    /// creature changes pose isolate the change, so the difference carries
    /// no other per-tick work. Headless (no window, no GPU): this is the
    /// CPU update cost, the same app minus the window the player's tests
    /// run. Measured in release and printed — the numbers land in
    /// docs/rust.md, "Viewer costs"; no bound is asserted beyond the frame
    /// classes existing.
    ///
    /// ```bash
    /// GAME_DIR=original/k1/rus/game cargo test --release \
    ///     --manifest-path crates/Cargo.toml -p kn-fe-viewer --lib \
    ///     tests::benchmark_pose_swap_cost -- --ignored --exact --nocapture
    /// ```
    #[test]
    #[ignore = "cost measurement: release only, and only when run explicitly"]
    fn benchmark_pose_swap_cost() {
        const MAP: u32 = 19;
        const TICKS: u64 = 400;
        let Some(dir) = bench_game_dir() else {
            return;
        };
        let game = Game::open(&dir).unwrap();
        let map_ref = map_by_number(&game, MAP).expect("the edition must have map 19");
        let mut motion = CreatureMotion::default();
        motion.request(
            map_ref,
            SIM_SEED,
            None,
            game.scene(map_ref, &SceneOptions::ALL).unwrap(),
        );
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AnimPlugin::new(TICK_MS as f32),
            CreatureAnimationPlugin,
        ))
        .insert_resource(GameResource(game))
        .init_resource::<TextureCache>()
        .insert_resource(Assets::<Image>::default())
        .insert_resource(motion);
        // The clock moves only through the test, one tick per update; the
        // settle frame (map decode, spawn, the state-4 load swaps) stays
        // outside the timed loop.
        app.world_mut()
            .resource_mut::<Clock>()
            .goto(PlayState::Paused);
        app.update();

        let mut steady: Vec<f64> = Vec::new();
        let mut single: Vec<f64> = Vec::new();
        let mut multi: Vec<f64> = Vec::new();
        let mut single_clips = 0usize;
        let mut worst = (0.0f64, 0usize);
        for tick in 1..=TICKS {
            let before = pose_snapshot(&mut app);
            let started = Instant::now();
            app.world_mut().resource_mut::<Clock>().ticks = tick;
            app.update();
            let elapsed = started.elapsed().as_secs_f64() * 1e3;
            let (creatures, clips) = pose_changes(&before, &pose_snapshot(&mut app));
            if elapsed > worst.0 {
                worst = (elapsed, creatures.len());
            }
            match creatures.len() {
                0 => steady.push(elapsed),
                1 => {
                    single.push(elapsed);
                    single_clips += clips;
                }
                _ => multi.push(elapsed),
            }
        }
        let sim = app.world().resource::<CreatureSim>();
        let simulation = sim.sim.as_ref().unwrap();
        assert_eq!(
            simulation.tick_number(),
            TICKS,
            "the loop must carry one sim tick per update"
        );
        let states = simulation.states();
        let walking = states
            .iter()
            .filter(|state| state.walk_state == WalkState::Walk)
            .count();
        println!(
            "pose swap cost, map {MAP} of {}, {TICKS} updates of one tick each:",
            dir.display()
        );
        println!(
            "population {} creatures, {walking} walking at the end",
            states.len()
        );
        let steady_med = median(&mut steady);
        let (steady_min, steady_max) = class_min_max(&steady);
        println!(
            "steady tick frames {:5} · median {:.4} ms · min {:.4} · max {:.4}",
            steady.len(),
            steady_med,
            steady_min,
            steady_max
        );
        assert!(
            !steady.is_empty(),
            "map 19 must give tick frames without pose changes"
        );
        // Game data is present here — a data-less environment already left
        // through `bench_game_dir` — so an empty `single` class is a failed
        // run (no target measurement), not a skip.
        assert!(
            !single.is_empty(),
            "no single-creature pose change in {TICKS} ticks on {} map {MAP}: the target \
             measurement is missing — rerun with more ticks",
            dir.display()
        );
        let single_med = median(&mut single);
        let single_min = class_min_max(&single).0;
        println!(
            "single pose change  {:5} · median {:.4} ms · min {:.4} · over steady Δ {:.4} ms \
             ({:.1} clips per change)",
            single.len(),
            single_med,
            single_min,
            single_med - steady_med,
            single_clips as f64 / single.len() as f64
        );
        if !multi.is_empty() {
            println!(
                "multi pose change   {:5} · median {:.4} ms · min {:.4}",
                multi.len(),
                median(&mut multi),
                class_min_max(&multi).0
            );
        }
        println!(
            "worst update {:.4} ms with {} creatures changing pose",
            worst.0, worst.1
        );
    }

    /// (min, max) of a frame-time class.
    fn class_min_max(values: &[f64]) -> (f64, f64) {
        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        (min, max)
    }

    /// The per-frame cost of the depth recompute, isolated as the pure map
    /// call: `DepthMap::creature_z` — the binary search over the static
    /// stacks, the interpolation and the per-id slot — over the real depth
    /// maps of maps 17 and 19 with the bottoms the walking creatures
    /// actually carry (the live row plus the creature's body height). That
    /// lookup is what `sync_creature_nodes` pays per creature per frame for
    /// the shadow's slot, and as `creature_z` for the body z whenever a
    /// creature's bottom moves; the per-frame extrapolation multiplies the
    /// call by the population. Measured in release and printed; the numbers
    /// land in docs/rust.md, "Viewer costs".
    ///
    /// ```bash
    /// GAME_DIR=original/k1/rus/game cargo test --release \
    ///     --manifest-path crates/Cargo.toml -p kn-fe-viewer --lib \
    ///     tests::benchmark_depth_slot_cost -- --ignored --exact --nocapture
    /// ```
    #[test]
    #[ignore = "cost measurement: release only, and only when run explicitly"]
    fn benchmark_depth_slot_cost() {
        const CALLS: usize = 200_000;
        let Some(dir) = bench_game_dir() else {
            return;
        };
        let game = Game::open(&dir).unwrap();
        let mut cache = TextureCache::default();
        let mut images = Assets::<Image>::default();
        for number in [17u32, 19] {
            let Some(map_ref) = map_by_number(&game, number) else {
                continue;
            };
            let scene = game.scene(map_ref, &SceneOptions::ALL).unwrap();
            let mut seen = HashSet::new();
            for piece in &scene.pieces {
                if let Some(id) = &piece.texture {
                    seen.insert(*id);
                }
                if let Some(animation) = &piece.animation {
                    for frame in &animation.frames {
                        if let Some(id) = &frame.texture {
                            seen.insert(*id);
                        }
                    }
                }
            }
            let problems = cache
                .load(&game, &seen.into_iter().collect::<Vec<_>>(), &mut images)
                .unwrap();
            assert!(problems.is_empty(), "map {number} textures: {problems:?}");
            let depth = build_depth_map(&scene, &cache);
            let simulation = game.simulation(map_ref, SIM_SEED).unwrap();
            let (anim, report) = build_creature_anim(
                &game,
                map_ref,
                None,
                &scene.creature_pieces,
                &mut cache,
                &mut images,
            );
            assert!(
                report.error.is_none(),
                "map {number} creature anim: {:?}",
                report.error
            );
            let spawns = build_creature_spawns(&scene, &cache, &anim, &simulation.initial_pixels());
            // The bottoms the nodes carry each frame: the live row plus the
            // creature's body height, exactly what sync_creature_nodes computes.
            let bottoms: Vec<(u32, i32)> = simulation
                .states()
                .iter()
                .filter_map(|state| {
                    spawns
                        .heights
                        .get(&state.id)
                        .map(|&height| (state.id as u32, (state.pixel.1 + height).max(0)))
                })
                .collect();
            assert!(!bottoms.is_empty(), "map {number} must carry creatures");
            for &(id, bottom) in &bottoms {
                let _ = depth.creature_z(bottom, id);
            }
            let started = Instant::now();
            let mut sink = 0.0f32;
            for index in 0..CALLS {
                let (id, bottom) = bottoms[index % bottoms.len()];
                sink += depth.creature_z(bottom, id);
            }
            let per_call = started.elapsed().as_secs_f64() * 1e9 / CALLS as f64;
            println!(
                "depth recompute, map {number} of {}: {} creatures · creature_z \
                 {:.1} ns/call · a frame of all creatures ≈ {:.2} µs",
                dir.display(),
                bottoms.len(),
                per_call,
                per_call * bottoms.len() as f64 / 1e3
            );
            assert!(sink.is_finite(), "the sink keeps the calls from vanishing");
        }
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

    // --- Scene lighting -------------------------------------------------------

    /// The k1 mapping into the render contract: the fade follows the phase for
    /// every surface — the terrain carries the same ambient as the objects,
    /// window or not — and the ground window gates only the tile light.
    #[test]
    fn the_lighting_state_follows_the_phase_and_gates_the_tile_light() {
        let night = lighting::params(11000, SIM_SEED);
        assert!(
            night.ground_window,
            "tick 11000 is inside the ground window"
        );
        let state = lighting_state(night);
        assert_eq!(
            state.terrain_blend,
            Vec3::from(night.ambient),
            "the terrain fades with the ambient like every surface"
        );
        assert_eq!(state.ambient_blend, Vec3::from(night.ambient));
        assert_eq!(state.light_color, Vec3::from(night.ground_light));
        assert_eq!(state.light_gain, 1.0, "the window is open: masks live");

        let day = lighting::params(900, SIM_SEED);
        assert!(!day.ground_window, "tick 900 is outside the ground window");
        let state = lighting_state(day);
        assert_eq!(
            state.terrain_blend,
            Vec3::ZERO,
            "daylight is the zero triple — the original's transform no-ops"
        );
        assert_eq!(state.ambient_blend, Vec3::ZERO);
        assert_eq!(
            state.light_gain, 0.0,
            "the window is closed: the tile light is off, the fade is untouched"
        );

        // Dusk (window 6 plateau): outside the ground window, and the terrain
        // still carries the ambient — the ground dims together with the objects.
        let dusk = lighting::params(7000, SIM_SEED);
        assert!(!dusk.ground_window);
        let state = lighting_state(dusk);
        assert_eq!(state.terrain_blend, Vec3::from(dusk.ambient));
        assert_eq!(state.ambient_blend, Vec3::from(dusk.ambient));
        assert_eq!(state.light_gain, 0.0);
    }

    /// The moving-creature lighting regression: a creature in a dark cell
    /// carries the ambient material, stepping into a lit cell drops it —
    /// the same entity, animator untouched, nothing respawned — and stepping
    /// back out restores it.
    ///
    /// The lit patch is placed on a cell the creature provably walks through:
    /// a dry run of the deterministic simulation (same seed) records its
    /// cells, the grid lights exactly one of them, and the replayed app walks
    /// the same route cell for cell.
    #[test]
    fn a_moving_creature_gains_and_loses_its_lighting_material_with_its_cell() {
        use bevy::sprite_render::SpriteMaterial;
        use kn_game::movement::grid::{FINE_HEIGHT, FINE_WIDTH};
        use kn_render::KonungLightMaterial;
        #[allow(unused_imports)]
        use kn_render::update_creature_lighting;

        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();
        let scene = game.scene(map_ref, &SceneOptions::ALL).unwrap();

        // The dry run: every creature's cell at every tick of the window.
        let mut probe = game.simulation(map_ref, SIM_SEED).unwrap();
        let mut per_tick: Vec<Vec<(usize, (u32, u32))>> = Vec::new();
        for _ in 0..=90 {
            per_tick.push(probe.states().iter().map(|s| (s.id, s.cell)).collect());
            probe.tick();
        }

        // The real pipeline, exactly as the viewer window runs it.
        let mut cache = TextureCache::default();
        let mut images = Assets::<Image>::default();
        let (anim, report) = build_creature_anim(
            &game,
            map_ref,
            None,
            &scene.creature_pieces,
            &mut cache,
            &mut images,
        );
        assert!(
            report.error.is_none(),
            "clips must build: {:?}",
            report.error
        );
        let mut sim_resource = CreatureSim::default();
        sim_resource.sim = game.simulation(map_ref, SIM_SEED).ok();
        sim_resource.initial_pixels = sim_resource.sim.as_ref().unwrap().initial_pixels();
        let spawns = build_creature_spawns(&scene, &cache, &anim, &sim_resource.initial_pixels);
        assert!(!spawns.plans.is_empty(), "the map must give creatures");
        let depth = build_depth_map(&scene, &cache);

        let mut materials = Assets::<KonungLightMaterial>::default();
        // No tile masks needed: only the plain ambient material is asserted.
        let pool = LightingMaterials::new(&mut images, &mut materials, &[]);
        let ambient = pool.ambient();

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::transform::TransformPlugin,
            AnimPlugin::new(TICK_MS as f32),
            CreatureAnimationPlugin,
        ))
        .insert_resource(GameResource(game))
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
        .insert_resource(pool)
        // The system under test, at the place the plugin runs it.
        .add_systems(Update, update_creature_lighting.after(CreatureSet::Nodes));
        app.world_mut()
            .resource_mut::<Clock>()
            .goto(PlayState::Paused);
        app.update();
        // Body sprite of every creature the pipeline spawned (layer 0, body).
        let mut bodies: HashMap<usize, Entity> = HashMap::new();
        let mut query = app.world_mut().query::<(Entity, &CreatureSprite)>();
        for (entity, tag) in query.iter(app.world()) {
            if !tag.is_shadow && tag.layer == 0 {
                bodies.insert(tag.creature_id, entity);
            }
        }
        let mut node_query = app.world_mut().query::<&CreatureNode>();
        let node_count = node_query.iter(app.world()).count();
        let mut sprite_query = app.world_mut().query::<&CreatureSprite>();
        let sprite_count = sprite_query.iter(app.world()).count();
        assert!(node_count > 0, "the map must give creature nodes");

        // A creature with a body sprite that enters and leaves a cell within
        // the window: on at `t`, dark before and after, not its home cell.
        let cell_at = |id: usize, t: usize| {
            per_tick[t]
                .iter()
                .find(|(other, _)| *other == id)
                .map(|(_, cell)| *cell)
        };
        // A stay, not a single tick: a creature that arrives at `t` (cell
        // changed) and departs at `leave` (cell changes again) stands on the
        // cell for [arrive, leave), and the material must follow exactly that.
        let mut picked = None;
        'outer: for &id in bodies.keys() {
            let home = cell_at(id, 0).expect("every creature has a cell");
            for arrive in 2..per_tick.len() - 2 {
                let on = cell_at(id, arrive).expect("in range");
                if on == cell_at(id, arrive - 1).expect("in range") || on == home {
                    continue;
                }
                for leave in arrive + 1..per_tick.len() {
                    if cell_at(id, leave) != Some(on) {
                        picked = Some((id, on, arrive, leave));
                        break 'outer;
                    }
                }
            }
        }
        let Some((id, lit_cell, lit_tick, lit_leave)) = picked else {
            panic!("ни одно существо не вошло и не вышло из клетки за 90 тиков");
        };

        // The whole grid dark except the cell the creature walks through.
        let mut fine = vec![vec![0u8; FINE_WIDTH as usize]; FINE_HEIGHT as usize];
        fine[lit_cell.1 as usize][lit_cell.0 as usize] = 7;
        app.insert_resource(FineLightGrid(Some(fine)));

        let body = bodies[&id];
        let mut samples: Vec<(bool, bool)> = Vec::new();
        for tick in 0..per_tick.len() as u64 {
            app.world_mut().resource_mut::<Clock>().ticks = tick;
            app.update();
            let world = app.world();
            let state = world
                .resource::<CreatureSim>()
                .sim
                .as_ref()
                .unwrap()
                .states()
                .into_iter()
                .find(|state| state.id == id)
                .expect("the creature keeps its state");
            let tick = tick as usize;
            if (lit_tick..lit_leave).contains(&tick) {
                assert_eq!(
                    state.cell, lit_cell,
                    "the replay stands on the lit cell for the whole stay"
                );
            }
            assert!(
                world.get_entity(body).is_ok(),
                "the body sprite is never despawned"
            );
            samples.push((
                world
                    .get::<SpriteMaterial<KonungLightMaterial>>(body)
                    .is_some(),
                world.get::<Animator>(body).is_some(),
            ));
        }
        let world = app.world();

        // Dark at home: the ambient material from the very first frame.
        assert!(
            samples[0].0,
            "a creature in a dark cell must carry the ambient material"
        );
        assert_eq!(
            world
                .get::<SpriteMaterial<KonungLightMaterial>>(body)
                .map(|material| &material.0),
            Some(&ambient),
            "the material is the pool's plain ambient one"
        );
        // On the lit cell: the material is gone — without a respawn.
        assert!(
            !samples[lit_tick].0,
            "a creature on a lit cell must drop the material"
        );
        // Back on a dark cell: the material returns.
        assert!(
            samples[lit_leave].0,
            "back in the dark the material is restored"
        );
        for (tick, (has_material, has_animator)) in samples.iter().enumerate() {
            assert!(
                *has_animator,
                "tick {tick}: the animator must stay on the sprite"
            );
            let want_material = !(lit_tick..lit_leave).contains(&tick);
            assert_eq!(
                *has_material, want_material,
                "tick {tick}: the material follows the stay on the cell"
            );
        }
        // Nothing respawned: node and sprite counts are what spawn built.
        let mut node_query = app.world_mut().query::<&CreatureNode>();
        assert_eq!(
            node_query.iter(app.world()).count(),
            node_count,
            "no creature node was respawned by the light changes"
        );
        let mut sprite_query = app.world_mut().query::<&CreatureSprite>();
        assert_eq!(
            sprite_query.iter(app.world()).count(),
            sprite_count,
            "no creature sprite was respawned by the light changes"
        );
    }

    // --- Time-of-day scrub ----------------------------------------------------

    /// A headless scrub app: the lighting writer and the scrub systems in the
    /// chained order the window runs them. `Some((game, map_ref))` arms the
    /// shared catch-up's backward branch (its seed replay needs the map).
    fn scrub_app(live: Option<(Game, MapRef)>) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(Clock::new(TICK_MS as f32))
            .insert_resource(CreatureSim::default())
            .insert_resource(LightingState::default())
            .insert_resource(viewer([false; 4]))
            .init_resource::<ScrubState>()
            .init_resource::<Rebuild>()
            .add_observer(change_scrub)
            .add_observer(jump_to_preset)
            .add_observer(change_day_night)
            // The window's chain runs the scrub and the switch state before
            // the lighting read; the shared catch-up lands between the settle
            // and the next frame.
            .add_systems(
                Update,
                (
                    apply_scrub,
                    sync_scrub_slider,
                    sync_daynight_boxes,
                    update_lighting_state,
                )
                    .chain(),
            );
        if let Some((game, map_ref)) = live {
            app.insert_resource(GameResource(game))
                .insert_resource(CreatureMotion {
                    map: Some(map_ref),
                    seed: SIM_SEED,
                    ..Default::default()
                })
                .add_systems(Update, advance_creature_sim.after(apply_scrub));
        }
        app
    }

    /// The panel's slider, headless: the range and step of the template change
    /// nothing the handlers read.
    fn spawn_scrub_slider(app: &mut App) -> Entity {
        app.world_mut().spawn((ScrubSlider, SliderValue(0.0))).id()
    }

    /// Drives the slider the way the widget does: a drag move holds `Pressed`
    /// and arrives unfinished, a release (or a keyboard step) drops it and
    /// arrives final. One frame runs after the event, as in the window.
    fn scrub_to(app: &mut App, slider: Entity, position: f32, finished: bool) {
        {
            let mut held = app.world_mut().entity_mut(slider);
            if finished {
                held.remove::<Pressed>();
            } else {
                held.insert(Pressed);
            }
        }
        app.world_mut().trigger(ValueChange {
            source: slider,
            value: position,
            is_final: finished,
        });
        app.update();
    }

    fn scrub_clock(app: &App) -> u64 {
        app.world().resource::<Clock>().ticks
    }

    fn scrub_lighting(app: &App) -> LightingState {
        *app.world().resource::<LightingState>()
    }

    fn scrub_thumb(app: &App, slider: Entity) -> f32 {
        app.world().entity(slider).get::<SliderValue>().unwrap().0
    }

    /// The panel's slider with its value label, as the widget template nests it.
    fn spawn_scrub_slider_with_label(app: &mut App) -> (Entity, Entity) {
        let label = app.world_mut().spawn(Text::new("0")).id();
        let slider = app
            .world_mut()
            .spawn((ScrubSlider, SliderValue(0.0)))
            .add_child(label)
            .id();
        (slider, label)
    }

    fn slider_label(app: &App, label: Entity) -> String {
        app.world().entity(label).get::<Text>().unwrap().0.clone()
    }

    /// The adapter finds its label inside the real FeathersSlider hierarchy:
    /// the widget's own scene (the nested value text among fill and knob) is
    /// spawned the way the panel template spawns it, and the clock face lands
    /// on the value text — the only text of the subtree.
    #[test]
    fn the_scrub_label_is_found_in_the_real_slider_hierarchy() {
        let mut app = scrub_app(None);
        // The widget's scene resolves its embedded font through the asset
        // plugin and reads the theme at spawn — the same resources the real
        // window carries. Any theme serves: the hooks only need the resource
        // to exist, and reaching for kn_ui's palette would couple the test to
        // the theme rework (#110).
        app.add_plugins((
            bevy::asset::AssetPlugin::default(),
            bevy::scene::ScenePlugin,
        ));
        app.init_asset::<bevy::text::Font>();
        app.insert_resource(bevy::feathers::theme::UiTheme::default());
        app.add_systems(PostUpdate, scrub_clock_label);
        let entities = app
            .world_mut()
            .spawn_scene_list(bsn! {
                @FeathersSlider { @min: 0.0, @max: 100.0 } ScrubSlider
            })
            .expect("the slider scene spawns");
        let slider = entities[0];
        app.update();

        // Every Text of the widget's real subtree, collected recursively.
        let mut texts = app.world_mut().query::<&Text>();
        let mut children = app.world_mut().query::<&Children>();
        let mut stack = vec![slider];
        let mut found = Vec::new();
        while let Some(entity) = stack.pop() {
            if let Ok(nodes) = children.get(app.world(), entity) {
                for child in nodes.iter() {
                    stack.push(child);
                    if texts.get(app.world(), child).is_ok() {
                        found.push(child);
                    }
                }
            }
        }
        assert!(
            !found.is_empty(),
            "the real slider carries a value label in its subtree"
        );
        assert_eq!(
            found.len(),
            1,
            "the widget nests exactly one text — its value label"
        );
        assert_eq!(
            app.world().entity(found[0]).get::<Text>().unwrap().0,
            "06:00",
            "the adapter rewrites the real value label, not a synthetic one"
        );
        assert!(app.world().entity(slider).get::<SliderValue>().is_some());
    }

    /// The scrub label reads as clock hours and follows every time source —
    /// the drag, the presets and the clock's own advance — while the slider's
    /// value keeps holding the tick and texts outside the slider stay theirs.
    #[test]
    fn the_scrub_label_shows_clock_hours() {
        let mut app = scrub_app(None);
        app.add_systems(PostUpdate, scrub_clock_label);
        let (slider, label) = spawn_scrub_slider_with_label(&mut app);
        let foreign = app.world_mut().spawn(Text::new("map 17")).id();

        // The clock starts at tick 0 → 06:00 by our displayed-hour convention.
        app.update();
        assert_eq!(slider_label(&app, label), "06:00");

        // A drag to the warm preset tick: 2700 → 09:00, the value stays a tick.
        scrub_to(&mut app, slider, 2700.0, true);
        assert_eq!(slider_label(&app, label), "09:00");
        assert_eq!(scrub_thumb(&app, slider), 2700.0);

        // A preset jump lands on the same face as its button promises.
        let button = app.world_mut().spawn(ScrubPreset(18960)).id();
        app.world_mut().trigger(Activate { entity: button });
        app.update();
        assert_eq!(slider_label(&app, label), "03:04");

        // The clock's own advance moves the face: 15 ticks — one minute.
        app.world_mut().resource_mut::<Clock>().ticks += 15;
        app.update();
        assert_eq!(slider_label(&app, label), "03:05");

        // The write is one-way: the slider's value is untouched by the label,
        // and a text outside the slider keeps its own string.
        assert_eq!(scrub_thumb(&app, slider), 18975.0);
        assert_eq!(
            app.world().entity(foreign).get::<Text>().unwrap().0,
            "map 17"
        );
    }

    /// Scrubbing forward and back lights exactly the scrubbed tick, and the
    /// same position lights the same way however it was reached — the state is
    /// the pure `params(clock.ticks, seed)` of the single time source.
    #[test]
    fn a_scrub_lights_the_scrubbed_tick_forward_and_back() {
        let mut app = scrub_app(None);
        let slider = spawn_scrub_slider(&mut app);
        for position in [900u64, 2700, 11000, 18960, 2700, 900] {
            scrub_to(&mut app, slider, position as f32, false);
            let ticks = scrub_clock(&app);
            assert_eq!(ticks, position, "a drag move reaches the Clock at once");
            let want = lighting_state(lighting::params(ticks, SIM_SEED));
            assert_eq!(scrub_lighting(&app), want, "at {position}");
        }
    }

    /// A scrub inside a day keeps that day: the night palette — a function of
    /// the absolute day — stays the day's, so the same cycle position lights
    /// differently on two days whose coin flips differ.
    #[test]
    fn a_scrub_keeps_the_day_of_the_night_palette() {
        // Window 9 (tick 11950) is where the palettes differ in the ambient.
        let position = 11950u64;
        let (green_day, deep_day) = (2..32u64)
            .find_map(|deep| {
                (1..deep).find_map(|green| {
                    ((lighting::night_palette(green, SIM_SEED)
                        == k1_game::time_of_day::NightPalette::Green)
                        && (lighting::night_palette(deep, SIM_SEED)
                            == k1_game::time_of_day::NightPalette::Deep))
                        .then_some((green, deep))
                })
            })
            .expect("both palettes occur within 32 days");

        let mut app = scrub_app(None);
        let slider = spawn_scrub_slider(&mut app);
        app.world_mut().resource_mut::<Clock>().ticks = deep_day * CYCLE_TICKS + 5000;
        scrub_to(&mut app, slider, position as f32, true);
        assert_eq!(
            scrub_clock(&app),
            deep_day * CYCLE_TICKS + position,
            "the scrub moves the cycle position, never the day"
        );
        let deep = scrub_lighting(&app);
        assert_eq!(
            deep,
            lighting_state(lighting::params(
                deep_day * CYCLE_TICKS + position,
                SIM_SEED
            )),
            "the deep day lights as its own full tick"
        );

        // The same position on the green day: a different palette, still its
        // own day's.
        app.world_mut().resource_mut::<Clock>().ticks = green_day * CYCLE_TICKS + 5000;
        scrub_to(&mut app, slider, position as f32, true);
        let green = scrub_lighting(&app);
        assert_eq!(
            green,
            lighting_state(lighting::params(
                green_day * CYCLE_TICKS + position,
                SIM_SEED
            )),
            "the green day lights as its own full tick"
        );
        assert_ne!(
            deep.ambient_blend, green.ambient_blend,
            "the palette must follow the scrubbed day, not reset to day 0"
        );
    }

    /// The release walks the simulation to the Clock through the shared
    /// catch-up: `tick_number == clock.ticks` and the creatures stand exactly
    /// where a forward run to that tick stands — forward by catch-up, backward
    /// by the shared seed replay — while the held slider moves no creature.
    #[test]
    fn a_released_scrub_walks_the_simulation_to_the_clock() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();
        // Forward-run references for both targets: one simulation, two stops.
        let mut reference = game.simulation(map_ref, SIM_SEED).unwrap();
        for _ in 0..700 {
            reference.tick();
        }
        let at_700 = home(&reference);
        for _ in 0..200 {
            reference.tick();
        }
        let at_900 = home(&reference);

        let mut sim = CreatureSim::default();
        sim.sim = game.simulation(map_ref, SIM_SEED).ok();
        sim.initial_pixels = sim.sim.as_ref().unwrap().initial_pixels();
        let mut app = scrub_app(Some((game, map_ref)));
        *app.world_mut().resource_mut::<CreatureSim>() = sim;
        let slider = spawn_scrub_slider(&mut app);

        scrub_to(&mut app, slider, 300.0, false);
        let simulation = app.world().resource::<CreatureSim>().sim.as_ref().unwrap();
        assert_eq!(
            simulation.tick_number(),
            0,
            "a held slider freezes the simulation"
        );
        assert_eq!(scrub_clock(&app), 300, "the Clock follows the held slider");

        // Released at 900: plain forward catch-up.
        scrub_to(&mut app, slider, 900.0, true);
        assert_eq!(scrub_clock(&app), 900);
        let simulation = app.world().resource::<CreatureSim>().sim.as_ref().unwrap();
        assert_eq!(
            simulation.tick_number(),
            900,
            "the release brings the simulation to the clock"
        );
        assert_eq!(
            home(simulation),
            at_900,
            "creatures stand on the scrubbed tick's positions"
        );

        // Released at 700: no reverse tick — the seed replay lands the same.
        scrub_to(&mut app, slider, 700.0, true);
        assert_eq!(scrub_clock(&app), 700);
        let simulation = app.world().resource::<CreatureSim>().sim.as_ref().unwrap();
        assert_eq!(simulation.tick_number(), 700);
        assert_eq!(
            home(simulation),
            at_700,
            "the backward scrub replays to the forward run's positions"
        );
    }

    /// Play on, slider held still: the session pins the Clock to
    /// `day * CYCLE_TICKS + target` every frame — `advance_clock` keeps
    /// ticking underneath while playback runs, and without the per-frame hold
    /// the light would drift off the scrubbed tick between value changes
    /// (review #106). On release the hold lifts, Play resumes from the target
    /// and the shared catch-up lands the simulation on the Clock, at the
    /// forward run's positions. `Clock::advance` with a large delta stands in
    /// for the `AnimSet::Clock` pass the real frame runs first: a
    /// deterministic way to hand the clock ticks it did not have.
    #[test]
    fn a_held_slider_pins_the_clock_under_playback_until_release() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();
        // The settle after the release catches the simulation up to the
        // absolute tick, day included: the reference run stops one `advance`
        // past the held position.
        const HELD: u64 = CYCLE_TICKS + 300;
        const FINAL: u64 = HELD + 10;
        let mut reference = game.simulation(map_ref, SIM_SEED).unwrap();
        for _ in 0..FINAL {
            reference.tick();
        }
        let expected = home(&reference);

        let mut sim = CreatureSim::default();
        sim.sim = game.simulation(map_ref, SIM_SEED).ok();
        sim.initial_pixels = sim.sim.as_ref().unwrap().initial_pixels();
        let mut app = scrub_app(Some((game, map_ref)));
        *app.world_mut().resource_mut::<CreatureSim>() = sim;
        // `PlayState::Playing` is the Clock's own default: playback runs.
        let slider = spawn_scrub_slider(&mut app);

        app.world_mut().resource_mut::<Clock>().ticks = CYCLE_TICKS + 5000;
        scrub_to(&mut app, slider, 300.0, false);
        assert_eq!(
            scrub_clock(&app),
            HELD,
            "the drag pins the captured day's cycle position"
        );

        // Held still, playback on, zero new value changes: every frame ends
        // on the target however many ticks the playing clock gained, the
        // light follows that tick, and no creature moves.
        for _ in 0..5 {
            app.world_mut().resource_mut::<Clock>().advance(1.0);
            app.update();
            assert_eq!(scrub_clock(&app), HELD, "the hold beats the playing clock");
            assert_eq!(
                scrub_lighting(&app),
                lighting_state(lighting::params(HELD, SIM_SEED)),
                "the light follows the held tick"
            );
            let simulation = app.world().resource::<CreatureSim>().sim.as_ref().unwrap();
            assert_eq!(
                simulation.tick_number(),
                0,
                "a held slider freezes the simulation"
            );
        }

        // Released: the hold lifts, Play resumes from the target and the
        // settle brings the simulation to the Clock.
        scrub_to(&mut app, slider, 300.0, true);
        assert_eq!(scrub_clock(&app), HELD);
        // 0.6 s at the 60 ms tick: ten ticks of play past the held position.
        app.world_mut().resource_mut::<Clock>().advance(0.6);
        app.update();
        assert_eq!(
            scrub_clock(&app),
            FINAL,
            "the clock ticks on in play from the target"
        );
        let simulation = app.world().resource::<CreatureSim>().sim.as_ref().unwrap();
        assert_eq!(
            simulation.tick_number(),
            FINAL,
            "the simulation sits on the clock after the settle"
        );
        assert_eq!(home(simulation), expected, "the forward run's positions");
    }

    /// The Clock alone is the source: moved directly, no widget involved, it
    /// lights exactly what the same scrub would — there is no second time to
    /// disagree with.
    #[test]
    fn the_clock_alone_moves_the_lighting() {
        let mut app = scrub_app(None);
        let _slider = spawn_scrub_slider(&mut app);
        for ticks in [900u64, 2 * CYCLE_TICKS + 11000] {
            app.world_mut().resource_mut::<Clock>().ticks = ticks;
            app.update();
            let want = lighting_state(lighting::params(ticks, SIM_SEED));
            assert_eq!(scrub_lighting(&app), want, "at {ticks}");
        }
    }

    /// The uniforms read the state in the frame the Clock moves: the frontend
    /// writes [`LightingState`] before the plugin's `update_lighting_uniforms`
    /// runs, so a day change reaches every material of the pool in the same
    /// `update` — never a frame late (review #106). The wiring mirrors
    /// `run_viewer`'s: the chained frontend systems pinned before the writer
    /// that the plugin owns.
    #[test]
    fn the_uniforms_apply_the_new_tick_in_the_same_frame() {
        let mut images = Assets::<Image>::default();
        let mut materials = Assets::<KonungLightMaterial>::default();
        // One mask so both material kinds are judged: masked and plain terrain
        // take the ground blend, the ambient one the plain blend.
        let pool = LightingMaterials::new(&mut images, &mut materials, &[(vec![1u8; 4], 2, 2)]);
        let handles = (pool.terrain(Some(0)), pool.terrain(None), pool.ambient());

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()))
            // The stores a real window's AssetPlugin/RenderPlugin/SpritePlugin
            // provide, that the plugin's own systems and the embedded shader
            // load read: `init_asset` replaces whatever stands in the way, so
            // the pool's stores land after the plugins.
            .init_asset::<bevy::shader::Shader>()
            .init_asset::<Mesh>()
            .init_asset::<TextureAtlasLayout>()
            .add_plugins(KonungLightingPlugin)
            .insert_resource(images)
            .insert_resource(materials)
            .insert_resource(pool)
            .insert_resource(Clock::new(TICK_MS as f32))
            .insert_resource(CreatureSim::default())
            .insert_resource(viewer([false; 4]))
            .init_resource::<ScrubState>()
            .add_systems(
                Update,
                (apply_scrub, sync_scrub_slider, update_lighting_state)
                    .chain()
                    .before(update_lighting_uniforms),
            );

        // Night first: the pool carries the open-window ground blend.
        app.world_mut().resource_mut::<Clock>().ticks = 11000;
        app.update();
        let night = lighting_state(lighting::params(11000, SIM_SEED));
        {
            let materials = app.world().resource::<Assets<KonungLightMaterial>>();
            let material = materials.get(&handles.0).expect("pool material alive");
            assert_eq!(
                material.blend(),
                night.terrain_blend,
                "masked terrain lights the night before the change"
            );
            assert_eq!(material.light_color(), night.light_color);
        }

        // Day arrives; ONE update must land it on every material of the pool.
        app.world_mut().resource_mut::<Clock>().ticks = 900;
        app.update();
        let day = lighting_state(lighting::params(900, SIM_SEED));
        let materials = app.world().resource::<Assets<KonungLightMaterial>>();
        for (name, handle, blend) in [
            ("masked terrain", &handles.0, day.terrain_blend),
            ("plain terrain", &handles.1, day.terrain_blend),
            ("ambient", &handles.2, day.ambient_blend),
        ] {
            let material = materials.get(handle).expect("pool material alive");
            assert_eq!(material.blend(), blend, "{name} lags the day change");
            assert_eq!(material.light_color(), day.light_color, "{name}");
        }
    }

    /// While nobody holds the slider, the thumb rides the Clock's cycle
    /// position; a held slider keeps the user's value, and the release hands
    /// the mirror back.
    #[test]
    fn the_slider_rides_the_clock_until_someone_holds_it() {
        let mut app = scrub_app(None);
        let slider = spawn_scrub_slider(&mut app);
        app.world_mut().resource_mut::<Clock>().ticks = 2 * CYCLE_TICKS + 317;
        app.update();
        assert_eq!(
            scrub_thumb(&app, slider),
            317.0,
            "the thumb rides the cycle position of the clock"
        );

        scrub_to(&mut app, slider, 900.0, false);
        app.world_mut().resource_mut::<Clock>().ticks = 15000;
        app.update();
        assert_eq!(
            scrub_thumb(&app, slider),
            900.0,
            "a held slider keeps the user's value"
        );

        scrub_to(&mut app, slider, 900.0, true);
        app.world_mut().resource_mut::<Clock>().ticks = 15200;
        app.update();
        assert_eq!(
            scrub_thumb(&app, slider),
            15200.0,
            "the release hands the mirror back to the clock"
        );
    }

    /// A preset is the jump without a drag: position, lighting and simulation
    /// all land in the same frame.
    #[test]
    fn a_preset_jumps_the_whole_frame_at_once() {
        let Some((dir, map_ref)) = game_and_first_map() else {
            return;
        };
        let game = Game::open(&dir).unwrap();
        let mut reference = game.simulation(map_ref, SIM_SEED).unwrap();
        for _ in 0..2700 {
            reference.tick();
        }
        let at_2700 = home(&reference);

        let mut sim = CreatureSim::default();
        sim.sim = game.simulation(map_ref, SIM_SEED).ok();
        sim.initial_pixels = sim.sim.as_ref().unwrap().initial_pixels();
        let mut app = scrub_app(Some((game, map_ref)));
        *app.world_mut().resource_mut::<CreatureSim>() = sim;
        let button = app.world_mut().spawn(ScrubPreset(2700)).id();

        app.world_mut().trigger(Activate { entity: button });
        app.update();

        assert_eq!(scrub_clock(&app), 2700);
        let want = lighting_state(lighting::params(2700, SIM_SEED));
        assert_eq!(scrub_lighting(&app), want);
        let simulation = app.world().resource::<CreatureSim>().sim.as_ref().unwrap();
        assert_eq!(
            simulation.tick_number(),
            2700,
            "the preset settles the simulation in the same frame"
        );
        assert_eq!(home(simulation), at_2700);
    }

    fn fire_reset(mut commands: Commands, buttons: Query<Entity, With<ResetPositions>>) {
        for button in &buttons {
            commands.trigger(Activate { entity: button });
        }
    }
}
