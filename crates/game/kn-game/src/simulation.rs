//! Game simulation: the movement world driven by confirmed K1 rules.
//!
//! One tick: read live states → evaluate decisions → apply them through the
//! world → advance the engine. The viewer owns a [`Simulation`] per loaded
//! map and calls [`Simulation::tick`] once per game tick (60 ms).

use std::collections::HashSet;

use k1_data::game_state::GameState;
use k1_data::map::{self, KonungMap};

use crate::movement::CreatureId;
use crate::movement::grid::{Cell, DIRECTION_COUNT, FINE_HEIGHT, FINE_WIDTH, Grid};
use crate::movement::passage::Passage;
use crate::movement::rng::Rng;
use crate::movement::world::{Event, World, WorldError, WorldRules};
use crate::step_durations::StepDurations;
use k1_game::creatures::{self, Creature, Workplace};
use k1_game::rules::{Context, Decision, Live};

use crate::rules;

/// Walkability from the map's foot grid.
#[derive(Clone)]
pub struct MapPassage {
    blocked: HashSet<(u32, u32)>,
}

impl MapPassage {
    pub fn from_map(map: &KonungMap) -> MapPassage {
        let mut blocked = HashSet::new();
        for (row, bytes) in map.foot.iter().enumerate() {
            for (column, &foot) in bytes.iter().enumerate() {
                if map::is_blocked(foot) {
                    blocked.insert((column as u32, row as u32));
                }
            }
        }
        MapPassage { blocked }
    }
}

impl Passage for MapPassage {
    fn is_walkable(&self, cell: Cell) -> bool {
        !self.blocked.contains(&(cell.column, cell.row))
    }
}

/// One creature's display state at the current tick.
#[derive(Debug, Clone, Copy)]
pub struct CreatureState {
    pub id: usize,
    pub cell: (u32, u32),
    pub pixel: (i32, i32),
    pub step_to_pixel: Option<(i32, i32)>,
    pub direction: Option<u8>,
    pub moving: bool,
    pub step_elapsed: u32,
    pub step_ticks: u32,
    pub walk_state: WalkState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkState {
    Rest,
    Walk,
}

/// A simulation over one map.
pub struct Simulation {
    world: World<MapPassage, StepDurations>,
    roster: Vec<Creature>,
    ids: Vec<Option<CreatureId>>,
    jobs: Vec<Workplace>,
    rng: Rng,
    tick: u64,
    /// Creatures the world refused to place (impassable or taken cell), with the
    /// roster `Creature.id` and the reason; never dropped silently, the caller
    /// reports them.
    spawn_failures: Vec<(usize, WorldError)>,
}

impl Simulation {
    pub fn new(
        map: &KonungMap,
        state: &GameState,
        map_number: usize,
        hero: Option<usize>,
        step_table: Vec<[u32; DIRECTION_COUNT]>,
        jobs: Vec<Workplace>,
        seed: u64,
    ) -> Simulation {
        let passage = MapPassage::from_map(map);
        let grid = Grid::new(FINE_WIDTH, FINE_HEIGHT);
        let pace = StepDurations::new(step_table);
        let mut world = World::new(grid, passage, pace, WorldRules::default(), seed);

        let roster = creatures::roster(state, map_number, hero);
        let mut ids = Vec::with_capacity(roster.len());
        let mut spawn_failures = Vec::new();
        for creature in &roster {
            let column = u32::try_from(creature.cell.0.max(0)).unwrap_or(0);
            let row = u32::try_from(creature.cell.1.max(0)).unwrap_or(0);
            let cell = Cell::new(column, row);
            match world.add(cell) {
                Ok(id) => ids.push(Some(id)),
                Err(error) => {
                    ids.push(None);
                    spawn_failures.push((creature.id, error));
                }
            }
        }

        Simulation {
            world,
            roster,
            ids,
            jobs,
            rng: Rng::new(seed ^ 0xDEAD_BEEF),
            tick: 0,
            spawn_failures,
        }
    }

    /// Creatures the world refused to place at spawn, each with the roster
    /// `Creature.id` and the reason; a non-empty list is a data problem the
    /// consumer should surface, not a silent gap in [`Simulation::states`].
    pub fn spawn_failures(&self) -> &[(usize, WorldError)] {
        &self.spawn_failures
    }

    pub fn tick_number(&self) -> u64 {
        self.tick
    }

    pub fn creature_count(&self) -> usize {
        self.ids.iter().filter(|id| id.is_some()).count()
    }

