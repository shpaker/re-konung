//! Game simulation: the movement world driven by confirmed K1 rules.
//!
//! One tick: read live states → evaluate decisions → apply them through the
//! world → advance the engine. The viewer owns a [`Simulation`] per loaded
//! map and calls [`Simulation::tick`] once per game tick (60 ms).

use std::collections::{BTreeMap, HashMap, HashSet};

use k1_data::executable::Texts;
use k1_data::game_state::{self, GameState};
use k1_data::map::{self, KonungMap};

use crate::movement::CreatureId;
use crate::movement::grid::{Cell, DIRECTION_COUNT, FINE_HEIGHT, FINE_WIDTH, Grid};
use crate::movement::passage::Passage;
use crate::movement::rng::Rng;
use crate::movement::world::{Event, GoalPolicy, MovementMode, World, WorldError, WorldRules};
use crate::step_durations::StepDurations;
use k1_game::creature_anim::{self, Machine as CreatureAnimMachine, Seed as CreatureAnimSeed};
use k1_game::creatures::{self, Creature, Workplace};
use k1_game::rules::{Context, CreatureType, Decision, Ground, Live, classify, is_adjacent};

pub use k1_game::creature_anim::Stage as CreatureStage;
pub use k1_game::rules::GroupState;

use crate::rules;

mod melee;
pub use melee::{
    AttackError, AttackOrder, AttackStatus, DebugPursuit, PursuitError, PursuitStatus,
};

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

/// The same foot data as the rules' standing test: walkable and inside the
/// fine grid. Occupancy belongs to the world and to `place_roaming`'s own
/// bookkeeping, so the terrain answer here is terrain only.
impl Ground for MapPassage {
    fn is_free(&self, cell: (i16, i16)) -> bool {
        let (Ok(column), Ok(row)) = (u32::try_from(cell.0), u32::try_from(cell.1)) else {
            return false;
        };
        column < FINE_WIDTH && row < FINE_HEIGHT && self.is_walkable(Cell::new(column, row))
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

/// Read-only facts for one stable character record, including the squad snapshot
/// consumed by the last decision pass. At load/reset the snapshot is unevaluated.
#[derive(Debug, Clone)]
pub struct CreatureInspection {
    pub id: usize,
    pub label: String,
    pub recorded_state: u8,
    pub animation_state: Option<u8>,
    pub animation_stage: Option<CreatureStage>,
    pub walk_state: Option<WalkState>,
    pub group: usize,
    pub group_state: GroupState,
    pub group_state_tick: Option<u64>,
    pub attack: Option<AttackOrder>,
    pub pursuit: Option<DebugPursuit>,
}

struct DebugActor {
    id: u64,
    world_id: CreatureId,
    profile: crate::debug::DebugPlayerProfile,
}

/// A rejected player command; movement errors retain the common engine's verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeroOrderError {
    HeroUnavailable,
    Movement(WorldError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeroOrderStatus {
    Pending,
    Moving,
    Arrived,
    Refused(HeroOrderError),
}

/// The latest command only: presenting a running session needs no event backlog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeroOrder {
    pub target: Cell,
    pub mode: MovementMode,
    pub status: HeroOrderStatus,
}

/// A simulation over one map.
pub struct Simulation {
    melee: melee::CombatRuntime,
    world: World<MapPassage, StepDurations>,
    hero_id: Option<usize>,
    hero_world_id: Option<CreatureId>,
    hero_order: Option<HeroOrder>,
    hero_active_target: Option<Cell>,
    hero_goal_reaches: u64,
    roster: Vec<Creature>,
    group_states: Vec<GroupState>,
    group_state_tick: Option<u64>,
    debug_actor: Option<DebugActor>,
    next_debug_id: u64,
    debug_groups: Vec<crate::debug::GroupGeometry>,
    /// Ready `#<id> <name>` label per record number, built once at load from
    /// the executable's texts (issue #84). The consumer — the viewer's label
    /// layer — reads the finished strings from here and never touches the K1
    /// name tables itself (review #116: the K1 knowledge stays in the game
    /// layer). Labels exist for every placed creature's id.
    labels: HashMap<usize, String>,
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
    /// reports them. After the load-time re-placement every refusal is a fact
    /// of the record itself — the file stands it in blocked foot data or in a
    /// cell another record already holds — so the consumer presents it as
    /// data, not as an error (docs/research/k1/creatures/placement.md, "Load-time refusals").
    spawn_failures: Vec<(usize, WorldError)>,
    /// Load-time cell of each roster entry once the roaming squads have been
    /// re-placed (indexed like the roster): what `world.add` was asked for,
    /// and what the renderer stands refused creatures on.
    load_cells: Vec<(i16, i16)>,
    /// Record numbers the load placement trimmed for want of a free cell in
    /// the squad's spawn area — the original shrinks `GroupSize` and leaves
    /// them out of the game (`game.cpp:654-655`, docs/research/k1/creatures/placement.md,
    /// «Roaming squad placement on map load»). Confirmed behavior, so they
    /// are reported as data, not as refusals.
    load_trimmed: Vec<usize>,
    /// Creature number → roster index, so an event names the record it
    /// happened to. The world hands out numbers in `add` order, and only
    /// successful adds push here.
    by_creature: Vec<usize>,
    /// Last accepted `MoveTo` target per creature number, kept until the
    /// creature arrives there: the goal-reach counters compare an arrival
    /// against the cell the order commanded.
    tracked_goal: Vec<Option<Cell>>,
    /// Completed orders whose final cell equalled the commanded target, by
    /// roster record number, counted over the confirmed targets only.
    goal_reaches: BTreeMap<usize, GoalReach>,
}

/// One creature's completed confirmed orders, counted from the movement
/// events (docs/research/k1/creatures/placement.md, "Goal reach"): an order counts when the
/// creature arrived on the very cell the rules commanded, and only targets
/// the rules were confirmed for are counted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GoalReach {
    /// The stable roster record number.
    pub id: usize,
    /// Arrivals at the creature's workplace: the place cell of the village
    /// slot it holds, or the free neighbour a static-work order joins the
    /// place from.
    pub workplace: usize,
    /// Arrivals at a walk target strictly inside the creature's spawn area.
    pub walk: usize,
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
        texts: Option<&Texts>,
    ) -> Simulation {
        let passage = MapPassage::from_map(map);
        let grid = Grid::new(FINE_WIDTH, FINE_HEIGHT);
        let pace = StepDurations::new(step_table);

        let hero_id = game_state::on_map(state, map_number, hero)
            .start
            .and_then(|group| group.members().next())
            .and_then(|slot| state.characters.get(slot)?.as_ref())
            .map(|character| character.slot);
        let roster = creatures::roster(state, map_number, hero);
        let labels = creatures::labels(&roster, texts);
        // The confirmed load-time rule (docs/research/k1/creatures/placement.md, «Roaming squad
        // placement on map load»): the game never leaves a roaming squad at
        // the cells its records carry — every member is re-placed near the
        // spawn area's centre, and the squad is trimmed where the area runs
        // out of free cells. Standing the file cells verbatim left whole
        // squads refused as `ImpassableCell` (#82).
        let placed = k1_game::placement::place_roaming(&roster, &passage, &mut Rng::new(seed));
        let trimmed: HashSet<usize> = placed.dropped.iter().copied().collect();

        let mut world = World::new(grid, passage, pace, WorldRules::default(), seed);
        let mut ids = Vec::with_capacity(roster.len());
        let mut spawn_failures = Vec::new();
        let mut load_trimmed = Vec::new();
        let mut by_creature = Vec::new();
        for (index, creature) in roster.iter().enumerate() {
            if trimmed.contains(&index) {
                // Out of the game, not refused: the original shrinks the
                // squad and stops placing (`game.cpp:654-655`).
                ids.push(None);
                load_trimmed.push(creature.id);
                continue;
            }
            let (x, y) = placed.cells[index];
            let column = u32::try_from(x.max(0)).unwrap_or(0);
            let row = u32::try_from(y.max(0)).unwrap_or(0);
            let cell = Cell::new(column, row);
            match world.add(cell) {
                Ok(id) => {
                    by_creature.push(index);
                    ids.push(Some(id));
                }
                Err(error) => {
                    ids.push(None);
                    spawn_failures.push((creature.id, error));
                }
            }
        }

        // The "the creature entered the world" event (docs/research/k1/creatures/animation.md,
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

        let live = ids
            .iter()
            .map(|id| {
                let state = world.state((*id)?)?;
                Some(Live {
                    cell: (state.cell.column as i16, state.cell.row as i16),
                    direction: state.direction.map(|direction| direction.index() as u8),
                    busy: state.moving || state.goal.is_some(),
                })
            })
            .collect::<Vec<_>>();
        let group_states = k1_game::rules::group_states(&roster, &live);
        let debug_groups = state
            .groups
            .iter()
            .flatten()
            .filter(|group| roster.iter().any(|creature| creature.group == group.slot))
            .map(|group| crate::debug::GroupGeometry {
                group: group.slot,
                spawn: group.spawn.into(),
                second_rectangle: group.leash.into(),
            })
            .collect();
        let hero_world_id = hero_id
            .and_then(|hero| roster.iter().position(|creature| creature.id == hero))
            .and_then(|index| ids[index]);
        Simulation {
            melee: melee::CombatRuntime::new(&state.groups, &roster),
            world,
            hero_id,
            hero_world_id,
            hero_order: None,
            hero_active_target: None,
            hero_goal_reaches: 0,
            group_states,
            group_state_tick: None,
            debug_actor: None,
            next_debug_id: 0,
            debug_groups,
            roster,
            labels,
            ids,
            jobs,
            rng: Rng::new(seed ^ 0xDEAD_BEEF),
            tick: 0,
            creature_anim,
            order_failures: Vec::new(),
            cooldown: rules::FailedOrderCooldown::new(WorldRules::default().repath_cooldown_ticks),
            spawn_failures,
            load_cells: placed.cells,
            load_trimmed,
            by_creature,
            tracked_goal: Vec::new(),
            goal_reaches: BTreeMap::new(),
        }
    }

