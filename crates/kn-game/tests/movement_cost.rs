//! Measurement of the movement engine's cost (issue #35, criterion 7).
//!
//! An `#[ignore]`d test: it measures in release and prints a table, so it does
//! not get in the way of an ordinary `cargo test`. The numbers go to `docs/rust.md`.
//!
//! ```bash
//! cargo test --release --manifest-path crates/Cargo.toml -p kn-game \
//!     --test movement_cost -- --ignored --nocapture
//! ```

use std::collections::HashSet;
use std::hint::black_box;
use std::time::{Duration, Instant};

use kn_game::movement::CreatureId;
use kn_game::movement::grid::{Cell, Direction, Grid};
use kn_game::movement::occupancy::Occupancy;
use kn_game::movement::pace::Pace;
use kn_game::movement::passage::Passage;
use kn_game::movement::rng::Rng;
use kn_game::movement::route::{Route, RouteRequest, RouteRules, find_route};
use kn_game::movement::world::{World, WorldRules};

/// Creatures in the test world: the "dozens" of criterion 7.
const CROWD: usize = 50;
/// Ticks in the world-tick measurement.
const TICKS: usize = 300;
/// The creature the measured searches are searched for.
const MOVER: CreatureId = CreatureId::new(1000);

/// Walkability as a set of blocked cells.
#[derive(Default, Clone)]
struct Map {
    blocked: HashSet<Cell>,
}

impl Passage for Map {
    fn is_walkable(&self, cell: Cell) -> bool {
        !self.blocked.contains(&cell)
    }
}

/// Step duration, like an animation frame count: three ticks in any direction.
#[derive(Clone, Copy)]
struct Walk3;

impl Pace for Walk3 {
    fn step_ticks(&self, _creature: CreatureId, _direction: Direction) -> u32 {
        3
    }
}

fn report(label: &str, iterations: u32, total: Duration, unit: &str) {
    let per = total.as_secs_f64() * 1e6 / f64::from(iterations);
    println!(
        "{label:<38} {per:>10.3} {unit}  ({} iterations)",
        iterations
    );
}

fn rules() -> RouteRules {
    RouteRules::default()
}

/// A route over the K1 fine grid — what a creature does on an order or a repath.
fn route_between<P: Passage>(
    passage: &P,
    occupancy: &Occupancy,
    from: Cell,
    goal: Cell,
    rng: &mut Rng,
) -> Option<Route> {
    let grid = Grid::fine();
    let request = RouteRequest {
        grid: &grid,
        passage,
        occupancy,
        rules: &rules(),
        mover: MOVER,
        from,
        goal,
    };
    find_route(&request, rng)
}

/// A world of `count` creatures already walking routes: the tick is measured over them.
fn crowded_world(count: usize) -> World<Map, Walk3> {
    let mut world = World::new(
        Grid::fine(),
        Map::default(),
        Walk3,
        WorldRules {
            wait_before_repath_ticks: 4,
            repath_cooldown_ticks: 10,
            ..WorldRules::default()
        },
        7,
    );
    // A column of creatures down the left of the map, goals on the right: routes
    // cross and some steps are blocked by other creatures' cells.
    for index in 0..count {
        let row = 4 + 2 * (index % 150) as u32;
        let cell = Cell::new(2 + (index / 150) as u32, row);
        let creature = world.add(cell).unwrap();
        world
            .move_to(creature, Cell::new(140 + (index % 7) as u32, row))
            .unwrap();
    }
    // Spread them over ticks so the creatures are walking, not standing.
    for _ in 0..60 {
        world.tick_step();
    }
    world
}

