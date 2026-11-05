//! Measurement of what advancing game time costs per tick (issue #95).
//!
//! An `#[ignore]`d test: it measures in release and prints a table, so it does
//! not get in the way of an ordinary `cargo test`. The numbers go to `docs/rust.md`.
//!
//! ```bash
//! cargo test --release --manifest-path crates/Cargo.toml -p k1-game \
//!     --test time_of_day_cost -- --ignored --nocapture
//! ```
//!
//! Needs no `original/`: the model is arithmetic over the tables recorded in
//! `docs/k1/time-of-day.md`, so the measurement runs anywhere.

use std::hint::black_box;
use std::time::{Duration, Instant};

use k1_game::time_of_day::{
    CYCLE_TICKS, ambient_kind, ambient_value, clock_face, day_phase, game_time, ground_window,
};

/// Whole cycles per measurement: 21600 ticks each, enough to hide the timer's
/// own granularity and cheap in release.
const CYCLES: u64 = 200;

/// Runs `body` over every tick of [`CYCLES`] cycles and reports nanoseconds per
/// call. The loop shape is the same in every measurement, including the floor,
/// so what is left over is the body.
fn measure(label: &str, floor: f64, body: impl Fn(u64)) {
    let start = Instant::now();
    for _ in 0..CYCLES {
        for tick in 0..CYCLE_TICKS {
            body(tick);
        }
    }
    report(label, start.elapsed(), floor);
}

fn report(label: &str, total: Duration, floor: f64) {
    let calls = (CYCLES * CYCLE_TICKS) as f64;
    let per = total.as_secs_f64() * 1e9 / calls;
    let note = if per < floor * 2.0 {
        " — at the floor of the harness, not resolvable"
    } else {
        ""
    };
    println!("{label:<44} {per:>8.2} ns/tick{note}");
}

#[test]
#[ignore = "cost measurement: release only, and only when run explicitly"]
fn measures_the_cost_of_advancing_game_time() {
    println!(
        "\n{} calls per row ({} cycles of {CYCLE_TICKS} ticks)",
        CYCLES * CYCLE_TICKS,
        CYCLES
    );
    println!("{:<44} {:>14}", "operation", "");

    // The loop with nothing in it: everything measured below has to clear this.
    let start = Instant::now();
    for _ in 0..CYCLES {
        for tick in 0..CYCLE_TICKS {
            black_box(tick);
        }
    }
    let calls = (CYCLES * CYCLE_TICKS) as f64;
    let floor = start.elapsed().as_secs_f64() * 1e9 / calls;
    println!("{:<44} {floor:>8.2} ns/tick", "empty loop (harness floor)");

    measure("ground_window (window + range check)", floor, |tick| {
        black_box(ground_window(tick));
    });
    measure("clock_face (three divisions)", floor, |tick| {
        black_box(clock_face(tick));
    });
    measure("ambient_value (window + branch)", floor, |tick| {
        black_box(ambient_value(tick));
    });
    measure("ambient_kind (the triple and its sign)", floor, |tick| {
        black_box(ambient_kind(tick));
    });
    measure("day_phase (ambient + ground window)", floor, |tick| {
        black_box(day_phase(tick));
    });
    measure(
        "game_time — the whole readout the panel shows",
        floor,
        |tick| {
            black_box(game_time(tick));
        },
    );
}
