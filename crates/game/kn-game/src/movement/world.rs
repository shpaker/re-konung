//! The engine's world: several creatures, the game tick, disputes over cells
//! and repathing.
//!
//! The one place that brings route search ([`super::route`]), walking
//! ([`super::walk`]), walkability ([`super::passage`]), occupancy
//! ([`super::occupancy`]) and step duration ([`super::pace`]) together. Game
//! rules — who goes where and why — are not here: epic #33 part 3 adds them.

use super::CreatureId;
use super::grid::{Cell, Direction, Grid};
use super::occupancy::{Holder, Occupancy};
use super::pace::Pace;
use super::passage::Passage;
use super::rng::Rng;
use super::route::{self, RouteRules};
use super::walk::{Step, WalkState, Walker};

/// World policy: numbers the engine chose itself because nothing is known
/// about them (docs/rust.md, "Decision: repathing without thrash" and
/// "Decision: who yields a cell").
#[derive(Debug, Clone, Copy)]
pub struct WorldRules {
    /// How many ticks a creature stands with a route it cannot start before
    /// that route is recomputed: the cell ahead may free itself in a tick or
    /// two, so running a search every tick is pointless.
    pub wait_before_repath_ticks: u32,
    /// Shortest gap between two searches for the same creature, in ticks: the
    /// guard against thrash when even the detour does not get through, and the
    /// window a repeated order to a goal that had no route is throttled to.
    pub repath_cooldown_ticks: u32,
    /// Route search rules.
    pub route: RouteRules,
}

impl Default for WorldRules {
    fn default() -> WorldRules {
        WorldRules {
            wait_before_repath_ticks: 12,
            repath_cooldown_ticks: 60,
            route: RouteRules::default(),
        }
    }
}

/// What happened to a creature on this tick.
///
/// Events are discrete transitions; [`World::state`] is what a sprite looks
/// like now — the viewer places it in a cell and picks an animation from that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// A step started: the creature walks from `from` to `to` for this many
    /// ticks.
    StepStarted {
        creature: CreatureId,
        from: Cell,
        to: Cell,
        direction: Direction,
        ticks: u32,
    },
    /// A step finished: the creature stands in `cell`.
    StepFinished {
        creature: CreatureId,
        cell: Cell,
        direction: Direction,
    },
    /// The route is spent and the creature stopped.
    Arrived { creature: CreatureId, cell: Cell },
    /// A route was assigned — the first one or a repath — with this many steps
    /// ahead.
    RouteSet { creature: CreatureId, steps: usize },
    /// The cell ahead is held by another creature.
    Waiting {
        creature: CreatureId,
        cell: Cell,
        blocked_by: CreatureId,
    },
    /// The cell ahead — or the cell whose corner a step was crossing — became
    /// impassable.
    TerrainBlocked { creature: CreatureId, cell: Cell },
    /// No route to the goal: the creature dropped the order and stands still.
    NoRoute { creature: CreatureId, goal: Cell },
}

