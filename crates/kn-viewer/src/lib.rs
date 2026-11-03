//! Просмотрщик карт «Князя»: состояние, перестроение сцены, интерфейс и
//! headless-диагностика.
//!
//! Состояние ([Viewer]) и команды интерфейса отделены от отрисовки: обработчики
//! виджетов правят только состояние и взводят [Rebuild], а сцену перестраивает
//! одна система. Всё, что можно проверить без окна, живёт в [check].

pub mod cli;

use std::process::ExitCode;
use std::time::{Duration, Instant};

use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    feathers::{
        FeathersPlugins,
        controls::{FeathersCheckbox, FeathersSelect, OptionIndex, list_rows_from_strings},
        display::caption,
        theme::ThemeBackgroundColor,
        tokens,
    },
    input::mouse::{AccumulatedMouseMotion, MouseWheel},
    picking::hover::Hovered,
    prelude::*,
    ui::Checked,
    ui_widgets::ValueChange,
};
use kn_game::{Game, MapRef, MapSource, Piece, SceneOptions, TICK_MS, group_is_shown};
use kn_input::{ZoomScroll, drag_delta, pan_direction, wheel_steps};
use kn_render::{
    AnimPlugin, CameraApply, CameraBounds, CameraInput, CameraState, Clock, GameResource,
    MapCamera, MapCameraPlugin, PAN_SPEED, PendingFrame, PlayState, SceneGroup, ScenePiece,
    TextureCache, clear_scene, frame_scene, ortho_scale, spawn_scene,
};
use kn_ui::{dark_theme, pointer_over_ui, sync_checked};
use thiserror::Error;

use crate::cli::{Cli, Format};