    /// Stable source-record identity of the selected starting hero.
    pub fn hero_id(&self) -> Option<usize> {
        self.hero_id
    }

    pub fn hero_order(&self) -> Option<HeroOrder> {
        self.hero_order
    }

    pub fn hero_goal_reaches(&self) -> u64 {
        self.hero_goal_reaches
    }

    fn hero_world_id(&self) -> Option<CreatureId> {
        self.hero_world_id
    }

    /// Orders the real hero through the shared engine, with an exact target.
    pub fn order_hero_move(&mut self, target: Cell) -> Result<usize, HeroOrderError> {
        self.order_hero_move_with_mode(target, MovementMode::Walk)
    }

    /// Selects the hero's pace through the same exact-target movement order.
    pub fn order_hero_move_with_mode(
        &mut self,
        target: Cell,
        mode: MovementMode,
    ) -> Result<usize, HeroOrderError> {
        if self.defer_combat_move(target, mode)? {
            return Ok(0);
        }
        if self.world.grid().contains(target) {
            self.clear_hero_attack();
        }
        let result = self
            .hero_world_id()
            .ok_or(HeroOrderError::HeroUnavailable)
            .and_then(|hero| {
                self.world
                    .move_to_with_mode(hero, target, GoalPolicy::Exact, mode)
                    .map_err(HeroOrderError::Movement)
            });
        let status = match result {
            Ok(_) => {
                let hero = self.hero_world_id().unwrap();
                let state = self.world.state(hero).unwrap();
                let (requested_target, pending) = self.world.order_state(hero).unwrap();
                if state.cell == target && requested_target.is_none() {
                    HeroOrderStatus::Arrived
                } else {
                    self.hero_active_target = Some(target);
                    if pending {
                        HeroOrderStatus::Pending
                    } else {
                        HeroOrderStatus::Moving
                    }
                }
            }
            Err(error) => {
                if !matches!(error, HeroOrderError::Movement(WorldError::OutsideGrid)) {
                    self.hero_active_target = None;
                }
                HeroOrderStatus::Refused(error)
            }
        };
        self.hero_order = Some(HeroOrder {
            target,
            mode,
            status,
        });
        result
    }

    fn record_hero_events(&mut self, events: &[Event]) {
        let Some(hero) = self.hero_world_id() else {
            return;
        };
        let Some(target) = self.hero_active_target else {
            return;
        };
        let Some((requested_target, pending)) = self.world.order_state(hero) else {
            return;
        };
        let arrived = events.iter().any(|event| {
            matches!(event, Event::Arrived { creature, cell } if *creature == hero && *cell == target)
        });
        let refused = requested_target.is_none()
            && events.iter().any(|event| {
                matches!(event, Event::NoRoute { creature, goal } if *creature == hero && *goal == target)
            });
        let status = if arrived {
            self.hero_goal_reaches += 1;
            self.hero_active_target = None;
            HeroOrderStatus::Arrived
        } else if refused {
            self.hero_active_target = None;
            HeroOrderStatus::Refused(HeroOrderError::Movement(WorldError::NoRoute))
        } else if pending {
            HeroOrderStatus::Pending
        } else {
            HeroOrderStatus::Moving
        };
        if let Some(order) = &mut self.hero_order
            && order.target == target
            && !matches!(order.status, HeroOrderStatus::Refused(_))
        {
            order.status = status;
        }
    }

    /// Creatures the world refused to place at spawn, each with the roster
    /// `Creature.id` and the reason; never dropped silently. After the
    /// load-time re-placement a refusal can only be the record's own fact —
    /// the file stands it in blocked foot data or in a cell another record
    /// holds — so the consumer reports it as data (docs/research/k1/creatures/placement.md,
    /// "Load-time refusals"), while the shipped tests hold the exact list
    /// against the regressions a data fact can never excuse.
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

    /// Record numbers the load-time re-placement trimmed: the squad's spawn
    /// area held no free cell for them, so the original shrinks the squad and
    /// leaves them out (`game.cpp:654-655`). Confirmed load behavior — data
    /// to report, not a refusal to act on.
    pub fn load_trimmed(&self) -> &[usize] {
        &self.load_trimmed
    }

    /// Completed orders whose final cell equalled the commanded target, over
    /// the confirmed targets only — workplace arrivals and walks inside the
    /// spawn area. Deterministic under the same seed, and counters, not
    /// logs: no per-tick position is kept.
    pub fn goal_reaches(&self) -> Vec<GoalReach> {
        self.goal_reaches
            .values()
            .copied()
            .filter(|reach| reach.workplace + reach.walk > 0)
            .collect()
    }

    pub fn tick_number(&self) -> u64 {
        self.tick
    }

    /// Time of day derived from the simulation tick — no independent clock.
    pub fn game_time(&self) -> k1_game::time_of_day::GameTime {
        k1_game::time_of_day::game_time(self.tick)
    }

    pub fn creature_count(&self) -> usize {
        self.ids.iter().filter(|id| id.is_some()).count()
    }

    /// Initial pixel position for each creature in the game, keyed by
    /// `Creature.id` — one entry per node a renderer should build.
    ///
    /// A creature the world refused to place stays in the result at its
    /// load-time cell: a refused record is still part of the game (the world
    /// rejected it only because the file stands it in blocked foot data or on
    /// a cell another record holds), so it is drawn at that cell — the record
    /// cell for everyone the load keeps where the file stands it, the jittered
    /// cell near the spawn centre for a roaming member the world then refused
    /// — and reported through [`Self::spawn_failures`] next to it. A creature
    /// the load trimmed ([`Self::load_trimmed`]) is absent: the original
    /// shrank `GroupSize`, so it is out of the game and stands nowhere — the
    /// renderer must not build a node for it (docs/research/k1/creatures/placement.md, «Roaming
    /// squad placement on map load»).
    pub fn initial_pixels(&self) -> Vec<(usize, (i32, i32))> {
        self.roster
            .iter()
            .zip(self.load_cells.iter())
            .filter(|(creature, _)| !self.load_trimmed.contains(&creature.id))
            .map(|(creature, &(x, y))| {
                let cell = Cell::new(
                    u32::try_from(x.max(0)).unwrap_or(0),
                    u32::try_from(y.max(0)).unwrap_or(0),
                );
                (creature.id, cell.centre())
            })
            .collect()
    }

