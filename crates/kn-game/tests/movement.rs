//! Synthetic tests of the movement engine (issue #35): routes, repathing,
//! walking a route, simultaneous creatures and determinism. No game data and no
//! Bevy here — everything runs headless.

use std::collections::{HashMap, HashSet};

use kn_game::movement::CreatureId;
use kn_game::movement::grid::{Cell, Direction, Grid};
use kn_game::movement::pace::Pace;
use kn_game::movement::passage::Passage;
use kn_game::movement::world::{Event, World, WorldError, WorldRules};

/// Walkability as a set of blocked cells: the engine does not care where they come from.
#[derive(Default, Clone)]
struct Map {
    blocked: HashSet<Cell>,
}

impl Passage for Map {
    fn is_walkable(&self, cell: Cell) -> bool {
        !self.blocked.contains(&cell)
    }
}

/// Step duration per direction — the supplier's animation-frame table: west,
/// north-west, north, north-east, east, south-east, south, south-west.
#[derive(Clone, Copy)]
struct Table([u32; 8]);

impl Table {
    fn uniform(ticks: u32) -> Table {
        Table([ticks; 8])
    }
}

impl Pace for Table {
    fn step_ticks(&self, _creature: CreatureId, direction: Direction) -> u32 {
        self.0[direction.index()]
    }
}

fn rules() -> WorldRules {
    WorldRules {
        wait_before_repath_ticks: 4,
        repath_cooldown_ticks: 10,
        ..WorldRules::default()
    }
}

fn world(columns: u32, rows: u32, pace: Table) -> World<Map, Table> {
    World::new(Grid::new(columns, rows), Map::default(), pace, rules(), 1)
}

/// Who stands where: cell to creatures. No cell may hold two creatures.
fn occupants(world: &World<Map, Table>) -> HashMap<Cell, Vec<CreatureId>> {
    let mut map: HashMap<Cell, Vec<CreatureId>> = HashMap::new();
    for state in world.states() {
        map.entry(state.cell).or_default().push(state.creature);
    }
    map
}

/// Movement invariants checked on every tick.
fn assert_invariants(world: &World<Map, Table>) {
    for (cell, list) in occupants(world) {
        assert!(list.len() < 2, "cell {cell:?} holds {list:?}");
        assert!(
            world.passage().is_walkable(cell) && world.grid().contains(cell),
            "a creature stands in an impassable cell {cell:?}"
        );
        assert_eq!(
            world.creature_at(cell),
            Some(*list.first().unwrap()),
            "cell {cell:?} is not marked as taken"
        );
    }
    for state in world.states() {
        if let Some(goal) = state.goal {
            assert!(
                world.grid().contains(goal),
                "goal {goal:?} is outside the grid"
            );
        }
        if state.moving {
            let step_to = state.step_to.expect("a step with no destination cell");
            assert!(state.step_ticks > 0, "a step of zero length: {state:?}");
            assert!(
                state.step_elapsed <= state.step_ticks,
                "a step longer than promised: {state:?}"
            );
            assert!(
                world.passage().is_walkable(step_to),
                "a creature walks into an impassable cell {step_to:?}"
            );
        } else {
            assert_eq!(
                state.step_to, None,
                "a stopped creature is walking: {state:?}"
            );
        }
    }
}

fn run(world: &mut World<Map, Table>, ticks: usize) -> Vec<Event> {
    let mut all = Vec::new();
    for _ in 0..ticks {
        all.extend(world.tick_step());
        assert_invariants(world);
    }
    all
}

fn route_sets(events: &[Event]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, Event::RouteSet { .. }))
        .count()
}

fn arrived(events: &[Event], creature: CreatureId) -> bool {
    events
        .iter()
        .any(|event| matches!(event, Event::Arrived { creature: c, .. } if *c == creature))
}

// --- criterion 2: routes ----------------------------------------------------

