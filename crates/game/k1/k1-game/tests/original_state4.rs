//! The state-4 special animation on real data of both editions (skipped when
//! `original/` is absent — docs/k1/editions.md).
//!
//! Issue #70 AC5 asserts the counters of the state-4 population:
//!
//! | creature | class | dynamic object | records | frames |
//! |---|---|---|---|---|
//! | Skeleton | `0x4c` | 11 | 249 | 17 |
//! | Eechetik | `0x46` | 5 | 152 | 13 |
//! | Ion the werewolf | `0x55` | 22 | 3 | 12 |
//!
//! over `Game.0`–`Game.2` of both RUS and ENG copies (404 records total, see
//! docs/k1/creatures.md, "Our measurement"). The same test classifies every
//! one of them as a one-shot under the machine — Special at frame 0 on world
//! load, Idle after `special_frames` ticks — and asserts the machine's
//! Special → Idle edge happens exactly once per creature and not again, so no
//! sequence exhausts into a re-entry.

use std::collections::HashMap;
use std::path::PathBuf;

use k1_data::{
    game_state::{self, GameState},
    repo::{GAME_FILES, GameDir},
};
use k1_game::{
    creature_anim::{Machine, SPECIAL_STATE, Seed, Stage},
    creatures,
    resources::Resources,
};

const SKELETON_CLASS: u8 = 0x4c;
const EECHETIK_CLASS: u8 = 0x46;
const ION_CLASS: u8 = 0x55;
const SKELETON_SPRITE: u8 = 11;
const EECHETIK_SPRITE: u8 = 5;
const ION_SPRITE: u8 = 22;
const TOTAL_STATE4: usize = 404;
const SKELETON_COUNT: usize = 249;
const EECHETIK_COUNT: usize = 152;
const ION_COUNT: usize = 3;
const SKELETON_FRAMES: u32 = 17;
const EECHETIK_FRAMES: u32 = 13;
const ION_FRAMES: u32 = 12;

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

/// Every state-4 record of `Game.0`–`Game.2` of both editions is a dynamic
/// creature of the three known classes, and the machine seeds it into
/// Special { frame: 0 } — the one-shot appearance (issue #70 AC5).
#[test]
fn every_state_four_record_of_both_editions_is_a_one_shot_special() {
    let mut total_by_edition = HashMap::new();
    for (edition, game) in game_dirs() {
        let resources = Resources::load(&game).unwrap();
        let states = states(&game);
        let mut counters = Counters::default();
        resources
            .with_graphics(|graph, objects| {
                for (hero, state) in &states {
                    for number in maps(&game) {
                        let roster = creatures::roster(state, number, Some(*hero));
                        let specials =
                            creatures::special_frame_counts(state, &roster, graph, objects);
                        // The whole-map machine is a single seed pass with no
                        // ticks: it is the shared seeding snapshot taken before
                        // any creature's sequence advances, used only for the
                        // negative check. A creature's own play-through runs on
                        // a machine of its own below — the shared machine would
                        // otherwise advance every creature's frame counter while
                        // one creature is stepped, so a later state-4 creature
                        // would already be Idle when its turn comes.
                        let mut machine = Machine::new();
                        machine.enter_world(
                            roster
                                .iter()
                                .zip(specials.iter())
                                .map(|(c, &frames)| Seed {
                                    id: c.id,
                                    recorded_state: c.state,
                                    special_frames: frames,
                                }),
                        );
                        for creature in &roster {
                            if creature.state != SPECIAL_STATE {
                                assert_ne!(
                                    machine.stage(creature.id),
                                    Some(Stage::Special { frame: 0 }),
                                    "{edition} Game.{hero} map {}: creature {} is not a \
                                     state-4 record but entered Special",
                                    number,
                                    creature.id
                                );
                            }
                        }
                        for (creature, &frames) in roster.iter().zip(specials.iter()) {
                            if creature.state != SPECIAL_STATE {
                                continue;
                            }
                            counters.observe(edition, creature, frames);
                            let mut machine = Machine::new();
                            machine.enter_world([Seed {
                                id: creature.id,
                                recorded_state: creature.state,
                                special_frames: frames,
                            }]);
                            assert_eq!(
                                machine.stage(creature.id),
                                Some(Stage::Special { frame: 0 }),
                                "{edition} Game.{hero} map {}: creature {} did not start its special",
                                number,
                                creature.id
                            );
                            // The sequence plays once and lands in idle: after
                            // `frames` ticks the stage is Idle, and no further
                            // tick brings it back.
                            for tick in 1..frames {
                                machine.tick();
                                assert_eq!(
                                    machine.stage(creature.id),
                                    Some(Stage::Special { frame: tick }),
                                    "{edition} Game.{hero} map {}: creature {} lost frame {tick} \
                                     of its special",
                                    number,
                                    creature.id
                                );
                            }
                            machine.tick(); // the last tick lands in Idle
                            assert_eq!(
                                machine.stage(creature.id),
                                Some(Stage::Idle),
                                "{edition} Game.{hero} map {}: creature {} did not finish \
                                 its special into idle after {frames} ticks",
                                number,
                                creature.id
                            );
                            for _ in 0..frames * 2 {
                                machine.tick();
                                assert_eq!(
                                    machine.stage(creature.id),
                                    Some(Stage::Idle),
                                    "{edition} Game.{hero} map {}: creature {} re-entered \
                                     Special without an event",
                                    number,
                                    creature.id
                                );
                            }
                        }
                    }
                }
            })
            .unwrap();
        counters.assert(edition);
        total_by_edition.insert(edition, counters.total);
    }
    for (edition, total) in total_by_edition {
        assert_eq!(
            total, TOTAL_STATE4,
            "{edition}: state-4 records in Game.0-Game.2 differ from the count in \
             docs/k1/creatures.md (Our measurement)"
        );
    }
}