    /// Initial pixel position for each creature, keyed by `Creature.id`.
    pub fn initial_pixels(&self) -> Vec<(usize, (i32, i32))> {
        self.roster
            .iter()
            .map(|c| {
                let cell = Cell::new(
                    u32::try_from(c.cell.0.max(0)).unwrap_or(0),
                    u32::try_from(c.cell.1.max(0)).unwrap_or(0),
                );
                (c.id, cell.centre())
            })
            .collect()
    }

    pub fn tick(&mut self) -> Vec<Event> {
        self.tick += 1;
        let live = self.live_picture();
        let group_states = k1_game::rules::group_states(&self.roster, &live);
        let mut context = Context {
            ground: &self.world,
            random: &mut self.rng,
            jobs: &self.jobs,
            states: &group_states,
        };
        let decisions = k1_game::rules::evaluate(&self.roster, &live, &mut context);
        let commands: Vec<(CreatureId, Decision)> = self
            .ids
            .iter()
            .zip(decisions.iter())
            .filter_map(|(id, decision)| id.map(|id| (id, *decision)))
            .collect();
        rules::apply(&mut self.world, &commands);
        self.world.tick_step()
    }

    pub fn states(&self) -> Vec<CreatureState> {
        (0..self.roster.len())
            .filter_map(|index| self.creature_state(index))
            .collect()
    }

    pub fn creature_state(&self, index: usize) -> Option<CreatureState> {
        let id = *self.ids.get(index)?;
        let id = id?;
        let walk = self.world.state(id)?;
        let (px, py) = walk.cell.centre();
        let step_to_pixel = walk.step_to.map(|c| c.centre());
        Some(CreatureState {
            id: self.roster[index].id,
            cell: (walk.cell.column, walk.cell.row),
            pixel: (px, py),
            step_to_pixel,
            direction: walk.direction.map(|d| d.index() as u8),
            moving: walk.moving,
            step_elapsed: walk.step_elapsed,
            step_ticks: walk.step_ticks,
            walk_state: if walk.moving {
                WalkState::Walk
            } else {
                WalkState::Rest
            },
        })
    }

    fn live_picture(&self) -> Vec<Option<Live>> {
        self.ids
            .iter()
            .map(|id| {
                let id = (*id)?;
                let state = self.world.state(id)?;
                Some(Live {
                    cell: (
                        i16::try_from(state.cell.column).unwrap_or(-1),
                        i16::try_from(state.cell.row).unwrap_or(-1),
                    ),
                    direction: state.direction.map(|d| d.index() as u8),
                    busy: state.moving || state.goal.is_some(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_centre_is_isometric() {
        let (x, y) = Cell::new(2, 4).centre();
        // even row: shift = (2-0)*29 = 58; x = 2*58 + 58 = 174; y = 4*16 + 16 = 80
        assert_eq!(y, 80);
        assert_eq!(x, 174);
    }

    #[test]
    fn cell_centre_odd_row_shifts_left() {
        let (even_x, _) = Cell::new(4, 2).centre();
        let (odd_x, _) = Cell::new(4, 3).centre();
        assert!(even_x > odd_x);
    }

    /// Routes gate movement on exactly the confirmed foot rule and nothing more.
    ///
    /// A cell is impassable iff the low nibble of its `foot` byte is `0xF`
    /// (docs/k1/formats/map.md, `is_blocked`). No road, terrain or other modifier
    /// belongs here unless it is confirmed — the original's route *preference* for
    /// roads (if any) is unconfirmed and must not be invented (review #68). This
    /// locks `MapPassage` to `!is_blocked` so a future unconfirmed rule fails here.
    #[test]
    fn passage_is_the_confirmed_foot_rule_only() {
        let row_a = [0x00u8, 0x0F, 0x40, 0x4F, 0x1F, 0x0E, 0xFF];
        let row_b: Vec<u8> = row_a
            .iter()
            .map(|byte| byte | map::FOOT_STOPS_ARROWS)
            .collect();
        let foot = vec![row_a.to_vec(), row_b];
        let map = KonungMap {
            ground: Vec::new(),
            overlay: Vec::new(),
            foot: foot.clone(),
            decorations: Vec::new(),
            objects: Vec::new(),
        };
        let passage = MapPassage::from_map(&map);
        for (row, bytes) in foot.iter().enumerate() {
            for (column, &byte) in bytes.iter().enumerate() {
                let cell = Cell::new(column as u32, row as u32);
                assert_eq!(
                    passage.is_walkable(cell),
                    !map::is_blocked(byte),
                    "foot byte {byte:#04x} at ({column}, {row})"
                );
            }
        }
    }
}
