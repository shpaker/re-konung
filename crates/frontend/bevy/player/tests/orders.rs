//! The player's actual click-to-command path, without a native window or GPU.

mod common;

use bevy::{
    camera::{CameraProjection, ComputedCameraValues, RenderTargetInfo, Viewport},
    picking::hover::Hovered,
    prelude::*,
    window::PrimaryWindow,
};
use kn_app::{Action, HeroSelectAction, MainMenuAction, Screen};
use kn_fe_player::{flow::AppFlow, orders::PlayerOrdersPlugin, session};
use kn_game::{
    Game,
    movement::{Cell, MovementMode},
    simulation::HeroOrderStatus,
};
use kn_render::{Clock, CreatureSim, MapCamera};

fn start(app: &mut App, hero: u32) {
    let mut flow = app.world_mut().resource_mut::<AppFlow>();
    flow.submit(Action::MainMenu(MainMenuAction::NewGame));
    for choice in [
        HeroSelectAction::Preview(hero),
        HeroSelectAction::SetClass(0),
        HeroSelectAction::SetCombatStyle(0),
        HeroSelectAction::SetSecondary(0),
        HeroSelectAction::Choose(hero),
    ] {
        flow.submit(Action::HeroSelect(choice));
    }
    app.update();
    app.update();
    assert_eq!(
        app.world().resource::<AppFlow>().state().screen,
        Screen::Playing
    );
}

fn camera_at(app: &mut App, target: Cell, scale: f32) -> Entity {
    let (x, y) = target.centre();
    let mut projection = OrthographicProjection::default_2d();
    projection.scale = scale;
    projection.update(800.0, 600.0);
    app.world_mut()
        .spawn((
            MapCamera,
            Camera {
                viewport: Some(Viewport {
                    physical_size: UVec2::new(800, 600),
                    ..default()
                }),
                computed: ComputedCameraValues {
                    clip_from_view: projection.get_clip_from_view(),
                    target_info: Some(RenderTargetInfo {
                        physical_size: UVec2::new(800, 600),
                        scale_factor: 1.0,
                    }),
                    ..default()
                },
                ..default()
            },
            Projection::Orthographic(projection),
            Transform::from_xyz(x as f32, -y as f32, 0.0),
            GlobalTransform::from_translation(Vec3::new(x as f32, -y as f32, 0.0)),
        ))
        .id()
}

fn next(app: &mut App) {
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .clear();
}

fn click(app: &mut App) {
    next(app);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.update();
    next(app);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    app.update();
    next(app);
}

