//! Every roaming member the game loads must get a creature node (review of
//! #101, finding P2-1): the static renderer emits only a squad's first member
//! (`k1_data::characters::placed`), so members 2..size had no scene pieces and
//! [`kn_render::build_creature_spawns`] silently dropped them — no plan, no
//! node, no creature on screen although the simulation runs them. The fix
//! drives creature geometry and clips from the roster (`Scene::creature_pieces`
//! and `Game::creature_animations`) instead of the digest-guarded `Scene::pieces`,
//! leaving the static source list untouched.
//!
//! The subject is map 17: six calm roaming squads, twenty-eight monsters, in
//! both shipped editions (docs/k1/creatures.md), the same set
//! `original_simulation.rs` counts through the roster. Tests skip when
//! `original/` is absent (docs/k1/editions.md).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use bevy::prelude::*;
use k1_data::{
    game_state,
    repo::{GAME_FILES, GameDir},
};
use kn_game::{Game, MapRef, MapSource, SceneOptions, TICK_MS};
use kn_render::{
    AnimPlugin, Clock, CreatureAnimationPlugin, CreatureMotion, CreatureNode, GameResource,
    PlayState, TextureCache, build_creature_anim, build_creature_spawns,
};

/// The multi-creature map: six roaming squads (twenty-eight monsters).
const MAP: u32 = 17;
/// The seed is arbitrary: this branch places no squad, so `initial_pixels` is
/// the file cells and independent of it.
const SEED: u64 = 0x5EED_0171;
/// Squad 1416 of map 17 in the shipped files: five records from 1416. The
/// subject group — its absence is a panic, not a silent skip.
const SQUAD_FIRST: usize = 1416;
const SQUAD_SIZE: usize = 5;
/// 28 = 5 + 5 + 5 + 4 + 5 + 4 roaming members across the six squads.
const ROAMING_MEMBERS: usize = 28;

