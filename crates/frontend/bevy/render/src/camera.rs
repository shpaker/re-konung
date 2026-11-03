//! Камера просмотра карты: панорамирование, ступенчатый масштаб, границы.
//!
//! Масштаб — дискретные ступени [`ZOOM_LEVELS`], чтобы на целых значениях
//! картинка оставалась пиксель-в-пиксель: проекция ортографическая с
//! `scale = 1 / zoom`, поэтому при zoom = 1 один мировой юнит равен одному
//! пикселю окна.

use bevy::prelude::*;

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

    /// Moves the zoom `delta` steps along the ladder; whether it changed.
    pub fn step(&mut self, delta: i32) -> bool {
        let max = ZOOM_LEVELS.len() - 1;
        let next = (self.zoom as i32 + delta).clamp(0, max as i32) as usize;
        let changed = next != self.zoom;
        self.zoom = next;
        changed
    }
}

/// Set for the apps' input bindings: frontends register their camera-control
/// systems here.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct CameraInput;

/// Set for the camera write-out: ordered after [`CameraInput`] in this crate,
/// so the schedule does not depend on plugin registration order.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct CameraApply;

/// Camera and its plugin: spawn, write-out and bounds. Input handling lives in
/// the apps' bindings ([`CameraInput`]).
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
                    // The frame's zoom must reach the projection on the same frame:
                    // apply_pending_frame picks the fitting step into CameraState,
                    // and apply_zoom is what writes the state out to the camera
                    apply_pending_frame,
                    apply_zoom,
                )
                    .chain()
                    .in_set(CameraApply)
                    .after(CameraInput),
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

/// Orthographic scale of a projection; the pan/drag world step is the screen
/// step times this.
pub fn ortho_scale(projection: &Projection) -> f32 {
    match projection {
        Projection::Orthographic(ortho) => ortho.scale,
        // The map has no perspective or custom projections
        _ => 1.0,
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
/// поэтому центр и границы берутся из фактических спрайтов. `shown` — какие куски
/// считать содержимым: скрытый оверлей не должен двигать камеру (см.
/// [`crate::scene::scene_bounds`]). Само наведение откладывается в
/// [PendingFrame]: камеру спавнят команды, и в `Startup` её ещё может не быть.
pub fn frame_scene(
    scene: &kn_game::Scene,
    cache: &crate::textures::TextureCache,
    shown: impl Fn(&kn_game::Piece) -> bool,
    pending: &mut PendingFrame,
) {
    let Some((min, max)) = crate::scene::scene_bounds(scene, cache, shown) else {
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
    fn a_framed_scene_reaches_the_projection_on_the_same_frame() {
        // A world without a window and GPU; MinimalPlugins provides only time
        // and the scheduler, and the plugin's systems read nothing else
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, MapCameraPlugin));
        // The fitting step is picked only with a window in the world — spawn one
        app.world_mut().spawn(Window {
            resolution: (1280, 800).into(),
            ..default()
        });
        let bounds = (Vec2::new(-4669.0, -1344.0), Vec2::new(4669.0, 1344.0));
        app.world_mut().insert_resource(PendingFrame {
            center: Some(Vec2::ZERO),
            bounds: Some(bounds),
        });

        app.update();

        let want = fitting_zoom(bounds.1 - bounds.0, Vec2::new(1280.0, 800.0));
        assert_eq!(app.world().resource::<CameraState>().zoom, want);
        let mut projections = app
            .world_mut()
            .query_filtered::<&Projection, With<MapCamera>>();
        let scale = match projections.single(app.world()).expect("the camera lives") {
            Projection::Orthographic(ortho) => ortho.scale,
            other => panic!("the map is drawn with an orthographic projection, not {other:?}"),
        };
        // Before the order fix apply_zoom ran first, and the projection got
        // the new step only on the next frame
        assert_eq!(scale, 1.0 / ZOOM_LEVELS[want]);
    }
}
