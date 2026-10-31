//! Камера просмотра карты: панорамирование, ступенчатый масштаб, границы.
//!
//! Масштаб — дискретные ступени [`ZOOM_LEVELS`], чтобы на целых значениях
//! картинка оставалась пиксель-в-пиксель: проекция ортографическая с
//! `scale = 1 / zoom`, поэтому при zoom = 1 один мировой юнит равен одному
//! пикселю окна.

use bevy::{
    input::mouse::{AccumulatedMouseMotion, MouseScrollUnit, MouseWheel},
    picking::hover::Hovered,
    prelude::*,
    ui::Node,
};

/// Камера просмотра карты.
#[derive(Component)]
pub struct MapCamera;

/// Область, в которой может находиться центр камеры: содержимое карты с отступом.
///
/// Не симметрична относительно нуля: нарисованная часть карты занимает не всю
/// сетку 80×160, а камера должна доходить до её краёв.
#[derive(Resource, Clone, Copy, Debug)]
pub struct CameraBounds {
    pub min: Vec2,
    pub max: Vec2,
    pub margin: f32,
}

impl Default for CameraBounds {
    fn default() -> Self {
        Self {
            min: Vec2::ZERO,
            max: Vec2::ZERO,
            margin: 256.0,
        }
    }
}

/// Ступени масштаба; 1.0 — пиксель в пиксель.
pub const ZOOM_LEVELS: [f32; 9] = [0.125, 0.25, 0.5, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0];
/// Индекс ступени по умолчанию — 1:1.
pub const DEFAULT_ZOOM: usize = 3;
/// Скорость панорамирования с клавиатуры, экранных пикселей в секунду.
pub const PAN_SPEED: f32 = 900.0;
/// Сколько «строк» прокрутки дают одну ступень масштаба: один щелчок колеса.
pub const LINES_PER_ZOOM_STEP: f32 = 1.0;
/// Сколько пикселей прокрутки дают одну ступень масштаба: умеренный свайп трекпада.
pub const PIXELS_PER_ZOOM_STEP: f32 = 100.0;

/// Накопитель прокрутки для масштаба.
///
/// Одна ступень требует порога: без него свайп трекпада и инерция колеса
/// присылают десятки событий и пролистывают весь диапазон за один жест.
#[derive(Debug, Default)]
pub struct ZoomScroll {
    accum: f32,
}

impl ZoomScroll {
    /// Учитывает прокрутку и возвращает, на сколько ступеней изменить масштаб.
    pub fn push(&mut self, unit: MouseScrollUnit, y: f32) -> i32 {
        let normalized = match unit {
            MouseScrollUnit::Line => y / LINES_PER_ZOOM_STEP,
            MouseScrollUnit::Pixel => y / PIXELS_PER_ZOOM_STEP,
        };
        self.accum = (self.accum + normalized).clamp(-1.0, 1.0);
        let steps = self.accum.trunc() as i32;
        self.accum -= steps as f32;
        steps
    }
}

/// Текущая ступень масштаба.
#[derive(Resource, Debug)]
pub struct CameraState {
    pub zoom: usize,
}

impl Default for CameraState {
    fn default() -> Self {
        Self { zoom: DEFAULT_ZOOM }
    }
}

impl CameraState {
    pub fn scale(&self) -> f32 {
        ZOOM_LEVELS[self.zoom.min(ZOOM_LEVELS.len() - 1)]
    }

    fn step(&mut self, delta: i32) -> bool {
        let max = ZOOM_LEVELS.len() - 1;
        let next = (self.zoom as i32 + delta).clamp(0, max as i32) as usize;
        let changed = next != self.zoom;
        self.zoom = next;
        changed
    }
}

/// Камера и её плагин: спавн, ввод, границы.
pub struct MapCameraPlugin;

impl Plugin for MapCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraState>()
            .init_resource::<CameraBounds>()
            .init_resource::<PendingFrame>()
            .add_systems(Startup, spawn_camera)
            .add_systems(
                Update,
                (
                    keyboard_pan,
                    drag_pan,
                    wheel_zoom,
                    apply_zoom,
                    apply_pending_frame,
                )
                    .chain(),
            )
            .add_systems(PostUpdate, clamp_to_bounds);
    }
}

fn spawn_camera(mut commands: Commands, state: Res<CameraState>) {
    commands.spawn((
        MapCamera,
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: 1.0 / state.scale(),
            ..OrthographicProjection::default_2d()
        }),
    ));
}

