//! The player window shows living creatures, built only through the AppFlow
//! pipeline: the main menu loads nothing, «New game» builds the session, and
//! readiness is what promotes `Loading` to `Playing` — never the request
//! itself. A failed session tears down completely and a retry starts clean.
//!
//! Tests on real data skip when `original/` is absent (docs/k1/editions.md).

mod common;

use std::sync::Arc;

use bevy::prelude::*;
use common::{all_heroes, first_hero, game_dir, player_app};
use kn_app::{Action, HeroSelectAction, LoadingAction, MainMenuAction, Screen, SessionError};
use kn_fe_player::START_SEED;
use kn_fe_player::flow::AppFlow;
use kn_fe_player::session;
use kn_game::{Game, MapRef, SceneOptions};
use kn_render::{
    Clock, CreatureMotion, CreatureNode, CreatureRoot, CreatureSim, FineLightGrid,
    LightingMaterials, MapRoot, PlayState, Sequence,
};

/// The sequence the body sprite of `creature` draws right now.
fn shown_sequence(app: &mut App, creature: usize) -> Arc<Sequence> {
    let mut query = app
        .world_mut()
        .query::<(&kn_render::CreatureSprite, &kn_render::Animator)>();
    let world = app.world();
    let (_, animator) = query
        .iter(world)
        .find(|(tag, _)| tag.creature_id == creature && !tag.is_shadow && tag.layer == 0)
        .unwrap_or_else(|| panic!("creature #{creature} must have a body sprite"));
    Arc::clone(&animator.sequence)
}

/// The one-shot clip built for `creature`, if it has one.
fn special_sequence(app: &App, creature: usize) -> Arc<Sequence> {
    let anim = app.world().resource::<kn_render::CreatureAnim>();
    let sequences = anim
        .creatures
        .get(&creature)
        .unwrap_or_else(|| panic!("creature #{creature} must have sequences"));
    let clip = sequences
        .body
        .values()
        .find_map(|layer| layer.special.as_ref())
        .unwrap_or_else(|| panic!("creature #{creature} must have a special clip"));
    Arc::clone(&clip.sequence)
}

/// Drives the flow to `Playing` through legal actions only: the whole
/// new-game flow — «New game», then the hero choice — reduced by one update
/// together with the session build, one more for the watcher's readiness
/// report.
fn play_to_playing(app: &mut App, hero: u32) {
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::MainMenu(MainMenuAction::NewGame));
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::HeroSelect(HeroSelectAction::Choose(hero)));
    app.update();
    assert_eq!(
        app.world().resource::<AppFlow>().state().screen,
        Screen::Loading,
        "the session build is in flight"
    );
    app.update();
    assert_eq!(
        app.world().resource::<AppFlow>().state().screen,
        Screen::Playing,
        "the watcher must report readiness on the next frame"
    );
}

#[test]
fn the_menu_loads_nothing_and_new_game_reaches_playing_only_when_ready() {
    let Some(dir) = game_dir() else {
        return;
    };
    let game = Game::open(&dir).unwrap();
    let hero = first_hero(&game);
    let mut app = player_app(game, vec![hero]);

    // The menu frame: no session of any kind — the old startup loader's map
    // is gone, and nothing ticks behind the menu.
    app.update();
    let flow = app.world().resource::<AppFlow>();
    assert_eq!(flow.state().screen, Screen::MainMenu);
    let mut maps = app.world_mut().query_filtered::<(), With<MapRoot>>();
    assert!(
        maps.iter(app.world()).next().is_none(),
        "no map behind the menu"
    );
    let sim = app.world().get_resource::<CreatureSim>();
    assert!(
        sim.is_none_or(|sim| sim.sim.is_none()),
        "no simulation behind the menu"
    );

    play_to_playing(&mut app, hero);

    // The request the window itself makes: the fixed seed, the chosen hero's
    // start map — the same `Game.N` the scene, the simulation and the clips
    // load — and creatures drawn through the shared subsystem.
    let motion = app.world().resource::<CreatureMotion>();
    assert_eq!(motion.seed, START_SEED, "the window's own seed is used");
    assert_eq!(motion.hero, Some(hero), "the chosen hero rides the request");
    assert_eq!(
        motion.map,
        Some(
            app.world()
                .resource::<kn_render::GameResource>()
                .0
                .start_map(hero)
        ),
        "the session opens on the chosen hero's own start map"
    );
    let mut nodes = app.world_mut().query_filtered::<(), With<CreatureNode>>();
    assert!(
        nodes.iter(app.world()).next().is_some(),
        "the start map must put creature nodes on screen"
    );
    let sim = app.world().resource::<CreatureSim>();
    assert!(sim.sim.is_some(), "the window's map must simulate");
    assert_eq!(sim.error, None, "no reason to be without creatures");
}

