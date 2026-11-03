//! The villager and monster rules over the real rosters of both editions
//! (skipped when `original/` is absent — docs/k1/editions.md).
//!
//! Criteria 3 and 4 of #36 on shipped data: the same seed must decide the
//! same way twice, every order a creature gets must land strictly inside its
//! own spawn area and on a free cell, calm squads must stand, workplace
//! holders standing on their place must not be sent anywhere, and the job
//! slots must point at villagers of this map, never at the hero.

use std::collections::HashSet;
use std::path::PathBuf;

use k1_data::{
    game_state::{self, GameState},
    map::{self, KonungMap},
    repo::{GAME_FILES, GameDir},
};
use k1_game::{
    creatures,
    rules::{self, CreatureType, Decision, Ground, GroupState, Live, Random, class, group::PLAYER},
};

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

/// The map's blocked foot cells plus every cell a roster creature stands in.
/// Coordinates outside the fine grid are never free.
struct FootGround {
    taken: HashSet<(i16, i16)>,
}

impl Ground for FootGround {
    fn is_free(&self, cell: (i16, i16)) -> bool {
        if cell.0 < 0 || cell.1 < 0 {
            return false;
        }
        let column = usize::try_from(cell.0).unwrap_or(usize::MAX);
        let row = usize::try_from(cell.1).unwrap_or(usize::MAX);
        column < map::FINE_WIDTH && row < map::FINE_HEIGHT && !self.taken.contains(&cell)
    }
}

fn foot_ground(map: &KonungMap, roster: &[creatures::Creature]) -> FootGround {
    let mut taken: HashSet<(i16, i16)> = HashSet::new();
    for (row, bytes) in map.foot.iter().enumerate() {
        for (column, &foot) in bytes.iter().enumerate() {
            if map::is_blocked(foot) {
                taken.insert((column as i16, row as i16));
            }
        }
    }
    taken.extend(roster.iter().map(|creature| creature.cell));
    FootGround { taken }
}

/// A second deterministic source besides the engine's own: the determinism
/// check must prove the rules repeat, not lean on one generator's quirk.
/// Multiply-shift over an LCG word — bias is irrelevant for a test script.
struct Seeded(u64);

impl Seeded {
    fn new(seed: u64) -> Seeded {
        Seeded(seed)
    }
}

impl Random for Seeded {
    fn below(&mut self, limit: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        if limit == 0 {
            return 0;
        }
        (((self.0 as u128) * (limit as u128)) >> 64) as u64
    }
}

fn run(
    roster: &[creatures::Creature],
    live: &[Option<Live>],
    ground: &FootGround,
    jobs: &[creatures::Workplace],
    squad_states: &[GroupState],
    seed: u64,
) -> Vec<Decision> {
    let mut random = Seeded::new(seed);
    let mut context = rules::Context {
        ground,
        random: &mut random,
        jobs,
        states: squad_states,
    };
    rules::evaluate(roster, live, &mut context)
}

/// The starting state is the file cells and nobody walking: the picture the
/// rules read is where the records say the creature stands (re-placing
/// roaming squads on map load is criterion 2, not this slice). Every creature
/// of the roster is known, so every entry is `Some`.
fn file_live(roster: &[creatures::Creature]) -> Vec<Option<Live>> {
    roster
        .iter()
        .map(|creature| {
            Some(Live {
                cell: creature.cell,
                direction: Some(creature.direction),
                busy: false,
            })
        })
        .collect()
}

fn strictly_inside(creature: &creatures::Creature, cell: (i16, i16)) -> bool {
    creature.spawn.left < cell.0
        && cell.0 < creature.spawn.right
        && creature.spawn.top < cell.1
        && cell.1 < creature.spawn.bottom
}

