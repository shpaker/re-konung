//! Workspace input bindings and gesture ownership.
use crate::action::{ActionQueue, InteractionMode, ViewerAction, ViewerWorkspace};
use bevy::input::mouse::{AccumulatedMouseMotion, MouseWheel};
use bevy::input_focus::InputFocus;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use kn_input::{ZoomScroll, pan_direction, wheel_steps};
use kn_render::{
    CameraInput, CameraState, CreaturePicker, MapCamera, MapViewport, PAN_SPEED, ortho_scale,
};
use kn_ui::pointer_over_ui;

#[derive(Resource, Default)]
pub(crate) struct Gesture {
    left_origin: Option<Vec2>,
    left_blocked: bool,
    left_moved: bool,
    middle_blocked: bool,
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<Gesture>()
        .add_systems(Startup, spawn_highlights)
        .add_systems(
            Update,
            (hotkeys, camera_control).chain().in_set(CameraInput),
        )
        .add_systems(
            PostUpdate,
            (pick, highlight)
                .chain()
                .after(bevy::transform::TransformSystems::Propagate)
                .after(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate)
                .before(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
        );
}

fn keyboard_owned(workspace: &ViewerWorkspace, focus: Option<&InputFocus>) -> bool {
    workspace.keyboard_captured || focus.is_some_and(|focus| focus.get().is_some())
}

fn hotkeys(
    viewer: Res<crate::Viewer>,
    capture: Option<Res<crate::control::ControlServer>>,
    keys: Res<ButtonInput<KeyCode>>,
    focus: Option<Res<InputFocus>>,
    workspace: Res<ViewerWorkspace>,
    mut actions: ResMut<ActionQueue>,
    mut gesture: ResMut<Gesture>,
) {
    if capture.as_ref().is_some_and(|capture| capture.capturing()) {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        actions.0.push_back(ViewerAction::Escape {});
        gesture.left_origin = None;
        gesture.left_blocked = true;
        gesture.middle_blocked = true;
        return;
    }
    if keyboard_owned(&workspace, focus.as_deref()) {
        return;
    }
    if keys.just_pressed(KeyCode::KeyN) {
        actions.0.push_back(ViewerAction::ConstantDay {
            enabled: !viewer.constant_day,
        });
    }
    for (key, action) in [
        (KeyCode::Space, ViewerAction::TogglePlayback {}),
        (KeyCode::Period, ViewerAction::Step { ticks: 1 }),
        (KeyCode::Comma, ViewerAction::Step { ticks: -1 }),
        (KeyCode::KeyF, ViewerAction::FocusSelection {}),
        (KeyCode::Home, ViewerAction::FitMap {}),
    ] {
        if keys.just_pressed(key) {
            actions.0.push_back(action);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn camera_control(
    capture: Option<Res<crate::control::ControlServer>>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    hovered: Query<&Hovered, With<Node>>,
    windows: Query<&Window>,
    mut focus: Option<ResMut<InputFocus>>,
    workspace: Res<ViewerWorkspace>,
    viewport: Res<MapViewport>,
    time: Res<Time>,
    mut gesture: ResMut<Gesture>,
    mut state: ResMut<CameraState>,
    camera: Single<(&mut Transform, &Projection), With<MapCamera>>,
    mut scroll: Local<ZoomScroll>,
) {
    if capture.as_ref().is_some_and(|capture| capture.capturing()) {
        wheel.clear();
        return;
    }
    let cursor = windows.iter().next().and_then(Window::cursor_position);
    let over_ui = pointer_over_ui(hovered.iter().map(|hovered| hovered.0));
    let inside = cursor.is_some_and(|cursor| {
        viewport
            .logical_rect
            .is_none_or(|rect| rect.contains(cursor))
    });
    if buttons.just_pressed(MouseButton::Left) {
        gesture.left_origin = cursor;
        gesture.left_blocked = over_ui || !inside;
        if !gesture.left_blocked
            && let Some(focus) = focus.as_mut()
        {
            focus.clear();
        }
        gesture.left_moved = false;
    }
    if buttons.just_pressed(MouseButton::Middle) {
        gesture.middle_blocked = over_ui || !inside;
    }
    if let (Some(origin), Some(cursor)) = (gesture.left_origin, cursor)
        && origin.distance(cursor) > 4.0
    {
        gesture.left_moved = true;
    }
    let (mut transform, projection) = camera.into_inner();
    let scale = ortho_scale(projection);
    let direction = if keyboard_owned(&workspace, focus.as_deref()) {
        Vec2::ZERO
    } else {
        pan_direction(&keys)
    };
    transform.translation.x += direction.x * PAN_SPEED * time.delta_secs() * scale;
    transform.translation.y += direction.y * PAN_SPEED * time.delta_secs() * scale;
    let left_pan = workspace.mode == InteractionMode::Pan
        && buttons.pressed(MouseButton::Left)
        && !gesture.left_blocked;
    let middle_pan = buttons.pressed(MouseButton::Middle) && !gesture.middle_blocked;
    if left_pan || middle_pan {
        transform.translation.x -= motion.delta.x * scale;
        transform.translation.y += motion.delta.y * scale;
    }
    let steps = wheel_steps(&mut scroll, &mut wheel, over_ui || !inside);
    if steps != 0 {
        state.step(steps);
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn pick(
    capture: Option<Res<crate::control::ControlServer>>,
    mut queue: ResMut<ActionQueue>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    viewport: Res<MapViewport>,
    hovered: Query<&Hovered, With<Node>>,
    picker: CreaturePicker,
    camera: Single<(&Camera, &GlobalTransform), With<MapCamera>>,
    mut workspace: ResMut<ViewerWorkspace>,
    mut gesture: ResMut<Gesture>,
) {
    if capture.as_ref().is_some_and(|capture| capture.capturing()) {
        return;
    }
    let cursor = windows.iter().next().and_then(Window::cursor_position);
    let inside = cursor.is_some_and(|cursor| {
        viewport
            .logical_rect
            .is_none_or(|rect| rect.contains(cursor))
    });
    let over_ui = pointer_over_ui(hovered.iter().map(|hovered| hovered.0));
    let (camera, transform) = camera.into_inner();
    workspace.hovered_creature = if inside && !over_ui {
        cursor.and_then(|cursor| picker.pick_screen(cursor, camera, transform))
    } else {
        None
    };
    if buttons.just_released(MouseButton::Left) {
        if workspace.mode != InteractionMode::Pan
            && !gesture.left_blocked
            && !gesture.left_moved
            && gesture.left_origin.is_some()
            && inside
            && !over_ui
        {
            if workspace.mode == InteractionMode::Select {
                queue.0.push_back(ViewerAction::Select {
                    id: workspace.hovered_creature,
                });
            } else if let Some(cell) = cursor
                .and_then(|cursor| camera.viewport_to_world_2d(transform, cursor).ok())
                .and_then(crate::debug::world_cell)
            {
                queue.0.push_back(ViewerAction::DebugClick {
                    column: cell.column,
                    row: cell.row,
                });
            } else {
                workspace.set_feedback("Error: point is outside the fine grid");
            }
        }
        gesture.left_origin = None;
    }
}

#[derive(Component)]
struct Highlight {
    slot: usize,
}

#[derive(Clone, Copy, PartialEq)]
struct HighlightFrame {
    selected: Option<Rect>,
    hovered: Option<Rect>,
    scale: f32,
}

type HighlightLines<'w, 's> = Query<
    'w,
    's,
    (
        &'static Highlight,
        &'static mut Sprite,
        &'static mut Transform,
        &'static mut GlobalTransform,
        &'static mut Visibility,
        &'static mut InheritedVisibility,
    ),
    Without<kn_render::CreatureSprite>,
>;

fn spawn_highlights(mut commands: Commands) {
    for slot in 0..8 {
        commands.spawn((
            Highlight { slot },
            Sprite::from_color(Color::NONE, Vec2::ONE),
            Transform::default(),
            GlobalTransform::default(),
            Visibility::Hidden,
            InheritedVisibility::HIDDEN,
            bevy::camera::visibility::NoFrustumCulling,
        ));
    }
}

fn highlight(
    picker: CreaturePicker,
    workspace: Res<ViewerWorkspace>,
    mut lines: HighlightLines,
    camera: Single<&Projection, With<MapCamera>>,
    mut last: Local<Option<HighlightFrame>>,
) {
    let frame = HighlightFrame {
        selected: workspace
            .selected_creature
            .and_then(|id| picker.bounds_world(id)),
        hovered: workspace
            .hovered_creature
            .filter(|id| Some(*id) != workspace.selected_creature)
            .and_then(|id| picker.bounds_world(id)),
        scale: ortho_scale(&camera),
    };
    if *last == Some(frame) {
        return;
    }
    *last = Some(frame);
    update_highlight_pool(frame, &mut lines);
}

fn update_highlight_pool(frame: HighlightFrame, lines: &mut HighlightLines) {
    for (line, mut sprite, mut transform, mut global, mut visibility, mut inherited) in
        lines.iter_mut()
    {
        let selected = line.slot < 4;
        let bounds = if selected {
            frame.selected
        } else {
            frame.hovered
        };
        let Some(bounds) = bounds else {
            *visibility = Visibility::Hidden;
            *inherited = InheritedVisibility::HIDDEN;
            continue;
        };
        let thickness = if selected {
            2.0 * frame.scale
        } else {
            frame.scale
        };
        let min = bounds.min - Vec2::splat(3.0 * frame.scale);
        let max = bounds.max + Vec2::splat(3.0 * frame.scale);
        let center = (min + max) * 0.5;
        let edges = [
            (
                Vec2::new(center.x, min.y),
                Vec2::new(max.x - min.x, thickness),
            ),
            (
                Vec2::new(center.x, max.y),
                Vec2::new(max.x - min.x, thickness),
            ),
            (
                Vec2::new(min.x, center.y),
                Vec2::new(thickness, max.y - min.y),
            ),
            (
                Vec2::new(max.x, center.y),
                Vec2::new(thickness, max.y - min.y),
            ),
        ];
        let (position, size) = edges[line.slot % 4];
        sprite.custom_size = Some(size);
        sprite.color = if selected {
            kn_ui::palette::ACCENT
        } else {
            kn_ui::palette::TEXT
        };
        *transform = Transform::from_translation(position.extend(900.0));
        // Picking reads propagated creature geometry, so feedback writes both
        // transforms before visibility checking in this same render frame.
        *global = GlobalTransform::from(*transform);
        *visibility = Visibility::Visible;
        *inherited = InheritedVisibility::VISIBLE;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DEFAULT_OVERLAYS, Rebuild, Viewer};
    use kn_render::{Clock, CreatureSim, TextureCache};

    fn app() -> (App, Entity, Entity, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Gesture>()
            .init_resource::<ViewerWorkspace>()
            .init_resource::<ActionQueue>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<AccumulatedMouseMotion>()
            .init_resource::<CameraState>()
            .init_resource::<InputFocus>()
            .init_resource::<TextureCache>()
            .init_resource::<CreatureSim>()
            .init_resource::<Rebuild>()
            .insert_resource(Clock::new(60.0))
            .insert_resource(Viewer {
                maps: vec![],
                selected: 0,
                overlays: DEFAULT_OVERLAYS,
                playback: crate::PlayState::Playing,
                constant_day: false,
            })
            .insert_resource(MapViewport {
                logical_rect: Some(Rect::from_corners(
                    Vec2::new(100.0, 0.0),
                    Vec2::new(1000.0, 800.0),
                )),
                scale_factor: 1.0,
            })
            .add_message::<MouseWheel>()
            .add_systems(Update, (hotkeys, camera_control).chain())
            .add_systems(PostUpdate, (pick, crate::action::drain_actions).chain());
        let mut window = Window {
            resolution: (1000, 800).into(),
            ..default()
        };
        window.set_cursor_position(Some(Vec2::new(200.0, 200.0)));
        let window = app.world_mut().spawn(window).id();
        let camera = app
            .world_mut()
            .spawn((
                MapCamera,
                Camera::default(),
                GlobalTransform::default(),
                Transform::default(),
                Projection::Orthographic(OrthographicProjection::default_2d()),
            ))
            .id();
        let ui = app
            .world_mut()
            .spawn((Node::default(), Hovered(false)))
            .id();
        (app, window, camera, ui)
    }
    fn next(app: &mut App) {
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .clear();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut()
            .resource_mut::<AccumulatedMouseMotion>()
            .delta = Vec2::ZERO;
    }
    #[test]
    fn a_ui_origin_drag_cannot_pan_after_leaving_ui() {
        for button in [MouseButton::Left, MouseButton::Middle] {
            let (mut app, window, camera, ui) = app();
            app.world_mut().resource_mut::<ViewerWorkspace>().mode = InteractionMode::Pan;
            app.world_mut().entity_mut(ui).insert(Hovered(true));
            app.world_mut()
                .entity_mut(window)
                .get_mut::<Window>()
                .unwrap()
                .set_cursor_position(Some(Vec2::new(50.0, 200.0)));
            app.world_mut()
                .resource_mut::<ButtonInput<MouseButton>>()
                .press(button);
            app.update();
            next(&mut app);
            app.world_mut().entity_mut(ui).insert(Hovered(false));
            app.world_mut()
                .entity_mut(window)
                .get_mut::<Window>()
                .unwrap()
                .set_cursor_position(Some(Vec2::new(200.0, 200.0)));
            app.world_mut()
                .resource_mut::<AccumulatedMouseMotion>()
                .delta = Vec2::new(50.0, 50.0);
            app.update();
            assert_eq!(
                app.world()
                    .entity(camera)
                    .get::<Transform>()
                    .unwrap()
                    .translation,
                Vec3::ZERO,
                "{button:?}"
            );
        }
    }
    #[test]
    fn empty_map_click_clears_selection_through_dispatcher() {
        let (mut app, _, _, _) = app();
        app.world_mut()
            .resource_mut::<ViewerWorkspace>()
            .selected_creature = Some(7);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        next(&mut app);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .release(MouseButton::Left);
        app.update();
        assert_eq!(
            app.world().resource::<ViewerWorkspace>().selected_creature,
            None
        );
        assert_eq!(
            app.world().resource::<ViewerWorkspace>().feedback,
            "Selection cleared"
        );
    }
    #[test]
    fn escape_cancels_drag_and_retains_selection() {
        let (mut app, _, camera, _) = app();
        app.world_mut().insert_resource(ViewerWorkspace {
            selected_creature: Some(7),
            mode: InteractionMode::Pan,
            display_open: true,
            time_open: true,
            ..default()
        });
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        next(&mut app);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.world_mut()
            .resource_mut::<AccumulatedMouseMotion>()
            .delta = Vec2::splat(20.0);
        app.update();
        let workspace = app.world().resource::<ViewerWorkspace>();
        assert_eq!(workspace.selected_creature, Some(7));
        assert_eq!(workspace.mode, InteractionMode::Select);
        assert!(!workspace.display_open && !workspace.time_open);
        assert_eq!(
            app.world()
                .entity(camera)
                .get::<Transform>()
                .unwrap()
                .translation,
            Vec3::ZERO
        );
    }
    #[test]
    fn focused_controls_block_world_keyboard_actions() {
        let (mut app, _, camera, ui) = app();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(ui, bevy::input_focus::FocusCause::Navigated);
        for key in [
            KeyCode::KeyW,
            KeyCode::Space,
            KeyCode::Period,
            KeyCode::Comma,
            KeyCode::KeyN,
        ] {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(key);
        }
        app.update();
        assert_eq!(
            app.world().resource::<Viewer>().playback,
            crate::PlayState::Playing
        );
        assert!(!app.world().resource::<Viewer>().constant_day);
        assert_eq!(app.world().resource::<Clock>().ticks, 0);
        assert_eq!(
            app.world()
                .entity(camera)
                .get::<Transform>()
                .unwrap()
                .translation,
            Vec3::ZERO
        );
    }

    #[test]
    fn n_hotkey_uses_the_shared_constant_day_action_once_per_press() {
        let (mut app, _, _, _) = app();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyN);
        app.update();
        assert!(app.world().resource::<Viewer>().constant_day);
        next(&mut app);
        app.update();
        assert!(app.world().resource::<Viewer>().constant_day);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::KeyN);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyN);
        app.update();
        assert!(!app.world().resource::<Viewer>().constant_day);
    }
    #[test]
    fn focused_ui_owns_keyboard_shortcuts() {
        let workspace = ViewerWorkspace::default();
        assert!(!keyboard_owned(&workspace, None));
        assert!(keyboard_owned(
            &ViewerWorkspace {
                keyboard_captured: true,
                ..default()
            },
            None
        ));
        assert!(keyboard_owned(
            &workspace,
            Some(&InputFocus::from_entity(Entity::PLACEHOLDER))
        ));
    }
    #[derive(Resource)]
    struct TestHighlightFrame(HighlightFrame);
    fn drive_highlights(frame: Res<TestHighlightFrame>, mut lines: HighlightLines) {
        update_highlight_pool(frame.0, &mut lines);
    }
    #[test]
    fn moving_selection_updates_pooled_visible_global_transforms_in_the_same_frame() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TestHighlightFrame(HighlightFrame {
                selected: Some(Rect::from_corners(
                    Vec2::new(10.0, 20.0),
                    Vec2::new(30.0, 50.0),
                )),
                hovered: None,
                scale: 8.0,
            }))
            .add_systems(Startup, spawn_highlights)
            .add_systems(Update, drive_highlights);
        app.update();
        let mut query = app.world_mut().query::<(
            Entity,
            &Highlight,
            &Transform,
            &GlobalTransform,
            &Sprite,
            &InheritedVisibility,
        )>();
        let original = query
            .iter(app.world())
            .find(|(_, line, _, _, _, _)| line.slot == 0)
            .map(|(entity, _, transform, global, sprite, visible)| {
                assert!(visible.get());
                assert_eq!(global.translation(), transform.translation);
                assert_eq!(sprite.custom_size.unwrap().y, 16.0);
                (entity, global.translation())
            })
            .unwrap();
        app.world_mut()
            .resource_mut::<TestHighlightFrame>()
            .0
            .selected = Some(Rect::from_corners(
            Vec2::new(60.0, 70.0),
            Vec2::new(80.0, 100.0),
        ));
        app.update();
        let (_, _, transform, global, _, visible) = query.get(app.world(), original.0).unwrap();
        assert!(visible.get());
        assert_eq!(global.translation(), transform.translation);
        assert_eq!(
            global.translation() - original.1,
            Vec3::new(50.0, 50.0, 0.0)
        );
        assert_eq!(query.iter(app.world()).count(), 8);
        app.world_mut()
            .resource_mut::<TestHighlightFrame>()
            .0
            .selected = None;
        app.update();
        assert!(!query.get(app.world(), original.0).unwrap().5.get());
    }
    #[test]
    fn capture_still_synchronizes_selection_feedback_to_current_bounds() {
        use std::io::Write;
        let (mut app, _, _, _) = app();
        let map = kn_game::MapRef {
            source: kn_game::MapSource::Game,
            number: 1,
        };
        app.world_mut().resource_mut::<Viewer>().maps = vec![map];
        app.insert_resource(crate::Stats {
            map: Some(map),
            ..default()
        })
        .add_systems(Startup, spawn_highlights);
        let directory = tempfile::tempdir().unwrap();
        let session = directory.path().join("session.json");
        crate::control::install(&mut app, &session).unwrap();
        app.update();
        let ids = app
            .world_mut()
            .query_filtered::<Entity, With<Highlight>>()
            .iter(app.world())
            .collect::<Vec<_>>();
        for &entity in &ids {
            app.world_mut()
                .entity_mut(entity)
                .insert((Visibility::Visible, InheritedVisibility::VISIBLE));
        }
        app.add_systems(PostUpdate, highlight.after(crate::control::finish));
        let session: serde_json::Value =
            serde_json::from_slice(&std::fs::read(session).unwrap()).unwrap();
        let mut stream =
            std::net::TcpStream::connect(session["address"].as_str().unwrap()).unwrap();
        writeln!(stream,"{}",serde_json::json!({"id":1,"token":session["token"],"action":{"type":"capture","path":directory.path().join("capture.png")}})).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            app.update();
            if app
                .world()
                .resource::<crate::control::ControlServer>()
                .capturing()
            {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        for entity in ids {
            assert!(
                !app.world()
                    .entity(entity)
                    .get::<InheritedVisibility>()
                    .unwrap()
                    .get()
            );
        }
    }
}
