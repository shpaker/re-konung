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
use k1_game::creature_anim::{self, Machine as CreatureAnimMachine, Seed as CreatureAnimSeed};
use k1_game::creatures::{self, Creature, Workplace};
use k1_game::rules::{Context, Decision, Live};

pub use k1_game::creature_anim::Stage as CreatureStage;

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
    /// Per-creature animation state: state-4 creatures play their special
    /// once on world load and land in idle; every other record is Idle from
    /// the start. Owns no clock — [`Simulation::tick`] advances it once per
    /// caller-supplied tick (issue #70 AC4).
    creature_anim: CreatureAnimMachine,
    /// Orders the world rejected for a reason other than "no route": the rules
    /// send no orders that should fail — an unknown creature or an off-grid
    /// goal is a bug or bad data — so they are kept for the caller to report
    /// instead of being dropped.
    order_failures: Vec<(usize, WorldError)>,
    /// The rules re-decide every tick and may pick a fresh cell each time:
    /// without this a creature whose goal is unreachable would pay a route
    /// search per tick. The world throttles only a repeated goal; the window
    /// here is `WorldRules::repath_cooldown_ticks` for any goal that failed.
    cooldown: rules::FailedOrderCooldown,
    /// Creatures the world refused to place (impassable or taken cell), with the
    /// roster `Creature.id` and the reason; never dropped silently, the caller
    /// reports them.
    spawn_failures: Vec<(usize, WorldError)>,
}