#[test]
fn a_failed_session_tears_down_and_a_retry_succeeds() {
    let Some(dir) = game_dir() else {
        return;
    };
    let game = Game::open(&dir).unwrap();
    let hero = first_hero(&game);
    let mut app = player_app(game, vec![hero]);
    app.update();

    // The failure report of a build that died inside the subsystem: reported
    // while still Loading — the watcher's failure branch — and reduced back
    // to the menu with the typed reason, exactly as the table says.
    play_to_loading(&mut app, hero);
    session::teardown(app.world_mut());
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::Loading(LoadingAction::Failed(SessionError::Map)));
    app.update();
    let flow = app.world().resource::<AppFlow>();
    assert_eq!(flow.state().screen, Screen::MainMenu);
    assert_eq!(flow.state().session_error, Some(SessionError::Map));

    // The retry from the failed state: a full session over the cleaned world,
    // the typed reason cleared by the new attempt — through the whole
    // new-game flow again.
    play_to_loading(&mut app, hero);
    app.update();
    let flow = app.world().resource::<AppFlow>();
    assert_eq!(flow.state().screen, Screen::Playing);
    assert_eq!(flow.state().session_error, None);
    let playing_entities = app.world().iter_entities().count();

    // Teardown is the exact inverse of the build and idempotent: a second run
    // finds nothing and changes nothing.
    session::teardown(app.world_mut());
    let torn_down = app.world().iter_entities().count();
    assert!(
        torn_down < playing_entities,
        "teardown must remove entities"
    );
    session::teardown(app.world_mut());
    assert_eq!(
        app.world().iter_entities().count(),
        torn_down,
        "a second teardown must change nothing"
    );
    let mut maps = app
        .world_mut()
        .query_filtered::<(), Or<(With<MapRoot>, With<CreatureRoot>)>>();
    assert!(maps.iter(app.world()).next().is_none());
    assert!(
        app.world().get_resource::<FineLightGrid>().is_none()
            && app.world().get_resource::<LightingMaterials>().is_none()
            && app.world().get_resource::<session::SessionInit>().is_none()
    );
}

/// Drives the flow to `Loading` through legal actions only: the hero chosen,
/// the build in flight — one update runs the reducer and the session build.
fn play_to_loading(app: &mut App, hero: u32) {
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::MainMenu(MainMenuAction::NewGame));
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::HeroSelect(HeroSelectAction::Choose(hero)));
    app.update();
    assert_eq!(
        app.world().resource::<AppFlow>().state().screen,
        Screen::Loading,
        "the session build is in flight"
    );
}

/// Every offered hero starts a session of its own: the chosen `Game.N` rides
/// into the scene, the simulation and the clips as one number, and each
/// choice opens that hero's own start map (#121).
#[test]
fn every_hero_reaches_playing_with_its_own_start_map() {
    let Some(dir) = game_dir() else {
        return;
    };
    let heroes = all_heroes(&Game::open(&dir).unwrap());
    for hero in &heroes {
        let game = Game::open(&dir).unwrap();
        let start_map = game.start_map(*hero);
        let mut app = player_app(game, heroes.clone());
        app.update();
        play_to_playing(&mut app, *hero);
        assert_eq!(
            app.world().resource::<AppFlow>().state().screen,
            Screen::Playing,
            "hero {hero}: the watcher must report readiness"
        );
        let motion = app.world().resource::<CreatureMotion>();
        assert_eq!(
            motion.hero,
            Some(*hero),
            "hero {hero}: one chosen number drives scene, simulation and clips"
        );
        assert_eq!(
            motion.map,
            Some(start_map),
            "hero {hero}: the session opens on the hero's own start map"
        );
        assert!(
            app.world().resource::<CreatureSim>().sim.is_some(),
            "hero {hero}: the start map must simulate"
        );
    }
}

#[test]
fn a_state_four_creature_plays_its_special_once_into_idle_on_screen() {
    let Some(dir) = game_dir() else {
        return;
    };
    let game = Game::open(&dir).unwrap();
    // A map whose simulation holds a creature the file leaves in state 4: the
    // machine seeds it as playing its one-shot special on world load.
    let (map_ref, creature) =
        special_creature(&game).expect("an edition map must have state-4 creatures");

    // The subsystem alone, without the flow: the clip machine is the
    // creature subsystem's own, and this test pins its on-screen behaviour,
    // not the pipeline that asks for it.
    let mut motion = CreatureMotion::default();
    motion.request(
        map_ref,
        START_SEED,
        None,
        game.scene(map_ref, &SceneOptions::ALL).unwrap(),
    );
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        kn_render::AnimPlugin::new(kn_game::TICK_MS as f32),
        kn_render::CreatureAnimationPlugin,
    ))
    .insert_resource(kn_render::GameResource(game))
    .init_resource::<kn_render::TextureCache>()
    .insert_resource(Assets::<Image>::default())
    .insert_resource(motion);
    app.world_mut()
        .resource_mut::<Clock>()
        .goto(PlayState::Paused);
    app.update();

    let special = special_sequence(&app, creature);
    assert!(
        Arc::ptr_eq(&shown_sequence(&mut app, creature), &special),
        "the body sprite of a state-4 creature must be its special clip on world load"
    );

    // The machine runs on the ticks the window hands it: the special ends while
    // the clock advances, never on its own, and the sprite follows it.
    let mut shown_special = 0;
    for tick in 1..600u64 {
        app.world_mut().resource_mut::<Clock>().ticks = tick;
        app.update();
        let sim = app.world().resource::<CreatureSim>();
        let in_special = sim
            .sim
            .as_ref()
            .and_then(|simulation| simulation.creature_stage(creature))
            .is_some_and(|stage| stage.is_special());
        let shown = Arc::ptr_eq(&shown_sequence(&mut app, creature), &special);
        if in_special {
            shown_special += 1;
            assert!(
                shown,
                "creature #{creature} left its special before the machine reached idle (tick {tick})"
            );
        } else {
            assert!(
                !shown,
                "creature #{creature} still shows the special after the machine went idle"
            );
            assert!(
                shown_special > 0,
                "the special must have been shown before idle, not skipped"
            );
            return;
        }
    }
    panic!("creature #{creature} never reached idle on {map_ref:?}");
}

/// First map with a creature the machine seeds into its special, and that creature.
fn special_creature(game: &Game) -> Option<(MapRef, usize)> {
    for &map_ref in game.maps().ok()?.iter() {
        let Ok(simulation) = game.simulation(map_ref, START_SEED) else {
            continue;
        };
        for state in simulation.states() {
            if simulation
                .creature_stage(state.id)
                .is_some_and(|stage| stage.is_special())
            {
                return Some((map_ref, state.id));
            }
        }
    }
    None
}