#[test]
fn rules_hold_their_invariants_on_every_map_of_both_editions() {
    const SEED: u64 = 0x1234_abcd_5678_ef90;
    for (edition, game) in game_dirs() {
        let states = states(&game);
        let mut covered_maps = 0usize;
        let mut walks = 0usize;
        let mut turns = 0usize;
        let mut strayed = 0usize;
        let mut holders_on_place = 0usize;
        for number in maps(&game) {
            let data = game
                .map_bytes(u32::try_from(number).unwrap(), GAME_FILES)
                .unwrap();
            let map = map::parse(&data).unwrap();
            let mut touched = false;
            for (hero, state) in &states {
                let roster = creatures::roster(state, number, Some(*hero));
                if roster.is_empty() {
                    continue;
                }
                touched = true;
                let jobs = creatures::job_slots(state, number);
                let live = file_live(&roster);
                let ground = foot_ground(&map, &roster);
                let squad_states = rules::group_states(&roster, &live);
                let decided = run(&roster, &live, &ground, &jobs, &squad_states, SEED);
                let replay = run(&roster, &live, &ground, &jobs, &squad_states, SEED);
                assert_eq!(
                    decided, replay,
                    "{edition} map {number} Game.{hero}: the same seed decided differently"
                );

                for (index, (creature, decision)) in roster.iter().zip(&decided).enumerate() {
                    let squad_state = squad_states[index];
                    match decision {
                        Decision::MoveTo(column, row) => {
                            let (Ok(column), Ok(row)) =
                                (i16::try_from(*column), i16::try_from(*row))
                            else {
                                panic!(
                                    "{edition} map {number} Game.{hero}: order onto \
                                     ({column}, {row}) leaves the coordinate space"
                                );
                            };
                            assert!(
                                strictly_inside(creature, (column, row)),
                                "{edition} map {number} Game.{hero}: creature {} is sent \
                                 to ({column}, {row}), outside its spawn area {:?}",
                                creature.id,
                                creature.spawn,
                            );
                            assert!(
                                ground.is_free((column, row)),
                                "{edition} map {number} Game.{hero}: creature {} is sent \
                                 onto a blocked or taken cell ({column}, {row})",
                                creature.id,
                            );
                            walks += 1;
                        }
                        Decision::Turn(direction) => {
                            assert!(
                                *direction < 8,
                                "{edition} map {number} Game.{hero}: creature {} is turned \
                                 to byte {direction}, outside the 0–7 direction table",
                                creature.id,
                            );
                            turns += 1;
                        }
                        Decision::None => {}
                    }
                    // A calm roaming squad stands, and only roaming squads
                    // can hold a state: the file's `ai_state` is 0x80 for
                    // every squad in `Game.0`–`Game.2` and no rule reads it,
                    // so these states are recomputed from positions alone
                    // (docs/k1/creatures.md).
                    match rules::classify(creature) {
                        CreatureType::RoamingMonster => {
                            if squad_state == GroupState::Calm {
                                assert!(
                                    matches!(decision, Decision::None),
                                    "{edition} map {number} Game.{hero}: squad member {} \
                                     was ordered {decision:?} while its squad is calm",
                                    creature.id,
                                );
                            }
                            if squad_state == GroupState::MemberOutside {
                                strayed += 1;
                            }
                        }
                        CreatureType::Villager
                        | CreatureType::LoneNpc
                        | CreatureType::PlayerOrWarrior => {
                            assert_eq!(
                                squad_state,
                                GroupState::Calm,
                                "{edition} map {number} Game.{hero}: a non-roaming squad \
                                 got a state from the position scan",
                            );
                        }
                    }
                }

                // Job holders that start on their place are turned or left
                // alone, never sent away: the starting data stands every
                // holder exactly on its cell (docs/k1/creatures.md).
                for job in &jobs {
                    if job.character <= 0 {
                        continue;
                    }
                    let index = usize::try_from(job.character)
                        .ok()
                        .and_then(|id| roster.iter().position(|creature| creature.id == id));
                    let Some(index) = index else {
                        continue;
                    };
                    let (holder, decision) = (&roster[index], decided[index]);
                    let peaceable =
                        holder.class_id != class::WOMAN && holder.class_id != class::CHILD;
                    if holder.cell == job.cell && peaceable {
                        holders_on_place += 1;
                        assert!(
                            matches!(decision, Decision::None | Decision::Turn(_)),
                            "{edition} map {number} Game.{hero}: holder {} stands on its \
                             workplace yet is ordered {decision:?}",
                            holder.id,
                        );
                    }
                }

                // The workplace fix in real data: record 0 is the player's
                // hero and never owns a slot, and every filled slot points
                // at a villager of this map's village squad
                // (docs/k1/creatures.md).
                for creature in &roster {
                    if creature.group_kind == PLAYER {
                        assert!(
                            creature.workplace.is_none(),
                            "{edition} map {number} Game.{hero}: the hero owns workplace {:?}",
                            creature.workplace,
                        );
                    }
                }
                for job in &jobs {
                    if job.character <= 0 {
                        continue;
                    }
                    let id = usize::try_from(job.character).unwrap();
                    let holder = roster
                        .iter()
                        .find(|creature| creature.id == id)
                        .unwrap_or_else(|| {
                            panic!(
                                "{edition} map {number} Game.{hero}: slot {:?} points at \
                             record {id}, which does not stand on this map",
                                job.role,
                            )
                        });
                    assert!(
                        matches!(
                            holder.group_kind,
                            rules::group::VILLAGE_FIRST..=rules::group::VILLAGE_LAST
                        ),
                        "{edition} map {number} Game.{hero}: slot {:?} points at record \
                         {id} of group kind {:#04x}, not a villager squad",
                        job.role,
                        holder.group_kind,
                    );
                }
            }
            if touched {
                covered_maps += 1;
            }
        }
        // The guard sits inside the per-edition loop: an absent edition is a
        // skip through `game_dirs`, an edition present must be covered
        // (corollary of PR #48). The counters keep the invariants honest — an
        // evaluation that decided nothing at all would satisfy every one of
        // them.
        assert!(
            covered_maps > 0,
            "{edition}: no map with a roster was ever evaluated"
        );
        assert!(
            walks > 0,
            "{edition}: not one creature was sent anywhere, so no order was ever checked"
        );
        assert!(
            strayed > 0,
            "{edition}: no squad member stood outside its spawn area, so the monster return was never reached"
        );
        assert!(
            holders_on_place > 0,
            "{edition}: no job holder stood on its place, so the workplace branch was never reached"
        );
        println!(
            "{edition}: {covered_maps} maps, {walks} move orders, {turns} turns, \
             {holders_on_place} holders on their places, {strayed} strayed squad members"
        );
    }
}