#[test]
fn routes_straight_and_around_an_obstacle() {
    {
        let mut world = world(24, 24, Table::uniform(1));
        let creature = world.add(Cell::new(4, 10)).unwrap();
        let steps = world.move_to(creature, Cell::new(12, 10)).unwrap();
        assert_eq!(steps, 8);
        let events = run(&mut world, steps + 2);
        assert_eq!(world.cell_of(creature), Some(Cell::new(12, 10)));
        assert!(arrived(&events, creature), "{events:?}");
    }

    // An obstacle in the way: the creature goes around it and still arrives.
    let mut world = world(24, 24, Table::uniform(1));
    let creature = world.add(Cell::new(4, 10)).unwrap();
    for column in 8..=10 {
        for row in 8..=12 {
            world.passage_mut().blocked.insert(Cell::new(column, row));
        }
    }
    let steps = world.move_to(creature, Cell::new(14, 10)).unwrap();
    assert!(
        steps > 10,
        "the detour must be longer than the straight 10 steps: {steps}"
    );
    let events = run(&mut world, steps + 4);
    assert_eq!(world.cell_of(creature), Some(Cell::new(14, 10)));
    assert!(arrived(&events, creature), "{events:?}");
}

#[test]
fn route_is_longer_than_the_original_32_step_limit() {
    let mut world = world(160, 320, Table::uniform(1));
    let creature = world.add(Cell::new(1, 10)).unwrap();
    let steps = world.move_to(creature, Cell::new(121, 10)).unwrap();
    assert_eq!(steps, 120, "the original's limit came back");
    run(&mut world, steps + 2);
    assert_eq!(world.cell_of(creature), Some(Cell::new(121, 10)));
}

#[test]
fn unreachable_goal_is_refused_not_a_panic() {
    let mut world = world(20, 20, Table::uniform(1));
    let creature = world.add(Cell::new(2, 10)).unwrap();
    for row in 0..20 {
        world.passage_mut().blocked.insert(Cell::new(10, row));
    }
    let goal = Cell::new(15, 10);
    world.rules_mut().route.goal_fallback_radius = 0;
    assert_eq!(
        world.move_to(creature, goal),
        Err(WorldError::NoRoute),
        "a free but unreachable goal is not swapped for a neighbouring cell"
    );
    assert_eq!(world.cell_of(creature), Some(Cell::new(2, 10)));
    // no movement, but the refusal reaches the consumer as one event on the first tick
    assert_eq!(
        run(&mut world, 50),
        vec![Event::NoRoute { creature, goal }],
        "the refusal produced movement"
    );
}

#[test]
fn occupied_goal_routes_to_a_free_cell_nearby() {
    let mut world = world(20, 20, Table::uniform(1));
    let walker = world.add(Cell::new(2, 10)).unwrap();
    let stander = world.add(Cell::new(8, 10)).unwrap();
    let steps = world.move_to(walker, Cell::new(8, 10)).unwrap();
    assert!(steps > 0);
    let goal = world.state(walker).unwrap().goal.unwrap();
    assert_ne!(
        goal,
        Cell::new(8, 10),
        "the route leads into an occupied cell"
    );
    assert_eq!(goal.lattice_distance(Cell::new(8, 10)), 1);
    let events = run(&mut world, steps + 4);
    assert_eq!(world.cell_of(walker), Some(goal));
    assert_eq!(world.cell_of(stander), Some(Cell::new(8, 10)));
    assert!(arrived(&events, walker), "{events:?}");
}

// --- criterion 3: repathing -------------------------------------------------

