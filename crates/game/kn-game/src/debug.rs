//! Runtime-only debugging and deterministic command replay. Nothing writes game files.

use crate::{movement::world::ActorDebugState, simulation::Simulation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebugPlayerProfile {
    pub hero: u32,
    pub character: usize,
    pub group: usize,
    pub direction: u8,
    pub class_id: u8,
    pub recorded_state: u8,
    pub step_ticks: [u32; 8],
}

impl DebugPlayerProfile {
    pub fn validate(&self) -> Result<(), String> {
        if self.step_ticks.contains(&0) {
            return Err("Debug player source has a missing walk duration".into());
        }
        if self.direction >= 8 {
            return Err("Debug player source has an invalid direction".into());
        }
        if self.class_id & 0x80 != 0 || matches!(self.recorded_state, 3 | 9) {
            return Err("Debug player source is not alive".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DebugCommand {
    Place { cell: (u32, u32) },
    Move { cell: (u32, u32) },
    Remove,
    ResetPositions,
    ResetAnimation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MovementDebugSnapshot {
    pub world_id: u32,
    pub cell: (u32, u32),
    pub pixel: (i32, i32),
    pub step_to: Option<(u32, u32)>,
    pub step_to_pixel: Option<(i32, i32)>,
    pub step_elapsed: u32,
    pub step_ticks: u32,
    pub moving: bool,
    pub direction: Option<u8>,
    pub requested_target: Option<(u32, u32)>,
    pub accepted_target: Option<(u32, u32)>,
    pub route: Vec<(u32, u32)>,
}
impl From<ActorDebugState> for MovementDebugSnapshot {
    fn from(value: ActorDebugState) -> Self {
        let cell = |cell: crate::movement::grid::Cell| (cell.column, cell.row);
        let state = value.state;
        Self {
            world_id: state.creature.raw(),
            cell: cell(state.cell),
            pixel: state.pixel,
            step_to: state.step_to.map(cell),
            step_to_pixel: state.step_to.map(|cell| cell.centre()),
            step_elapsed: state.step_elapsed,
            step_ticks: state.step_ticks,
            moving: state.moving,
            direction: state.direction.map(|direction| direction.index() as u8),
            requested_target: value.requested_target.map(cell),
            accepted_target: value.accepted_target.map(cell),
            route: value.route.into_iter().map(cell).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebugPlayerSnapshot {
    /// Runtime identity, separate from character record ids and movement-world slots.
    pub id: u64,
    pub source_hero: u32,
    pub source_character: usize,
    pub movement: MovementDebugSnapshot,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridRectangle {
    pub left: i16,
    pub top: i16,
    pub right: i16,
    pub bottom: i16,
}
impl From<k1_data::game_state::Rect> for GridRectangle {
    fn from(value: k1_data::game_state::Rect) -> Self {
        Self {
            left: value.left,
            top: value.top,
            right: value.right,
            bottom: value.bottom,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupGeometry {
    pub group: usize,
    pub spawn: GridRectangle,
    pub second_rectangle: GridRectangle,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebugCreatureSnapshot {
    pub record_id: usize,
    pub group: usize,
    pub used_group_state: u8,
    pub preview_group_state: u8,
    pub used_group_tick: Option<u64>,
    pub pursuit: Option<crate::simulation::DebugPursuit>,
    pub movement: Option<MovementDebugSnapshot>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebugSnapshot {
    pub tick: u64,
    pub player: Option<DebugPlayerSnapshot>,
    pub creatures: Vec<DebugCreatureSnapshot>,
    pub groups: Vec<GroupGeometry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalEntry {
    pub tick: u64,
    pub sequence: u64,
    pub command: DebugCommand,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Result<(), String>>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchOutcome {
    pub tick: u64,
    pub sequence: u64,
    pub branched: bool,
    pub result: Result<(), String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayProgress {
    pub tick: u64,
    pub target: u64,
    pub caught_up: bool,
}

/// Portable local replay descriptor. Authored entries may omit expected results;
/// live exports include observed outcomes and retain commands beyond the playhead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebugScript {
    pub version: u32,
    pub map: crate::MapRef,
    pub hero: Option<u32>,
    pub seed: u64,
    pub profile: Option<DebugPlayerProfile>,
    pub entries: Vec<JournalEntry>,
    pub through_tick: u64,
}
impl DebugScript {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("Unsupported debug script version".into());
        }
        if let Some(profile) = &self.profile {
            profile.validate()?;
        }
        let mut journal = DebugJournal::default();
        journal.replace_entries(self.entries.clone())
    }
    pub fn validate_profile(&self, actual: Option<&DebugPlayerProfile>) -> Result<(), String> {
        if self
            .profile
            .as_ref()
            .is_some_and(|expected| Some(expected) != actual)
        {
            return Err("Script profile differs from the actual game source profile".into());
        }
        Ok(())
    }
}

/// Sidecar for an existing Simulation. The caller supplies the same map/seed
/// factory for backward replay and position resets; forward calls never reload.
#[derive(Debug, Clone, Default)]
pub struct DebugJournal {
    entries: Vec<JournalEntry>,
    applied: usize,
    furthest_tick: u64,
    next_sequence: u64,
    replay_error: Option<String>,
}
impl DebugJournal {
    pub fn applied_count(&self) -> usize {
        self.applied
    }
    pub fn furthest_tick(&self) -> u64 {
        self.furthest_tick
    }
    pub fn replay_error(&self) -> Option<&str> {
        self.replay_error.as_deref()
    }

    pub fn entries(&self) -> &[JournalEntry] {
        &self.entries
    }
    pub fn observe_tick(&mut self, tick: u64) {
        self.furthest_tick = self.furthest_tick.max(tick);
    }
    /// Imports commands before replaying them into a fresh, tick-zero simulation.
    pub fn replace_entries(&mut self, entries: Vec<JournalEntry>) -> Result<(), String> {
        if entries
            .windows(2)
            .any(|pair| pair[0].tick > pair[1].tick || pair[0].sequence >= pair[1].sequence)
        {
            return Err(
                "Journal ticks must not decrease and sequences must strictly increase".into(),
            );
        }
        let next = entries
            .last()
            .map_or(Some(0), |entry| entry.sequence.checked_add(1))
            .ok_or("Journal sequence exhausted")?;
        self.furthest_tick = entries.last().map_or(0, |entry| entry.tick);
        self.entries = entries;
        self.applied = 0;
        self.next_sequence = next;
        self.replay_error = None;
        Ok(())
    }
    pub fn dispatch(
        &mut self,
        sim: &mut Simulation,
        profile: Option<&DebugPlayerProfile>,
        command: DebugCommand,
        mut fresh: impl FnMut() -> Result<Simulation, String>,
    ) -> DispatchOutcome {
        let tick = sim.tick_number();
        if let Some(error) = &self.replay_error {
            return DispatchOutcome {
                tick,
                sequence: self.next_sequence,
                branched: false,
                result: Err(error.clone()),
            };
        }
        if self.next_sequence == u64::MAX {
            return DispatchOutcome {
                tick,
                sequence: self.next_sequence,
                branched: false,
                result: Err("Journal sequence exhausted before dispatch".into()),
            };
        }
        let branched = tick < self.furthest_tick || self.applied < self.entries.len();
        self.entries.truncate(self.applied);
        let sequence = self.next_sequence;
        let result = match self.next_sequence.checked_add(1) {
            Some(next) => {
                self.next_sequence = next;
                sim.apply_debug_command(&command, profile, &mut fresh)
            }
            None => Err("Journal sequence exhausted".into()),
        };
        self.entries.push(JournalEntry {
            tick,
            sequence,
            command,
            result: Some(result.clone()),
        });
        self.applied = self.entries.len();
        self.furthest_tick = tick;
        DispatchOutcome {
            tick,
            sequence,
            branched,
            result,
        }
    }
    pub fn seek(
        &mut self,
        sim: &mut Simulation,
        profile: Option<&DebugPlayerProfile>,
        target: u64,
        max_ticks: u64,
        mut fresh: impl FnMut() -> Result<Simulation, String>,
    ) -> Result<ReplayProgress, String> {
        if let Some(error) = &self.replay_error {
            return Err(error.clone());
        }
        let result = self.seek_inner(sim, profile, target, max_ticks, &mut fresh);
        if let Err(error) = &result {
            self.replay_error = Some(error.clone());
        }
        result
    }

    fn seek_inner(
        &mut self,
        sim: &mut Simulation,
        profile: Option<&DebugPlayerProfile>,
        target: u64,
        max_ticks: u64,
        mut fresh: impl FnMut() -> Result<Simulation, String>,
    ) -> Result<ReplayProgress, String> {
        if target < sim.tick_number() {
            let rebuilt = fresh()?;
            if rebuilt.tick_number() != 0 {
                return Err("Replay factory must return tick zero".into());
            }
            *sim = rebuilt;
            self.applied = 0;
        }
        let mut advanced = 0;
        'advance: loop {
            while let Some(entry) = self.entries.get(self.applied) {
                if entry.tick > sim.tick_number() {
                    break;
                }
                if entry.tick < sim.tick_number() {
                    return Err("Replay skipped a command boundary; seek from tick zero".into());
                }
                if advanced >= max_ticks {
                    break 'advance;
                }
                let actual = sim.apply_debug_command(&entry.command, profile, &mut fresh);
                if entry
                    .result
                    .as_ref()
                    .is_some_and(|expected| actual != *expected)
                {
                    return Err(format!(
                        "Replay outcome differs at tick {} sequence {}: expected {:?}, got {:?}",
                        entry.tick, entry.sequence, entry.result, actual
                    ));
                }
                if self.entries[self.applied].result.is_none() {
                    self.entries[self.applied].result = Some(actual);
                }
                self.applied += 1;
                advanced += 1;
            }
            if sim.tick_number() >= target || advanced >= max_ticks {
                break;
            }
            sim.tick();
            advanced += 1;
        }
        self.observe_tick(sim.tick_number());
        Ok(ReplayProgress {
            tick: sim.tick_number(),
            target,
            caught_up: sim.tick_number() == target
                && self
                    .entries
                    .get(self.applied)
                    .is_none_or(|entry| entry.tick > target),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movement::grid::{FINE_HEIGHT, FINE_WIDTH};
    use k1_data::{
        game_state::{Character, GameState, Group, Rect},
        map::KonungMap,
    };

    fn profile() -> DebugPlayerProfile {
        DebugPlayerProfile {
            hero: 0,
            character: 0,
            group: 0,
            direction: 4,
            class_id: 1,
            recorded_state: 0,
            step_ticks: [7; 8],
        }
    }
    fn factory() -> Result<Simulation, String> {
        fixture(false)
    }

    fn fixture(refuse_first: bool) -> Result<Simulation, String> {
        let map = KonungMap {
            ground: Vec::new(),
            overlay: Vec::new(),
            foot: (0..FINE_HEIGHT)
                .map(|_| {
                    (0..FINE_WIDTH)
                        .map(|column| if column == 50 { 15 } else { 0 })
                        .collect()
                })
                .collect(),
            decorations: Vec::new(),
            objects: Vec::new(),
        };
        let state = GameState {
            items: Vec::new(),
            groups: vec![
                None,
                Some(Group {
                    slot: 1,
                    first_character: 2,
                    size: 1,
                    map_id: 1,
                    kind: 0x53,
                    ..Default::default()
                }),
                Some(Group {
                    slot: 2,
                    first_character: 3,
                    size: 1,
                    map_id: 1,
                    kind: 0x41,
                    spawn: Rect {
                        left: 4,
                        top: 4,
                        right: 8,
                        bottom: 8,
                    },
                    leash: Rect {
                        left: 1,
                        top: 2,
                        right: 30,
                        bottom: 40,
                    },
                    ..Default::default()
                }),
            ],
            characters: vec![
                None,
                None,
                Some(Character {
                    slot: 2,
                    x: if refuse_first { 50 } else { 20 },
                    y: 20,
                    group: 1,
                    state: 4,
                    class_id: 0x46,
                    ..Default::default()
                }),
                Some(Character {
                    slot: 3,
                    group: 2,
                    class_id: 0x46,
                    ..Default::default()
                }),
            ],
            loot: Vec::new(),
            exits: Vec::new(),
            villages: Vec::new(),
        };
        Ok(Simulation::new(
            &map,
            &state,
            1,
            None,
            vec![[2; 8]; 2],
            vec![12, 0],
            Vec::new(),
            42,
            None,
        ))
    }
    fn free_inside(sim: &Simulation) -> (u32, u32) {
        (4..=8)
            .flat_map(|row| (4..=8).map(move |column| (column, row)))
            .find(|cell| {
                sim.debug_snapshot()
                    .creatures
                    .iter()
                    .all(|c| c.movement.as_ref().is_none_or(|m| m.cell != *cell))
            })
            .unwrap()
    }
    fn advance(
        journal: &mut DebugJournal,
        sim: &mut Simulation,
        target: u64,
        profile: Option<&DebugPlayerProfile>,
    ) {
        loop {
            if journal
                .seek(sim, profile, target, 2, factory)
                .unwrap()
                .caught_up
            {
                break;
            }
        }
    }

    #[test]
    fn runtime_player_uses_real_pace_reacts_through_existing_group_rules_and_frees_reservations() {
        let profile = profile();
        let mut sim = factory().unwrap();
        let original_ids = sim.states().iter().map(|s| s.id).collect::<Vec<_>>();
        let cell = free_inside(&sim);
        sim.apply_debug_command(&DebugCommand::Place { cell }, Some(&profile), factory)
            .unwrap();
        let preview = sim.debug_snapshot();
        let monster = preview.creatures.iter().find(|c| c.record_id == 3).unwrap();
        assert_eq!(
            (monster.used_group_state, monster.preview_group_state),
            (0, 1)
        );
        sim.tick();
        assert_eq!(sim.inspect_creature(3).unwrap().group_state.byte(), 1);
        sim.apply_debug_command(&DebugCommand::Move { cell: (12, 12) }, None, factory)
            .unwrap();
        sim.tick();
        let actor = sim.debug_player().unwrap();
        assert_eq!(actor.movement.step_ticks, 7);
        assert!(actor.movement.step_to.is_some());
        let ordinary = sim.debug_snapshot().creatures;
        sim.apply_debug_command(&DebugCommand::Remove, None, factory)
            .unwrap();
        assert_eq!(
            sim.debug_snapshot()
                .creatures
                .iter()
                .map(|c| &c.movement)
                .collect::<Vec<_>>(),
            ordinary.iter().map(|c| &c.movement).collect::<Vec<_>>()
        );
        for cell in [actor.movement.cell, actor.movement.step_to.unwrap()] {
            sim.apply_debug_command(&DebugCommand::Place { cell }, Some(&profile), factory)
                .unwrap();
            assert!(sim.debug_player().unwrap().movement.world_id > actor.movement.world_id);
            sim.apply_debug_command(&DebugCommand::Move { cell: (12, 12) }, None, factory)
                .unwrap();
            sim.tick();
            assert_eq!(sim.debug_player().unwrap().movement.step_ticks, 7);
            sim.apply_debug_command(&DebugCommand::Remove, None, factory)
                .unwrap();
        }
        assert_eq!(
            sim.states().iter().map(|s| s.id).collect::<Vec<_>>(),
            original_ids
        );
        let geometry = sim
            .debug_snapshot()
            .groups
            .into_iter()
            .find(|g| g.group == 2)
            .unwrap();
        assert_eq!(
            geometry.second_rectangle,
            GridRectangle {
                left: 1,
                top: 2,
                right: 30,
                bottom: 40
            }
        );
    }

    #[test]
    fn pace_registration_uses_world_id_after_an_original_spawn_refusal() {
        let mut sim = fixture(true).unwrap();
        let profile = profile();
        let cell = free_inside(&sim);
        assert_eq!(sim.states().len(), 1);
        sim.apply_debug_command(&DebugCommand::Place { cell }, Some(&profile), factory)
            .unwrap();
        assert_eq!(sim.debug_player().unwrap().movement.world_id, 1);
        sim.apply_debug_command(&DebugCommand::Move { cell: (12, 12) }, None, factory)
            .unwrap();
        sim.tick();
        assert_eq!(sim.debug_player().unwrap().movement.step_ticks, 7);
    }

    #[test]
    fn journal_replays_failed_searches_both_resets_and_runtime_ids_in_bounded_batches() {
        let profile = profile();
        let mut sim = factory().unwrap();
        let mut journal = DebugJournal::default();
        let cell = free_inside(&sim);
        assert!(
            journal
                .dispatch(
                    &mut sim,
                    Some(&profile),
                    DebugCommand::Place { cell },
                    factory
                )
                .result
                .is_ok()
        );
        assert!(
            journal
                .dispatch(
                    &mut sim,
                    None,
                    DebugCommand::Move { cell: (100, 10) },
                    factory
                )
                .result
                .is_err()
        );
        advance(&mut journal, &mut sim, 5, Some(&profile));
        journal
            .dispatch(&mut sim, None, DebugCommand::ResetAnimation, factory)
            .result
            .unwrap();
        let actor = sim.debug_player().unwrap();
        advance(&mut journal, &mut sim, 8, Some(&profile));
        let stage = sim.creature_stage(2);
        journal
            .dispatch(&mut sim, None, DebugCommand::ResetPositions, factory)
            .result
            .unwrap();
        assert!(sim.debug_player().is_none());
        assert_eq!(sim.tick_number(), 8);
        assert_eq!(sim.creature_stage(2), stage);
        journal
            .dispatch(
                &mut sim,
                Some(&profile),
                DebugCommand::Place { cell },
                factory,
            )
            .result
            .unwrap();
        assert!(sim.debug_player().unwrap().id > actor.id);
        journal
            .dispatch(
                &mut sim,
                None,
                DebugCommand::Move { cell: (12, 12) },
                factory,
            )
            .result
            .unwrap();
        advance(&mut journal, &mut sim, 30, Some(&profile));
        let expected = sim.debug_snapshot();
        advance(&mut journal, &mut sim, 0, Some(&profile));
        advance(&mut journal, &mut sim, 30, Some(&profile));
        assert_eq!(sim.debug_snapshot(), expected);
        assert!(
            journal
                .entries()
                .windows(2)
                .all(|p| p[0].sequence < p[1].sequence)
        );
    }

    #[test]
    fn rejected_dispatch_after_rewind_branches_and_missing_profile_only_blocks_place() {
        let profile = profile();
        let mut sim = factory().unwrap();
        let mut journal = DebugJournal::default();
        let cell = free_inside(&sim);
        journal
            .dispatch(
                &mut sim,
                Some(&profile),
                DebugCommand::Place { cell },
                factory,
            )
            .result
            .unwrap();
        advance(&mut journal, &mut sim, 10, Some(&profile));
        journal
            .dispatch(&mut sim, None, DebugCommand::Remove, factory)
            .result
            .unwrap();
        advance(&mut journal, &mut sim, 3, Some(&profile));
        let outcome = journal.dispatch(
            &mut sim,
            Some(&profile),
            DebugCommand::Place { cell },
            factory,
        );
        assert!(outcome.branched);
        assert!(outcome.result.is_err());
        assert_eq!(journal.entries().len(), 2);
        assert_eq!(journal.entries()[1].tick, 3);
        advance(&mut journal, &mut sim, 20, Some(&profile));
        assert!(sim.debug_player().is_some());
        let mut sim = factory().unwrap();
        let mut journal = DebugJournal::default();
        assert!(
            journal
                .dispatch(&mut sim, None, DebugCommand::Place { cell }, factory)
                .result
                .is_err()
        );
        journal
            .dispatch(&mut sim, None, DebugCommand::ResetPositions, factory)
            .result
            .unwrap();
        journal
            .dispatch(&mut sim, None, DebugCommand::ResetAnimation, factory)
            .result
            .unwrap();
        advance(&mut journal, &mut sim, 8, None);
        advance(&mut journal, &mut sim, 0, None);
    }

    #[test]
    fn replay_failure_is_latched_after_a_mutating_outcome_mismatch() {
        let mut sim = factory().unwrap();
        let profile = profile();
        let mut journal = DebugJournal::default();
        journal
            .replace_entries(vec![JournalEntry {
                tick: 0,
                sequence: 0,
                command: DebugCommand::Place { cell: (4, 4) },
                result: Some(Err("wrong expected outcome".into())),
            }])
            .unwrap();
        let first = journal
            .seek(&mut sim, Some(&profile), 0, 1, factory)
            .unwrap_err();
        let actor = sim.debug_player().unwrap();
        assert_eq!(
            journal
                .seek(&mut sim, Some(&profile), 0, 1, || panic!("must not retry"))
                .unwrap_err(),
            first
        );
        assert_eq!(sim.debug_player().unwrap(), actor);
        let entries = journal.entries().to_vec();
        assert_eq!(
            journal
                .dispatch(&mut sim, None, DebugCommand::Remove, factory)
                .result,
            Err(first)
        );
        assert_eq!(journal.entries(), entries);
        assert_eq!(sim.debug_player().unwrap(), actor);
    }

    #[test]
    #[ignore = "manual synthetic debug command cost measurement"]
    fn debug_command_costs() {
        use std::{hint::black_box, time::Instant};
        fn measure(mut run: impl FnMut(), count: u32) -> f64 {
            (0..5)
                .map(|_| {
                    let start = Instant::now();
                    for _ in 0..count {
                        run();
                    }
                    start.elapsed().as_nanos() as f64 / f64::from(count)
                })
                .fold(f64::INFINITY, f64::min)
        }
        let profile = profile();
        let mut direct = factory().unwrap();
        let cell = free_inside(&direct);
        let direct_ns = measure(
            || {
                direct
                    .apply_debug_command(&DebugCommand::Place { cell }, Some(&profile), factory)
                    .unwrap();
                direct
                    .apply_debug_command(&DebugCommand::Remove, None, factory)
                    .unwrap();
            },
            1000,
        );
        let mut sim = factory().unwrap();
        let mut journal = DebugJournal::default();
        let journal_ns = measure(
            || {
                journal
                    .dispatch(
                        &mut sim,
                        Some(&profile),
                        DebugCommand::Place { cell },
                        factory,
                    )
                    .result
                    .unwrap();
                journal
                    .dispatch(&mut sim, None, DebugCommand::Remove, factory)
                    .result
                    .unwrap();
            },
            1000,
        );
        let idle_ns = measure(
            || {
                black_box(sim.tick());
            },
            1000,
        );
        journal
            .dispatch(
                &mut sim,
                Some(&profile),
                DebugCommand::Place { cell },
                factory,
            )
            .result
            .unwrap();
        let player_tick_ns = measure(
            || {
                black_box(sim.tick());
            },
            1000,
        );
        let snapshot_ns = measure(
            || {
                black_box(sim.debug_snapshot());
            },
            1000,
        );
        let mut alternate = false;
        let move_ns = measure(
            || {
                alternate = !alternate;
                journal
                    .dispatch(
                        &mut sim,
                        None,
                        DebugCommand::Move {
                            cell: if alternate { (12, 12) } else { (13, 12) },
                        },
                        factory,
                    )
                    .result
                    .unwrap();
            },
            100,
        );
        println!(
            "DEBUG_COST direct_place_remove_ns={direct_ns:.0} journal_place_remove_ns={journal_ns:.0} idle_tick_ns={idle_ns:.0} player_tick_ns={player_tick_ns:.0} snapshot_ns={snapshot_ns:.0} journal_route_move_ns={move_ns:.0}"
        );
    }

    #[test]
    fn exhausted_sequence_rejects_before_mutation_and_keeps_exports_valid() {
        let mut sim = factory().unwrap();
        let mut journal = DebugJournal::default();
        journal
            .replace_entries(vec![JournalEntry {
                tick: 0,
                sequence: u64::MAX - 1,
                command: DebugCommand::ResetAnimation,
                result: Some(Ok(())),
            }])
            .unwrap();
        for _ in 0..2 {
            assert!(
                journal
                    .dispatch(&mut sim, None, DebugCommand::ResetPositions, factory)
                    .result
                    .is_err()
            );
        }
        assert_eq!(journal.entries().len(), 1);
        let mut imported = DebugJournal::default();
        imported
            .replace_entries(journal.entries().to_vec())
            .unwrap();
    }

    #[test]
    fn authored_commands_record_outcomes_without_predicting_error_text() {
        let mut sim = factory().unwrap();
        let mut journal = DebugJournal::default();
        journal
            .replace_entries(vec![JournalEntry {
                tick: 0,
                sequence: 0,
                command: DebugCommand::Remove,
                result: None,
            }])
            .unwrap();
        assert!(
            journal
                .seek(&mut sim, None, 0, 1, factory)
                .unwrap()
                .caught_up
        );
        assert!(journal.entries()[0].result.as_ref().unwrap().is_err());
    }

    #[test]
    fn same_tick_commands_are_bounded_and_zero_budget_does_no_work() {
        let mut sim = factory().unwrap();
        let mut journal = DebugJournal::default();
        journal
            .replace_entries(
                (0..5)
                    .map(|sequence| JournalEntry {
                        tick: 0,
                        sequence,
                        command: DebugCommand::ResetAnimation,
                        result: Some(Ok(())),
                    })
                    .collect(),
            )
            .unwrap();
        assert!(
            !journal
                .seek(&mut sim, None, 0, 0, factory)
                .unwrap()
                .caught_up
        );
        for _ in 0..4 {
            assert!(
                !journal
                    .seek(&mut sim, None, 0, 1, factory)
                    .unwrap()
                    .caught_up
            );
        }
        assert!(
            journal
                .seek(&mut sim, None, 0, 1, factory)
                .unwrap()
                .caught_up
        );
    }
}