impl Simulation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        map: &KonungMap,
        state: &GameState,
        map_number: usize,
        hero: Option<usize>,
        step_table: Vec<[u32; DIRECTION_COUNT]>,
        special_frames: Vec<u32>,
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

        // The "the creature entered the world" event (docs/k1/creatures.md,
        // our divergence for #38): every state-4 record starts its special at
        // frame 0. The temporary trigger is this call site, not the original's
        // AI-released hold of frame 0 (which needs #38's decision slot).
        let mut creature_anim = CreatureAnimMachine::new();
        creature_anim.enter_world(roster.iter().zip(special_frames.iter()).map(
            |(creature, &frames)| CreatureAnimSeed {
                id: creature.id,
                recorded_state: creature.state,
                special_frames: frames,
            },
        ));

        Simulation {
            world,
            roster,
            ids,
            jobs,
            rng: Rng::new(seed ^ 0xDEAD_BEEF),
            tick: 0,
            creature_anim,
            order_failures: Vec::new(),
            cooldown: rules::FailedOrderCooldown::new(WorldRules::default().repath_cooldown_ticks),
            spawn_failures,
        }
    }

    /// Creatures the world refused to place at spawn, each with the roster
    /// `Creature.id` and the reason; a non-empty list is a data problem the
    /// consumer should surface, not a silent gap in [`Simulation::states`].
    pub fn spawn_failures(&self) -> &[(usize, WorldError)] {
        &self.spawn_failures
    }

    /// Orders rejected on some tick for a reason other than "no route" —
    /// "no route" is a normal verdict the rules may aim at an unreachable goal,
    /// these are bugs or bad data. A non-empty list must be reported by the
    /// consumer like [`Self::spawn_failures`], not swallowed.
    pub fn order_failures(&self) -> &[(usize, WorldError)] {
        &self.order_failures
    }

    pub fn tick_number(&self) -> u64 {
        self.tick
    }

    pub fn creature_count(&self) -> usize {
        self.ids.iter().filter(|id| id.is_some()).count()
    }

    /// Initial pixel position for each creature, keyed by `Creature.id`.
    ///
    /// The whole roster, including creatures the world refused to place: those
    /// never appear in [`Self::states`], so a renderer that shows them draws
    /// them from this record cell — the original places a record wherever the
    /// file says and refuses nobody. The refusal is [`Self::spawn_failures`],
    /// which the consumer must report next to such a static creature.
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

    /// Adopts the movement side of a freshly built simulation of the same map
    /// and seed while keeping this one's creature animation machine playing.
    ///
    /// "Reset positions" restarts the walk world (routes, walk states, RNG,
    /// cooldown) back to the file's load cells but must NOT restart an in-flight
    /// state-4 special: the machine's per-creature stage is the animation, and a
    /// positions reset is not an animation reset (review of #101, P2-#3). The
    /// fresh simulation is built from the same `map_ref` and `seed`, so its
    /// roster and the machine's creature ids line up; only the world-side state is
    /// carried over, while `roster`, `jobs` and `creature_anim` stay as they were.
    pub fn adopt_positions_from(&mut self, fresh: Simulation) {
        let Simulation {
            world,
            ids,
            rng,
            tick,
            order_failures,
            cooldown,
            spawn_failures,
            ..
        } = fresh;
        self.world = world;
        self.ids = ids;
        self.rng = rng;
        self.tick = tick;
        self.order_failures = order_failures;
        self.cooldown = cooldown;
        self.spawn_failures = spawn_failures;
    }

    pub fn tick(&mut self) -> Vec<Event> {
        self.tick += 1;
        // The state machine advances on the caller's tick — never on a
        // render-frame boundary — so creature state transitions are
        // frame-rate independent (issue #70).
        self.creature_anim.tick();
        let live = self.live_picture();
        let group_states = k1_game::rules::group_states(&self.roster, &live);
        let mut context = Context {
            ground: &self.world,
            random: &mut self.rng,
            jobs: &self.jobs,
            states: &group_states,
        };
        let decisions = k1_game::rules::evaluate(&self.roster, &live, &mut context);
        let commands: Vec<(usize, CreatureId, Decision)> = self
            .ids
            .iter()
            .enumerate()
            .zip(decisions.iter())
            .filter_map(|((index, id), decision)| {
                (*id).map(|id| (self.roster[index].id, id, *decision))
            })
            .collect();
        let pairs: Vec<(CreatureId, Decision)> = commands
            .iter()
            .map(|&(_, id, decision)| (id, decision))
            .collect();
        // Through the cooldown: the rules re-decide every tick and may pick a
        // fresh cell, so a creature with an unreachable goal would otherwise
        // pay a search per tick (docs/rust.md, "Decision: repathing without
        // thrash"). The outcomes are never dropped: "no route" is the rules'
        // normal verdict, every other rejection is a bug or bad data.
        let applied = self.cooldown.apply(&mut self.world, &pairs);
        self.order_failures
            .extend(commands.iter().zip(applied.iter()).filter_map(
                |((creature_id, _, _), outcome)| match outcome {
                    rules::Applied::Failed(error) if *error != WorldError::NoRoute => {
                        Some((*creature_id, *error))
                    }
                    _ => None,
                },
            ));
        self.world.tick_step()
    }

    pub fn states(&self) -> Vec<CreatureState> {
        (0..self.roster.len())
            .filter_map(|index| self.creature_state(index))
            .collect()
    }

    /// The animation stage of one creature (issue #70): `Special { frame }`
    /// while its state-4 sequence runs, `Idle` otherwise. `None` for an id the
    /// machine never seeded.
    pub fn creature_stage(&self, creature_id: usize) -> Option<creature_anim::Stage> {
        self.creature_anim.stage(creature_id)
    }

    /// The animation state to show for one creature right now: 4 while its
    /// special plays, or the record's pair when idle. The caller overlays walk
    /// from [`CreatureState::walk_state`] on top of the idle answer.
    pub fn creature_shown_state(&self, creature_id: usize) -> Option<u8> {
        self.creature_anim.shown_state(creature_id)
    }

    /// Restart one creature's special from frame 0 — the seam a game event
    /// drives (issue #70 AC3); a new appearance is never triggered by
    /// sequence exhaustion, only by this call.
    pub fn restart_creature_special(&mut self, creature_id: usize) -> bool {
        self.creature_anim.restart_special(creature_id)
    }

    /// Counted pose rebuilds: one per creature whose animation-stage kind
    /// actually changed since the machine was created (issue #70 AC6). A plain
    /// frame advance within a running Special does not count.
    pub fn pose_rebuilds(&self) -> u64 {
        self.creature_anim.pose_rebuilds()
    }

    /// Creatures whose animation stage kind changed since the last call, and
    /// updates the tracker. The viewer calls it once per tick and rebuilds
    /// each listed creature's pose at most once (issue #70 AC6).
    pub fn drain_creature_anim_changes(&mut self) -> Vec<usize> {
        self.creature_anim.drain_changed()
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

    // ── the rules driving the world through tick() ──

    use k1_data::game_state::{Character, Group, Rect};

    const MAP_NUMBER: usize = 3;
    /// Record numbers below this stay empty: the hero would own 0 and 1.
    const FIRST_RECORD: usize = 2;

    /// A fine grid of the engine's size; `blocked` picks impassable cells
    /// (`is_blocked` reads the low nibble, so `0x0F` is the wall byte).
    fn open_map(blocked: impl Fn(u32, u32) -> bool) -> KonungMap {
        let foot = (0..FINE_HEIGHT)
            .map(|row| {
                (0..FINE_WIDTH)
                    .map(|column| if blocked(column, row) { 0x0F } else { 0x00 })
                    .collect()
            })
            .collect();
        KonungMap {
            ground: Vec::new(),
            overlay: Vec::new(),
            foot,
            decorations: Vec::new(),
            objects: Vec::new(),
        }
    }

    /// One roaming squad of `cells`, on [`MAP_NUMBER`], inside `spawn`: the
    /// minimum `Game.N` the roster reads (`Group` kind in the roaming range,
    /// `size` consecutive character records).
    fn roaming_state(spawn: Rect, cells: &[(i16, i16)]) -> GameState {
        let group = Group {
            slot: 1,
            first_character: FIRST_RECORD as i16,
            size: cells.len() as u8,
            max_size: cells.len() as u8,
            growth_limit: cells.len() as u8,
            kind: k1_game::rules::group::FIRST_ROAMING,
            map_id: MAP_NUMBER as u8,
            spawn,
            ..Group::default()
        };
        let mut characters: Vec<Option<Character>> =
            (0..FIRST_RECORD + cells.len()).map(|_| None).collect();
        for (offset, &(x, y)) in cells.iter().enumerate() {
            characters[FIRST_RECORD + offset] = Some(Character {
                slot: FIRST_RECORD + offset,
                x,
                y,
                group: 1,
                class_id: k1_game::rules::class::WARRIOR,
                ..Character::default()
            });
        }
        GameState {
            items: Vec::new(),
            groups: vec![None, Some(group)],
            loot: Vec::new(),
            exits: Vec::new(),
            characters,
            villages: Vec::new(),
        }
    }

    fn simulation(map: &KonungMap, state: &GameState, creatures: usize) -> Simulation {
        Simulation::new(
            map,
            state,
            MAP_NUMBER,
            None,
            vec![[2; DIRECTION_COUNT]; creatures],
            vec![0; creatures],
            Vec::new(),
            7,
        )
    }

    fn inside(spawn: &Rect, cell: (u32, u32)) -> bool {
        let (column, row) = (cell.0 as i16, cell.1 as i16);
        spawn.left <= column && column <= spawn.right && spawn.top <= row && row <= spawn.bottom
    }

    /// The full loop over production data structures: a strayed monster is seen
    /// outside its area, the return order reaches the world, the creature
    /// really walks it — and nothing on the way is rejected for a reason
    /// other than "no route". The rules only aim at cells `Ground::is_free`
    /// has vetted, so any other rejection is a bug; this is the guard.
    #[test]
    fn a_driven_run_walks_a_strayed_monster_home_without_order_failures() {
        let map = open_map(|_, _| false);
        let spawn = Rect {
            left: 4,
            top: 4,
            right: 8,
            bottom: 8,
        };
        let state = roaming_state(spawn, &[(12, 12)]);
        let mut sim = simulation(&map, &state, 1);

        let mut walked = false;
        let mut went_home = false;
        for _ in 0..300 {
            let events = sim.tick();
            walked |= events
                .iter()
                .any(|event| matches!(event, Event::StepFinished { .. }));
            assert!(
                sim.order_failures().is_empty(),
                "{:?}",
                sim.order_failures()
            );
            if inside(&spawn, sim.states()[0].cell) {
                went_home = true;
                break;
            }
        }
        assert!(walked, "the order must produce real steps");
        assert!(went_home, "the monster must reach its spawn area");
    }

    /// An unreachable spawn area is the rules' normal verdict, not a failure:
    /// the re-deciding loop may aim at it every tick, so `NoRoute` must not
    /// pollute [`Simulation::order_failures`], and the connected
    /// `FailedOrderCooldown` must keep the searches far below one per tick.
    #[test]
    fn an_unreachable_spawn_costs_throttled_searches_not_order_failures() {
        const TICKS: usize = 130;
        let ring = |column: u32, row: u32| {
            (3..=9).contains(&column) && (row == 3 || row == 9)
                || (3..=9).contains(&row) && (column == 3 || column == 9)
        };
        let map = open_map(ring);
        let spawn = Rect {
            left: 4,
            top: 4,
            right: 8,
            bottom: 8,
        };
        let state = roaming_state(spawn, &[(13, 13)]);
        let mut sim = simulation(&map, &state, 1);

        let mut no_route = 0;
        for _ in 0..TICKS {
            let events = sim.tick();
            no_route += events
                .iter()
                .filter(|event| matches!(event, Event::NoRoute { .. }))
                .count();
        }
        assert!(
            sim.order_failures().is_empty(),
            "\"no route\" is the rules' normal verdict, never an order failure: {:?}",
            sim.order_failures()
        );
        assert!(
            no_route >= 1,
            "the return order must have been issued and searched"
        );
        assert!(
            no_route * 4 <= TICKS,
            "the rules re-decide every tick; the cooldown must hold it far below \
             one search per tick: {no_route} in {TICKS}"
        );
        assert_eq!(
            sim.states()[0].cell,
            (13, 13),
            "an unreachable area moves nobody"
        );
    }

    /// A creature the world refuses to place is never dropped silently and
    /// never invented back into the live picture: it is reported once in
    /// `spawn_failures`, absent from `states`, and still offered in
    /// `initial_pixels` so the renderer can stand it on its record cell —
    /// the documented fallback, matching the original, which places everyone.
    #[test]
    fn an_unplaced_creature_is_reported_and_keeps_its_record_cell_for_the_renderer() {
        let map = open_map(|column, row| column == 20 && row == 20);
        let spawn = Rect {
            left: 24,
            top: 24,
            right: 26,
            bottom: 26,
        };
        let state = roaming_state(spawn, &[(20, 20), (25, 25)]);
        let mut sim = simulation(&map, &state, 2);

        assert_eq!(
            sim.spawn_failures(),
            [(2, WorldError::ImpassableCell)],
            "the record on the impassable cell is refused and reported"
        );
        for _ in 0..10 {
            sim.tick();
        }
        assert!(
            sim.order_failures().is_empty(),
            "{:?}",
            sim.order_failures()
        );
        assert_eq!(
            sim.states()
                .iter()
                .map(|state| state.id)
                .collect::<Vec<_>>(),
            vec![3],
            "the refused creature never enters the live picture"
        );
        assert_eq!(
            sim.initial_pixels(),
            vec![
                (2, Cell::new(20, 20).centre()),
                (3, Cell::new(25, 25).centre()),
            ],
            "the renderer still gets the refused creature at its record cell"
        );
    }
}