/// The cell ahead became impassable: the creature repaths, arrives, and does
/// not repath on every tick.
#[test]
fn blocked_route_is_repathed_once_and_the_creature_arrives() {
    let mut world = world(24, 24, Table::uniform(1));
    let creature = world.add(Cell::new(2, 10)).unwrap();
    world.move_to(creature, Cell::new(18, 10)).unwrap();
    // One step taken, the next one blocked.
    world.tick_step();
    world.passage_mut().blocked.insert(Cell::new(3, 10));

    let mut events = Vec::new();
    for _ in 0..200 {
        let tick_events = world.tick_step();
        assert_invariants(&world);
        events.extend(tick_events);
        if arrived(&events, creature) {
            break;
        }
    }
    assert!(
        events.iter().any(|event| *event
            == Event::TerrainBlocked {
                creature,
                cell: Cell::new(3, 10)
            }),
        "{events:?}"
    );
    assert!(
        arrived(&events, creature),
        "the creature got stuck: {events:?}"
    );
    assert_eq!(world.cell_of(creature), Some(Cell::new(18, 10)));
    // No thrash: over 200 ticks with a 10-tick cooldown, repaths are counted in units.
    assert!(
        route_sets(&events) <= 3,
        "too many repaths ({}): {events:?}",
        route_sets(&events)
    );
}

/// Another creature holds the cell ahead: the one with the lower number gets
/// through, the second waits, repaths and follows.
#[test]
fn creature_ahead_frees_the_cell_and_the_second_follows() {
    let mut world = world(24, 24, Table::uniform(2));
    let first = world.add(Cell::new(2, 10)).unwrap();
    let second = world.add(Cell::new(1, 10)).unwrap();
    let goal = Cell::new(9, 10);
    assert_eq!(world.move_to(first, goal), Ok(7));
    // The first creature stands ahead, so the second has to go around it.
    assert!(world.move_to(second, goal).unwrap() >= 8);

    let mut events = Vec::new();
    for _ in 0..200 {
        let tick_events = world.tick_step();
        assert_invariants(&world);
        events.extend(tick_events);
        if arrived(&events, first) && arrived(&events, second) {
            break;
        }
    }
    assert!(arrived(&events, first), "{events:?}");
    assert!(arrived(&events, second), "{events:?}");
    assert_eq!(world.cell_of(first), Some(goal));
    let end = world.cell_of(second).unwrap();
    assert!(
        end.lattice_distance(goal) <= 1,
        "the second ended up too far from the goal: {end:?}"
    );
    assert_eq!(
        world.occupancy().holder(Cell::new(2, 10)),
        None,
        "the cell was not freed once the creature left it"
    );
    assert!(
        events.iter().any(
            |event| matches!(event, Event::Waiting { creature: c, blocked_by, .. }
                if *c == second && *blocked_by == first)
        ),
        "the second never waited for the first: {events:?}"
    );
}

/// Head-on orders: no creature enters a cell another is already stepping into,
/// and both stop in the end.
#[test]
fn head_on_exchange_never_shares_a_cell() {
    let mut world = world(24, 24, Table::uniform(1));
    let a = world.add(Cell::new(10, 10)).unwrap();
    let b = world.add(Cell::new(16, 10)).unwrap();
    world.move_to(a, Cell::new(16, 10)).unwrap();
    world.move_to(b, Cell::new(10, 10)).unwrap();

    let mut events = Vec::new();
    for _ in 0..200 {
        let tick_events = world.tick_step();
        assert_invariants(&world);
        events.extend(tick_events);
        if arrived(&events, a) && arrived(&events, b) {
            break;
        }
    }
    assert!(arrived(&events, a), "they never parted: {events:?}");
    assert!(arrived(&events, b), "they never parted: {events:?}");
    assert_ne!(world.cell_of(a), world.cell_of(b));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Waiting { .. })),
        "the head-on order produced no waiting: {events:?}"
    );
}

// --- criterion 4: movement --------------------------------------------------

