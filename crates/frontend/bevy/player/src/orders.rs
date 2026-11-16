//! Player gestures and feedback over the shared simulation's hero commands.

use bevy::{picking::hover::Hovered, prelude::*, window::PrimaryWindow};
use kn_app::Screen;
use kn_game::{
    movement::MovementMode,
    movement::grid::{Cell, Grid},
    simulation::{AttackStatus, HeroOrderStatus},
};
use kn_render::{
    CameraInput, CreaturePicker, CreatureSet, CreatureSim, MapCamera, MapViewport, ortho_scale,
};
use kn_ui::{KonungFonts, palette, pointer_over_ui, spacing, typo};

use crate::{
    flow::{AppFlow, FlowSet},
    i18n::{self, Text as InterfaceText},
};

const DRAG_DISTANCE: f32 = 4.0;
const DOUBLE_CLICK_INTERVAL: std::time::Duration = std::time::Duration::from_millis(350);
const DOUBLE_CLICK_DISTANCE: f32 = 6.0;

#[derive(Clone, Copy)]
struct LastClick {
    target: Cell,
    cursor: Vec2,
    at: std::time::Duration,
}

#[derive(Resource, Default)]
struct DoubleClick {
    enabled: bool,
    last: Option<LastClick>,
}

impl DoubleClick {
    fn configure(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.last = None;
            self.enabled = enabled;
        }
    }

    fn mode(&mut self, target: Cell, cursor: Vec2, at: std::time::Duration) -> MovementMode {
        if !self.enabled {
            self.last = None;
            return MovementMode::Walk;
        }
        let paired = self.last.is_some_and(|last| {
            last.target == target
                && cursor.distance_squared(last.cursor)
                    <= DOUBLE_CLICK_DISTANCE * DOUBLE_CLICK_DISTANCE
                && at.saturating_sub(last.at) <= DOUBLE_CLICK_INTERVAL
        });
        self.last = (!paired).then_some(LastClick { target, cursor, at });
        if paired {
            MovementMode::Run
        } else {
            MovementMode::Walk
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct MapGesture {
    origin: Option<Vec2>,
    previous: Option<Vec2>,
    dragging: bool,
    pub(crate) pan_delta: Option<Vec2>,
}

impl MapGesture {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn update(&mut self, buttons: &ButtonInput<MouseButton>, cursor: Vec2, allowed: bool) -> bool {
        self.pan_delta = None;
        if buttons.just_pressed(MouseButton::Left) {
            self.reset();
            if allowed {
                self.origin = Some(cursor);
                self.previous = Some(cursor);
            }
        }
        if !allowed {
            self.reset();
            return false;
        }
        let Some(origin) = self.origin else {
            return false;
        };
        if cursor.distance_squared(origin) > DRAG_DISTANCE * DRAG_DISTANCE {
            self.dragging = true;
        }
        if buttons.just_released(MouseButton::Left) {
            let click = !self.dragging;
            self.reset();
            return click;
        }
        if !buttons.pressed(MouseButton::Left) {
            self.reset();
            return false;
        }
        if self.dragging {
            self.pan_delta = self.previous.map(|previous| cursor - previous);
        }
        self.previous = Some(cursor);
        false
    }
}

/// Installs movement and melee orders; the existing creature plugin owns time.
pub struct PlayerOrdersPlugin;

impl Plugin for PlayerOrdersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MapGesture>()
            .init_resource::<DoubleClick>()
            .init_resource::<MapViewport>()
            .add_systems(
                Update,
                read_orders
                    .after(CreatureSet::Prepare)
                    .before(CreatureSet::Advance)
                    .before(CameraInput),
            )
            .add_systems(
                Update,
                (feedback, attack_feedback)
                    .after(CreatureSet::Advance)
                    .after(FlowSet),
            );
    }
}