/// Every state-4 record's Special→Idle edge is counted exactly once (issue #70
/// AC6): a full map load's worth of ticks adds exactly one rebuild per
/// state-4 creature, no more, no less.
#[test]
fn pose_rebuilds_count_one_per_state_four_creature_and_no_more() {
    let mut total_by_edition = HashMap::new();
    for (edition, game) in game_dirs() {
        let resources = Resources::load(&game).unwrap();
        let states = states(&game);
        resources
            .with_graphics(|graph, objects| {
                let mut counted_total = 0usize;
                for (hero, state) in &states {
                    for number in maps(&game) {
                        let roster = creatures::roster(state, number, Some(*hero));
                        let specials =
                            creatures::special_frame_counts(state, &roster, graph, objects);
                        let mut machine = Machine::new();
                        machine.enter_world(roster.iter().zip(specials.iter()).map(
                            |(c, &frames)| Seed {
                                id: c.id,
                                recorded_state: c.state,
                                special_frames: frames,
                            },
                        ));
                        let seeded = roster.iter().filter(|c| c.state == SPECIAL_STATE).count();
                        // Each entry into Special is one rebuild.
                        assert_eq!(
                            machine.pose_rebuilds(),
                            seeded as u64,
                            "{edition} Game.{hero} map {}: enter_world counted a rebuild for a \
                             non-state-4 creature",
                            number
                        );
                        let longest = specials
                            .iter()
                            .zip(roster.iter())
                            .filter(|(_, c)| c.state == SPECIAL_STATE)
                            .map(|(&frames, _)| frames)
                            .max()
                            .unwrap_or(0);
                        for _ in 0..longest {
                            machine.tick();
                        }
                        // Each Special → Idle edge is another rebuild; the total
                        // per creature is exactly two (enter + finish).
                        assert_eq!(
                            machine.pose_rebuilds(),
                            (seeded * 2) as u64,
                            "{edition} Game.{hero} map {}: the ticks after Special counted \
                             more than one rebuild per creature",
                            number
                        );
                        counted_total += seeded;
                    }
                }
                total_by_edition.insert(edition, counted_total);
            })
            .unwrap();
    }
    for (edition, total) in total_by_edition {
        assert_eq!(
            total, TOTAL_STATE4,
            "{edition}: pose-rebuild counting covered the wrong number of state-4 creatures"
        );
    }
}

#[derive(Default)]
struct Counters {
    skeleton: usize,
    eechetik: usize,
    ion: usize,
    total: usize,
}

impl Counters {
    fn observe(&mut self, edition: &str, creature: &creatures::Creature, frames: u32) {
        self.total += 1;
        let (label, expected_frames, expected_sprite) = match creature.class_id {
            SKELETON_CLASS => {
                self.skeleton += 1;
                ("Skeleton", SKELETON_FRAMES, SKELETON_SPRITE)
            }
            EECHETIK_CLASS => {
                self.eechetik += 1;
                ("Eechetik", EECHETIK_FRAMES, EECHETIK_SPRITE)
            }
            ION_CLASS => {
                self.ion += 1;
                ("Ion", ION_FRAMES, ION_SPRITE)
            }
            other => panic!(
                "{edition}: creature {} in state 4 has an unexpected class {other:#04x} \
                 (docs/k1/creatures.md names only 0x46, 0x4c and 0x55)",
                creature.id
            ),
        };
        assert_eq!(
            creature.sprite_set, expected_sprite,
            "{edition}: {label} creature {} draws from dynamic object {} rather than {expected_sprite}",
            creature.id, creature.sprite_set
        );
        assert_eq!(
            frames, expected_frames,
            "{edition}: {label} creature {} has a state-4 sequence of {frames} frames, not \
             {expected_frames} (docs/k1/creatures.md, the appearance table)",
            creature.id
        );
    }

    fn assert(&self, edition: &str) {
        assert_eq!(
            (self.skeleton, self.eechetik, self.ion, self.total),
            (SKELETON_COUNT, EECHETIK_COUNT, ION_COUNT, TOTAL_STATE4),
            "{edition}: the state-4 population differs from the count in docs/k1/creatures.md \
             (Our measurement)"
        );
    }
}