#[test]
fn step_takes_exactly_the_ticks_the_supplier_gave() {
    // East takes 5 ticks, south 3, the rest 1: the supplier sets the duration.
    let mut world = world(24, 24, Table([5, 5, 1, 1, 5, 3, 3, 3]));
    let creature = world.add(Cell::new(4, 10)).unwrap();
    world.move_to(creature, Cell::new(5, 10)).unwrap();

    let mut cells = Vec::new();
    let mut directions = Vec::new();
    let mut events = Vec::new();
    for _ in 0..8 {
        events.extend(world.tick_step());
        assert_invariants(&world);
        let state = world.state(creature).unwrap();
        cells.push(state.cell);
        directions.push(state.direction);
    }
    // Step started on tick 1, the creature is in the new cell on tick 6 — exactly 5 ticks.
    assert_eq!(cells[0], Cell::new(4, 10));
    assert_eq!(cells[4], Cell::new(4, 10));
    assert_eq!(cells[5], Cell::new(5, 10));
    assert_eq!(directions[..5], [Some(Direction::East); 5]);
    assert!(arrived(&events, creature), "{events:?}");
}

#[test]
fn direction_changes_only_on_the_cell_border() {
    let mut world = world(24, 24, Table::uniform(3));
    let creature = world.add(Cell::new(4, 10)).unwrap();
    world.move_to(creature, Cell::new(8, 14)).unwrap();
    let mut previous_cell = world.cell_of(creature).unwrap();
    let mut previous_direction = world.state(creature).unwrap().direction;
    for _ in 0..60 {
        world.tick_step();
        let state = world.state(creature).unwrap();
        if state.cell == previous_cell {
            assert_eq!(
                state.direction, previous_direction,
                "direction changed in the middle of a cell: {state:?}"
            );
        }
        previous_cell = state.cell;
        previous_direction = state.direction;
        assert_invariants(&world);
    }
}

#[test]
fn route_is_consumed_step_by_step_and_the_creature_stops() {
    let mut world = world(24, 24, Table::uniform(1));
    let creature = world.add(Cell::new(3, 6)).unwrap();
    world.move_to(creature, Cell::new(6, 6)).unwrap();
    let mut visited = vec![world.cell_of(creature).unwrap()];
    for _ in 0..20 {
        let events = world.tick_step();
        assert_invariants(&world);
        for event in events {
            if let Event::StepFinished { cell, .. } = event {
                visited.push(cell);
            }
        }
    }
    assert_eq!(
        visited,
        vec![
            Cell::new(3, 6),
            Cell::new(4, 6),
            Cell::new(5, 6),
            Cell::new(6, 6)
        ],
        "the route is not consumed one step at a time"
    );
    assert_eq!(world.cell_of(creature), Some(Cell::new(6, 6)));
    assert_eq!(world.state(creature).unwrap().goal, None);
    assert!(!world.state(creature).unwrap().moving);
    assert_eq!(
        world.tick_step(),
        Vec::new(),
        "a stopped creature makes noise"
    );
}

// --- criterion 5: several creatures -----------------------------------------

#[test]
fn dozens_of_creatures_move_in_the_same_ticks() {
    let mut world = world(24, 60, Table::uniform(2));
    let mut crowd = Vec::new();
    for index in 0..40 {
        let cell = Cell::new(2 + (index % 4) as u32, 4 + index as u32);
        crowd.push(world.add(cell).unwrap());
    }
    // Each has its own goal in the eastern quarter: routes cross and the
    // creatures get in each other's way for no reason.
    for (index, &creature) in crowd.iter().enumerate() {
        let goal = Cell::new(14 + (index % 5) as u32, 8 + (index / 5) as u32);
        world.move_to(creature, goal).unwrap();
    }

    let mut events = Vec::new();
    for _ in 0..600 {
        let tick_events = world.tick_step();
        assert_invariants(&world);
        events.extend(tick_events);
        if crowd.iter().all(|&creature| arrived(&events, creature)) {
            break;
        }
    }
    for &creature in &crowd {
        assert!(
            arrived(&events, creature),
            "not everyone arrived: {creature:?}"
        );
    }
    let cells: Vec<Cell> = world.states().iter().map(|state| state.cell).collect();
    assert_eq!(cells.iter().collect::<HashSet<_>>().len(), cells.len());
    // Each holds exactly its own cell: no reservation is left dangling.
    assert_eq!(world.occupancy().len(), crowd.len());
}