#[test]
fn a_real_click_orders_the_session_hero_and_catchup_preserves_arrival_feedback() {
    let Some(directory) = common::game_dir() else {
        return;
    };
    let game = Game::open(&directory).unwrap();
    let hero = common::first_hero(&game);
    let mut probe = game
        .simulation_with_hero(game.start_map(hero), session::START_SEED, Some(hero))
        .unwrap();
    let id = probe.hero_id().unwrap();
    let origin = probe
        .states()
        .into_iter()
        .find(|state| state.id == id)
        .unwrap()
        .cell;
    let target = (-4_i32..=4)
        .flat_map(|dy| (-4_i32..=4).map(move |dx| (dx, dy)))
        .filter_map(|(dx, dy)| {
            Some(Cell::new(
                origin.0.checked_add_signed(dx)?,
                origin.1.checked_add_signed(dy)?,
            ))
        })
        .find(|target| {
            target.column.abs_diff(origin.0) + target.row.abs_diff(origin.1) >= 3
                && probe.order_hero_move(*target).is_ok()
        })
        .expect("the start map must provide a nearby reachable destination");
    let mut app = common::player_app(game, vec![hero]);
    app.init_resource::<ButtonInput<MouseButton>>()
        .add_plugins(PlayerOrdersPlugin);
    let mut window = Window {
        resolution: (800, 600).into(),
        focused: true,
        ..default()
    };
    window.set_cursor_position(Some(Vec2::new(400.0, 300.0)));
    let window = app.world_mut().spawn((window, PrimaryWindow)).id();
    let camera = camera_at(&mut app, target, 0.5);
    let ui = app
        .world_mut()
        .spawn((Node::default(), Hovered(false)))
        .id();
    start(&mut app, hero);
    assert!(
        app.world()
            .resource::<CreatureSim>()
            .sim
            .as_ref()
            .unwrap()
            .hero_order()
            .is_none()
    );

    app.world_mut().entity_mut(ui).insert(Hovered(true));
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.update();
    next(&mut app);
    app.world_mut().entity_mut(ui).insert(Hovered(false));
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    app.update();
    next(&mut app);
    assert!(
        app.world()
            .resource::<CreatureSim>()
            .sim
            .as_ref()
            .unwrap()
            .hero_order()
            .is_none()
    );
    app.world_mut()
        .entity_mut(window)
        .get_mut::<Window>()
        .unwrap()
        .focused = false;
    click(&mut app);
    assert!(
        app.world()
            .resource::<CreatureSim>()
            .sim
            .as_ref()
            .unwrap()
            .hero_order()
            .is_none()
    );
    app.world_mut()
        .entity_mut(window)
        .get_mut::<Window>()
        .unwrap()
        .focused = true;
    click(&mut app);
    let simulation = app.world().resource::<CreatureSim>().sim.as_ref().unwrap();
    assert_eq!(simulation.hero_order().unwrap().target, target);
    let before_tick = simulation.tick_number();
    let before_creatures = simulation.states();
    for _ in 0..20 {
        app.world_mut().resource_mut::<Clock>().step(32);
        app.update();
    }
    let simulation = app.world().resource::<CreatureSim>().sim.as_ref().unwrap();
    assert!(simulation.tick_number() > before_tick);
    assert_eq!(
        simulation.hero_order().unwrap().status,
        HeroOrderStatus::Arrived
    );
    assert_eq!(simulation.hero_goal_reaches(), 1);
    assert_eq!(
        simulation
            .states()
            .into_iter()
            .find(|state| state.id == id)
            .unwrap()
            .cell,
        (target.column, target.row)
    );
    assert!(
        simulation.states().iter().any(|state| state.id != id
            && before_creatures
                .iter()
                .any(|before| before.id == state.id && before.cell != state.cell)),
        "NPCs continue walking while the hero's order executes"
    );
    let mut sprites = app.world_mut().query::<&Sprite>();
    assert_eq!(
        sprites
            .iter(app.world())
            .filter(|sprite| sprite.color == kn_ui::palette::ACCENT)
            .count(),
        4
    );

    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.update();
    next(&mut app);
    session::teardown(app.world_mut());
    assert!(app.world().resource::<CreatureSim>().sim.is_none());
    let mut sprites = app.world_mut().query::<&Sprite>();
    assert_eq!(
        sprites
            .iter(app.world())
            .filter(|sprite| sprite.color == kn_ui::palette::ACCENT)
            .count(),
        0
    );
    *app.world_mut().resource_mut::<AppFlow>() = AppFlow::new(vec![hero], kn_settings::defaults());
    start(&mut app, hero);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    app.update();
    assert!(
        app.world()
            .resource::<CreatureSim>()
            .sim
            .as_ref()
            .unwrap()
            .hero_order()
            .is_none(),
        "a held gesture cannot leak into a restarted session"
    );
    app.world_mut().despawn(camera);
}

#[test]
fn an_occupied_clicked_target_shows_a_refusal_and_outside_points_issue_nothing() {
    let Some(directory) = common::game_dir() else {
        return;
    };
    let game = Game::open(&directory).unwrap();
    let hero = common::all_heroes(&game)[0];
    let mut app = common::player_app(game, vec![hero]);
    app.init_resource::<ButtonInput<MouseButton>>()
        .add_plugins(PlayerOrdersPlugin);
    let mut window = Window {
        resolution: (800, 600).into(),
        focused: true,
        ..default()
    };
    window.set_cursor_position(Some(Vec2::new(900.0, 300.0)));
    let window = app.world_mut().spawn((window, PrimaryWindow)).id();
    start(&mut app, hero);
    let simulation = app.world().resource::<CreatureSim>().sim.as_ref().unwrap();
    let target = simulation
        .states()
        .into_iter()
        .find(|state| Some(state.id) != simulation.hero_id())
        .unwrap()
        .cell;
    let target = Cell::new(target.0, target.1);
    camera_at(&mut app, target, 1.0);
    click(&mut app);
    assert!(
        app.world()
            .resource::<CreatureSim>()
            .sim
            .as_ref()
            .unwrap()
            .hero_order()
            .is_none()
    );
    app.world_mut()
        .entity_mut(window)
        .get_mut::<Window>()
        .unwrap()
        .set_cursor_position(Some(Vec2::new(400.0, 300.0)));
    click(&mut app);
    assert!(matches!(
        app.world()
            .resource::<CreatureSim>()
            .sim
            .as_ref()
            .unwrap()
            .hero_order()
            .unwrap()
            .status,
        HeroOrderStatus::Refused(_)
    ));
    let mut sprites = app.world_mut().query::<(&Sprite, &Visibility)>();
    assert_eq!(
        sprites
            .iter(app.world())
            .filter(
                |(sprite, visibility)| sprite.color == kn_ui::palette::DANGER
                    && **visibility == Visibility::Visible
            )
            .count(),
        2
    );
}