#[allow(clippy::too_many_arguments)]
fn read_orders(
    flow: Res<AppFlow>,
    time: Res<Time<Real>>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    hovered: Query<&Hovered, With<Node>>,
    viewport: Res<MapViewport>,
    cameras: Query<(&Camera, &GlobalTransform), With<MapCamera>>,
    picker: CreaturePicker,
    mut gesture: ResMut<MapGesture>,
    mut double_click: ResMut<DoubleClick>,
    mut simulation: ResMut<CreatureSim>,
) {
    double_click.configure(flow.state().applied.deviations.double_click_run);
    let Some(window) = windows.iter().next() else {
        gesture.reset();
        double_click.last = None;
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        gesture.reset();
        double_click.last = None;
        return;
    };
    let inside = cursor.x >= 0.0
        && cursor.y >= 0.0
        && cursor.x < window.width()
        && cursor.y < window.height()
        && viewport
            .logical_rect
            .is_none_or(|rect| rect.contains(cursor));
    let allowed = flow.state().screen == Screen::Playing
        && simulation.sim.is_some()
        && window.focused
        && inside
        && !pointer_over_ui(hovered.iter().map(|hovered| hovered.0));
    let was_dragging = gesture.dragging;
    let clicked = gesture.update(&buttons, cursor, allowed);
    if !allowed
        || was_dragging
        || gesture.dragging
        || (buttons.just_released(MouseButton::Left) && !clicked)
    {
        double_click.last = None;
    }
    if !clicked {
        return;
    }
    let Some((camera, transform)) = cameras.iter().next() else {
        double_click.last = None;
        return;
    };
    if let Some(target_id) = picker.pick_screen(cursor, camera, transform)
        && let Some(simulation) = simulation.sim.as_mut()
        && simulation.can_hero_attack(target_id)
    {
        double_click.last = None;
        let _ = simulation.order_hero_attack(target_id);
        return;
    }
    if let Some(target) = camera
        .viewport_to_world_2d(transform, cursor)
        .ok()
        .and_then(world_cell)
        && let Some(simulation) = simulation.sim.as_mut()
    {
        let mode = double_click.mode(target, cursor, time.elapsed());
        let _ = simulation.order_hero_move_with_mode(target, mode);
    } else {
        double_click.last = None;
    }
}

fn world_cell(point: Vec2) -> Option<Cell> {
    if !point.is_finite() {
        return None;
    }
    Grid::fine().cell_at(
        point.x.round_ties_even() as i32,
        (-point.y).round_ties_even() as i32,
    )
}

#[derive(Component)]
pub(crate) struct OrderMarker;

#[derive(Component)]
struct MarkerLine(usize);

fn spawn_marker(commands: &mut Commands, transform: Transform, refused: bool) {
    commands
        .spawn((OrderMarker, transform, Visibility::Visible))
        .with_children(|root| {
            for slot in 0..8 {
                let (sprite, transform, visibility) = marker_line(slot, refused);
                root.spawn((MarkerLine(slot), sprite, transform, visibility));
            }
        });
}

fn feedback(
    mut commands: Commands,
    flow: Res<AppFlow>,
    simulation: Res<CreatureSim>,
    cameras: Query<&Projection, With<MapCamera>>,
    mut markers: Query<(Entity, &mut Transform, &mut Visibility), With<OrderMarker>>,
    mut lines: Query<
        (&MarkerLine, &mut Sprite, &mut Transform, &mut Visibility),
        Without<OrderMarker>,
    >,
) {
    let active = (flow.state().screen == Screen::Playing)
        .then_some(simulation.sim.as_ref())
        .flatten();
    let marker = active.and_then(|simulation| {
        if let Some(order) = simulation.hero_attack_order() {
            simulation
                .states()
                .into_iter()
                .find(|state| state.id == order.target_id)
                .map(|state| {
                    (
                        state.pixel,
                        matches!(order.status, AttackStatus::Refused(_)),
                    )
                })
        } else {
            simulation.hero_order().map(|order| {
                (
                    order.target.centre(),
                    matches!(order.status, HeroOrderStatus::Refused(_)),
                )
            })
        }
    });
    let Some(((x, y), refused)) = marker else {
        for (entity, _, _) in &mut markers {
            commands.entity(entity).despawn();
        }
        return;
    };
    let Some(projection) = cameras.iter().next() else {
        return;
    };
    let scale = ortho_scale(projection);
    let marker_transform =
        Transform::from_xyz(x as f32, -y as f32, 900.0).with_scale(Vec3::new(scale, scale, 1.0));
    if markers.is_empty() {
        spawn_marker(&mut commands, marker_transform, refused);
        return;
    }
    for (_, mut transform, mut visibility) in &mut markers {
        *transform = marker_transform;
        *visibility = Visibility::Visible;
    }
    for (line, mut sprite, mut transform, mut visibility) in &mut lines {
        let (next_sprite, next_transform, next_visibility) = marker_line(line.0, refused);
        *sprite = next_sprite;
        *transform = next_transform;
        *visibility = next_visibility;
    }
}

#[derive(Component)]
struct AttackFeedback;

