//! The creature roster and its step durations on real data of both editions
//! (skipped when `original/` is absent — docs/k1/editions.md).
//!
//! Mirror of `tests/k1/test_characters.py`; the measured duration table lives in
//! docs/k1/movement.md, "Step duration".

use std::collections::HashSet;
use std::path::PathBuf;

use k1_data::{
    game_state::{self, GameState},
    repo::{GAME_FILES, GameDir},
};
use k1_game::{creatures, resources::Resources};

/// Walk frames per direction, measured over `GRAPH.RES` and `Objects.res` of both
/// editions: everything walks 12 frames but these sprite sets.
const SPECIAL_DYNAMIC: &[(u8, [usize; 8])] = &[
    (3, [0, 0, 0, 0, 0, 0, 0, 0]),
    (19, [2, 2, 2, 2, 2, 12, 2, 2]),
    (20, [1, 1, 1, 12, 1, 1, 1, 12]),
    (23, [6, 6, 6, 6, 6, 6, 6, 6]),
];
const WALK_FRAMES: [usize; 8] = [12; 8];

fn game_dirs() -> Vec<(&'static str, GameDir)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../original/k1");
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

fn states(game: &GameDir) -> Vec<(usize, GameState)> {
    game.state_numbers(GAME_FILES)
        .unwrap()
        .into_iter()
        .map(|number| {
            let data = game.state_bytes(number, GAME_FILES).unwrap();
            (number as usize, game_state::parse(&data).unwrap())
        })
        .collect()
}

fn maps(game: &GameDir) -> Vec<usize> {
    game.map_numbers(GAME_FILES)
        .unwrap()
        .into_iter()
        .map(|number| number as usize)
        .collect()
}

/// Criterion 1 of #36: who is on the map, in which cell, facing where, in which
/// state, in which group, at which workplace.
#[test]
fn roster_names_every_creature_of_the_map_as_the_records_do() {
    for (edition, game) in game_dirs() {
        let states = states(&game);
        for number in maps(&game) {
            for (hero, state) in &states {
                let roster = creatures::roster(state, number, Some(*hero));
                let entities = game_state::on_map(state, number, Some(*hero));

                // Every member of a group of this map, roaming ones included.
                let mut expected: HashSet<usize> = HashSet::new();
                for group in &entities.groups {
                    expected.extend(group.members());
                }
                if let Some(start) = entities.start {
                    expected.extend(start.members());
                }
                assert_eq!(
                    roster.iter().map(|c| c.id).collect::<HashSet<_>>(),
                    expected,
                    "{edition} {number}: the roster of Game.{hero} names someone else"
                );

                let mut ids = roster.iter().map(|c| c.id).collect::<Vec<_>>();
                let unique = ids.iter().collect::<HashSet<_>>().len() == ids.len();
                assert!(unique, "{edition} {number}: one creature twice");
                ids.dedup();
                assert!(
                    ids.windows(2).all(|pair| pair[0] < pair[1]),
                    "{edition} {number}: the roster is not ordered by record number"
                );

                for creature in &roster {
                    let record = state
                        .characters
                        .get(creature.id)
                        .and_then(Option::as_ref)
                        .unwrap_or_else(|| panic!("{edition} {number}: no record {}", creature.id));
                    let group = state
                        .groups
                        .get(creature.group)
                        .and_then(Option::as_ref)
                        .unwrap_or_else(|| {
                            panic!("{edition} {number}: no group {}", creature.group)
                        });
                    assert_eq!((record.x, record.y), creature.cell, "{edition} {number}");
                    assert_eq!(record.direction, creature.direction);
                    assert_eq!(record.state, creature.state);
                    assert_eq!(record.group as usize, creature.group);
                    assert_eq!(group.spawn, creature.spawn);
                    assert_eq!(group.is_roaming(), creature.roaming);
                    match creature.workplace {
                        Some(place) => assert!(
                            entities
                                .jobs
                                .iter()
                                .any(|(_, job)| job.character as usize == creature.id
                                    && (job.x, job.y, job.direction)
                                        == (place.cell.0, place.cell.1, place.direction)),
                            "{edition} {number}: workplace {} is not a job of this map",
                            creature.id
                        ),
                        // `character == 0` marks an empty slot, not record 0 —
                        // the hero never owns an open slot
                        // (docs/k1/creatures.md, "The villager decision").
                        None => assert!(
                            !entities.jobs.iter().any(|(_, job)| job.character > 0
                                && job.character as usize == creature.id),
                            "{edition} {number}: creature {} has a job and no workplace",
                            creature.id
                        ),
                    }
                }
            }
        }
    }
}

/// The player group stands only on its own start map (docs/k1/formats/game-state.md).
#[test]
fn the_player_group_arrives_on_the_start_map_only() {
    for (edition, game) in game_dirs() {
        for (hero, state) in states(&game) {
            let start = game_state::start_map(hero);
            let on_start = creatures::roster(&state, start, Some(hero));
            let elsewhere = creatures::roster(&state, start + 1, Some(hero));
            let groups = |roster: &Vec<creatures::Creature>| -> HashSet<usize> {
                roster.iter().map(|creature| creature.group).collect()
            };
            assert!(
                groups(&on_start).contains(&0),
                "{edition} Game.{hero}: the player group is missing on map {start}"
            );
            assert!(
                !groups(&elsewhere).contains(&0),
                "{edition} Game.{hero}: the player group stands on two maps at once"
            );
        }
    }
}

/// Criterion 6 of #36: a step lasts as many ticks as the walk animation has frames
/// in that direction — for monsters from dynamic objects, for humans from models.
#[test]
fn step_durations_are_the_walk_animation_frame_counts() {
    for (edition, game) in game_dirs() {
        let resources = Resources::load(&game).unwrap();
        let states = states(&game);
        resources
            .with_graphics(|graph, objects| {
                let mut seen = HashSet::new();
                for (hero, state) in &states {
                    for number in maps(&game) {
                        let roster = creatures::roster(state, number, Some(*hero));
                        let durations = creatures::step_durations(state, &roster, graph, objects);
                        assert_eq!(durations.len(), roster.len());
                        for (creature, row) in roster.iter().zip(&durations) {
                            let record = state.characters[creature.id].as_ref().unwrap();
                            let expected = if record.has_dynamic_sprite() {
                                SPECIAL_DYNAMIC
                                    .iter()
                                    .find(|(set, _)| *set == record.sprite_set)
                                    .map_or(WALK_FRAMES, |(_, frames)| *frames)
                            } else {
                                WALK_FRAMES
                            };
                            let source = if record.has_dynamic_sprite() {
                                "dynamic object"
                            } else {
                                "model"
                            };
                            assert_eq!(
                                row.map(|ticks| ticks as usize),
                                expected,
                                "{} Game.{} map {}: creature {} of sprite set {} ({}) \
                                 walks differently from the measured table",
                                edition,
                                hero,
                                number,
                                creature.id,
                                record.sprite_set,
                                source
                            );
                            seen.insert((record.has_dynamic_sprite(), record.sprite_set as usize));
                        }
                    }
                }
                let reached = seen.len();
                assert!(
                    seen.len() > 20,
                    "{edition}: only {reached} sprite sets were reached — the roster is empty"
                );
            })
            .unwrap();
    }
}
