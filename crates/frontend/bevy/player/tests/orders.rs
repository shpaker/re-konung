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
use kn_game::{Game, movement::Cell, simulation::HeroOrderStatus};
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