fn attack_feedback(
    mut commands: Commands,
    flow: Res<AppFlow>,
    simulation: Res<CreatureSim>,
    fonts: Res<KonungFonts>,
    mut labels: Query<(Entity, &mut Text, &mut TextColor), With<AttackFeedback>>,
) {
    let order = (flow.state().screen == Screen::Playing)
        .then(|| simulation.sim.as_ref()?.hero_attack_order())
        .flatten();
    let Some(order) = order else {
        for (entity, _, _) in &mut labels {
            commands.entity(entity).despawn();
        }
        return;
    };
    let simulation = simulation.sim.as_ref().unwrap();
    let status = i18n::t(
        flow.state().applied.language,
        InterfaceText::AttackStatus(order.status),
    );
    let caption = simulation
        .creature_name(order.target_id)
        .map_or_else(|| status.to_owned(), |name| format!("{status}: {name}"));
    let color = if matches!(order.status, AttackStatus::Refused(_)) {
        palette::DANGER
    } else {
        palette::TEXT
    };
    if labels.is_empty() {
        commands.spawn((
            AttackFeedback,
            Text::new(caption),
            TextFont {
                font: fonts.regular.clone().into(),
                font_size: typo::BODY,
                ..default()
            },
            TextColor(color),
            BackgroundColor(palette::WINDOW_BG),
            Node {
                position_type: PositionType::Absolute,
                left: spacing::SPACE_16,
                bottom: spacing::SPACE_16,
                max_width: Val::Percent(90.0),
                padding: UiRect::all(spacing::SPACE_8),
                ..default()
            },
        ));
    } else {
        for (_, mut text, mut text_color) in &mut labels {
            if text.0 != caption {
                text.0.clone_from(&caption);
            }
            text_color.0 = color;
        }
    }
}

fn marker_line(slot: usize, refused: bool) -> (Sprite, Transform, Visibility) {
    let diamond = [
        (Vec2::new(0.0, 9.0), Vec2::new(15.0, 0.0)),
        (Vec2::new(15.0, 0.0), Vec2::new(0.0, -9.0)),
        (Vec2::new(0.0, -9.0), Vec2::new(-15.0, 0.0)),
        (Vec2::new(-15.0, 0.0), Vec2::new(0.0, 9.0)),
    ];
    let cross = [
        (Vec2::new(-9.0, -9.0), Vec2::new(9.0, 9.0)),
        (Vec2::new(-9.0, 9.0), Vec2::new(9.0, -9.0)),
    ];
    let segments = if refused { &cross[..] } else { &diamond[..] };
    let Some(&(start, end)) = segments.get(slot % 4) else {
        return (Sprite::default(), Transform::default(), Visibility::Hidden);
    };
    let outline = slot < 4;
    let delta = end - start;
    let center = (start + end) * 0.5;
    let color = if outline {
        palette::WINDOW_BG
    } else if refused {
        palette::DANGER
    } else {
        palette::ACCENT
    };
    let sprite = Sprite::from_color(
        color,
        Vec2::new(delta.length(), if outline { 5.0 } else { 2.5 }),
    );
    let transform = Transform::from_xyz(center.x, center.y, if outline { 0.0 } else { 0.1 })
        .with_rotation(Quat::from_rotation_z(delta.y.atan2(delta.x)));
    (sprite, transform, Visibility::Visible)
}

