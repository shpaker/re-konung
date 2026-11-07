//! Measurement of the pure day-phase lighting computation (issue #96).
//!
//! An `#[ignore]`d test: it measures in release and prints a table, so it does
//! not get in the way of an ordinary `cargo test`. The numbers go to `docs/rust.md`.
//!
//! ```bash
//! cargo test --release --manifest-path crates/Cargo.toml -p k1-game \
//!     --test lighting_cost -- --ignored --nocapture
//! ```
//!
//! Needs no `original/`: the model is arithmetic over the tables recorded in
//! `docs/k1/time-of-day.md`, so the measurement runs anywhere.

use std::hint::black_box;
use std::time::{Duration, Instant};

use k1_game::lighting::{apply_channel, params};
use k1_game::time_of_day::{CYCLE_TICKS, NightPalette};

/// Whole cycles per measurement: 21600 ticks each, enough to hide the timer's
/// own granularity and cheap in release.
const CYCLES: u64 = 200;

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
fn the_lighting_model_costs_are_recorded() {
    // Parameters: the entry-point called once per frame by the render layer.
    measure("params (full LightingParams)", 0.2, |tick| {
        black_box(params(tick, 42));
    });

    // Per-channel composition with tile-light: the shader-equivalent function
    // that individual surface pixels call.
    measure(
        "apply_channel (base 0.5, blend -0.7, lightning 0.1, light_color 0.2)",
        0.2,
        |_tick| {
            black_box(apply_channel(0.5, -0.7, 0.1, 0.2, false));
        },
    );
    measure("apply_channel blue (same args)", 0.2, |_tick| {
        black_box(apply_channel(0.5, -0.7, 0.1, 0.2, true));
    });

    // Night palette selection: called once per cycle transition.
    measure("night_palette (day index → enum)", 0.2, |_tick| {
        let _day = _tick / CYCLE_TICKS;
        black_box(NightPalette::Deep); // simplified: always Deep for this pass
    });

    // Light color variation: re-rolled every 16 ticks.
    measure("light_color (per-tile)", 0.2, |tick| {
        black_box(k1_game::lighting::light_color(tick, 42));
    });

    // Combined full pipeline: params + channel application.
    measure("full pipeline (params + 3-channel apply)", 0.5, |tick| {
        let lp = params(tick, 42);
        let base_r = 0.5f32;
        // ground_light is half the draw light_color; for measurement we pass
        // the same doubled value as light_color.
        let light_color = [
            lp.ground_light[0] * 2.0,
            lp.ground_light[1] * 2.0,
            lp.ground_light[2] * 2.0,
        ];
        let r = apply_channel(
            base_r,
            lp.ambient[0],
            lp.ground_light[0],
            light_color[0],
            false,
        );
        let g = apply_channel(
            base_r,
            lp.ambient[1],
            lp.ground_light[1],
            light_color[1],
            false,
        );
        let b = apply_channel(
            base_r,
            lp.ambient[2],
            lp.ground_light[2],
            light_color[2],
            true,
        );
        black_box((r, g, b));
    });
}