fn ortho_scale(projection: &Projection) -> f32 {
    match projection {
        Projection::Orthographic(ortho) => ortho.scale,
        // Перспективы и пользовательских проекций у карты нет
        _ => 1.0,
    }
}

fn keyboard_pan(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    camera: Single<(&mut Transform, &Projection), With<MapCamera>>,
) {
    let mut direction = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        direction.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        direction.x += 1.0;
    }
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        direction.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        direction.y -= 1.0;
    }
    if direction == Vec2::ZERO {
        return;
    }
    let (mut transform, projection) = camera.into_inner();
    transform.translation.x +=
        direction.x * PAN_SPEED * time.delta_secs() * ortho_scale(projection);
    transform.translation.y +=
        direction.y * PAN_SPEED * time.delta_secs() * ortho_scale(projection);
}

/// Панорамирование перетаскиванием левой кнопкой: мир следует за курсором.
/// Над интерфейсом не работает — иначе список карт двигал бы карту.
fn drag_pan(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    over_ui: Query<&Hovered, With<Node>>,
    camera: Single<(&mut Transform, &Projection), With<MapCamera>>,
) {
    if !buttons.pressed(MouseButton::Left) || !over_ui.is_empty() {
        return;
    }
    let (mut transform, projection) = camera.into_inner();
    let scale = ortho_scale(projection);
    transform.translation.x -= motion.delta.x * scale;
    // Экранная y вниз, мировая y вверх
    transform.translation.y += motion.delta.y * scale;
}

fn wheel_zoom(
    mut wheel: MessageReader<MouseWheel>,
    mut state: ResMut<CameraState>,
    mut scroll: Local<ZoomScroll>,
) {
    let mut steps = 0i32;
    for event in wheel.read() {
        steps += scroll.push(event.unit, event.y);
    }
    if steps != 0 {
        state.step(steps);
    }
}

fn apply_zoom(state: Res<CameraState>, mut camera: Single<&mut Projection, With<MapCamera>>) {
    let scale = 1.0 / state.scale();
    if let Projection::Orthographic(ortho) = camera.as_mut()
        && ortho.scale != scale
    {
        ortho.scale = scale;
    }
}

fn clamp_to_bounds(bounds: Res<CameraBounds>, mut camera: Single<&mut Transform, With<MapCamera>>) {
    let margin = Vec2::splat(bounds.margin);
    let low = bounds.min - margin;
    let high = bounds.max + margin;
    camera.translation.x = camera.translation.x.clamp(low.x, high.x);
    camera.translation.y = camera.translation.y.clamp(low.y, high.y);
}

/// Куда навести камеру и какие границы ей поставить, как только она появится.
#[derive(Resource, Default)]
pub struct PendingFrame {
    pub center: Option<Vec2>,
    pub bounds: Option<(Vec2, Vec2)>,
}

/// Наводит камеру на содержимое сцены и сужает её границы до этого содержимого.
///
/// Номинальный размер карты — вся сетка, а нарисована бывает только её часть,
/// поэтому центр и границы берутся из фактических спрайтов. Само наведение
/// откладывается в [PendingFrame]: камеру спавнят команды, и в `Startup` её ещё
/// может не быть.
pub fn frame_scene(
    scene: &kn_game::Scene,
    cache: &crate::textures::TextureCache,
    pending: &mut PendingFrame,
) {
    let Some((min, max)) = crate::scene::scene_bounds(scene, cache) else {
        return;
    };
    pending.center = Some((min + max) / 2.0);
    pending.bounds = Some((min, max));
}

fn apply_pending_frame(
    mut pending: ResMut<PendingFrame>,
    mut bounds: ResMut<CameraBounds>,
    mut state: ResMut<CameraState>,
    windows: Query<&Window>,
    mut camera: Query<&mut Transform, With<MapCamera>>,
) {
    if let Some((min, max)) = pending.bounds.take() {
        bounds.min = min;
        bounds.max = max;
        if let Some(window) = windows.iter().next() {
            state.zoom = fitting_zoom(max - min, Vec2::new(window.width(), window.height()));
        }
    }
    let Some(center) = pending.center.take() else {
        return;
    };
    for mut transform in &mut camera {
        transform.translation = Vec3::new(center.x, center.y, 0.0);
    }
}