pub(crate) fn teardown(world: &mut World) {
    if let Some(mut gesture) = world.get_resource_mut::<MapGesture>() {
        gesture.reset();
    }
    if let Some(mut double_click) = world.get_resource_mut::<DoubleClick>() {
        double_click.last = None;
    }
    let mut markers =
        world.query_filtered::<Entity, Or<(With<OrderMarker>, With<AttackFeedback>)>>();
    let markers: Vec<_> = markers.iter(world).collect();
    for marker in markers {
        world.despawn(marker);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::camera::{CameraProjection, ComputedCameraValues, RenderTargetInfo, Viewport};

    #[test]
    fn running_requires_an_enabled_close_pair_and_consumes_it() {
        let target = Cell::new(40, 80);
        let cursor = Vec2::new(400.0, 300.0);
        let at = |milliseconds| std::time::Duration::from_millis(milliseconds);
        let mut clicks = DoubleClick::default();
        assert_eq!(clicks.mode(target, cursor, at(0)), MovementMode::Walk);
        assert_eq!(clicks.mode(target, cursor, at(100)), MovementMode::Walk);
        clicks.configure(true);
        assert_eq!(clicks.mode(target, cursor, at(200)), MovementMode::Walk);
        assert_eq!(
            clicks.mode(target, cursor + Vec2::X * 6.0, at(550)),
            MovementMode::Run
        );
        assert_eq!(clicks.mode(target, cursor, at(600)), MovementMode::Walk);
        assert_eq!(clicks.mode(target, cursor, at(951)), MovementMode::Walk);
        assert_eq!(
            clicks.mode(target, cursor + Vec2::X * 7.0, at(1000)),
            MovementMode::Walk
        );
        assert_eq!(
            clicks.mode(Cell::new(41, 80), cursor + Vec2::X * 7.0, at(1050)),
            MovementMode::Walk
        );
        clicks.configure(false);
        clicks.configure(true);
        assert_eq!(
            clicks.mode(Cell::new(41, 80), cursor + Vec2::X * 7.0, at(1100)),
            MovementMode::Walk
        );
    }

    #[test]
    fn session_teardown_forgets_a_pending_double_click() {
        let mut world = World::new();
        let mut clicks = DoubleClick::default();
        clicks.configure(true);
        clicks.mode(Cell::new(40, 80), Vec2::ZERO, std::time::Duration::ZERO);
        world.insert_resource(clicks);
        teardown(&mut world);
        assert_eq!(
            world.resource_mut::<DoubleClick>().mode(
                Cell::new(40, 80),
                Vec2::ZERO,
                std::time::Duration::from_millis(100)
            ),
            MovementMode::Walk
        );
    }

    #[test]
    fn clicking_and_dragging_are_exclusive_and_blocked_gestures_stay_cancelled() {
        let mut gesture = MapGesture::default();
        let mut buttons = ButtonInput::default();
        let origin = Vec2::new(200.0, 100.0);
        buttons.press(MouseButton::Left);
        assert!(!gesture.update(&buttons, origin, true));
        buttons.clear();
        buttons.release(MouseButton::Left);
        assert!(gesture.update(&buttons, origin + Vec2::X * 2.0, true));

        buttons.clear();
        buttons.press(MouseButton::Left);
        gesture.update(&buttons, origin, true);
        buttons.clear();
        gesture.update(&buttons, origin + Vec2::X * 10.0, true);
        assert_eq!(gesture.pan_delta, Some(Vec2::X * 10.0));
        buttons.release(MouseButton::Left);
        assert!(
            !gesture.update(&buttons, origin, true),
            "returning a drag to its origin is not a click"
        );

        for cancel_on_press in [false, true] {
            buttons.clear();
            buttons.press(MouseButton::Left);
            gesture.update(&buttons, origin, !cancel_on_press);
            buttons.clear();
            gesture.update(&buttons, origin + Vec2::X, false);
            gesture.update(&buttons, origin + Vec2::X * 20.0, true);
            assert_eq!(gesture.pan_delta, None);
            buttons.release(MouseButton::Left);
            assert!(!gesture.update(&buttons, origin, true));
        }
    }

    #[test]
    fn actual_camera_projection_picks_cells_at_different_zoom_viewport_and_dpi() {
        let target = Cell::new(41, 83);
        let (x, y) = target.centre();
        let point = Vec3::new(x as f32, -y as f32, 0.0);
        for scale in [0.125, 0.5, 1.0, 4.0, 32.0] {
            for dpi in [1.0, 2.0] {
                let mut projection = OrthographicProjection::default_2d();
                projection.scale = scale;
                projection.update(600.0, 400.0);
                let camera = Camera {
                    viewport: Some(Viewport {
                        physical_position: UVec2::new((100.0 * dpi) as u32, (50.0 * dpi) as u32),
                        physical_size: UVec2::new((600.0 * dpi) as u32, (400.0 * dpi) as u32),
                        ..default()
                    }),
                    computed: ComputedCameraValues {
                        clip_from_view: projection.get_clip_from_view(),
                        target_info: Some(RenderTargetInfo {
                            physical_size: UVec2::new((1000.0 * dpi) as u32, (800.0 * dpi) as u32),
                            scale_factor: dpi,
                        }),
                        ..default()
                    },
                    ..default()
                };
                let transform = GlobalTransform::from_translation(point + Vec3::new(7.0, 3.0, 0.0));
                let cursor = camera.world_to_viewport(&transform, point).unwrap();
                let world = camera.viewport_to_world_2d(&transform, cursor).unwrap();
                assert_eq!(world_cell(world), Some(target), "scale {scale}, dpi {dpi}");
            }
        }
        assert_eq!(world_cell(Vec2::splat(f32::NAN)), None);
        assert_eq!(world_cell(Vec2::new(-1000.0, 1000.0)), None);
    }

    #[test]
    fn refusal_changes_shape_and_color_with_a_fixed_marker_budget() {
        let accepted: Vec<_> = (0..8).map(|slot| marker_line(slot, false)).collect();
        let refused: Vec<_> = (0..8).map(|slot| marker_line(slot, true)).collect();
        assert_eq!(
            accepted
                .iter()
                .filter(|(_, _, visibility)| *visibility == Visibility::Visible)
                .count(),
            8
        );
        assert_eq!(
            refused
                .iter()
                .filter(|(_, _, visibility)| *visibility == Visibility::Visible)
                .count(),
            4
        );
        assert_eq!(accepted[4].0.color, palette::ACCENT);
        assert_eq!(refused[4].0.color, palette::DANGER);
    }
}