fn game_dirs() -> Vec<(&'static str, PathBuf)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../original/k1");
    let dirs = ["rus", "eng"]
        .into_iter()
        .filter_map(|edition| {
            let game = root.join(edition).join("game");
            if game.is_dir() {
                Some((edition, game))
            } else {
                eprintln!("skipping: no original/k1/{edition}/game (docs/k1/editions.md)");
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(
        !root.is_dir() || !dirs.is_empty(),
        "{root:?} exists but holds no edition with a game/ folder: the data-backed \
         tests would not have run (docs/k1/editions.md)"
    );
    dirs
}

/// The map's expectations, read from the file itself rather than the code under
/// test, so a change to the subject data is a panic rather than a green skip.
fn assert_subject_data(edition: &str, dir: PathBuf, hero: u32) -> HashSet<usize> {
    let dir = GameDir::new(dir);
    let state = game_state::parse(
        &dir.state_bytes(hero, GAME_FILES)
            .expect("the hero's starting state"),
    )
    .unwrap();
    let squad = state
        .groups
        .iter()
        .flatten()
        .find(|group| {
            group.map_id as u32 == MAP
                && group.is_roaming()
                && group.first_character as usize == SQUAD_FIRST
        })
        .unwrap_or_else(|| {
            panic!(
                "{edition}: map {MAP} must hold the squad of records {SQUAD_FIRST}.. — \
                 the review subject is gone from the data"
            )
        });
    assert_eq!(
        squad.size as usize, SQUAD_SIZE,
        "{edition}: squad of record {SQUAD_FIRST} changed its size"
    );
    state
        .groups
        .iter()
        .flatten()
        .filter(|group| group.map_id as u32 == MAP && group.is_roaming())
        .flat_map(|group| group.members())
        .collect()
}

/// The review's claim at the build boundary: every id the simulation starts with
/// gets exactly one plan and one origin, so the roster-driven path no longer
/// drops the members the static scene under-emits — while `Scene::pieces` itself
/// keeps under-emitting them, which is what proves the digest source is untouched.
#[test]
fn every_loaded_roaming_member_gets_a_unique_creature_plan() {
    for (edition, dir) in game_dirs() {
        let game = Game::open(&dir).unwrap();
        let hero = game.heroes(MapSource::Game).unwrap()[0];
        let map_ref = MapRef {
            source: MapSource::Game,
            number: MAP,
        };
        let roaming = assert_subject_data(edition, dir.clone(), hero);
        assert_eq!(
            roaming.len(),
            ROAMING_MEMBERS,
            "{edition}: map {MAP} holds six squads and {ROAMING_MEMBERS} monsters, found {}",
            roaming.len()
        );

        let scene = game.scene(map_ref, &SceneOptions::ALL).unwrap();
        let simulation = game.simulation(map_ref, SEED).unwrap();
        let initial = simulation.initial_pixels();
        let mut cache = TextureCache::default();
        let mut images = Assets::<Image>::default();
        let (anim, _report) = build_creature_anim(
            &game,
            map_ref,
            None,
            &scene.creature_pieces,
            &mut cache,
            &mut images,
        );
        let spawns = build_creature_spawns(&scene, &cache, &anim, &initial);

        let want: HashSet<usize> = initial.iter().map(|(id, _)| *id).collect();
        let got: HashSet<usize> = spawns.plans.iter().map(|plan| plan.id as usize).collect();
        let missing: Vec<usize> = want
            .iter()
            .copied()
            .filter(|id| !got.contains(id))
            .collect();
        assert!(
            missing.is_empty(),
            "{edition}: map {MAP}: {missing:?} stand in the simulation with no creature plan — \
             the roster piece set under-emits them (P2-1)"
        );
        assert_eq!(
            got.len(),
            initial.len(),
            "{edition}: map {MAP}: one plan per initial pixel, no duplicates"
        );
        // No plan or origin for a creature the simulation never started with.
        assert!(
            spawns.origins.keys().all(|id| want.contains(id)),
            "{edition}: map {MAP}: a spawn origin exists for an id absent from the simulation"
        );

        // The subject squad: every member, not just the first, carries a plan
        // and an origin that matches the simulation's own initial pixel.
        let initial_by_id: HashMap<usize, (i32, i32)> = initial.iter().copied().collect();
        for id in SQUAD_FIRST..SQUAD_FIRST + SQUAD_SIZE {
            assert!(
                got.contains(&id),
                "{edition}: map {MAP}: squad member #{id} got no creature plan"
            );
            assert_eq!(
                spawns.origins.get(&id).copied(),
                initial_by_id.get(&id).copied(),
                "{edition}: map {MAP}: squad member #{id} origin does not match the simulation"
            );
        }
        for id in &roaming {
            assert!(
                got.contains(id),
                "{edition}: map {MAP}: roaming member #{id} got no creature plan"
            );
        }

        // Static-scene-unchanged contrast, the falsifiable core: the non-first
        // members are absent from the digest-guarded `Scene::pieces` (the static
        // list still emits only the squad's first member) yet present in the
        // roster piece set the nodes are built from.
        let static_ids: HashSet<usize> = scene
            .pieces
            .iter()
            .filter_map(|piece| piece.creature_id)
            .collect();
        let roster_ids: HashSet<usize> = scene
            .creature_pieces
            .iter()
            .filter_map(|piece| piece.creature_id)
            .collect();
        assert!(
            static_ids.contains(&SQUAD_FIRST),
            "{edition}: map {MAP}: the squad's first member must stay in the static scene"
        );
        for id in SQUAD_FIRST + 1..SQUAD_FIRST + SQUAD_SIZE {
            assert!(
                !static_ids.contains(&id),
                "{edition}: map {MAP}: squad member #{id} leaked into the static draw list — \
                 the digest source must stay under-emitting (P2-1)"
            );
            assert!(
                roster_ids.contains(&id),
                "{edition}: map {MAP}: squad member #{id} missing from the roster piece set"
            );
        }
    }
}

/// The spawned node stands where the simulation places the creature: the node's
/// world position is the simulation pixel, read through the real transform the
/// pipeline writes (root node positions match Simulation).
#[test]
fn roaming_member_node_positions_match_the_simulation() {
    let Some((edition, dir)) = game_dirs().into_iter().next() else {
        return;
    };
    let game = Game::open(&dir).unwrap();
    let map_ref = MapRef {
        source: MapSource::Game,
        number: MAP,
    };
    let simulation = game.simulation(map_ref, SEED).unwrap();
    let initial_by_id: HashMap<usize, (i32, i32)> =
        simulation.initial_pixels().into_iter().collect();
    let states: HashMap<usize, (i32, i32)> = simulation
        .states()
        .into_iter()
        .map(|state| (state.id, state.pixel))
        .collect();

    let scene = game.scene(map_ref, &SceneOptions::ALL).unwrap();
    let mut motion = CreatureMotion::default();
    motion.request(map_ref, SEED, None, scene);
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        bevy::transform::TransformPlugin,
        AnimPlugin::new(TICK_MS as f32),
        CreatureAnimationPlugin,
    ))
    .insert_resource(GameResource(game))
    .init_resource::<TextureCache>()
    .insert_resource(Assets::<Image>::default())
    .insert_resource(motion);
    app.world_mut()
        .resource_mut::<Clock>()
        .goto(PlayState::Paused);
    // One update runs Prepare → Advance → Spawn → Swap → Nodes: the nodes are
    // spawned and placed on the simulation pixel.
    app.update();

    let nodes: HashMap<usize, (f32, f32)> = {
        let mut query = app.world_mut().query::<(&CreatureNode, &Transform)>();
        query
            .iter(app.world())
            .map(|(node, transform)| {
                (
                    node.id as usize,
                    (transform.translation.x, transform.translation.y),
                )
            })
            .collect()
    };

    for id in SQUAD_FIRST..SQUAD_FIRST + SQUAD_SIZE {
        let (x, y) = *states
            .get(&id)
            .or_else(|| initial_by_id.get(&id))
            .unwrap_or_else(|| panic!("{edition}: squad member #{id} must be simulated"));
        let (tx, ty) = nodes.get(&id).unwrap_or_else(|| {
            panic!("{edition}: map {MAP}: squad member #{id} got no spawned node (P2-1)")
        });
        assert_eq!(
            (*tx, *ty),
            (x as f32, -(y as f32)),
            "{edition}: map {MAP}: squad member #{id} node does not sit on the simulation pixel"
        );
    }
    // No node for a creature the simulation never started with, and every
    // simulated creature is represented. A creature the world refused to place
    // is still in `initial_pixels` and drawn statically (the established
    // `spawn_failures` policy), so it appears here too — it is never a foreign
    // id outside the starting set.
    let initial_ids: HashSet<usize> = initial_by_id.keys().copied().collect();
    assert!(
        nodes.keys().all(|id| initial_ids.contains(id)),
        "{edition}: map {MAP}: a node exists for a creature absent from the simulation"
    );
    for id in states.keys() {
        assert!(
            nodes.contains_key(id),
            "{edition}: map {MAP}: simulated creature #{id} has no creature node"
        );
    }
}