    /// Adopts the movement side of a freshly built simulation of the same map
    /// and seed while keeping this one's creature animation machine playing.
    ///
    /// "Reset positions" restarts the walk world (routes, walk states, RNG,
    /// cooldown) back to the file's load cells but must NOT restart an in-flight
    /// state-4 special (the machine's per-creature stage is the animation) and
    /// must NOT rewind the simulation's own clock — `tick` is the game-time
    /// source (#95), so it is deliberately left untouched: only the world-side
    /// movement state is carried over, while `roster`, `jobs`, `creature_anim`
    /// and `tick` stay as they were (review of #101, P2-#3). The fresh
    /// simulation is built from the same `map_ref` and `seed`, so its roster and
    /// the machine's creature ids line up.
    pub fn adopt_positions_from(&mut self, fresh: Simulation) {
        let Simulation {
            world,
            hero_id,
            hero_world_id,
            ids,
            rng,
            order_failures,
            cooldown,
            spawn_failures,
            group_states,
            group_state_tick,
            ..
        } = fresh;
        self.world = world;
        self.hero_id = hero_id;
        self.hero_world_id = hero_world_id;
        self.ids = ids;
        self.rng = rng;
        self.order_failures = order_failures;
        self.cooldown = cooldown;
        self.spawn_failures = spawn_failures;
        self.group_states = group_states;
        self.group_state_tick = group_state_tick;
        self.debug_actor = None;
        self.hero_order = None;
        self.hero_active_target = None;
        self.hero_goal_reaches = 0;
        self.reset_combat();
    }

    pub fn tick(&mut self) -> Vec<Event> {
        self.tick += 1;
        // The state machine advances on the caller's tick — never on a
        // render-frame boundary — so creature state transitions are
        // frame-rate independent (issue #70).
        self.creature_anim.tick();
        let live = self.live_picture();
        self.group_states = self.debug_group_states(&live);
        self.group_state_tick = Some(self.tick);
        let combat_busy = self.tick_combat();
        let mut context = Context {
            ground: &self.world,
            random: &mut self.rng,
            jobs: &self.jobs,
            states: &self.group_states,
        };
        let decisions = k1_game::rules::evaluate(&self.roster, &live, &mut context);
        let commands: Vec<(usize, CreatureId, Decision)> = self
            .ids
            .iter()
            .enumerate()
            .zip(decisions.iter())
            .filter_map(|((index, id), decision)| {
                (*id)
                    .filter(|_| !combat_busy[index])
                    .map(|id| (self.roster[index].id, id, *decision))
            })
            .collect();
        let pairs: Vec<(CreatureId, Decision)> = commands
            .iter()
            .map(|&(_, id, decision)| (id, decision))
            .collect();
        // Through the cooldown: the rules re-decide every tick and may pick a
        // fresh cell, so a creature with an unreachable goal would otherwise
        // pay a search per tick (docs/development/rust/movement.md, "Decision: repathing without
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
        // Remember what the world accepted, so an arrival can be judged
        // against the cell the order commanded (the goal-reach counters).
        for (command, outcome) in commands.iter().zip(applied.iter()) {
            let (_, creature_id, decision) = *command;
            if !matches!(outcome, rules::Applied::Accepted { .. }) {
                continue;
            }
            let Decision::MoveTo(column, row) = decision else {
                continue;
            };
            let index = creature_id.raw() as usize;
            if self.tracked_goal.len() <= index {
                self.tracked_goal.resize(index + 1, None);
            }
            self.tracked_goal[index] = Some(Cell::new(column, row));
        }
        let events = self.world.tick_step();
        self.record_hero_events(&events);
        for event in &events {
            if let Event::Arrived { creature, cell } = event {
                self.record_goal_reach(*creature, *cell);
            }
        }
        events
    }

    /// One arrival against the order the creature was sent on: when the
    /// route spent itself on the very cell the rules commanded and that
    /// target is a confirmed one, the creature's counter for it grows.
    fn record_goal_reach(&mut self, creature: CreatureId, cell: Cell) {
        let index = creature.raw() as usize;
        if self
            .tracked_goal
            .get(index)
            .is_none_or(|tracked| *tracked != Some(cell))
        {
            return;
        }
        self.tracked_goal[index] = None;
        let Some(&roster_index) = self.by_creature.get(index) else {
            return;
        };
        let creature = &self.roster[roster_index];
        let (Ok(column), Ok(row)) = (i16::try_from(cell.column), i16::try_from(cell.row)) else {
            return;
        };
        let target = (column, row);
        let Some(field) = goal_reach_field(creature, &self.jobs, target) else {
            return;
        };
        let reach = self.goal_reaches.entry(creature.id).or_insert(GoalReach {
            id: creature.id,
            workplace: 0,
            walk: 0,
        });
        match field {
            GoalField::Workplace => reach.workplace += 1,
            GoalField::Walk => reach.walk += 1,
        }
    }

    pub fn apply_debug_command(
        &mut self,
        command: &crate::debug::DebugCommand,
        profile: Option<&crate::debug::DebugPlayerProfile>,
        mut fresh: impl FnMut() -> Result<Simulation, String>,
    ) -> Result<(), String> {
        use crate::debug::DebugCommand;
        match command {
            DebugCommand::Place {
                cell: (column, row),
            } => {
                if self.debug_actor.is_some() {
                    return Err("Debug player is already placed".into());
                }
                let profile = profile.ok_or("Debug player profile is unavailable")?;
                profile.validate()?;
                let next = self
                    .next_debug_id
                    .checked_add(1)
                    .ok_or("Debug runtime id exhausted")?;
                let id = self
                    .world
                    .add(Cell::new(*column, *row))
                    .map_err(|error| format!("Debug placement: {error:?}"))?;
                self.world.pace_mut().set_row(id, profile.step_ticks);
                self.world
                    .turn(
                        id,
                        crate::movement::grid::Direction::ALL[usize::from(profile.direction)],
                    )
                    .map_err(|error| format!("Debug direction: {error:?}"))?;
                self.debug_actor = Some(DebugActor {
                    id: self.next_debug_id,
                    world_id: id,
                    profile: profile.clone(),
                });
                self.next_debug_id = next;
            }
            DebugCommand::Move {
                cell: (column, row),
            } => {
                let actor = self
                    .debug_actor
                    .as_ref()
                    .ok_or("Debug player is not placed")?;
                self.world
                    .move_to(actor.world_id, Cell::new(*column, *row))
                    .map_err(|error| format!("Debug movement: {error:?}"))?;
            }
            DebugCommand::Remove => {
                let actor = self
                    .debug_actor
                    .as_ref()
                    .ok_or("Debug player is not placed")?;
                self.world
                    .remove(actor.world_id)
                    .map_err(|error| format!("Debug removal: {error:?}"))?;
                self.debug_actor = None;
            }
            DebugCommand::ResetPositions => {
                let rebuilt = fresh()?;
                if rebuilt.tick_number() != 0 || rebuilt.roster != self.roster {
                    return Err(
                        "Position reset factory returned a different roster or nonzero tick".into(),
                    );
                }
                self.adopt_positions_from(rebuilt);
            }
            DebugCommand::ResetAnimation => self.restart_all_specials(),
        }
        Ok(())
    }

    pub fn debug_player(&self) -> Option<crate::debug::DebugPlayerSnapshot> {
        let actor = self.debug_actor.as_ref()?;
        Some(crate::debug::DebugPlayerSnapshot {
            id: actor.id,
            source_hero: actor.profile.hero,
            source_character: actor.profile.character,
            movement: self.world.debug_state(actor.world_id)?.into(),
        })
    }

    pub fn debug_snapshot(&self) -> crate::debug::DebugSnapshot {
        let preview = self.debug_group_states(&self.live_picture());
        crate::debug::DebugSnapshot {
            tick: self.tick,
            player: self.debug_player(),
            groups: self.debug_groups.clone(),
            creatures: self
                .roster
                .iter()
                .enumerate()
                .map(|(index, creature)| crate::debug::DebugCreatureSnapshot {
                    record_id: creature.id,
                    group: creature.group,
                    used_group_state: self.group_states[index].byte(),
                    preview_group_state: preview[index].byte(),
                    used_group_tick: self.group_state_tick,
                    pursuit: self.melee.pursuits[index],
                    movement: self.ids[index]
                        .and_then(|id| self.world.debug_state(id))
                        .map(Into::into),
                })
                .collect(),
        }
    }

    fn debug_group_states(&self, live: &[Option<Live>]) -> Vec<GroupState> {
        let Some(actor) = self.debug_actor.as_ref() else {
            return k1_game::rules::group_states(&self.roster, live);
        };
        let mut roster = self.roster.clone();
        let profile = &actor.profile;
        roster.push(Creature {
            id: usize::MAX,
            cell: (0, 0),
            direction: profile.direction,
            state: profile.recorded_state,
            class_id: profile.class_id,
            group: profile.group,
            group_kind: k1_game::rules::group::PLAYER,
            roaming: false,
            spawn: Default::default(),
            workplace: None,
            name_id: 0,
            nick_id: 0,
            sprite_set: 0,
        });
        let mut picture = live.to_vec();
        picture.push(self.world.state(actor.world_id).map(|state| Live {
            cell: (state.cell.column as i16, state.cell.row as i16),
            direction: state.direction.map(|direction| direction.index() as u8),
            busy: state.moving || state.goal.is_some(),
        }));
        k1_game::rules::group_states(&roster, &picture)
    }