#[test]
#[ignore = "cost measurement: release only, and only when run explicitly"]
fn measures_the_cost_of_the_movement_engine() {
    let grid = Grid::fine();
    println!(
        "grid {}x{} cells, step {}x{} px",
        grid.columns,
        grid.rows,
        kn_game::movement::grid::STEP_X,
        kn_game::movement::grid::STEP_Y
    );
    let open = Map::default();
    let empty = Occupancy::new();
    let mut rng = Rng::new(1);
    let iterations = 200;

    // 1. A route across the whole map over open ground.
    let start = Instant::now();
    for _ in 0..iterations {
        let route = route_between(
            &open,
            &empty,
            Cell::new(1, 2),
            Cell::new(158, 318),
            &mut rng,
        );
        black_box(route.expect("a route across the whole map"));
    }
    report(
        "route across the whole map",
        iterations,
        start.elapsed(),
        "us",
    );

    // 2. The same route among walls: A* spreads wider.
    let mut walls = Map::default();
    for column in 0..grid.columns {
        for row in (0..grid.rows).step_by(7) {
            if column % 3 == 0 {
                walls.blocked.insert(Cell::new(column, row));
            }
        }
    }
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(route_between(
            &walls,
            &empty,
            Cell::new(1, 2),
            Cell::new(158, 318),
            &mut rng,
        ));
    }
    report("route among walls", iterations, start.elapsed(), "us");

    // 3. The worst case of search: there is no goal, A* sweeps the whole reachable area.
    let mut cage = Map::default();
    for column in 20..25 {
        for row in 0..grid.rows {
            cage.blocked.insert(Cell::new(column, row));
        }
    }
    let start = Instant::now();
    for _ in 0..50 {
        black_box(route_between(
            &cage,
            &empty,
            Cell::new(1, 2),
            Cell::new(158, 318),
            &mut rng,
        ));
    }
    report("route to an unreachable goal", 50, start.elapsed(), "us");

    // 4. Repath under blockage: the goal sits in a dense crowd, fallback over rings.
    let mut crowd = Occupancy::new();
    for index in 0..400u32 {
        crowd.stand(
            Cell::new(120 + index % 20, 100 + index / 20),
            CreatureId::new(index),
        );
    }
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(route_between(
            &open,
            &crowd,
            Cell::new(2, 4),
            Cell::new(128, 104),
            &mut rng,
        ));
    }
    report(
        "search next to 400 occupied cells",
        iterations,
        start.elapsed(),
        "us",
    );

    // 5. A world tick over creatures in motion: dozens and hundreds.
    for count in [CROWD, 500] {
        let mut world = crowded_world(count);
        let moving = world.states().iter().filter(|state| state.moving).count();
        let start = Instant::now();
        for _ in 0..TICKS {
            black_box(world.tick_step());
        }
        let total = start.elapsed();
        report(
            &format!("tick of {} creatures ({moving} walking)", count),
            TICKS as u32,
            total,
            "us",
        );
        println!(
            "{:<38} {:>10.3} us  (per creature per tick)",
            "",
            total.as_secs_f64() * 1e6 / (TICKS * count) as f64
        );
    }

    // 6. A tick with blockage: every 20 ticks a cell ahead of a creature is
    // closed, so the tick includes a repath.
    let mut world = crowded_world(CROWD);
    let start = Instant::now();
    for tick in 0..TICKS {
        if tick % 20 == 0 {
            let cell = Cell::new(20 + (tick / 20) as u32, 10);
            world.passage_mut().blocked.insert(cell);
        }
        black_box(world.tick_step());
    }
    report(
        &format!("tick of {} creatures + blockage", CROWD),
        TICKS as u32,
        start.elapsed(),
        "us",
    );

    // 7. Neighbouring operations for comparison.
    let iterations = 20_000;
    let lookups = |label: &'static str, probe: &dyn Fn(u32)| {
        let start = Instant::now();
        for index in 0..iterations {
            probe(index);
        }
        report(label, iterations, start.elapsed(), "us");
    };
    lookups("cell occupancy (lookup)", &|index| {
        black_box(crowd.blocks(Cell::new(index % 160, (index / 160) % 320), MOVER));
    });
    lookups("A* heuristic (isqrt)", &|index| {
        black_box(Cell::new(index % 160, (index / 160) % 320).cost_distance(Cell::new(150, 200)));
    });
    lookups("neighbour cell", &|index| {
        black_box(Cell::new(index % 160, (index / 160) % 320).neighbour(Direction::NorthEast));
    });
    lookups("cell to pixels", &|index| {
        black_box(Cell::new(index % 160, (index / 160) % 320).centre());
    });
}
