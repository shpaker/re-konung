//! Step durations the movement engine reads from real K1 data (skipped when
//! `original/` is absent — docs/k1/editions.md).
//!
//! This is the seam between the two halves of epic #36: the K1 layer counts the
//! walk animation's frames (`k1_game::creatures`), the engine asks
//! [`Pace`] for a duration and knows nothing about animations.

use std::path::PathBuf;

use k1_data::{
    game_state::{self, GameState},
    repo::{GAME_FILES, GameDir},
};
use k1_game::{creatures, resources::Resources};
use kn_game::{
    movement::{CreatureId, Direction, Pace},
    step_durations::StepDurations,
};

fn game_dirs() -> Vec<(&'static str, GameDir)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../original/k1");
    let dirs = ["rus", "eng"]
        .into_iter()
        .filter_map(|edition| {
            let game = GameDir::new(root.join(edition).join("game"));
            if game.root().is_dir() {
                Some((edition, game))
            } else {
                eprintln!("skipping: no original/k1/{edition}/game (docs/k1/editions.md)");
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(
        !root.is_dir() || !dirs.is_empty(),
        "{} exists but holds no edition with a game/ folder: the tests on data would \
         not have run. How to unpack one — docs/k1/editions.md",
        root.display()
    );
    dirs
}

fn state(game: &GameDir, hero: u32) -> GameState {
    let data = game.state_bytes(hero, GAME_FILES).unwrap();
    game_state::parse(&data).unwrap()
}

/// The creature number is its place in the roster, because that is the order the
/// world adds creatures in.
#[test]
fn a_creature_steps_as_long_as_its_walk_animation() {
    for (edition, game) in game_dirs() {
        let resources = Resources::load(&game).unwrap();
        let map_number = game_state::start_map(0);
        let state = state(&game, 0);
        let roster = creatures::roster(&state, map_number, Some(0));
        assert!(
            !roster.is_empty(),
            "{edition}: no creatures on map {map_number}"
        );
        resources
            .with_graphics(|graph, objects| {
                let durations =
                    StepDurations::new(creatures::step_durations(&state, &roster, graph, objects));
                assert_eq!(durations.len(), roster.len());

                let mut checked = 0;
                for (index, creature) in roster.iter().enumerate() {
                    let record = state.characters[creature.id].as_ref().unwrap();
                    let id = CreatureId::new(index as u32);
                    // A human model walks 12 frames in every direction.
                    if !record.has_dynamic_sprite() {
                        for direction in Direction::ALL {
                            assert_eq!(
                                durations.step_ticks(id, direction),
                                12,
                                "{edition} map {map_number}: creature {} of model {}",
                                creature.id,
                                record.sprite_set
                            );
                        }
                        checked += 1;
                    }
                    // A dynamic object without walk frames still steps: the engine
                    // must never be handed a step of no length.
                    if record.has_dynamic_sprite() && record.sprite_set == 3 {
                        for direction in Direction::ALL {
                            assert_eq!(
                                durations.step_ticks(id, direction),
                                1,
                                "{edition} map {map_number}: creature {} of set 3 has no walk frames",
                                creature.id
                            );
                        }
                        checked += 1;
                    }
                }
                assert!(
                    checked > 1,
                    "{edition} map {map_number}: neither a model nor set 3 was reached"
                );
            })
            .unwrap();
    }
}

#[test]
fn an_unknown_creature_does_not_stop_the_engine() {
    for (edition, game) in game_dirs() {
        let resources = Resources::load(&game).unwrap();
        let state = state(&game, 0);
        let roster = creatures::roster(&state, game_state::start_map(0), Some(0));
        resources
            .with_graphics(|graph, objects| {
                let durations =
                    StepDurations::new(creatures::step_durations(&state, &roster, graph, objects));
                let outside = CreatureId::new(durations.len() as u32 + 5);
                assert_eq!(
                    durations.step_ticks(outside, Direction::North),
                    1,
                    "{edition}: a creature the table does not know must still step"
                );
            })
            .unwrap();
    }
}