#[test]
fn freed_cell_is_taken_by_another_creature() {
    let mut world = world(24, 24, Table::uniform(1));
    let first = world.add(Cell::new(4, 8)).unwrap();
    let second = world.add(Cell::new(4, 10)).unwrap();
    world.move_to(first, Cell::new(10, 8)).unwrap();
    let vacated = Cell::new(4, 8);
    // The first leaves its cell, and the second is already heading for it.
    run(&mut world, 2);
    assert_eq!(world.cell_of(first), Some(Cell::new(5, 8)));
    assert_eq!(world.creature_at(vacated), None, "the cell was not freed");
    world.move_to(second, vacated).unwrap();
    run(&mut world, 8);
    assert_eq!(world.cell_of(second), Some(vacated));
    assert_eq!(world.creature_at(vacated), Some(second));
    assert_eq!(world.cell_of(first), Some(Cell::new(10, 8)));
}

#[test]
fn creatures_do_not_occupy_the_same_cell_in_the_same_ticks() {
    // A narrow yard: six creatures head for one cell and never overlap on any
    // tick.
    let mut world = world(16, 16, Table::uniform(1));
    let mut crowd = Vec::new();
    for index in 0..6 {
        let cell = Cell::new(2, 2 + 2 * index as u32);
        crowd.push(world.add(cell).unwrap());
    }
    for &creature in &crowd {
        world.move_to(creature, Cell::new(13, 8)).unwrap();
    }
    run(&mut world, 200);
    let cells: Vec<Cell> = world.states().iter().map(|state| state.cell).collect();
    assert_eq!(
        cells.iter().collect::<HashSet<_>>().len(),
        cells.len(),
        "two stood in one cell: {cells:?}"
    );
    assert!(
        world.states().iter().all(|state| !state.moving),
        "the crowd never settled"
    );
}

// --- criterion 6: determinism ------------------------------------------------

/// A trace: tick, creature number, its cell and direction.
type Trace = Vec<(u64, u32, Cell, Option<Direction>)>;

/// One fixed script: the trace depends on the seed and repeats under it.
fn trace(seed: u64) -> Trace {
    let mut world = World::new(
        Grid::new(24, 24),
        Map::default(),
        Table::uniform(1),
        rules(),
        seed,
    );
    for column in 0..24 {
        if column != 6 && column != 18 {
            world.passage_mut().blocked.insert(Cell::new(column, 12));
        }
    }
    let a = world.add(Cell::new(12, 8)).unwrap();
    let b = world.add(Cell::new(12, 16)).unwrap();
    world.move_to(a, Cell::new(12, 20)).unwrap();
    world.move_to(b, Cell::new(12, 4)).unwrap();
    let mut trace = Vec::new();
    for _ in 0..80 {
        world.tick_step();
        for state in world.states() {
            trace.push((
                world.tick_number(),
                state.creature.raw(),
                state.cell,
                state.direction,
            ));
        }
    }
    trace
}

#[test]
fn same_seed_replays_the_same_trace() {
    assert_eq!(trace(7), trace(7));
    assert_eq!(trace(u64::MAX), trace(u64::MAX));
    // The same seed across separate runs gives the same trace.
    for seed in 0..6 {
        assert_eq!(trace(seed), trace(seed), "seed {seed} replays");
    }
}

/// The seed breaks ties: on a symmetric map different seeds give different
/// traces. Where the choice is not a tie the route is the same under any seed —
/// otherwise determinism would be out of reach.
#[test]
fn other_seed_changes_the_trace() {
    let traces: HashSet<Trace> = (0..6).map(trace).collect();
    assert!(
        traces.len() > 1,
        "the seed changes nothing: every trace matched"
    );
}
