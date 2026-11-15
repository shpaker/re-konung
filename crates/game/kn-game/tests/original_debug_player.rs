//! Runtime-only commands against user-owned data; no game-derived fixtures.
use k1_data::{
    characters::walk_frame_counts,
    game_state,
    repo::{GAME_FILES, GameDir},
};
use k1_game::resources::Resources;
use kn_game::{
    Game, MapRef, MapSource,
    debug::{DebugCommand, DebugJournal},
};
use std::path::PathBuf;

fn root(edition: &str) -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../original/k1")
        .join(edition)
        .join("game");
    if root.is_dir() {
        Some(root)
    } else {
        eprintln!("skipping: user-owned {edition} data absent");
        None
    }
}
#[test]
fn profiles_use_raw_source_walk_counts_in_both_editions() {
    for edition in ["rus", "eng"] {
        let Some(root) = root(edition) else { continue };
        let game = Game::open(&root).unwrap();
        let dir = GameDir::new(&root);
        let resources = Resources::load(&dir).unwrap();
        for hero in game.heroes(MapSource::Game).unwrap() {
            let profile = game
                .debug_player_profile(
                    MapRef {
                        source: MapSource::Game,
                        number: 17,
                    },
                    Some(hero),
                )
                .unwrap();
            let state = game_state::parse(&dir.state_bytes(hero, GAME_FILES).unwrap()).unwrap();
            let character = state.characters[profile.character].as_ref().unwrap();
            let raw = resources
                .with_graphics(|graph, objects| walk_frame_counts(character, graph, objects))
                .unwrap();
            assert_eq!(profile.hero, hero);
            assert_eq!(profile.step_ticks, raw.map(|ticks| ticks as u32));
            assert!(profile.step_ticks.iter().all(|ticks| *ticks > 0));
        }
    }
}
#[test]
fn eng_map17_debug_player_changes_group_state_and_replays_the_same_trace() {
    let Some(root) = root("eng") else { return };
    let game = Game::open(root).unwrap();
    let map = MapRef {
        source: MapSource::Game,
        number: 17,
    };
    let profile = game.debug_player_profile(map, None).unwrap();
    let fresh = || {
        game.simulation_with_hero(map, 42, None)
            .map_err(|error| error.to_string())
    };
    let mut trial = fresh().unwrap();
    let snapshot = trial.debug_snapshot();
    let mut chosen = None;
    'groups: for geometry in snapshot.groups {
        if !snapshot
            .creatures
            .iter()
            .any(|c| c.group == geometry.group && c.used_group_state == 0)
        {
            continue;
        }
        for row in geometry.spawn.top.max(0)..=geometry.spawn.bottom {
            for column in geometry.spawn.left.max(0)..=geometry.spawn.right {
                let cell = (column as u32, row as u32);
                if trial
                    .apply_debug_command(&DebugCommand::Place { cell }, Some(&profile), fresh)
                    .is_err()
                {
                    continue;
                }
                trial.tick();
                if trial
                    .debug_snapshot()
                    .creatures
                    .iter()
                    .any(|c| c.group == geometry.group && c.used_group_state == 1)
                {
                    chosen = Some((geometry, cell));
                    break 'groups;
                }
                trial
                    .apply_debug_command(&DebugCommand::Remove, None, fresh)
                    .unwrap();
            }
        }
    }
    let (geometry, inside) = chosen.expect("map17 must have an accessible roaming spawn");
    let mut outside = None;
    'targets: for row in (geometry.spawn.top - 2).max(0)..=geometry.spawn.bottom + 2 {
        for column in (geometry.spawn.left - 2).max(0)..=geometry.spawn.right + 2 {
            if column >= geometry.spawn.left
                && column <= geometry.spawn.right
                && row >= geometry.spawn.top
                && row <= geometry.spawn.bottom
            {
                continue;
            }
            let cell = (column as u32, row as u32);
            if trial
                .apply_debug_command(&DebugCommand::Move { cell }, None, fresh)
                .is_ok()
                && trial
                    .debug_player()
                    .unwrap()
                    .movement
                    .accepted_target
                    .is_some_and(|(x, y)| {
                        x < (geometry.spawn.left as u32)
                            || x > (geometry.spawn.right as u32)
                            || y < (geometry.spawn.top as u32)
                            || y > (geometry.spawn.bottom as u32)
                    })
            {
                outside = Some(cell);
                break 'targets;
            }
        }
    }
    let outside = outside.expect("roaming spawn must have a reachable outside target");
    eprintln!(
        "local ENG17 debug scenario: group {} inside {inside:?} outside {outside:?}",
        geometry.group
    );
    let trace = || {
        let mut sim = fresh().unwrap();
        let mut journal = DebugJournal::default();
        journal
            .dispatch(
                &mut sim,
                Some(&profile),
                DebugCommand::Place { cell: inside },
                fresh,
            )
            .result
            .unwrap();
        journal
            .seek(&mut sim, Some(&profile), 1, 64, fresh)
            .unwrap();
        assert!(
            sim.debug_snapshot()
                .creatures
                .iter()
                .filter(|c| c.group == geometry.group)
                .all(|c| c.used_group_state == 1)
        );
        journal
            .dispatch(&mut sim, None, DebugCommand::Move { cell: outside }, fresh)
            .result
            .unwrap();
        let mut trace = Vec::new();
        for tick in 2..=2000 {
            let progress = journal
                .seek(&mut sim, Some(&profile), tick, 64, fresh)
                .unwrap();
            assert!(progress.caught_up);
            let snapshot = sim.debug_snapshot();
            let out = snapshot
                .player
                .as_ref()
                .is_some_and(|p| p.movement.accepted_target.is_none());
            trace.push(snapshot);
            if out
                && trace
                    .last()
                    .unwrap()
                    .creatures
                    .iter()
                    .filter(|c| c.group == geometry.group)
                    .all(|c| matches!(c.used_group_state, 0 | 0x20))
            {
                break;
            }
        }
        let last = trace.last().unwrap();
        assert!(
            last.player
                .as_ref()
                .unwrap()
                .movement
                .requested_target
                .is_none()
        );
        assert!(
            last.creatures
                .iter()
                .filter(|c| c.group == geometry.group)
                .all(|c| matches!(c.used_group_state, 0 | 0x20))
        );
        (trace, journal.entries().to_vec())
    };
    assert_eq!(trace(), trace());
}
