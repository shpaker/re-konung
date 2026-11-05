//! Measurement of removals and the slot history they leave (issue #43 review).
//!
//! An `#[ignore]`d test: it measures in release and prints a table, so it does
//! not get in the way of an ordinary `cargo test`. The numbers go to
//! `docs/rust.md`.
//!
//! ```bash
//! cargo test --release --manifest-path crates/Cargo.toml -p kn-game \
//!     --test removal_cost -- --ignored --nocapture
//! ```

use std::collections::HashSet;
use std::hint::black_box;
use std::time::{Duration, Instant};

use kn_game::movement::CreatureId;
use kn_game::movement::grid::{Cell, Direction, Grid};
use kn_game::movement::pace::Pace;
use kn_game::movement::passage::Passage;
use kn_game::movement::world::{World, WorldRules};

/// Creatures left standing in the measured world: a crowd of a real map.
const LIVE: usize = 100;
/// Removed slots standing behind them: a debug player toggled two thousand
/// times is the #49 scenario.
const HISTORY: usize = 2_000;
/// Ticks in the world-tick measurement.
const TICKS: u32 = 1_000;

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

fn measured_world() -> World<Map, Uniform> {
    World::new(
        Grid::new(256, 256),
        Map::default(),
        Uniform(2),
        WorldRules::default(),
        7,
    )
}

fn report(label: &str, iterations: u32, total: Duration, unit: &str) {
    let per = total.as_secs_f64() * 1e6 / f64::from(iterations);
    println!(
        "{label:<48} {per:>10.3} {unit}  ({} iterations)",
        iterations
    );
}

#[test]
#[ignore = "cost measurement: release only, and only when run explicitly"]
fn measures_the_cost_of_removals_and_the_slot_history() {
    // 1. The placement/removal churn of the #49 debug player: each pair is an
    // add refusal away from nothing, and leaves one more slot behind.
    let mut world = measured_world();
    let cell = Cell::new(1, 1);
    let iterations = 20_000;
    let start = Instant::now();
    for _ in 0..iterations {
        let creature = world.add(cell).unwrap();
        world.remove(creature).unwrap();
    }
    report("add + remove", iterations, start.elapsed(), "us");

    // 2. The tick over idle creatures behind a slot history: no creature
    // walks, so the tick is traversal.
    let mut world = measured_world();
    for _ in 0..HISTORY {
        let ghost = world.add(cell).unwrap();
        world.remove(ghost).unwrap();
    }
    for column in 10..10 + LIVE as u32 {
        world.add(Cell::new(column, 1)).unwrap();
    }
    let start = Instant::now();
    for _ in 0..TICKS {
        black_box(world.tick_step());
    }
    report(
        &format!("tick of {LIVE} idle behind {HISTORY} empty slots"),
        TICKS,
        start.elapsed(),
        "us",
    );

    // 3. `states()` of the same world: the same walk, the consumer's side.
    let start = Instant::now();
    for _ in 0..TICKS {
        black_box(world.states());
    }
    report("states() of the same world", TICKS, start.elapsed(), "us");
}
