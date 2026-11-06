//! The player window shows living creatures: the same request the window makes
//! and the same subsystem the viewer runs must put composite sprites on screen
//! and play a state-4 creature's special once into idle (issue #70 AC4).
//!
//! Tests on real data skip when `original/` is absent (docs/k1/editions.md).

use std::path::PathBuf;
use std::sync::Arc;

use bevy::prelude::*;
use kn_fe_player::{START_SEED, request_start_creatures};
use kn_game::{Game, MapRef, MapSource, SceneOptions, TICK_MS};
use kn_render::{
    AnimPlugin, Clock, CreatureAnimationPlugin, CreatureMotion, CreatureNode, CreatureSim,
    CreatureSprite, GameResource, PlayState, Sequence, TextureCache,
};

/// First edition directory with data, or `None` to skip.
fn game_dir() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../original/k1");
    for edition in ["rus", "eng"] {
        let dir = root.join(edition).join("game");
        if dir.is_dir() {
            return Some(dir);
        }
    }
    eprintln!("skip: no original/k1/{{rus,eng}}/game (docs/k1/editions.md)");
    None
}

/// The app the window runs, minus the window: the shared subsystem, its clock
/// and the resources any window drawing this game provides anyway.
fn app(game: Game, motion: CreatureMotion) -> App {
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
    // The clock moves only through the test: the subsystem has no clock of its
    // own and must stand still while the test is not stepping it.
    app.world_mut()
        .resource_mut::<Clock>()
        .goto(PlayState::Paused);
    app
}

/// The sequence the body sprite of `creature` draws right now.
fn shown_sequence(app: &mut App, creature: usize) -> Arc<Sequence> {
    let mut query = app
        .world_mut()
        .query::<(&CreatureSprite, &kn_render::Animator)>();
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

#[test]
fn the_player_window_asks_for_creatures_and_draws_them() {
    let Some(dir) = game_dir() else {
        return;
    };
    let game = Game::open(&dir).unwrap();
    let hero = game.heroes(MapSource::Game).unwrap()[0];

    let mut motion = CreatureMotion::default();
    let Some((map_ref, _)) = request_start_creatures(&game, hero, &mut motion) else {
        panic!("the hero's start map must build");
    };
    assert_eq!(motion.seed, START_SEED, "the window's own seed is used");
    assert!(motion.rebuild, "the request is pending for the subsystem");

    let mut app = app(game, motion);
    app.update();

    // The scene's pieces carry the creatures and `spawn_scene` skips them:
    // without the subsystem the window would show an empty map.
    let mut nodes = app.world_mut().query_filtered::<(), With<CreatureNode>>();
    assert!(
        nodes.iter(app.world()).next().is_some(),
        "start map {map_ref:?} must put creature nodes on screen"
    );
    let sim = app.world().resource::<CreatureSim>();
    assert!(sim.sim.is_some(), "the window's map must simulate");
    assert_eq!(sim.error, None, "no reason to be without creatures");
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

    let mut motion = CreatureMotion::default();
    motion.request(
        map_ref,
        START_SEED,
        None,
        game.scene(map_ref, &SceneOptions::ALL).unwrap(),
    );
    let mut app = app(game, motion);
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