impl Event {
    /// Whose state change this event reports.
    fn creature(&self) -> CreatureId {
        match self {
            Event::StepStarted { creature, .. }
            | Event::StepFinished { creature, .. }
            | Event::Arrived { creature, .. }
            | Event::RouteSet { creature, .. }
            | Event::Waiting { creature, .. }
            | Event::TerrainBlocked { creature, .. }
            | Event::NoRoute { creature, .. } => *creature,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldError {
    UnknownCreature,
    OutsideGrid,
    CellTaken,
    /// The cell is impassable: nobody may stand in it, even if it is free.
    ImpassableCell,
    /// There is no route to the goal, so the creature was not sent anywhere.
    NoRoute,
}

/// What holds the cell ahead of a creature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Block {
    None,
    Terrain,
    Creature(CreatureId),
}

/// A creature in the world: its walking plus the order and the waiting.
#[derive(Debug, Clone)]
struct Actor {
    walker: Walker,
    /// The cell the creature was sent to; kept until it arrives.
    goal: Option<Cell>,
    /// The last goal an order named, kept across arrival so a tick loop that
    /// re-sends the same order is recognized as a no-op rather than a new
    /// search (docs/rust.md, "Decision: a repeated order is idempotent").
    last_commanded: Option<Cell>,
    /// The goal whose last route search found no route, kept only until that
    /// order is retried, replaced or withdrawn: it is what the repeat order is
    /// throttled against, and cannot be `last_commanded` — that one has to stay
    /// cleared so a retry after a crowd clears is not a silent no-op
    /// (docs/rust.md, "Decision: a failed goal is retried on a cooldown").
    failed_goal: Option<Cell>,
    /// A new order not yet turned into a route: it arrived while the creature
    /// was mid-step and will be executed at the cell border.
    pending: bool,
    /// Ticks the creature has stood with a route it cannot start.
    waiting_ticks: u32,
    /// Tick of the last route search.
    last_search: u64,
}

impl Actor {
    fn new(walker: Walker) -> Actor {
        Actor {
            walker,
            goal: None,
            last_commanded: None,
            failed_goal: None,
            pending: false,
            waiting_ticks: 0,
            last_search: 0,
        }
    }
}

/// The movement world: grid, walkability, occupancy and creatures on one tick.
pub struct World<P: Passage, K: Pace> {
    grid: Grid,
    passage: P,
    pace: K,
    rules: WorldRules,
    occupancy: Occupancy,
    /// One slot per number ever issued: a taken one is an actor, an empty one
    /// is a creature [`World::remove`] took out. The slot index is the
    /// creature's number, so a removal shifts nobody.
    actors: Vec<Option<Actor>>,
    /// Number for the next creature: it only ever grows, so a removed
    /// creature's number is never handed to another (docs/rust.md, "Decision:
    /// taking a creature out keeps the numbers").
    next_creature: u32,
    /// The taken slots in ascending order: the tick walks the creatures that
    /// are in the world rather than the whole slot history, in the same
    /// number order walking the slots would give.
    live: Vec<usize>,
    rng: Rng,
    tick: u64,
    /// Events from orders given between ticks: they come out first on the next
    /// [`World::tick_step`], in the order they were issued. `tick_step` is the
    /// only event channel, so an order cannot vanish silently.
    pending_events: Vec<Event>,
}

impl<P: Passage, K: Pace> World<P, K> {
    /// `seed` starts the generator that breaks ties between equal routes: the
    /// same seed with the same data gives the same events.
    pub fn new(grid: Grid, passage: P, pace: K, rules: WorldRules, seed: u64) -> World<P, K> {
        World {
            grid,
            passage,
            pace,
            rules,
            occupancy: Occupancy::new(),
            actors: Vec::new(),
            next_creature: 0,
            live: Vec::new(),
            rng: Rng::new(seed),
            tick: 0,
            pending_events: Vec::new(),
        }
    }

    pub fn grid(&self) -> Grid {
        self.grid
    }

    pub fn rules(&self) -> WorldRules {
        self.rules
    }

    pub fn rules_mut(&mut self) -> &mut WorldRules {
        &mut self.rules
    }

    pub fn passage(&self) -> &P {
        &self.passage
    }

    /// The walkability supplier can be swapped: a felled tree or a new grove
    /// flips a cell, and the engine repaths around it.
    pub fn passage_mut(&mut self) -> &mut P {
        &mut self.passage
    }

    pub fn pace(&self) -> &K {
        &self.pace
    }

    /// The tick number elapsed since the world was created.
    pub fn tick_number(&self) -> u64 {
        self.tick
    }

    /// Place a creature in a cell; it gets the next number in line. A number
    /// a removed creature held is never handed out again, so a number taken by
    /// [`World::add`] names this creature for as long as the world lives.
    ///
    /// An impassable cell gives `Err(ImpassableCell)`: a creature inside a wall
    /// would break the invariant "nobody stands in an impassable cell" that the
    /// world checks on every tick.
    pub fn add(&mut self, cell: Cell) -> Result<CreatureId, WorldError> {
        if !self.grid.contains(cell) {
            return Err(WorldError::OutsideGrid);
        }
        if !self.passage.is_walkable(cell) {
            return Err(WorldError::ImpassableCell);
        }
        if self.occupancy.holder(cell).is_some() {
            return Err(WorldError::CellTaken);
        }
        let creature = CreatureId::new(self.next_creature);
        self.next_creature += 1;
        self.occupancy.stand(cell, creature);
        self.actors
            .push(Some(Actor::new(Walker::new(creature, cell))));
        // Numbers only grow, so the appended slot keeps the list ascending.
        self.live.push(self.actors.len() - 1);
        Ok(creature)
    }

    fn index(&self, creature: CreatureId) -> Option<usize> {
        let index = creature.raw() as usize;
        self.actors
            .get(index)
            .filter(|slot| {
                slot.as_ref()
                    .is_some_and(|actor| actor.walker.creature == creature)
            })
            .map(|_| index)
    }

    /// The actor of an index [`World::index`] vouches for: every caller of a
    /// method below has just checked it, so the slot is taken.
    fn actor(&self, index: usize) -> &Actor {
        self.actors[index]
            .as_ref()
            .expect("the index was vouched for by World::index")
    }

    fn actor_mut(&mut self, index: usize) -> &mut Actor {
        self.actors[index]
            .as_mut()
            .expect("the index was vouched for by World::index")
    }

    /// Send a creature to a cell and return the route's step count.
    ///
    /// If the creature is already mid-step the order does not undo that step:
    /// the route is recomputed at the cell border, where a cell can still be
    /// given up, and `Ok(0)` is returned then. `Err(NoRoute)` — there is no
    /// route and the creature stays where it is.
    ///
    /// A repeated order to the goal the creature was last sent to is idempotent:
    /// it neither resets the waiting counter nor forces a new search, and
    /// returns the steps the current route still has (or `0` once arrived). A
    /// tick loop that re-sends the same order every tick must not disturb the
    /// repath timing (docs/rust.md, "Decision: a repeated order is idempotent").
    ///
    /// A repeated order to a goal whose last search found no route is refused
    /// the same way it would be by a real search — `Err(NoRoute)` — but without
    /// running one and without a new [`Event::NoRoute`], until
    /// [`WorldRules::repath_cooldown_ticks`] have passed since that failed
    /// search (docs/rust.md, "Decision: a failed goal is retried on a
    /// cooldown"). The refusal reaches the caller through the return value; the
    /// event channel carries only the search that actually ran.
    ///
    /// The search events (`RouteSet`/`NoRoute`) go into the world's shared
    /// channel and come out first on the next [`World::tick_step`] — outside a
    /// tick the engine emits no events, but it does not lose them either.
    pub fn move_to(&mut self, creature: CreatureId, goal: Cell) -> Result<usize, WorldError> {
        let index = self.index(creature).ok_or(WorldError::UnknownCreature)?;
        if !self.grid.contains(goal) {
            return Err(WorldError::OutsideGrid);
        }
        if self.actor(index).last_commanded == Some(goal) {
            return Ok(self.actor(index).walker.route_len());
        }
        let actor = self.actor(index);
        if actor.failed_goal == Some(goal)
            && self.tick - actor.last_search < u64::from(self.rules.repath_cooldown_ticks)
        {
            return Err(WorldError::NoRoute);
        }
        self.actor_mut(index).goal = Some(goal);
        self.actor_mut(index).pending = true;
        self.actor_mut(index).waiting_ticks = 0;
        if self.actor(index).walker.is_walking() {
            // Deferred to the cell border: remember the goal so a repeat during
            // the step is a no-op. If the deferred search then fails,
            // `search_route` clears this, so a later retry can re-search.
            self.actor_mut(index).last_commanded = Some(goal);
            return Ok(0);
        }
        let mut events = Vec::new();
        let steps = self.search_route(index, &mut events);
        self.pending_events.append(&mut events);
        steps.ok_or(WorldError::NoRoute)
    }

    /// Withdraw the order: the creature finishes the step it started and stops.
    ///
    /// Clears the remembered goals too, so a later order to the same cell is a
    /// real new order: neither an idempotent no-op nor a throttled retry of a
    /// search that failed before the withdrawal.
    pub fn stop(&mut self, creature: CreatureId) -> Result<(), WorldError> {
        let index = self.index(creature).ok_or(WorldError::UnknownCreature)?;
        let actor = self.actor_mut(index);
        actor.goal = None;
        actor.last_commanded = None;
        actor.failed_goal = None;
        actor.pending = false;
        actor.waiting_ticks = 0;
        actor.walker.clear_route();
        Ok(())
    }

    /// Take a creature out of the world: it frees every cell the creature
    /// stood on or reserved with a step, and its movement state is dropped
    /// (docs/rust.md, "Decision: taking a creature out keeps the numbers").
    ///
    /// No [`Event`] is emitted: removal is an order from outside, and the
    /// outer world is the one that knows a creature died or left the map —
    /// the engine only has to stop holding cells and stop walking.
    ///
    /// The caller does not have to `stop` first: removal ends a mid-step
    /// where it stands, reservation and all, and the cells it held become
    /// enterable on the same tick. Every other creature keeps its number,
    /// cell and route; an order to a removed creature — or removing it again
    /// — answers `Err(UnknownCreature)`. The creature's number is not handed
    /// to a later one, so an id the outer world still carries — a corpse, a
    /// roster entry — never comes to name another creature.
    ///
    /// A search event of this creature still waiting between ticks is dropped
    /// with it: from the removal on, the channel names no creature the world
    /// has let go. `move_to`'s return value was the answer to the order and
    /// stands; events of the other creatures keep their place in the channel.
    pub fn remove(&mut self, creature: CreatureId) -> Result<(), WorldError> {
        let index = self.index(creature).ok_or(WorldError::UnknownCreature)?;
        self.occupancy.release_all(creature);
        self.pending_events
            .retain(|event| event.creature() != creature);
        if let Ok(position) = self.live.binary_search(&index) {
            self.live.remove(position);
        } else {
            // A slot `index` vouched for is in `live` by the invariant `add`
            // and `remove` keep; reaching this is a broken world, not data.
            debug_assert!(false, "slot {index} holds an actor but is not live");
        }
        self.actors[index] = None;
        Ok(())
    }

    pub fn state(&self, creature: CreatureId) -> Option<WalkState> {
        let index = self.index(creature)?;
        Some(self.actor(index).walker.state())
    }

    /// Turn a creature in place: it faces `direction` while it stands.
    ///
    /// Turning is not a ticked action, so it emits no [`Event`]: the new
    /// facing is readable from [`World::state`]. A creature mid-step keeps its
    /// step direction until the cell border, and a step that starts later
    /// consumes the turn — after the arrival the creature faces where it came
    /// from, exactly as it does without a turn.
    pub fn turn(&mut self, creature: CreatureId, direction: Direction) -> Result<(), WorldError> {
        let index = self.index(creature).ok_or(WorldError::UnknownCreature)?;
        self.actor_mut(index).walker.turn(direction);
        Ok(())
    }

    /// Every living creature's state, in order of their numbers, so traversal
    /// is deterministic. Removed creatures are not there.
    pub fn states(&self) -> Vec<WalkState> {
        self.live
            .iter()
            .map(|index| self.actor(*index).walker.state())
            .collect()
    }

    pub fn cell_of(&self, creature: CreatureId) -> Option<Cell> {
        let index = self.index(creature)?;
        Some(self.actor(index).walker.cell())
    }

    pub fn creature_at(&self, cell: Cell) -> Option<CreatureId> {
        self.occupancy.occupant(cell)
    }

    /// Who holds the cell: standing, or by a step reservation.
    pub fn holder(&self, cell: Cell) -> Option<Holder> {
        self.occupancy.holder(cell)
    }

    pub fn occupancy(&self) -> &Occupancy {
        &self.occupancy
    }

    /// One game tick: first the events of orders given between ticks, then
    /// arrivals, then repaths and new steps.
    ///
    /// This is the engine's only event channel: everything that happened —
    /// including the search an order asked for — comes out of here, in tick
    /// order. No event names a creature the world has let go: a search event
    /// still waiting in the channel when [`World::remove`] runs is dropped
    /// with it.
    ///
    /// Arrivals come first, so a creature that left a cell on this tick does not
    /// block the one entering that same cell on this tick. A step `d` ticks
    /// long started on tick `t` ends on tick `t + d`: exactly `d` ticks are
    /// spent walking, with no pause between steps.
    pub fn tick_step(&mut self) -> Vec<Event> {
        self.tick += 1;
        let mut events = std::mem::take(&mut self.pending_events);
        self.finish_steps(&mut events);
        self.start_steps(&mut events);
        events
    }

    /// Finish the steps that were in progress.
    fn finish_steps(&mut self, events: &mut Vec<Event>) {
        // `live` only changes between ticks, so walking it by position while
        // the actors change is safe; it is ascending, so the order is the
        // slot order.
        for position in 0..self.live.len() {
            let index = self.live[position];
            let creature = self.actor(index).walker.creature;
            let previous = self.actor(index).walker.cell();
            if let Some(step) = self.actor(index).walker.step()
                && step.elapsed >= step.ticks
                && !self.can_stand(step.to)
            {
                self.cancel_step(index, step, events);
                continue;
            }
            if !self.actor_mut(index).walker.tick() {
                continue;
            }
            let cell = self.actor(index).walker.cell();
            self.occupancy.release(previous, creature);
            // This creature held the cell, so taking it always succeeds; the
            // invariant is checked in debug and costs nothing in release.
            let held = self.occupancy.stand(cell, creature);
            debug_assert!(held, "on arrival cell {cell:?} is held by another creature");
            let direction = Direction::between(previous, cell);
            if let Some(direction) = direction {
                events.push(Event::StepFinished {
                    creature,
                    cell,
                    direction,
                });
            }
        }
    }

    /// Cancel a step whose destination became impassable: the creature stays
    /// where it was and waits for a repath.
    fn cancel_step(&mut self, index: usize, step: Step, events: &mut Vec<Event>) {
        let creature = self.actor(index).walker.creature;
        self.actor_mut(index).walker.cancel_step();
        self.occupancy.release(step.to, creature);
        self.actor_mut(index).waiting_ticks = self.rules.wait_before_repath_ticks;
        events.push(Event::TerrainBlocked {
            creature,
            cell: step.to,
        });
    }

    /// Start steps where a creature is waiting, carry out new orders and
    /// recompute blocked routes.
    ///
    /// Creatures are visited by their numbers: that is deterministic, and it
    /// settles a dispute over a cell because a reservation is exclusive
    /// (docs/rust.md, "Decision: who yields a cell").
    fn start_steps(&mut self, events: &mut Vec<Event>) {
        for position in 0..self.live.len() {
            let index = self.live[position];
            if self.actor(index).walker.is_walking() {
                continue;
            }
            let creature = self.actor(index).walker.creature;
            if self.actor(index).pending {
                // Deliberately no `continue`: a route found on this tick is
                // immediately spent by its first step, or an empty tick would
                // be inserted between arrival and the new step (60 ms of idling
                // on every change of goal).
                self.search_route(index, events);
            }
            if self.actor_mut(index).walker.finish() {
                self.actor_mut(index).goal = None;
                let cell = self.actor(index).walker.cell();
                events.push(Event::Arrived { creature, cell });
                continue;
            }
            if !self.actor(index).walker.is_waiting() {
                self.actor_mut(index).waiting_ticks = 0;
                continue;
            }
            let from = self.actor(index).walker.cell();
            let Some(next) = self.actor(index).walker.next_cell() else {
                continue;
            };
            match self.block_reason(creature, from, next) {
                Block::None => self.begin_step(index, next, events),
                Block::Creature(other) => {
                    events.push(Event::Waiting {
                        creature,
                        cell: next,
                        blocked_by: other,
                    });
                    self.actor_mut(index).waiting_ticks += 1;
                    self.repath_if_due(index, events);
                }
                Block::Terrain => {
                    events.push(Event::TerrainBlocked {
                        creature,
                        cell: next,
                    });
                    self.actor_mut(index).waiting_ticks = self.rules.wait_before_repath_ticks;
                    self.repath_if_due(index, events);
                }
            }
        }
    }

    /// Recompute the route when it is due: the creature has stood for
    /// [`WorldRules::wait_before_repath_ticks`] and at least
    /// [`WorldRules::repath_cooldown_ticks`] have passed since the last search.
    fn repath_if_due(&mut self, index: usize, events: &mut Vec<Event>) {
        let rules = self.rules;
        let actor = self.actor(index);
        let due = actor.waiting_ticks >= rules.wait_before_repath_ticks
            && self.tick - actor.last_search >= u64::from(rules.repath_cooldown_ticks);
        if !due || actor.goal.is_none() {
            return;
        }
        self.search_route(index, events);
    }

    /// Route search from the creature's current cell to its goal; returns the
    /// number of steps.
    fn search_route(&mut self, index: usize, events: &mut Vec<Event>) -> Option<usize> {
        let actor = self.actor(index);
        let (Some(goal), creature, from) = (actor.goal, actor.walker.creature, actor.walker.cell())
        else {
            return None;
        };
        self.actor_mut(index).last_search = self.tick;
        self.actor_mut(index).waiting_ticks = 0;
        let request = route::RouteRequest {
            grid: &self.grid,
            passage: &self.passage,
            occupancy: &self.occupancy,
            rules: &self.rules.route,
            mover: creature,
            from,
            goal,
        };
        let found = route::find_route(&request, &mut self.rng);
        match found {
            Some(route) => {
                let steps = route.len();
                let actor = self.actor_mut(index);
                actor.pending = false;
                actor.last_commanded = Some(goal);
                actor.failed_goal = None;
                actor.walker.set_route(&route);
                events.push(Event::RouteSet { creature, steps });
                Some(steps)
            }
            None => {
                let actor = self.actor_mut(index);
                actor.pending = false;
                actor.goal = None;
                // Forget the goal so a later order to it re-searches: re-sending
                // an unreachable goal is the only retry once a crowd clears, and
                // a stale memory would make that retry a silent no-op. The
                // repeat is throttled by `failed_goal` instead, so the retry
                // costs one search per cooldown and not one per tick.
                actor.last_commanded = None;
                actor.failed_goal = Some(goal);
                actor.walker.clear_route();
                events.push(Event::NoRoute { creature, goal });
                None
            }
        }
    }

    /// Start the next step: reserve the cell and set the tick counter running.
    fn begin_step(&mut self, index: usize, next: Cell, events: &mut Vec<Event>) {
        let creature = self.actor(index).walker.creature;
        let from = self.actor(index).walker.cell();
        let Some(direction) = Direction::between(from, next) else {
            self.actor_mut(index).waiting_ticks = self.rules.wait_before_repath_ticks;
            self.repath_if_due(index, events);
            return;
        };
        if !self.occupancy.reserve(next, creature) {
            return;
        }
        let ticks = self.pace.step_ticks(creature, direction);
        if self.actor_mut(index).walker.start_step(ticks).is_none() {
            self.occupancy.release(next, creature);
            return;
        }
        self.actor_mut(index).waiting_ticks = 0;
        events.push(Event::StepStarted {
            creature,
            from,
            to: next,
            direction,
            ticks: ticks.max(1),
        });
    }

    /// Whether one may stand here: the cell is inside the grid and walkable.
    fn can_stand(&self, cell: Cell) -> bool {
        self.grid.contains(cell) && self.passage.is_walkable(cell)
    }

    /// What holds the cell ahead. A step through a corner counts as blocked
    /// when its flank is closed.
    fn block_reason(&self, creature: CreatureId, from: Cell, next: Cell) -> Block {
        if !self.can_stand(next) {
            return Block::Terrain;
        }
        if let Some(holder) = self.occupancy.holder(next)
            && holder.creature != creature
        {
            return Block::Creature(holder.creature);
        }
        match Direction::between(from, next) {
            Some(direction)
                if route::is_open_corner(&self.grid, &self.passage, from, direction) =>
            {
                Block::None
            }
            _ => Block::Terrain,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::movement::route::Route;

    #[derive(Default, Clone)]
    struct Map {
        blocked: HashSet<Cell>,
    }

    impl Passage for Map {
        fn is_walkable(&self, cell: Cell) -> bool {
            !self.blocked.contains(&cell)
        }
    }

    #[derive(Clone, Copy)]
    struct Uniform(u32);

    impl Pace for Uniform {
        fn step_ticks(&self, _creature: CreatureId, _direction: Direction) -> u32 {
            self.0
        }
    }

    fn world(columns: u32, rows: u32) -> World<Map, Uniform> {
        World::new(
            Grid::new(columns, rows),
            Map::default(),
            Uniform(2),
            WorldRules::default(),
            0,
        )
    }

    #[test]
    fn creature_is_placed_and_taken_cells_are_refused() {
        let mut world = world(8, 8);
        let cell = Cell::new(2, 2);
        let first = world.add(cell).unwrap();
        assert_eq!(world.creature_at(cell), Some(first));
        assert_eq!(world.add(cell), Err(WorldError::CellTaken));
        assert_eq!(world.add(Cell::new(8, 0)), Err(WorldError::OutsideGrid));
    }

    #[test]
    fn add_refuses_an_impassable_cell() {
        // A creature inside a wall would break the invariant "nobody stands in
        // an impassable cell" that the world checks every tick
        let mut world = world(8, 8);
        let wall = Cell::new(3, 3);
        world.passage_mut().blocked.insert(wall);
        assert_eq!(world.add(wall), Err(WorldError::ImpassableCell));
        assert_eq!(world.occupancy().holder(wall), None);
        assert!(world.add(Cell::new(4, 4)).is_ok());
    }

    #[test]
    fn order_events_reach_the_consumer_through_the_tick() {
        // tick_step is the only event channel: an order issued outside a tick
        // must not be lost along with a temporary vector
        let mut world = world(8, 8);
        let creature = world.add(Cell::new(1, 2)).unwrap();
        assert_eq!(world.move_to(creature, Cell::new(5, 2)), Ok(4));
        let events = world.tick_step();
        assert!(
            events.contains(&Event::RouteSet { creature, steps: 4 }),
            "{events:?}"
        );
        assert!(
            events.contains(&Event::StepStarted {
                creature,
                from: Cell::new(1, 2),
                to: Cell::new(2, 2),
                direction: Direction::East,
                ticks: 2,
            }),
            "{events:?}"
        );
        // and the second tick brings no repeated search event
        assert!(
            !world
                .tick_step()
                .iter()
                .any(|event| matches!(event, Event::RouteSet { .. }))
        );
    }

    #[test]
    fn a_deferred_order_starts_its_first_step_on_the_border_tick() {
        let mut world = world(8, 8);
        let creature = world.add(Cell::new(1, 2)).unwrap();
        assert_eq!(world.move_to(creature, Cell::new(3, 2)), Ok(2));
        assert!(
            world
                .tick_step()
                .iter()
                .any(|event| matches!(event, Event::StepStarted { .. }))
        );
        // the order arrives while the creature is mid-step: it waits for the cell border
        assert_eq!(world.move_to(creature, Cell::new(6, 2)), Ok(0));
        world.tick_step();
        let events = world.tick_step();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::StepFinished { .. })),
            "{events:?}"
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::RouteSet { .. })),
            "{events:?}"
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::StepStarted { .. })),
            "the first step after a repath must start on the same tick: {events:?}"
        );
    }

    #[test]
    fn a_free_but_unreachable_goal_is_not_replaced_by_a_neighbour() {
        let mut world = world(8, 8);
        let creature = world.add(Cell::new(0, 0)).unwrap();
        let goal = Cell::new(5, 5);
        // The goal is free and walkable but cut off: all eight neighbours are wall
        let neighbours: Vec<Cell> = Direction::ALL
            .into_iter()
            .filter_map(|direction| goal.neighbour(direction))
            .collect();
        for cell in &neighbours {
            world.passage_mut().blocked.insert(*cell);
        }
        assert!(world.passage().is_walkable(goal), "the goal must stay free");
        assert_eq!(world.move_to(creature, goal), Err(WorldError::NoRoute));
        let events = world.tick_step();
        assert!(
            events.contains(&Event::NoRoute { creature, goal }),
            "there are reachable cells beside the goal, but the order named the goal itself: {events:?}"
        );
        assert_eq!(world.cell_of(creature), Some(Cell::new(0, 0)));
    }

    #[test]
    fn move_to_reports_the_route_length() {
        let mut world = world(8, 8);
        let creature = world.add(Cell::new(1, 2)).unwrap();
        assert_eq!(world.move_to(creature, Cell::new(5, 2)), Ok(4));
        assert_eq!(world.state(creature).unwrap().goal, Some(Cell::new(5, 2)));
        assert_eq!(
            world.move_to(CreatureId::new(9), Cell::new(1, 1)),
            Err(WorldError::UnknownCreature)
        );
        assert_eq!(
            world.move_to(creature, Cell::new(9, 9)),
            Err(WorldError::OutsideGrid)
        );
    }

    /// Block every in-grid neighbour of `goal` so nothing can reach it, and
    /// return the cells so the test can clear them again.
    fn seal(world: &mut World<Map, Uniform>, goal: Cell) -> Vec<Cell> {
        let sealed: Vec<Cell> = Direction::ALL
            .into_iter()
            .filter_map(|direction| goal.neighbour(direction))
            .filter(|cell| world.grid().contains(*cell))
            .collect();
        for cell in &sealed {
            world.passage_mut().blocked.insert(*cell);
        }
        sealed
    }

    fn unseal(world: &mut World<Map, Uniform>, sealed: &[Cell]) {
        for cell in sealed {
            world.passage_mut().blocked.remove(cell);
        }
    }

    /// A deferred order whose search finds no route must not be remembered: the
    /// creature has to retry the same goal once the path clears. Regression for
    /// the `last_commanded` hang where a deferred NoRoute left the goal sticky.
    ///
    /// The retry is allowed only once the failed-goal window has passed (#54),
    /// so the test waits out `repath_cooldown_ticks` before re-sending.
    #[test]
    fn deferred_no_route_can_be_retried_after_the_path_clears() {
        let mut world = world(8, 8);
        let creature = world.add(Cell::new(1, 2)).unwrap();
        // Start a real step so the next order is carried out at the cell border.
        world.move_to(creature, Cell::new(3, 2)).unwrap();
        world.tick_step();
        assert!(
            world.state(creature).unwrap().moving,
            "the creature should be mid-step"
        );

        // Sealed new goal: the deferred search cannot reach it.
        let goal = Cell::new(6, 2);
        let sealed = seal(&mut world, goal);
        // Change the goal while mid-step — this is the deferred path.
        assert_eq!(world.move_to(creature, goal), Ok(0));

        // Run until the deferred search for the sealed goal reports NoRoute.
        let mut saw_no_route = false;
        for _ in 0..10 {
            let events = world.tick_step();
            if events
                .iter()
                .any(|event| matches!(event, Event::NoRoute { goal: g, .. } if *g == goal))
            {
                saw_no_route = true;
                break;
            }
        }
        assert!(
            saw_no_route,
            "the deferred order to the sealed goal must report NoRoute"
        );

        // Free the path, but re-send inside the window: the search is throttled,
        // so the goal is still refused rather than routed (#54).
        unseal(&mut world, &sealed);
        assert_eq!(
            world.move_to(creature, goal),
            Err(WorldError::NoRoute),
            "a retry inside the failed-goal window must not search"
        );

        // Wait the window out, then re-send: the goal must be searched again.
        let cooldown = world.rules().repath_cooldown_ticks;
        for _ in 0..=cooldown {
            world.tick_step();
        }
        assert!(
            matches!(world.move_to(creature, goal), Ok(steps) if steps > 0),
            "a retry after the window to a now-reachable goal must build a route, not be ignored"
        );
    }

    /// A rules-style loop that re-sends the same unreachable order every tick
    /// must pay for the worst search the engine has once per cooldown window,
    /// not once per tick (#54).
    #[test]
    fn repeated_unreachable_order_is_throttled_to_the_cooldown() {
        let mut world = world(12, 12);
        let creature = world.add(Cell::new(1, 6)).unwrap();
        let goal = Cell::new(10, 6);
        for row in 0..12 {
            world.passage_mut().blocked.insert(Cell::new(6, row));
        }
        world.rules_mut().route.goal_fallback_radius = 0;
        let cooldown = world.rules().repath_cooldown_ticks;
        let ticks = 4 * cooldown;

        let mut refusals = 0;
        for _ in 0..ticks {
            assert_eq!(world.move_to(creature, goal), Err(WorldError::NoRoute));
            refusals += world
                .tick_step()
                .iter()
                .filter(|event| matches!(event, Event::NoRoute { .. }))
                .count();
        }
        assert_eq!(
            refusals,
            usize::try_from(ticks / cooldown).unwrap(),
            "{refusals} searches in {ticks} ticks: expected one per cooldown window"
        );
    }

    /// The window delays a retry, it does not cancel it: a loop that keeps
    /// re-sending the same order gets a real route once the goal is reachable
    /// again and the window has passed. The engine is not told when walkability
    /// changes, so "as soon as the path clears" is not something it can offer.
    #[test]
    fn a_repeated_unreachable_order_gets_a_route_once_the_window_passes() {
        let mut world = world(8, 8);
        let creature = world.add(Cell::new(1, 2)).unwrap();
        let goal = Cell::new(6, 2);
        let sealed = seal(&mut world, goal);
        world.rules_mut().route.goal_fallback_radius = 0;
        let cooldown = world.rules().repath_cooldown_ticks;

        assert_eq!(world.move_to(creature, goal), Err(WorldError::NoRoute));
        world.tick_step();

        // The path clears, but the failed search is still inside its window.
        unseal(&mut world, &sealed);
        assert_eq!(
            world.move_to(creature, goal),
            Err(WorldError::NoRoute),
            "a repeat inside the window must not search"
        );
        assert_eq!(world.tick_step(), Vec::new(), "the repeat ran a search");

        // Once the window passes the same repeated order builds a route, and the
        // creature walks it to the goal: the order never sticks.
        for _ in 0..cooldown {
            world.tick_step();
        }
        assert!(
            matches!(world.move_to(creature, goal), Ok(steps) if steps > 0),
            "a repeat after the window must search again"
        );
        for _ in 0..30 {
            world.tick_step();
        }
        assert_eq!(
            world.cell_of(creature),
            Some(goal),
            "the creature must reach the goal the loop kept ordering"
        );
    }

    /// Inside the window a repeat is refused by the return value only: no
    /// search runs, so the event channel stays silent and the creature is
    /// untouched.
    #[test]
    fn a_retry_inside_the_window_neither_searches_nor_reports_a_refusal() {
        let mut world = world(12, 12);
        let creature = world.add(Cell::new(1, 6)).unwrap();
        let goal = Cell::new(10, 6);
        for row in 0..12 {
            world.passage_mut().blocked.insert(Cell::new(6, row));
        }
        world.rules_mut().route.goal_fallback_radius = 0;

        assert_eq!(world.move_to(creature, goal), Err(WorldError::NoRoute));
        let cooldown = world.rules().repath_cooldown_ticks;
        // the search that ran surfaces as exactly one refusal
        assert_eq!(world.tick_step(), vec![Event::NoRoute { creature, goal }]);
        for _ in 1..cooldown {
            assert_eq!(world.move_to(creature, goal), Err(WorldError::NoRoute));
            assert_eq!(
                world.tick_step(),
                Vec::new(),
                "a throttled repeat must not report a new refusal"
            );
        }
        assert_eq!(world.cell_of(creature), Some(Cell::new(1, 6)));
        assert_eq!(world.states()[0].goal, None, "no goal was taken");
    }

    /// The window belongs to the goal that failed, not to the creature: an
    /// order to another cell is a new order and searches at once.
    #[test]
    fn a_failed_goal_does_not_hold_up_an_order_to_another_one() {
        let mut world = world(12, 12);
        let creature = world.add(Cell::new(1, 6)).unwrap();
        let unreachable = Cell::new(10, 6);
        for row in 0..12 {
            world.passage_mut().blocked.insert(Cell::new(6, row));
        }
        world.rules_mut().route.goal_fallback_radius = 0;
        assert_eq!(
            world.move_to(creature, unreachable),
            Err(WorldError::NoRoute)
        );
        world.tick_step();
        assert_eq!(world.move_to(creature, Cell::new(4, 6)), Ok(3));
    }

    /// Pins the bound of the window rather than asking to widen it: the memory
    /// holds one goal, so a loop that alternates between two unreachable orders
    /// searches on every switch, inside the window. Keeping the cadence sane is
    /// the rules layer's job (#36 part 3), not the engine's.
    #[test]
    fn alternating_unreachable_orders_each_search_inside_the_window() {
        let mut world = world(12, 12);
        let creature = world.add(Cell::new(1, 6)).unwrap();
        let first = Cell::new(10, 6);
        let second = Cell::new(10, 7);
        for row in 0..12 {
            world.passage_mut().blocked.insert(Cell::new(6, row));
        }
        world.rules_mut().route.goal_fallback_radius = 0;
        let cooldown = world.rules().repath_cooldown_ticks;

        let mut refusals = 0;
        for tick in 0..cooldown {
            let goal = if tick % 2 == 0 { first } else { second };
            assert_eq!(world.move_to(creature, goal), Err(WorldError::NoRoute));
            refusals += world
                .tick_step()
                .iter()
                .filter(|event| matches!(event, Event::NoRoute { .. }))
                .count();
        }
        assert!(
            refusals > usize::try_from(cooldown / 2).unwrap(),
            "{refusals} searches inside one window: a switching loop is not throttled"
        );
    }

    /// Withdrawing the order drops the failed goal too, so the very next order
    /// to it is a real search rather than a throttled refusal.
    #[test]
    fn stop_clears_a_failed_goal_so_the_next_order_searches() {
        let mut world = world(12, 12);
        let creature = world.add(Cell::new(1, 6)).unwrap();
        let goal = Cell::new(10, 6);
        for row in 0..12 {
            world.passage_mut().blocked.insert(Cell::new(6, row));
        }
        world.rules_mut().route.goal_fallback_radius = 0;

        assert_eq!(world.move_to(creature, goal), Err(WorldError::NoRoute));
        // the search that ran surfaces as one refusal
        assert_eq!(world.tick_step(), vec![Event::NoRoute { creature, goal }]);
        // still inside the window: a repeat is refused without a search
        assert_eq!(world.move_to(creature, goal), Err(WorldError::NoRoute));
        assert_eq!(world.tick_step(), Vec::new(), "the repeat ran a search");

        world.stop(creature).unwrap();
        assert_eq!(world.move_to(creature, goal), Err(WorldError::NoRoute));
        assert_eq!(
            world.tick_step(),
            vec![Event::NoRoute { creature, goal }],
            "after a withdrawal the order has to search again"
        );
    }

    /// A 2-tick step started on tick 1 lands the creature on tick 3; the next
    /// step begins on that same tick.
    #[test]
    fn step_finishes_on_the_expected_tick_and_creature_stops() {
        let mut world = world(8, 8);
        let from = Cell::new(1, 2);
        let creature = world.add(from).unwrap();
        world.move_to(creature, Cell::new(3, 2)).unwrap();

        let events = world.tick_step();
        assert_eq!(
            events,
            vec![
                // the order was issued outside a tick: its event comes first on this tick
                Event::RouteSet { creature, steps: 2 },
                Event::StepStarted {
                    creature,
                    from,
                    to: Cell::new(2, 2),
                    direction: Direction::East,
                    ticks: 2,
                },
            ]
        );
        assert_eq!(
            world.tick_step(),
            Vec::new(),
            "the middle of a step is silent"
        );
        assert_eq!(world.cell_of(creature), Some(from));

        let events = world.tick_step();
        assert!(events.contains(&Event::StepFinished {
            creature,
            cell: Cell::new(2, 2),
            direction: Direction::East,
        }));
        assert!(events.contains(&Event::StepStarted {
            creature,
            from: Cell::new(2, 2),
            to: Cell::new(3, 2),
            direction: Direction::East,
            ticks: 2,
        }));
        assert_eq!(world.cell_of(creature), Some(Cell::new(2, 2)));

        world.tick_step();
        let events = world.tick_step();
        assert_eq!(world.cell_of(creature), Some(Cell::new(3, 2)));
        assert!(
            events.contains(&Event::Arrived {
                creature,
                cell: Cell::new(3, 2)
            }),
            "{events:?}"
        );
        assert_eq!(
            world.tick_step(),
            Vec::new(),
            "a stopped creature makes noise"
        );
        assert!(!world.states()[0].moving);
    }

    #[test]
    fn reservation_is_dropped_when_the_target_becomes_impassable() {
        let mut world = world(8, 8);
        let creature = world.add(Cell::new(2, 2)).unwrap();
        world.move_to(creature, Cell::new(5, 2)).unwrap();
        world.tick_step();
        assert_eq!(
            world.holder(Cell::new(3, 2)).map(|holder| holder.creature),
            Some(creature)
        );
        world.passage_mut().blocked.insert(Cell::new(3, 2));
        world.tick_step();
        let events = world.tick_step();
        assert!(
            events.contains(&Event::TerrainBlocked {
                creature,
                cell: Cell::new(3, 2)
            }),
            "{events:?}"
        );
        assert_eq!(world.cell_of(creature), Some(Cell::new(2, 2)));
        assert_eq!(
            world.holder(Cell::new(3, 2)),
            None,
            "the reservation was not dropped"
        );
        assert_eq!(world.creature_at(Cell::new(2, 2)), Some(creature));
    }

    #[test]
    fn order_issued_mid_step_is_taken_at_the_border() {
        let mut world = world(12, 12);
        let from = Cell::new(2, 2);
        let creature = world.add(from).unwrap();
        world.move_to(creature, Cell::new(10, 2)).unwrap();
        world.tick_step();
        let other = Cell::new(2, 8);
        // A new order mid-step does not undo the step already started.
        assert_eq!(world.move_to(creature, other), Ok(0));
        world.tick_step();
        assert_eq!(world.cell_of(creature), Some(from));
        let events = world.tick_step();
        assert_eq!(world.cell_of(creature), Some(Cell::new(3, 2)));
        assert!(
            events.iter().any(
                |event| matches!(event, Event::RouteSet { creature: c, .. } if *c == creature)
            ),
            "{events:?}"
        );
        assert_eq!(world.state(creature).unwrap().goal, Some(other));
    }

    #[test]
    fn stop_keeps_the_current_step_but_drops_the_route() {
        let mut world = world(8, 8);
        let creature = world.add(Cell::new(1, 1)).unwrap();
        world.move_to(creature, Cell::new(6, 1)).unwrap();
        world.tick_step();
        world.stop(creature).unwrap();
        // The started step runs out: two ticks of a 2-tick step.
        world.tick_step();
        world.tick_step();
        assert_eq!(
            world.cell_of(creature),
            Some(Cell::new(2, 1)),
            "the started step was cancelled"
        );
        for _ in 0..8 {
            world.tick_step();
        }
        assert_eq!(world.cell_of(creature), Some(Cell::new(2, 1)));
        assert_eq!(world.states()[0].goal, None);
    }

    #[test]
    fn goal_reached_without_a_step_arrives_immediately() {
        let mut world = world(8, 8);
        let cell = Cell::new(3, 3);
        let creature = world.add(cell).unwrap();
        assert_eq!(world.move_to(creature, cell), Ok(0));
        let events = world.tick_step();
        assert!(
            events.contains(&Event::Arrived { creature, cell }),
            "{events:?}"
        );
        assert_eq!(world.cell_of(creature), Some(cell));
    }

    #[test]
    fn unreachable_order_is_refused() {
        let mut world = world(12, 12);
        let creature = world.add(Cell::new(1, 6)).unwrap();
        let goal = Cell::new(10, 6);
        for row in 0..12 {
            world.passage_mut().blocked.insert(Cell::new(6, row));
        }
        world.rules_mut().route.goal_fallback_radius = 0;
        assert_eq!(world.move_to(creature, goal), Err(WorldError::NoRoute));
        assert_eq!(world.cell_of(creature), Some(Cell::new(1, 6)));
        assert_eq!(world.states()[0].goal, None, "dropped the unreachable goal");
        // the refusal reaches the consumer too: the tick is the only event channel
        assert_eq!(world.tick_step(), vec![Event::NoRoute { creature, goal }]);
    }

    #[test]
    fn route_can_be_built_without_the_world() {
        let grid = Grid::new(8, 8);
        let map = Map::default();
        let occupancy = Occupancy::new();
        let direct =
            Route::from_cells(&[Cell::new(1, 1), Cell::new(2, 1)], Cell::new(2, 1)).unwrap();
        assert_eq!(direct.len(), 1);
        let request = route::RouteRequest {
            grid: &grid,
            passage: &map,
            occupancy: &occupancy,
            rules: &RouteRules::default(),
            mover: CreatureId::new(0),
            from: Cell::new(1, 1),
            goal: Cell::new(4, 4),
        };
        assert_eq!(
            route::find_route(&request, &mut Rng::new(0)).map(|found| found.len()),
            Some(5)
        );
    }

    /// A standing creature's facing is what `turn` set; mid-step the step wins
    /// until the cell border, and the walk consumes the turn — the arrival
    /// faces its own direction, and a later turn takes effect again (issue #36
    /// criterion 3 turns villagers at their workplace through this call).
    #[test]
    fn turn_faces_a_standing_creature_and_walking_consumes_it() {
        let mut world = world(8, 8);
        let creature = world.add(Cell::new(2, 2)).unwrap();
        assert_eq!(world.state(creature).unwrap().direction, None);
        world.turn(creature, Direction::NorthWest).unwrap();
        assert_eq!(
            world.state(creature).unwrap().direction,
            Some(Direction::NorthWest)
        );
        // Sent somewhere: the step's direction overrides the face while walking.
        world.move_to(creature, Cell::new(4, 2)).unwrap();
        world.tick_step();
        assert!(world.state(creature).unwrap().moving);
        assert_eq!(
            world.state(creature).unwrap().direction,
            Some(Direction::East),
            "a mid-step creature faces its step"
        );
        for _ in 0..4 {
            world.tick_step();
        }
        assert!(
            !world.state(creature).unwrap().moving,
            "the creature must have arrived"
        );
        assert_eq!(
            world.state(creature).unwrap().direction,
            Some(Direction::East),
            "the walk consumes the turn: the arrival faces where it came from"
        );
        // Turned again once it stands — the workplace flow of criterion 3.
        world.turn(creature, Direction::South).unwrap();
        assert_eq!(
            world.state(creature).unwrap().direction,
            Some(Direction::South)
        );
    }

    #[test]
    fn turn_reports_unknown_creature() {
        let mut world = world(8, 8);
        assert_eq!(
            world.turn(CreatureId::new(99), Direction::East),
            Err(WorldError::UnknownCreature)
        );
    }

    /// The reproduction the review asked for: `add → move_to → remove →
    /// tick_step` must not deliver the removed creature's still-pending
    /// search event, while the other creature keeps its own events in order.
    #[test]
    fn removal_drops_a_pending_search_event_of_the_removed_creature() {
        let mut world = world(8, 8);
        let removed = world.add(Cell::new(1, 2)).unwrap();
        let other = world.add(Cell::new(6, 6)).unwrap();
        world.move_to(removed, Cell::new(4, 2)).unwrap();
        world.move_to(other, Cell::new(7, 6)).unwrap();
        assert_eq!(world.remove(removed), Ok(()));
        let events = world.tick_step();
        assert!(
            !events.iter().any(|event| event.creature() == removed),
            "an event named the removed creature: {events:?}"
        );
        assert!(
            events.contains(&Event::RouteSet {
                creature: other,
                steps: 1
            }),
            "the other creature lost its event: {events:?}"
        );
        assert_eq!(events[0].creature(), other);
    }

    /// A refused search whose `NoRoute` still waits in the channel is dropped
    /// with the creature too: `move_to`'s return value was the answer to the
    /// order, the channel carries the search for living creatures only.
    #[test]
    fn removal_drops_a_pending_refusal_of_the_removed_creature() {
        let mut world = world(12, 12);
        let removed = world.add(Cell::new(1, 6)).unwrap();
        let other = world.add(Cell::new(11, 6)).unwrap();
        for row in 0..12 {
            world.passage_mut().blocked.insert(Cell::new(6, row));
        }
        world.rules_mut().route.goal_fallback_radius = 0;
        assert_eq!(
            world.move_to(removed, Cell::new(10, 6)),
            Err(WorldError::NoRoute)
        );
        world.remove(removed).unwrap();
        assert_eq!(
            world.tick_step(),
            Vec::new(),
            "the refused search of the removed creature reached the channel"
        );
        world.move_to(other, Cell::new(11, 7)).unwrap();
        assert!(
            world
                .tick_step()
                .iter()
                .any(|event| event.creature() == other),
            "the channel must still carry the living"
        );
    }

    #[test]
    fn removal_frees_cells_and_leaves_the_others_untouched() {
        let mut world = world(8, 8);
        let removed = world.add(Cell::new(1, 2)).unwrap();
        let walker = world.add(Cell::new(5, 2)).unwrap();
        world.move_to(removed, Cell::new(3, 6)).unwrap();
        world.move_to(walker, Cell::new(7, 2)).unwrap();
        let kept = world.state(walker).unwrap();
        assert_eq!(world.remove(removed), Ok(()));
        assert_eq!(world.holder(Cell::new(1, 2)), None);
        assert_eq!(world.creature_at(Cell::new(1, 2)), None);
        assert_eq!(world.state(removed), None);
        assert_eq!(world.cell_of(removed), None);
        assert_eq!(
            world.state(walker),
            Some(kept),
            "the other creature lost its number, cell or route"
        );
        assert_eq!(world.creature_at(Cell::new(5, 2)), Some(walker));
        assert_eq!(world.states(), vec![kept]);
    }

    #[test]
    fn orders_to_a_removed_creature_are_refused() {
        let mut world = world(8, 8);
        let creature = world.add(Cell::new(1, 2)).unwrap();
        assert_eq!(world.remove(creature), Ok(()));
        assert_eq!(
            world.move_to(creature, Cell::new(3, 3)),
            Err(WorldError::UnknownCreature)
        );
        assert_eq!(world.stop(creature), Err(WorldError::UnknownCreature));
        assert_eq!(world.remove(creature), Err(WorldError::UnknownCreature));
    }

    #[test]
    fn removal_mid_step_releases_the_reservation() {
        let mut world = world(8, 8);
        let walker = world.add(Cell::new(2, 2)).unwrap();
        let follower = world.add(Cell::new(1, 2)).unwrap();
        world.move_to(walker, Cell::new(5, 2)).unwrap();
        world.tick_step();
        assert!(
            world.state(walker).unwrap().moving,
            "the walker must be mid-step"
        );
        let ahead = Cell::new(3, 2);
        assert_eq!(
            world.holder(ahead).map(|holder| holder.creature),
            Some(walker)
        );
        assert_eq!(world.remove(walker), Ok(()));
        assert_eq!(
            world.holder(ahead),
            None,
            "the step's reservation survived the removal"
        );
        assert_eq!(world.holder(Cell::new(2, 2)), None);
        assert_eq!(world.move_to(follower, ahead), Ok(2));
        for _ in 0..8 {
            world.tick_step();
        }
        assert_eq!(
            world.creature_at(ahead),
            Some(follower),
            "the following creature must enter the freed cell"
        );
    }

    #[test]
    fn the_tick_walks_only_the_living_creatures() {
        // The #49 churn: slots pile up from placements that were removed
        // again, and the tick must neither visit them nor miss the keeper.
        let mut world = world(8, 8);
        for row in 0..8 {
            let ghost = world.add(Cell::new(1, row)).unwrap();
            world.remove(ghost).unwrap();
        }
        let keeper = world.add(Cell::new(4, 4)).unwrap();
        world.move_to(keeper, Cell::new(6, 4)).unwrap();
        for _ in 0..8 {
            world.tick_step();
        }
        assert_eq!(world.cell_of(keeper), Some(Cell::new(6, 4)));
        assert_eq!(world.states().len(), 1);
    }

    #[test]
    fn a_removed_number_is_never_handed_out_again() {
        let mut world = world(8, 8);
        let first = world.add(Cell::new(1, 1)).unwrap();
        assert_eq!(world.remove(first), Ok(()));
        let second = world.add(Cell::new(2, 2)).unwrap();
        assert_ne!(second, first);
        assert_eq!(world.state(second).unwrap().creature, second);
        // the removal of the first is not undone by the second: the old number
        // still names nobody
        assert_eq!(world.state(first), None);
    }

    /// Two runs with one seed and one removal order must give one trace:
    /// a removal must not disturb the generator or the visit order of the
    /// creatures that stay (issue #43 criterion 4).
    #[test]
    fn removals_replay_identically_under_the_same_seed() {
        fn trace(seed: u64) -> Vec<Vec<Event>> {
            let mut world = World::new(
                Grid::new(12, 12),
                Map::default(),
                Uniform(2),
                WorldRules::default(),
                seed,
            );
            let mut creatures = Vec::new();
            for column in [1, 4, 7, 10] {
                creatures.push(world.add(Cell::new(column, 4)).unwrap());
                creatures.push(world.add(Cell::new(column, 8)).unwrap());
            }
            for (number, creature) in creatures.iter().enumerate() {
                let goal = if number % 2 == 0 {
                    Cell::new(10, 8)
                } else {
                    Cell::new(1, 4)
                };
                let _ = world.move_to(*creature, goal);
            }
            let mut trace = Vec::new();
            for tick in 0..40 {
                if tick == 12 {
                    let _ = world.remove(creatures[3]);
                    let _ = world.remove(creatures[0]);
                }
                if tick == 24 {
                    let _ = world.remove(creatures[5]);
                    let _ = world.move_to(creatures[7], Cell::new(6, 1));
                }
                trace.push(world.tick_step());
            }
            trace
        }
        let first = trace(7);
        let second = trace(7);
        assert_eq!(first, second, "the same seed replayed a different trace");
    }
}