/// Ступень масштаба, на которой содержимое `content` влезает в окно `window`.
///
/// Берётся самая крупная ступень из [ZOOM_LEVELS], не превосходящая отношение
/// окна к содержимому: карта открывается целиком, а приближением её можно
/// рассматривать до пикселя.
pub fn fitting_zoom(content: Vec2, window: Vec2) -> usize {
    let content = content.max(Vec2::splat(1.0));
    let fit = (window.x / content.x).min(window.y / content.y);
    ZOOM_LEVELS
        .iter()
        .rposition(|&level| level <= fit)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_zoom_is_pixel_perfect() {
        let state = CameraState::default();
        assert_eq!(state.scale(), 1.0);
    }

    #[test]
    fn zoom_steps_stay_inside_the_ladder() {
        let mut state = CameraState::default();
        for _ in 0..ZOOM_LEVELS.len() {
            state.step(1);
        }
        assert_eq!(state.zoom, ZOOM_LEVELS.len() - 1);
        for _ in 0..ZOOM_LEVELS.len() {
            state.step(-1);
        }
        assert_eq!(state.zoom, 0);
        assert_eq!(state.scale(), ZOOM_LEVELS[0]);
    }

    #[test]
    fn scale_is_inverse_of_zoom() {
        let mut state = CameraState { zoom: 4 };
        assert_eq!(state.scale(), 2.0);
        state.step(-1);
        assert_eq!(state.scale(), 1.0);
    }

    #[test]
    fn one_wheel_notch_is_one_level() {
        let mut scroll = ZoomScroll::default();
        assert_eq!(scroll.push(MouseScrollUnit::Line, 1.0), 1);
        assert_eq!(scroll.push(MouseScrollUnit::Line, -1.0), -1);
    }

    #[test]
    fn fractional_scrolls_accumulate_to_a_level() {
        let mut scroll = ZoomScroll::default();
        // Половинки не дают ступень, пока не наберут целое
        assert_eq!(scroll.push(MouseScrollUnit::Line, 0.5), 0);
        assert_eq!(scroll.push(MouseScrollUnit::Line, 0.5), 1);
    }

    #[test]
    fn trackpad_swipe_is_bounded() {
        let mut scroll = ZoomScroll::default();
        // Свайп: 30 событий по 10 пикселей — три ступени, а не тридцать
        let steps: i32 = (0..30)
            .map(|_| scroll.push(MouseScrollUnit::Pixel, 10.0))
            .sum();
        assert_eq!(steps, 3, "свайп в 300 пикселей");
    }

    #[test]
    fn scroll_direction_is_kept() {
        let mut scroll = ZoomScroll::default();
        assert_eq!(scroll.push(MouseScrollUnit::Line, -1.0), -1);
        assert_eq!(scroll.push(MouseScrollUnit::Pixel, -100.0), -1);
    }

    #[test]
    fn fitting_zoom_shows_the_whole_map() {
        // Карта 3: содержимое ~9300×2700 в окне 1280×800 — самая мелкая ступень
        assert_eq!(
            fitting_zoom(Vec2::new(9338.0, 2688.0), Vec2::new(1280.0, 800.0)),
            0
        );
        // Содержимое вдвое меньше окна — ступень ×2
        assert_eq!(
            fitting_zoom(Vec2::new(600.0, 300.0), Vec2::new(1280.0, 800.0)),
            4
        );
        // Очень мелкое содержимое — самая крупная ступень
        assert_eq!(
            fitting_zoom(Vec2::new(50.0, 50.0), Vec2::new(1280.0, 800.0)),
            ZOOM_LEVELS.len() - 1
        );
    }

    #[test]
    fn fitting_zoom_survives_empty_content() {
        // Вырожденное содержимое не должно давать панику или выход за лестницу
        let zoom = fitting_zoom(Vec2::ZERO, Vec2::new(1280.0, 800.0));
        assert!(zoom < ZOOM_LEVELS.len());
    }

    #[test]
    fn opposite_scrolls_cancel_out() {
        let mut scroll = ZoomScroll::default();
        // Незакрытый порог в одну сторону гасится прокруткой в другую
        assert_eq!(scroll.push(MouseScrollUnit::Line, 0.5), 0);
        assert_eq!(scroll.push(MouseScrollUnit::Line, -0.5), 0);
    }
}