#[test]
fn the_applied_setting_enables_double_click_run_and_ui_cancels_the_pair() {
    let Some(directory) = common::game_dir() else {
        return;
    };
    for enabled in [false, true] {
        let game = Game::open(&directory).unwrap();
        let hero = common::first_hero(&game);
        let mut settings = kn_settings::defaults();
        settings.deviations.double_click_run = enabled;
        let mut app = common::player_app_with(game, vec![hero], settings);
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(50),
        ))
        .init_resource::<ButtonInput<MouseButton>>()
        .add_plugins(PlayerOrdersPlugin);
        let mut window = Window {
            resolution: (800, 600).into(),
            focused: true,
            ..default()
        };
        window.set_cursor_position(Some(Vec2::new(400.0, 300.0)));
        let window = app.world_mut().spawn((window, PrimaryWindow)).id();
        let ui = app
            .world_mut()
            .spawn((Node::default(), Hovered(false)))
            .id();
        start(&mut app, hero);
        let simulation = app.world().resource::<CreatureSim>().sim.as_ref().unwrap();
        let target = simulation
            .states()
            .into_iter()
            .find(|state| Some(state.id) == simulation.hero_id())
            .unwrap()
            .cell;
        camera_at(&mut app, Cell::new(target.0, target.1), 1.0);
        let mode = |app: &App| {
            app.world()
                .resource::<CreatureSim>()
                .sim
                .as_ref()
                .unwrap()
                .hero_order()
                .unwrap()
                .mode
        };
        click(&mut app);
        assert_eq!(mode(&app), MovementMode::Walk);
        click(&mut app);
        assert_eq!(
            mode(&app),
            if enabled {
                MovementMode::Run
            } else {
                MovementMode::Walk
            }
        );
        click(&mut app);
        assert_eq!(
            mode(&app),
            MovementMode::Walk,
            "a third click starts a fresh pair"
        );
        app.world_mut().entity_mut(ui).insert(Hovered(true));
        app.update();
        app.world_mut().entity_mut(ui).insert(Hovered(false));
        click(&mut app);
        assert_eq!(
            mode(&app),
            MovementMode::Walk,
            "crossing UI cancels the previous click"
        );
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        next(&mut app);
        app.world_mut()
            .entity_mut(window)
            .get_mut::<Window>()
            .unwrap()
            .set_cursor_position(Some(Vec2::new(410.0, 300.0)));
        app.update();
        next(&mut app);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .release(MouseButton::Left);
        app.update();
        next(&mut app);
        app.world_mut()
            .entity_mut(window)
            .get_mut::<Window>()
            .unwrap()
            .set_cursor_position(Some(Vec2::new(400.0, 300.0)));
        click(&mut app);
        assert_eq!(
            mode(&app),
            MovementMode::Walk,
            "dragging cancels the previous click"
        );
        app.world_mut()
            .entity_mut(window)
            .get_mut::<Window>()
            .unwrap()
            .focused = false;
        app.update();
        app.world_mut()
            .entity_mut(window)
            .get_mut::<Window>()
            .unwrap()
            .focused = true;
        click(&mut app);
        assert_eq!(
            mode(&app),
            MovementMode::Walk,
            "losing focus cancels the previous click"
        );
        app.world_mut()
            .entity_mut(window)
            .get_mut::<Window>()
            .unwrap()
            .set_cursor_position(None);
        app.update();
        app.world_mut()
            .entity_mut(window)
            .get_mut::<Window>()
            .unwrap()
            .set_cursor_position(Some(Vec2::new(400.0, 300.0)));
        click(&mut app);
        assert_eq!(
            mode(&app),
            MovementMode::Walk,
            "losing the cursor cancels the previous click"
        );
    }
}