#[derive(Debug, Error)]
pub enum ViewerError {
    #[error(transparent)]
    Game(#[from] kn_game::GameError),
    #[error("{dir}: нет карт источника {map_source}")]
    NoMaps {
        dir: std::path::PathBuf,
        map_source: MapSource,
    },
    #[error("{map_source} {number}: карты нет в {dir}")]
    NoSuchMap {
        dir: std::path::PathBuf,
        map_source: MapSource,
        number: u32,
    },
    #[error("нужно --map N или --all")]
    MapNotSelected,
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

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

/// Включена ли анимация при запуске и стоит ли она на паузе. Согласовано с
/// `Checked` чекбоксов `Animation` и `Paused` в [panel].
pub const DEFAULT_PLAYBACK: PlayState = PlayState::Playing;

/// Что показывает просмотрщик. Правится интерфейсом, читается системами.
#[derive(Resource)]
pub struct Viewer {
    pub maps: Vec<MapRef>,
    pub selected: usize,
    /// Включённые оверлеи в порядке [Overlay]
    pub overlays: [bool; 4],
    /// Воспроизведение анимации: пауза и выключение — разные состояния
    pub playback: PlayState,
}

/// Переключатель `Animation`: выключенная анимация показывает базовый кадр (тот,
/// что записан в данных), включённая — начинает с него же.
pub fn set_animation(on: bool) -> PlayState {
    if on {
        PlayState::Playing
    } else {
        PlayState::Off
    }
}

/// Переключатель `Paused`: держит текущий кадр, не трогая базовый. Пауза
/// бессмысленна, когда анимации нет, и в этом случае остаётся `Off`.
pub fn set_paused(playback: PlayState, paused: bool) -> PlayState {
    match (playback, paused) {
        (PlayState::Off, _) => PlayState::Off,
        (_, true) => PlayState::Paused,
        (_, false) => PlayState::Playing,
    }
}

/// Клавиша воспроизведения: пауза и продолжение одним нажатием. Если анимация
/// была выключена, клавиша её включает.
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

/// Переключатель воспроизведения: какой именно, видит компонент.
#[derive(Component, Clone, Copy, Default)]
struct PlaybackToggle(PlaybackKind);

/// Что переключает виджет воспроизведения.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum PlaybackKind {
    #[default]
    Animation,
    Paused,
}

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

/// Окно просмотрщика: работает, пока пользователь его не закроет.
pub fn run_viewer(cli: Cli) -> ExitCode {
    let game = match Game::open(&cli.game_dir) {
        Ok(game) => game,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let maps = match game.maps() {
        Ok(maps) if !maps.is_empty() => maps,
        Ok(_) => {
            eprintln!("{}: нет карт", cli.game_dir.display());
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    // Неверный номер карты — ошибка, а не тихий откат на первую
    let selected = cli.initial_map(&maps);
    if selected.is_none()
        && let Some(number) = cli.map
    {
        eprintln!(
            "{}: нет карты {number} источника {}",
            cli.game_dir.display(),
            MapSource::from(cli.source)
        );
        return ExitCode::FAILURE;
    }
    let selected = selected.unwrap_or(0);
    let labels = map_labels(&maps);

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: format!("re-konung viewer — {}", cli.game_dir.display()),
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
        .insert_resource(GameResource(game))
        .init_resource::<TextureCache>()
        .init_resource::<ActiveRoot>()
        .init_resource::<Stats>()
        .insert_resource(Rebuild { map: true })
        .insert_resource(Viewer {
            maps: maps.clone(),
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
                diagnostics,
            )
                .chain()
                .after(CameraInput)
                .before(CameraApply),
        )
        .run();
    ExitCode::SUCCESS
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
                @caption: bsn! { @caption("Foot") }
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
                @caption: bsn! { @caption("Markers") }
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
            @caption("Playback")
            --
            @FeathersCheckbox {
                @caption: bsn! { @caption("Animation") }
            }
            Checked
            PlaybackToggle(PlaybackKind::Animation)
            on(change_playback)
            --
            @FeathersCheckbox {
                @caption: bsn! { @caption("Paused") }
            }
            PlaybackToggle(PlaybackKind::Paused)
            on(change_playback)
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

/// Переключатели воспроизведения правят только состояние: сцена той же карты не
/// перестраивается, меняются лишь часы анимации. Виджет подписывает
/// [sync_playboxes] — иначе клик по `Paused` при выключенной анимации оставил бы
/// чекбокс отмеченным, а `Space` не отразился бы на панели вовсе.
fn change_playback(
    change: On<ValueChange<bool>>,
    toggles: Query<&PlaybackToggle>,
    mut viewer: ResMut<Viewer>,
) {
    let Ok(kind) = toggles.get(change.source) else {
        return;
    };
    viewer.playback = match kind.0 {
        PlaybackKind::Animation => set_animation(change.value),
        PlaybackKind::Paused => set_paused(viewer.playback, change.value),
    };
}

/// Отмечен ли переключатель при этом состоянии.
fn playback_checked(viewer: &Viewer, kind: PlaybackKind) -> bool {
    match kind {
        PlaybackKind::Animation => viewer.playback != PlayState::Off,
        PlaybackKind::Paused => viewer.playback == PlayState::Paused,
    }
}

/// Зеркалит состояние воспроизведения в чекбоксы.
///
/// `Checkbox` в Bevy сам `Checked` не пишет (см. [sync_checked]), и пишет его
/// только обработчик клика — а состояние правят и клавиши, и второй чекбокс
/// (пауза невозможна при выключенной анимации). Поэтому признак берётся из
/// `Viewer`, а не из того события, которое чекбокс послал.
fn sync_playboxes(
    viewer: Res<Viewer>,
    boxes: Query<(Entity, &PlaybackToggle, Has<Checked>)>,
    mut commands: Commands,
) {
    for (entity, kind, checked) in &boxes {
        let want = playback_checked(&viewer, kind.0);
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
        // поэтому он ставит паузу; выключенную анимацию шаг не включает
        viewer.playback = set_paused(viewer.playback, true);
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
    let ids = scene.texture_ids();
    let unique = ids.len();
    let animation = scene.animation_stats();
    let problems = match cache.load(&game.0, &ids, &mut images) {
        Ok(problems) => problems,
        Err(error) => {
            *stats = failed(map_ref, cache.len(), error.to_string(), started.elapsed());
            return;
        }
    };
    for problem in &problems {
        warn!("карта {map_ref:?}: {problem}");
    }
    if let Some(root) = active.0 {
        clear_scene(&mut commands, root);
    }
    let root = spawn_scene(&mut commands, &scene, &cache);
    active.0 = Some(root);
    // Кадр и масштаб сбрасываются только на новой карте: переключение оверлея
    // оставляет камеру там, где её оставил пользователь. Границы берутся по
    // видимым группам, иначе выключенный слой раздвигал бы их навсегда
    frame_scene(&scene, &cache, shown(&viewer.options()), &mut pending);
    *stats = Stats {
        map: Some(map_ref),
        unique_textures: unique,
        cached_textures: cache.len(),
        built_ms: started.elapsed().as_millis(),
        animated_pieces: animation.pieces,
        animation_frames: animation.frames,
        problems,
        error: None,
    };
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

/// Headless-проверка карт: без окна, GPU и рендерера.
pub fn run_check(cli: &Cli, map: Option<u32>, all: bool, format: Format) -> ExitCode {
    match check(cli, map, all, format) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// Отчёт по картам источника: `map` — одна карта, `all` — все.
pub fn check(
    cli: &Cli,
    map: Option<u32>,
    all: bool,
    format: Format,
) -> Result<ExitCode, ViewerError> {
    let game = Game::open(&cli.game_dir)?;
    let source: MapSource = cli.source.into();
    let available = game.maps()?;
    let maps = if all {
        available
            .into_iter()
            .filter(|map| map.source == source)
            .collect::<Vec<_>>()
    } else {
        let number = map.ok_or(ViewerError::MapNotSelected)?;
        let map_ref = MapRef { source, number };
        if !available.contains(&map_ref) {
            return Err(ViewerError::NoSuchMap {
                dir: cli.game_dir.clone(),
                map_source: source,
                number,
            });
        }
        vec![map_ref]
    };
    if maps.is_empty() {
        return Err(ViewerError::NoMaps {
            dir: cli.game_dir.clone(),
            map_source: source,
        });
    }
    let report = game.check(Some(&maps))?;
    match format {
        Format::Text => print!("{report}"),
        Format::Json => println!("{}", serde_json::to_string_pretty(&report)?),
    }
    Ok(if report.ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kn_game::PieceGroup;

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
    fn turning_animation_off_returns_to_the_base_frame() {
        // Выключение — не пауза: оно показывает кадр из данных, и с него же
        // продолжит воспроизведение
        assert_eq!(set_animation(false), PlayState::Off);
        assert_eq!(set_animation(true), PlayState::Playing);
    }

    #[test]
    fn pause_never_overrides_a_stopped_animation() {
        assert_eq!(set_paused(PlayState::Playing, true), PlayState::Paused);
        assert_eq!(set_paused(PlayState::Paused, false), PlayState::Playing);
        // Пауза вне анимации невозможна: она не должна её включить
        assert_eq!(set_paused(PlayState::Off, true), PlayState::Off);
        assert_eq!(set_paused(PlayState::Off, false), PlayState::Off);
    }

    #[test]
    fn a_frame_step_pauses_a_playing_animation() {
        // Шаг при идущем воспроизведении часы тут же увели бы кадр дальше,
        // поэтому клавиша шага сначала ставит паузу — и не будит выключенную
        assert_eq!(set_paused(PlayState::Playing, true), PlayState::Paused);
        assert_eq!(set_paused(PlayState::Paused, true), PlayState::Paused);
        assert_eq!(set_paused(PlayState::Off, true), PlayState::Off);
    }

    #[test]
    fn space_toggles_playback_and_wakes_a_stopped_animation() {
        assert_eq!(toggle_playback(PlayState::Playing), PlayState::Paused);
        assert_eq!(toggle_playback(PlayState::Paused), PlayState::Playing);
        assert_eq!(toggle_playback(PlayState::Off), PlayState::Playing);
    }

    #[test]
    fn playback_checkboxes_mirror_the_state() {
        for (playback, animation, paused) in [
            (PlayState::Playing, true, false),
            (PlayState::Paused, true, true),
            (PlayState::Off, false, false),
        ] {
            let mut viewer = viewer([false; 4]);
            viewer.playback = playback;
            assert_eq!(
                (
                    playback_checked(&viewer, PlaybackKind::Animation),
                    playback_checked(&viewer, PlaybackKind::Paused),
                ),
                (animation, paused),
                "{playback:?}"
            );
        }
    }

    #[test]
    fn default_playback_matches_its_checkboxes() {
        // Совпадает с `Checked` у чекбоксов Animation и Paused в panel()
        let viewer = viewer(DEFAULT_OVERLAYS);
        assert_eq!(viewer.playback, PlayState::Playing);
        assert!(playback_checked(&viewer, PlaybackKind::Animation));
        assert!(!playback_checked(&viewer, PlaybackKind::Paused));
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

    #[test]
    fn viewer_opens_only_a_real_game_dir() {
        let dir = tempfile::tempdir().unwrap();
        let cli = Cli {
            game_dir: dir.path().to_path_buf(),
            map: None,
            source: crate::cli::SourceArg::Game,
            command: None,
        };
        assert!(matches!(
            check(&cli, Some(1), false, Format::Text),
            Err(ViewerError::Game(_))
        ));
    }
}