    /// Inspects by character record id, never by roster or movement-world index.
    pub fn inspect_creature(&self, id: usize) -> Option<CreatureInspection> {
        let index = self.roster.iter().position(|creature| creature.id == id)?;
        Some(self.inspection_at(index))
    }

    fn inspection_at(&self, index: usize) -> CreatureInspection {
        let creature = &self.roster[index];
        let id = creature.id;
        CreatureInspection {
            id,
            label: self
                .labels
                .get(&id)
                .cloned()
                .unwrap_or_else(|| format!("#{id}")),
            recorded_state: creature.state,
            animation_state: self.creature_shown_state(id),
            animation_stage: self.creature_stage(id),
            walk_state: self.creature_state(index).map(|state| state.walk_state),
            group: creature.group,
            group_state: self.group_states[index],
            group_state_tick: self.group_state_tick,
            attack: self.attack_order(id),
            pursuit: self.melee.pursuits[index],
        }
    }

    pub fn inspect_creatures(&self) -> Vec<CreatureInspection> {
        (0..self.roster.len())
            .map(|index| self.inspection_at(index))
            .collect()
    }

    pub fn states(&self) -> Vec<CreatureState> {
        (0..self.roster.len())
            .filter_map(|index| self.creature_state(index))
            .collect()
    }

    /// Ready `#<id> <name>` label per creature record number ([`Creature::id`]),
    /// built once at load by [`k1_game::creatures::labels`] (issue #84). A
    /// consumer — the viewer's label layer — reads the finished strings from
    /// here and never touches the K1 name tables itself.
    pub fn labels(&self) -> &HashMap<usize, String> {
        &self.labels
    }