#[test]
fn a_visible_enemy_pixel_orders_attack_and_ui_capture_prevents_it() {
    let Some(directory) = common::game_dir() else {
        return;
    };
    let game = Game::open(&directory).unwrap();
    let hero = common::first_hero(&game);
    let mut app = common::player_app(game, vec![hero]);
    app.add_plugins(bevy::transform::TransformPlugin)
        .init_resource::<ButtonInput<MouseButton>>()
        .add_plugins(PlayerOrdersPlugin);
    let mut window = Window {
        resolution: (800, 600).into(),
        focused: true,
        ..default()
    };
    window.set_cursor_position(Some(Vec2::new(400.0, 300.0)));
    let window = app.world_mut().spawn((window, PrimaryWindow)).id();
    let ui = app.world_mut().spawn((Node::default(), Hovered(true))).id();
    start(&mut app, hero);
    // The headless fixture omits visibility propagation; picking consumes the
    // same propagated visibility the window supplies for these rendered layers.
    let mut layers = app
        .world_mut()
        .query_filtered::<&mut InheritedVisibility, With<kn_render::CreatureSprite>>();
    for mut visible in layers.iter_mut(app.world_mut()) {
        *visible = InheritedVisibility::VISIBLE;
    }
    let simulation = app.world().resource::<CreatureSim>().sim.as_ref().unwrap();
    let enemies: Vec<_> = simulation
        .states()
        .into_iter()
        .filter(|state| simulation.can_hero_attack(state.id))
        .collect();
    let mut picking =
        bevy::ecs::system::SystemState::<kn_render::CreaturePicker>::new(app.world_mut());
    let picker = picking.get(app.world()).unwrap();
    let (enemy, point) = enemies
        .into_iter()
        .find_map(|enemy| {
            let bounds = picker.bounds_world(enemy.id)?;
            for y in bounds.min.y.floor() as i32..bounds.max.y.ceil() as i32 {
                for x in bounds.min.x.floor() as i32..bounds.max.x.ceil() as i32 {
                    let point = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                    if picker.pick_world(point) == Some(enemy.id) {
                        return Some((enemy, point));
                    }
                }
            }
            None
        })
        .expect("the hero start map must contain a visible attackable enemy");
    let camera = camera_at(&mut app, Cell::new(enemy.cell.0, enemy.cell.1), 1.0);
    let camera_entity = app.world().entity(camera);
    let cursor = camera_entity
        .get::<Camera>()
        .unwrap()
        .world_to_viewport(
            camera_entity.get::<GlobalTransform>().unwrap(),
            point.extend(0.0),
        )
        .unwrap();
    app.world_mut()
        .entity_mut(window)
        .get_mut::<Window>()
        .unwrap()
        .set_cursor_position(Some(cursor));
    click(&mut app);
    assert!(
        app.world()
            .resource::<CreatureSim>()
            .sim
            .as_ref()
            .unwrap()
            .hero_attack_order()
            .is_none(),
        "UI must capture the attack click"
    );
    app.world_mut().entity_mut(ui).insert(Hovered(false));
    click(&mut app);
    let simulation = app.world().resource::<CreatureSim>().sim.as_ref().unwrap();
    let order = simulation.hero_attack_order().unwrap();
    assert_eq!(order.target_id, enemy.id);
    assert!(simulation.hero_order().is_none());
    let caption = kn_fe_player::i18n::t(
        app.world().resource::<AppFlow>().state().applied.language,
        kn_fe_player::i18n::Text::AttackStatus(order.status),
    );
    let mut labels = app.world_mut().query::<&Text>();
    assert!(
        labels
            .iter(app.world())
            .any(|text| text.0.starts_with(caption) && !text.0.contains(&format!("#{}", enemy.id)))
    );
}