    /// A source name without the viewer's diagnostic record-number prefix.
    pub fn creature_name(&self, creature_id: usize) -> Option<&str> {
        self.labels
            .get(&creature_id)?
            .split_once(' ')
            .map(|(_, name)| name)
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

    /// Restart every state-4 creature's special from frame 0 — the viewer's
    /// "Reset animation" (review of #101, P2-#3): only the animation machine
    /// rewinds, the walk world and the game clock stay exactly where they are.
    pub fn restart_all_specials(&mut self) {
        self.creature_anim.restart_all_specials();
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
            direction: Some(
                walk.direction
                    .map_or(self.roster[index].direction, |d| d.index() as u8),
            ),
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

// ─── Goal reach: the confirmed targets a completed order can hit ────────────
//
// The rules issue a bare `MoveTo`, so the counter re-derives the target's
// kind from the same inputs the decisions had — gated on the creature's
// decision category, because both confirmed counters originate in
// `k1_game::rules::villager_decision` (the dynamic workplace, the static-work
// neighbour, and the pick band of `walk_target`). A roaming monster's only
// `MoveTo` is `monster_return`, whose target lies inside the spawn interior
// but earns no counter (docs/research/k1/creatures/placement.md, "Goal reach"). Anything else an
// order aimed at — a cell no confirmed villager rule sends to — earns none.

/// Which confirmed counter a completed order earns, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GoalField {
    Workplace,
    Walk,
}

/// The confirmed counter a creature's arrival at `cell` earns, or `None`.
///
/// Gated on the decision category first: only a villager's order reaches a
/// confirmed target. Then workplace before walk, the same precedence the
/// decisions have — the dynamic place cell or the static-work neighbour via
/// [`stands_on_workplace`], else a cell inside the spawn interior via
/// [`inside_spawn_interior`]. A roaming monster's `monster_return` targets the
/// interior, so the gate is what stops its return home from being counted as
/// a walk (docs/research/k1/creatures/placement.md, "Goal reach").
fn goal_reach_field(
    creature: &Creature,
    jobs: &[Workplace],
    cell: (i16, i16),
) -> Option<GoalField> {
    if classify(creature) != CreatureType::Villager {
        return None;
    }
    if stands_on_workplace(creature, jobs, cell) {
        return Some(GoalField::Workplace);
    }
    if inside_spawn_interior(creature, cell) {
        return Some(GoalField::Walk);
    }
    None
}

/// Whether `cell` is a workplace target of `creature`: the place cell of the
/// slot the creature itself holds (the dynamic workplace — its arrival stands
/// exactly on the place), or a neighbour of the place of the job its class
/// bits 3–5 name while that slot is occupied (the static-work order joins
/// the place from a free neighbour, `rules.rs::static_work`).
fn stands_on_workplace(creature: &Creature, jobs: &[Workplace], cell: (i16, i16)) -> bool {
    if creature.workplace.is_some_and(|place| place.cell == cell) {
        return true;
    }
    if jobs
        .iter()
        .any(|place| usize::try_from(place.character) == Ok(creature.id) && place.cell == cell)
    {
        return true;
    }
    let job = usize::from((creature.class_id >> 3) & 7);
    job != 0
        && jobs
            .get(job)
            .is_some_and(|place| place.character != 0 && is_adjacent(cell, place.cell))
}

/// Whether `cell` lies strictly inside the creature's spawn area — the band
/// a walk order picks from (`k1_game::rules::walk_target` draws
/// `left + 1 ..= right - 1` on both axes); a degenerate area holds none.
fn inside_spawn_interior(creature: &Creature, cell: (i16, i16)) -> bool {
    let spawn = &creature.spawn;
    spawn.left.saturating_add(1) <= cell.0
        && cell.0 <= spawn.right.saturating_sub(1)
        && spawn.top.saturating_add(1) <= cell.1
        && cell.1 <= spawn.bottom.saturating_sub(1)
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
    /// (docs/research/k1/formats/map.md, `is_blocked`). No road, terrain or other modifier
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

    /// One squad of one `Group` on [`MAP_NUMBER`] with `record_cells` as the
    /// characters' file cells: the minimum `Game.N` the roster reads (kind in
    /// the range, `size` consecutive character records). The load rule
    /// re-places roaming members near the spawn centre; everyone else keeps
    /// the cell the record carries.
    fn squad_state(kind: u8, spawn: Rect, record_cells: &[(i16, i16)]) -> GameState {
        let group = Group {
            slot: 1,
            first_character: FIRST_RECORD as i16,
            size: record_cells.len() as u8,
            max_size: record_cells.len() as u8,
            growth_limit: record_cells.len() as u8,
            kind,
            map_id: MAP_NUMBER as u8,
            spawn,
            ..Group::default()
        };
        let mut characters: Vec<Option<Character>> = (0..FIRST_RECORD + record_cells.len())
            .map(|_| None)
            .collect();
        for (offset, &(x, y)) in record_cells.iter().enumerate() {
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

    /// One roaming squad whose records stand at (0, 0) — where the shipped
    /// files put roaming members before the load scatters them.
    fn roaming_state(spawn: Rect, count: usize) -> GameState {
        squad_state(
            k1_game::rules::group::FIRST_ROAMING,
            spawn,
            &vec![(0, 0); count],
        )
    }

    /// One village squad: kind 0x80, so the villager rules decide for it;
    /// the records stand at `cells` and the load keeps them there.
    fn village_state(spawn: Rect, cells: &[(i16, i16)]) -> GameState {
        squad_state(k1_game::rules::group::VILLAGE_FIRST, spawn, cells)
    }

    /// One jobless villager at (12, 12) whose village area is (4, 4)-(8, 8):
    /// the half roll sends it walking into the area, away from its cell.
    fn villager_outside_their_area() -> GameState {
        village_state(
            Rect {
                left: 4,
                top: 4,
                right: 8,
                bottom: 8,
            },
            &[(12, 12)],
        )
    }

    fn simulation_with(
        map: &KonungMap,
        state: &GameState,
        creatures: usize,
        jobs: Vec<Workplace>,
    ) -> Simulation {
        Simulation::new(
            map,
            state,
            MAP_NUMBER,
            None,
            vec![[2; DIRECTION_COUNT]; creatures],
            vec![0; creatures],
            jobs,
            7,
            None,
        )
    }

    fn simulation(map: &KonungMap, state: &GameState, creatures: usize) -> Simulation {
        simulation_with(map, state, creatures, Vec::new())
    }

    fn hero_simulation(blocked: impl Fn(u32, u32) -> bool) -> Simulation {
        let map = open_map(blocked);
        let mut state = village_state(
            Rect {
                left: 40,
                top: 40,
                right: 48,
                bottom: 48,
            },
            &[(44, 44)],
        );
        state.groups[1].as_mut().unwrap().map_id = 1;
        let hero_record = 7;
        state.characters.resize_with(hero_record + 1, || None);
        state.characters[hero_record] = Some(Character {
            slot: hero_record,
            x: 8,
            y: 8,
            group: 0,
            class_id: k1_game::rules::class::WARRIOR,
            ..Character::default()
        });
        state.groups[0] = Some(Group {
            slot: 0,
            first_character: hero_record as i16,
            size: 1,
            kind: 0x20,
            ..Group::default()
        });
        Simulation::new(
            &map,
            &state,
            1,
            Some(0),
            vec![[2; DIRECTION_COUNT]; 2],
            vec![0; 2],
            Vec::new(),
            7,
            None,
        )
    }

    #[test]
    fn hero_run_mode_changes_only_hero_steps_and_keeps_npc_ticks() {
        let mut sim = hero_simulation(|_, _| false);
        let mut baseline = hero_simulation(|_, _| false);
        let hero = sim.hero_world_id().unwrap();
        sim.world.pace_mut().set_row(hero, [12; DIRECTION_COUNT]);
        baseline
            .world
            .pace_mut()
            .set_row(hero, [12; DIRECTION_COUNT]);
        let target = Cell::new(18, 8);
        sim.order_hero_move_with_mode(target, MovementMode::Run)
            .unwrap();
        assert_eq!(sim.hero_order().unwrap().mode, MovementMode::Run);
        let mut npc_events = 0;
        let mut hero_steps = 0;
        for _ in 0..100 {
            let events = sim.tick();
            for event in &events {
                if let Event::StepStarted {
                    creature, ticks, ..
                } = event
                    && *creature == hero
                {
                    hero_steps += 1;
                    assert_eq!(*ticks, 6);
                }
            }
            let events = events
                .into_iter()
                .filter(|event| event.creature() != hero)
                .collect::<Vec<_>>();
            npc_events += events.len();
            let expected = baseline
                .tick()
                .into_iter()
                .filter(|event| event.creature() != hero)
                .collect::<Vec<_>>();
            assert_eq!(events, expected);
            assert_eq!(sim.tick_number(), baseline.tick_number());
        }
        assert!(hero_steps > 0 && npc_events > 0);
        assert_eq!(sim.hero_goal_reaches(), 1);
        assert_eq!(sim.hero_order().unwrap().status, HeroOrderStatus::Arrived);
        sim.order_hero_move(Cell::new(20, 8)).unwrap();
        assert_eq!(sim.hero_order().unwrap().mode, MovementMode::Walk);
    }

    #[test]
    fn hero_order_resolves_source_identity_and_records_exact_arrival_once() {
        let mut sim = hero_simulation(|column, row| column == 11 && (4..=12).contains(&row));
        assert_eq!(sim.hero_id(), Some(7));
        let hero = sim.hero_world_id().unwrap();
        assert_ne!(hero.raw(), 0, "the NPC has the first world id");
        let target = Cell::new(15, 8);
        assert!(sim.order_hero_move(target).is_ok());
        let mut route_seen = false;
        for _ in 0..100 {
            let events = sim.tick();
            route_seen |= events.iter().any(
                |event| matches!(event, Event::RouteSet { creature, .. } if *creature == hero),
            );
            if sim.hero_goal_reaches() == 1 {
                break;
            }
        }
        assert!(route_seen);
        assert_eq!(sim.world.cell_of(hero), Some(target));
        assert_eq!(sim.hero_order().unwrap().status, HeroOrderStatus::Arrived);
        assert_eq!(sim.hero_goal_reaches(), 1);
        for _ in 0..10 {
            assert_eq!(sim.order_hero_move(target), Ok(0));
            sim.tick();
        }
        assert_eq!(sim.hero_goal_reaches(), 1);
        assert_eq!(sim.hero_order().unwrap().status, HeroOrderStatus::Arrived);
        assert_eq!(
            sim.order_hero_move(Cell::new(FINE_WIDTH, FINE_HEIGHT)),
            Err(HeroOrderError::Movement(WorldError::OutsideGrid))
        );
        assert_eq!(sim.order_hero_move(target), Ok(0));
        assert_eq!(sim.hero_order().unwrap().status, HeroOrderStatus::Arrived);
        assert_eq!(sim.hero_goal_reaches(), 1);
    }

    #[test]
    fn hero_refusal_is_explicit_and_repeated_unreachable_orders_are_throttled() {
        let mut sim = hero_simulation(|column, _| column == 12);
        let target = Cell::new(15, 8);
        assert_eq!(
            sim.order_hero_move(target),
            Err(HeroOrderError::Movement(WorldError::NoRoute))
        );
        let hero = sim.hero_world_id().unwrap();
        let mut no_routes = 0;
        for _ in 0..30 {
            assert!(sim.order_hero_move(target).is_err());
            no_routes += sim
                .tick()
                .iter()
                .filter(
                    |event| matches!(event, Event::NoRoute { creature, .. } if *creature == hero),
                )
                .count();
        }
        assert_eq!(no_routes, 1);
        assert_eq!(sim.world.cell_of(hero), Some(Cell::new(8, 8)));
        assert_eq!(sim.hero_goal_reaches(), 0);
        assert_eq!(
            sim.hero_order().unwrap().status,
            HeroOrderStatus::Refused(HeroOrderError::Movement(WorldError::NoRoute))
        );
    }

    #[test]
    fn hero_reorder_keeps_independent_npc_events_and_world_ticks() {
        let mut sim = hero_simulation(|_, _| false);
        let mut baseline = hero_simulation(|_, _| false);
        let hero = sim.hero_world_id().unwrap();
        let first = Cell::new(18, 8);
        let second = Cell::new(8, 18);
        sim.order_hero_move(first).unwrap();
        let mut npc_events = 0;
        for tick in 0..100 {
            if tick == 1 {
                sim.order_hero_move(second).unwrap();
                assert_eq!(sim.hero_order().unwrap().status, HeroOrderStatus::Pending);
            }
            let events = sim
                .tick()
                .into_iter()
                .filter(|event| event.creature() != hero)
                .collect::<Vec<_>>();
            let expected = baseline
                .tick()
                .into_iter()
                .filter(|event| event.creature() != hero)
                .collect::<Vec<_>>();
            npc_events += events.len();
            assert_eq!(events, expected);
            assert_eq!(sim.tick_number(), baseline.tick_number());
        }
        assert!(npc_events > 0);
        assert_eq!(sim.world.cell_of(hero), Some(second));
        assert_eq!(sim.hero_goal_reaches(), 1);
        assert_eq!(sim.hero_order().unwrap().target, second);
    }

    #[test]
    fn hero_latest_order_ignores_superseded_search_events_and_latches_late_refusal() {
        let mut sim = hero_simulation(|_, _| false);
        let first = Cell::new(12, 8);
        let target = Cell::new(8, 16);
        sim.order_hero_move(first).unwrap();
        sim.order_hero_move(target).unwrap();
        sim.tick();
        assert_eq!(sim.hero_order().unwrap().target, target);
        assert_eq!(sim.hero_goal_reaches(), 0);
        sim.world
            .passage_mut()
            .blocked
            .insert((target.column, target.row));
        for _ in 0..100 {
            sim.tick();
        }
        assert_eq!(
            sim.hero_order().unwrap().status,
            HeroOrderStatus::Refused(HeroOrderError::Movement(WorldError::NoRoute))
        );
        assert_eq!(sim.hero_goal_reaches(), 0);
        assert_ne!(
            sim.world.cell_of(sim.hero_world_id().unwrap()),
            Some(target)
        );
    }

    #[test]
    fn hero_multiple_commands_before_a_tick_only_complete_the_latest_target() {
        let mut sim = hero_simulation(|_, _| false);
        let first = Cell::new(8, 8);
        let other = Cell::new(20, 8);
        sim.order_hero_move(first).unwrap();
        sim.order_hero_move(other).unwrap();
        sim.order_hero_move(first).unwrap();
        sim.tick();
        assert_eq!(sim.hero_goal_reaches(), 1);
        assert_eq!(
            sim.hero_order().unwrap(),
            HeroOrder {
                mode: MovementMode::Walk,
                target: first,
                status: HeroOrderStatus::Arrived
            }
        );
        sim.order_hero_move(Cell::new(159, 319)).unwrap();
        sim.order_hero_move(other).unwrap();
        for _ in 0..100 {
            sim.tick();
        }
        assert_eq!(sim.hero_goal_reaches(), 2);
        assert_eq!(
            sim.hero_order().unwrap(),
            HeroOrder {
                mode: MovementMode::Walk,
                target: other,
                status: HeroOrderStatus::Arrived
            }
        );
    }

    #[test]
    fn hero_outside_refusal_keeps_active_movement_and_its_arrival() {
        let mut sim = hero_simulation(|_, _| false);
        let target = Cell::new(18, 8);
        sim.order_hero_move(target).unwrap();
        sim.tick();
        let outside = Cell::new(FINE_WIDTH, FINE_HEIGHT);
        assert_eq!(
            sim.order_hero_move(outside),
            Err(HeroOrderError::Movement(WorldError::OutsideGrid))
        );
        for _ in 0..100 {
            sim.tick();
        }
        assert_eq!(sim.hero_goal_reaches(), 1);
        assert_eq!(
            sim.hero_order().unwrap(),
            HeroOrder {
                mode: MovementMode::Walk,
                target: outside,
                status: HeroOrderStatus::Refused(HeroOrderError::Movement(WorldError::OutsideGrid))
            }
        );
        assert_eq!(sim.order_hero_move(target), Ok(0));
        assert_eq!(sim.hero_order().unwrap().status, HeroOrderStatus::Arrived);
        assert_eq!(sim.hero_goal_reaches(), 1);
    }

    #[test]
    fn hero_order_without_a_placed_hero_is_typed_and_reset_clears_its_state() {
        let map = open_map(|_, _| false);
        let state = villager_outside_their_area();
        let mut absent = simulation(&map, &state, 1);
        assert_eq!(
            absent.order_hero_move(Cell::new(8, 8)),
            Err(HeroOrderError::HeroUnavailable)
        );
        let mut unplaced = hero_simulation(|column, row| column == 8 && row == 8);
        assert_eq!(unplaced.hero_id(), Some(7));
        assert_eq!(
            unplaced.order_hero_move(Cell::new(9, 8)),
            Err(HeroOrderError::HeroUnavailable)
        );
        let mut sim = hero_simulation(|_, _| false);
        sim.order_hero_move(Cell::new(9, 8)).unwrap();
        for _ in 0..10 {
            sim.tick();
        }
        assert_eq!(sim.hero_goal_reaches(), 1);
        sim.adopt_positions_from(hero_simulation(|_, _| false));
        assert_eq!(sim.hero_order(), None);
        assert_eq!(sim.hero_goal_reaches(), 0);
    }

    #[test]
    #[ignore = "manual synthetic hero-order cost measurement"]
    fn hero_order_costs() {
        use std::{hint::black_box, time::Instant};
        for policy in [GoalPolicy::Nearby, GoalPolicy::Exact] {
            for (label, failed, cached, count) in [
                ("reachable", false, false, 100),
                ("unreachable", true, false, 10),
                ("cached refusal", true, true, 100_000),
            ] {
                let mut minimum = f64::INFINITY;
                for _ in 0..5 {
                    let mut sim = hero_simulation(|column, _| failed && column == 12);
                    let hero = sim.hero_world_id().unwrap();
                    let target = if failed {
                        Cell::new(16, 8)
                    } else {
                        Cell::new(100, 100)
                    };
                    if cached {
                        assert!(sim.world.move_to_with_policy(hero, target, policy).is_err());
                    }
                    let start = Instant::now();
                    for _ in 0..count {
                        if !cached {
                            sim.world.stop(hero).unwrap();
                        }
                        black_box(sim.world.move_to_with_policy(hero, target, policy)).ok();
                    }
                    minimum = minimum.min(start.elapsed().as_secs_f64() * 1e6 / f64::from(count));
                }
                println!("{policy:?} {label}: {minimum:.6} us/order");
            }
        }
        let mut sim = hero_simulation(|_, _| false);
        let hero = sim.hero_world_id().unwrap();
        let start = Instant::now();
        for _ in 0..5000 {
            black_box(sim.tick());
        }
        println!(
            "independent simulation tick: {:.6} us/tick",
            start.elapsed().as_secs_f64() * 1e6 / 5000.0
        );
        let start = Instant::now();
        for _ in 0..100 {
            sim.world.stop(hero).unwrap();
            black_box(sim.order_hero_move(Cell::new(100, 100))).unwrap();
        }
        println!(
            "typed hero reachable: {:.6} us/order",
            start.elapsed().as_secs_f64() * 1e6 / 100.0
        );
    }

    #[test]
    #[ignore = "manual synthetic workspace cost measurement"]
    fn workspace_inspection_costs() {
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
        let map = open_map(|_, _| false);
        let cells = (0..200)
            .map(|id| ((id % 25 + 10) as i16, (id / 25 + 10) as i16))
            .collect::<Vec<_>>();
        let state = squad_state(k1_game::rules::group::LONE_NPC, Rect::default(), &cells);
        let mut sim = simulation(&map, &state, 200);
        assert_eq!(sim.creature_count(), 200);
        assert!(sim.inspect_creature(FIRST_RECORD + 199).is_some());
        let one_ns = measure(
            || {
                black_box(sim.inspect_creature(black_box(FIRST_RECORD + 199)));
            },
            1000,
        );
        let all_ns = measure(
            || {
                black_box(sim.inspect_creatures());
            },
            1000,
        );
        let states_ns = measure(
            || {
                black_box(sim.states());
            },
            1000,
        );
        let tick_ns = measure(
            || {
                black_box(sim.tick());
            },
            1000,
        );
        println!(
            "WORKSPACE_COST inspect_one_200_ns={one_ns:.0} inspect_all_200_ns={all_ns:.0} states_200_ns={states_ns:.0} tick_200_ns={tick_ns:.0}"
        );
    }

    #[test]
    fn inspection_keeps_the_exact_decision_snapshot_until_the_next_tick() {
        let map = open_map(|_, _| false);
        let state = roaming_state(
            Rect {
                left: 4,
                top: 4,
                right: 8,
                bottom: 8,
            },
            1,
        );
        let mut sim = simulation(&map, &state, 1);
        let id = sim.roster[0].id;
        let initial = sim.inspect_creature(id).unwrap();
        assert_eq!(initial.group_state, GroupState::Calm);
        assert_eq!(initial.group_state_tick, None);
        assert_eq!(initial.recorded_state, sim.roster[0].state);
        assert!(sim.inspect_creature(usize::MAX).is_none());
        let cell = (4..=8)
            .flat_map(|row| (4..=8).map(move |column| Cell::new(column, row)))
            .find(|cell| sim.world.holder(*cell).is_none())
            .unwrap();
        let player = sim.world.add(cell).unwrap();
        let mut record = sim.roster[0];
        record.id = 999;
        record.group_kind = k1_game::rules::group::PLAYER;
        record.group = 99;
        record.roaming = false;
        sim.roster.push(record);
        sim.ids.push(Some(player));
        sim.melee = melee::CombatRuntime::new(&state.groups, &sim.roster);
        sim.tick();
        let used = sim.inspect_creature(id).unwrap();
        assert_eq!(used.group_state, GroupState::PlayerInside);
        assert_eq!(used.group_state_tick, Some(1));
        sim.world.remove(player).unwrap();
        assert_eq!(
            sim.inspect_creature(id).unwrap().group_state,
            GroupState::PlayerInside
        );
        sim.tick();
        assert_eq!(
            sim.inspect_creature(id).unwrap().group_state,
            GroupState::Calm
        );
        let fresh = simulation(&map, &state, 1);
        sim.roster.pop();
        sim.adopt_positions_from(fresh);
        let reset = sim.inspect_creature(id).unwrap();
        assert_eq!(reset.group_state_tick, None);
        assert_eq!(reset.group_state, GroupState::Calm);
        assert_eq!(sim.tick_number(), 2);
    }

    fn inside(spawn: &Rect, cell: (u32, u32)) -> bool {
        let (column, row) = (cell.0 as i16, cell.1 as i16);
        spawn.left <= column && column <= spawn.right && spawn.top <= row && row <= spawn.bottom
    }

    /// The full loop over production data structures: the rules decide from
    /// the world's live picture, the orders reach the world, the creature
    /// really walks them — and nothing on the way is rejected for a reason
    /// other than "no route". The rules only aim at cells `Ground::is_free`
    /// has vetted, so any other rejection is a bug; this is the guard. The
    /// driver is a jobless villager walking into its village area: after the
    /// load rule puts every roaming member inside its own spawn area, the
    /// monster return can no longer be triggered from a fresh load.
    #[test]
    fn a_driven_run_walks_a_villager_into_its_area_without_order_failures() {
        let map = open_map(|_, _| false);
        let area = Rect {
            left: 4,
            top: 4,
            right: 8,
            bottom: 8,
        };
        let state = villager_outside_their_area();
        let mut sim = simulation(&map, &state, 1);

        let mut walked = false;
        let mut went_inside = false;
        for _ in 0..600 {
            let events = sim.tick();
            walked |= events
                .iter()
                .any(|event| matches!(event, Event::StepFinished { .. }));
            assert!(
                sim.order_failures().is_empty(),
                "{:?}",
                sim.order_failures()
            );
            if inside(&area, sim.states()[0].cell) {
                went_inside = true;
                break;
            }
        }
        assert!(walked, "the walk order must produce real steps");
        assert!(went_inside, "the villager must reach its village area");
    }

    /// An unreachable village area is the rules' normal verdict, not a failure:
    /// the re-deciding loop may aim at it every tick, so `NoRoute` must not
    /// pollute [`Simulation::order_failures`], and the connected
    /// `FailedOrderCooldown` must keep the searches far below one per tick.
    /// A villager stands outside a walled area the walk rule keeps picking.
    #[test]
    fn an_unreachable_area_costs_throttled_searches_not_order_failures() {
        const TICKS: usize = 130;
        let ring = |column: u32, row: u32| {
            (3..=9).contains(&column) && (row == 3 || row == 9)
                || (3..=9).contains(&row) && (column == 3 || column == 9)
        };
        let map = open_map(ring);
        let area = Rect {
            left: 4,
            top: 4,
            right: 8,
            bottom: 8,
        };
        let state = village_state(area, &[(13, 13)]);
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
            "the walk order must have been issued and searched"
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
    /// `initial_pixels` at its load-time cell — the documented fallback, the
    /// record cell for a creature the load keeps where the file stands it.
    #[test]
    fn an_unplaced_creature_is_reported_and_keeps_its_load_cell_for_the_renderer() {
        let map = open_map(|column, row| column == 20 && row == 20);
        let area = Rect {
            left: 24,
            top: 24,
            right: 26,
            bottom: 26,
        };
        // A villager the file stands in a wall: only roaming squads are
        // re-placed, so the load keeps it in the wall and the world refuses
        // it visibly.
        let state = village_state(area, &[(20, 20), (25, 25)]);
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
            "the renderer still gets the refused creature at its load cell"
        );
    }

    /// The confirmed load rule over the façade: a roaming squad whose
    /// records all stand at (0, 0) — blocked foot data there, as in every
    /// shipped file — is re-placed inside its spawn area, nobody refused,
    /// nobody trimmed (#82: standing the file cells verbatim refused whole
    /// squads as `ImpassableCell`).
    #[test]
    fn a_roaming_squad_is_re_placed_inside_its_area_on_load() {
        let map = open_map(|column, row| column == 0 && row == 0);
        let spawn = Rect {
            left: 10,
            top: 10,
            right: 14,
            bottom: 14,
        };
        let state = roaming_state(spawn, 3);
        let sim = simulation(&map, &state, 3);

        assert!(
            sim.spawn_failures().is_empty(),
            "the (0, 0) file cells are not where the squad stands: {:?}",
            sim.spawn_failures()
        );
        assert!(sim.load_trimmed().is_empty());
        let cells: Vec<_> = sim.states().iter().map(|state| state.cell).collect();
        let distinct: HashSet<_> = cells.iter().copied().collect();
        assert_eq!(distinct.len(), 3, "the squad shares cells: {cells:?}");
        for cell in &cells {
            assert!(
                spawn.left < (cell.0 as i16)
                    && (cell.0 as i16) < spawn.right
                    && spawn.top < (cell.1 as i16)
                    && (cell.1 as i16) < spawn.bottom,
                "{cell:?} left the interior of the spawn area"
            );
        }
    }

    /// The load rule trims a squad whose area runs out of free cells — the
    /// original's `GroupSize` shrink — instead of refusing the members the
    /// world could not take: the trimmed are reported by
    /// [`Simulation::load_trimmed`], never as `spawn_failures`.
    #[test]
    fn a_squad_without_room_in_its_area_is_trimmed_not_refused() {
        // A one-cell area: the degenerate rectangle places on its centre and
        // every later member finds no free cell in its hundred tries.
        let map = open_map(|_, _| false);
        let spot = Rect {
            left: 12,
            top: 12,
            right: 12,
            bottom: 12,
        };
        let state = roaming_state(spot, 3);
        let sim = simulation(&map, &state, 3);

        assert!(
            sim.spawn_failures().is_empty(),
            "a trimmed squad is data, not a refusal: {:?}",
            sim.spawn_failures()
        );
        assert_eq!(
            sim.load_trimmed(),
            &[3, 4],
            "the last two members are out of the game, the first stands on the spot"
        );
        assert_eq!(
            sim.states().iter().map(|s| s.cell).collect::<Vec<_>>(),
            vec![(12, 12)]
        );
    }

    /// The renderer's node list follows the game: a trimmed member (out of the
    /// game via `GroupSize`, [`Simulation::load_trimmed`]) is absent from
    /// [`Simulation::initial_pixels`], while the member that holds the spot
    /// keeps its load cell. Refused creatures still appear (the unplaced test
    /// pins that); a trimmed one must not, or the renderer draws a phantom.
    #[test]
    fn a_trimmed_creature_is_absent_from_the_renderer_pixels() {
        // The same degenerate one-cell area as the trim test: records 3 and 4
        // are trimmed, record 2 stands on the spot.
        let map = open_map(|_, _| false);
        let spot = Rect {
            left: 12,
            top: 12,
            right: 12,
            bottom: 12,
        };
        let state = roaming_state(spot, 3);
        let sim = simulation(&map, &state, 3);

        assert!(
            sim.spawn_failures().is_empty(),
            "the standing member is not refused: {:?}",
            sim.spawn_failures()
        );
        assert_eq!(sim.load_trimmed(), &[3, 4]);
        assert_eq!(
            sim.initial_pixels(),
            vec![(2, Cell::new(12, 12).centre())],
            "the trimmed members are out of the game, so the renderer gets \
             only the standing one at its load cell"
        );
    }

    /// The same seed loads and runs the same way: placement draws and
    /// decisions ride the seeded generators only.
    #[test]
    fn the_same_seed_loads_and_runs_the_same_way() {
        let map = open_map(|_, _| false);
        let spawn = Rect {
            left: 10,
            top: 10,
            right: 20,
            bottom: 20,
        };
        let state = roaming_state(spawn, 5);
        let mut first = simulation(&map, &state, 5);
        let mut second = simulation(&map, &state, 5);
        assert_eq!(first.initial_pixels(), second.initial_pixels());
        for _ in 0..50 {
            let a = first.tick();
            let b = second.tick();
            assert_eq!(a, b);
        }
        assert_eq!(first.goal_reaches(), second.goal_reaches());
    }

    /// The goal-reach counters count completed confirmed orders only: the
    /// villager sent onto the village slot it holds arrives on the place
    /// cell (workplace), the jobless one lands on walk cells inside the
    /// spawn area (walk). The counters are deterministic under the seed.
    #[test]
    fn goal_reaches_count_arrivals_at_commanded_confirmed_targets() {
        let map = open_map(|_, _| false);
        let area = Rect {
            left: 4,
            top: 4,
            right: 12,
            bottom: 12,
        };
        // Two villagers far from the village: record 2 holds the only slot,
        // record 3 holds nothing and walks on the half roll.
        let state = village_state(area, &[(30, 30), (31, 30)]);
        let jobs = vec![Workplace {
            cell: (5, 5),
            direction: 0,
            role: "elder",
            character: 2,
        }];
        let mut sim = simulation_with(&map, &state, 2, jobs.clone());
        for _ in 0..600 {
            sim.tick();
        }
        let reaches = sim.goal_reaches();
        let holder = reaches
            .iter()
            .find(|reach| reach.id == 2)
            .expect("the holder walks onto its place cell");
        assert!(
            holder.workplace >= 1,
            "the holder must arrive on its place cell at least once: {reaches:?}"
        );
        let jobless = reaches
            .iter()
            .find(|reach| reach.id == 3)
            .expect("the jobless villager walks into the area");
        assert!(
            jobless.walk >= 1,
            "the jobless rolls must land walks inside the area at least once: \
             {reaches:?}"
        );
        // A confirmed target of one kind is not counted under the other.
        assert_eq!(
            jobless.workplace, 0,
            "the jobless villager holds no slot to arrive at"
        );

        // Determinism: the same seed replays the same counters.
        let mut replay = simulation_with(&map, &state, 2, jobs);
        for _ in 0..600 {
            replay.tick();
        }
        assert_eq!(replay.goal_reaches(), reaches);
    }

    /// A bare roster record for the classifier tests, which need a `Creature`
    /// directly: the public `tick` can no longer drive a monster's return,
    /// because the load rule re-places every roaming member inside its own
    /// area, so `monster_return` never fires from a fresh [`Simulation`].
    fn record(
        group_kind: u8,
        class_id: u8,
        id: usize,
        spawn: Rect,
        workplace: Option<Workplace>,
    ) -> Creature {
        Creature {
            id,
            cell: (0, 0),
            direction: 0,
            state: 0,
            class_id,
            group: 1,
            group_kind,
            roaming: matches!(
                group_kind,
                k1_game::rules::group::FIRST_ROAMING..=k1_game::rules::group::LAST_ROAMING
            ),
            spawn,
            workplace,
            name_id: 0,
            nick_id: 0,
            sprite_set: 0,
        }
    }

    /// The false positive Finding 2 removes: a roaming monster standing inside
    /// its spawn interior — exactly where `monster_return` sends a strayed
    /// member — earns no counter, because the goal-reach verdict is gated on
    /// the decision category, not the arrival coordinates.
    #[test]
    fn a_monster_arrival_inside_its_area_earns_no_counter() {
        let spawn = Rect {
            left: 4,
            top: 4,
            right: 8,
            bottom: 8,
        };
        let monster = record(
            k1_game::rules::group::FIRST_ROAMING,
            k1_game::rules::class::WARRIOR,
            9,
            spawn,
            None,
        );
        // (6, 6) sits strictly inside the interior, the band a walk order — or
        // a monster's return — picks from; without the villager gate it would
        // have counted as a walk.
        assert_eq!(
            goal_reach_field(&monster, &[], (6, 6)),
            None,
            "a roaming monster's return home is not a confirmed walk"
        );
    }

    /// A villager holding a slot that arrives on the slot's place cell earns
    /// the workplace counter (the dynamic branch of `villager_decision`).
    #[test]
    fn a_villager_on_its_place_cell_counts_a_workplace() {
        let spawn = Rect {
            left: 4,
            top: 4,
            right: 8,
            bottom: 8,
        };
        let place = Workplace {
            cell: (6, 6),
            direction: 0,
            role: "elder",
            character: 9,
        };
        let holder = record(
            k1_game::rules::group::VILLAGE_FIRST,
            k1_game::rules::class::VILLAGER,
            9,
            spawn,
            Some(place),
        );
        assert_eq!(
            goal_reach_field(&holder, &[], (6, 6)),
            Some(GoalField::Workplace),
            "arrival on the held place cell is a workplace, before any walk"
        );
    }

    /// A jobless villager arriving on a cell inside its spawn area earns the
    /// walk counter (the `walk_target` band of `villager_decision`).
    #[test]
    fn a_jobless_villager_inside_its_area_counts_a_walk() {
        let spawn = Rect {
            left: 4,
            top: 4,
            right: 8,
            bottom: 8,
        };
        let jobless = record(
            k1_game::rules::group::VILLAGE_FIRST,
            k1_game::rules::class::VILLAGER,
            9,
            spawn,
            None,
        );
        assert_eq!(
            goal_reach_field(&jobless, &[], (6, 6)),
            Some(GoalField::Walk),
            "a jobless villager landing inside the interior is a walk"
        );
    }

    // ── game time derived from the simulation tick ──

    use k1_game::time_of_day::{self, Ambient, DayPhase, GameTime};

    /// The seed the `simulation` helper uses, and another one, to show the time of
    /// day does not depend on which run produced the tick.
    const RUN_SEED: u64 = 7;
    const OTHER_SEED: u64 = 99;

    /// Stepping one full cycle of the real simulation reproduces the interval trace
    /// `docs/research/k1/time.md` records: the ground window opens at 8100 and closes at
    /// 18900, and each ramp leaves the value it comes from on the tick whose integer
    /// half first turns non-zero — warm at 1802, neutral at 3593, dark at 5402 —
    /// while window 13's ramp reaches `(0, 0, 0)` at 19020.
    #[test]
    fn full_cycle_reproduces_the_interval_trace() {
        let map = open_map(|_, _| false);
        let spawn = Rect {
            left: 4,
            top: 4,
            right: 8,
            bottom: 8,
        };
        let state = roaming_state(spawn, 1);
        let mut sim = simulation(&map, &state, 1);

        let mut boundaries: Vec<u64> = Vec::new();
        let mut prev = sim.game_time();
        for _ in 0..time_of_day::CYCLE_TICKS {
            sim.tick();
            let gt = sim.game_time();
            if gt.ambient != prev.ambient || gt.ground_window != prev.ground_window {
                boundaries.push(gt.cycle_tick);
            }
            prev = gt;
        }

        assert_eq!(boundaries, vec![1802, 3593, 5402, 8100, 18900, 19020]);
    }

    /// The accessor is the model, not a copy of it: what the running simulation
    /// reports equals what the pure function reports for the same tick, including
    /// the four-name reading of DayPhase, which is our convention and not a named
    /// property of the original.
    #[test]
    fn the_simulation_reports_exactly_the_model() {
        let map = open_map(|_, _| false);
        let spawn = Rect {
            left: 4,
            top: 4,
            right: 8,
            bottom: 8,
        };
        let state = roaming_state(spawn, 1);
        let mut sim = simulation(&map, &state, 1);

        for tick in 1..=600u64 {
            sim.tick();
            assert_eq!(sim.game_time(), time_of_day::game_time(tick), "at {tick}");
        }
        assert_eq!(sim.game_time().phase, DayPhase::Day);
    }

    /// Game time is a function of the simulation tick, so it cannot drift with the
    /// run: a different seed at the same tick reads the same time, and a simulation
    /// rebuilt and replayed to a tick reports the time the forward run reached there
    /// — the property the viewer's backward step rests on (#68).
    #[test]
    fn game_time_follows_the_tick_and_not_the_run() {
        let map = open_map(|_, _| false);
        let spawn = Rect {
            left: 4,
            top: 4,
            right: 8,
            bottom: 8,
        };
        let state = roaming_state(spawn, 1);
        const TICKS: u64 = 400;

        let run = |seed: u64| -> Vec<GameTime> {
            let mut sim = Simulation::new(
                &map,
                &state,
                MAP_NUMBER,
                None,
                vec![[2; DIRECTION_COUNT]; 1],
                Vec::new(),
                Vec::new(),
                seed,
                None,
            );
            (0..TICKS)
                .map(|_| {
                    sim.tick();
                    sim.game_time()
                })
                .collect()
        };

        assert_eq!(run(RUN_SEED), run(OTHER_SEED));

        let mut replayed = Simulation::new(
            &map,
            &state,
            MAP_NUMBER,
            None,
            vec![[2; DIRECTION_COUNT]; 1],
            Vec::new(),
            Vec::new(),
            RUN_SEED,
            None,
        );
        for _ in 0..TICKS {
            replayed.tick();
        }
        assert_eq!(replayed.game_time(), run(RUN_SEED)[(TICKS - 1) as usize]);
        assert_eq!(replayed.game_time(), time_of_day::game_time(TICKS));
    }

    /// Game time is recreated with the simulation, not remembered: a fresh
    /// Simulation reports tick 0, day 0, cycle_tick 0.
    #[test]
    fn game_time_never_drifts_and_resets_with_a_fresh_simulation() {
        let map = open_map(|_, _| false);
        let spawn = Rect {
            left: 4,
            top: 4,
            right: 8,
            bottom: 8,
        };
        let state = roaming_state(spawn, 1);

        let mut sim1 = simulation(&map, &state, 1);
        let mut sim2 = simulation(&map, &state, 1);
        for _ in 0..1000 {
            sim1.tick();
            sim2.tick();
        }
        assert_eq!(sim1.game_time(), sim2.game_time());

        let fresh = simulation(&map, &state, 1);
        let gt = fresh.game_time();
        assert_eq!(gt.day, 0);
        assert_eq!(gt.cycle_tick, 0);
        assert_eq!(gt.phase, DayPhase::Day);
        assert_eq!(gt.ambient, Ambient::Neutral);
    }
}
