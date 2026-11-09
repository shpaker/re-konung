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
use k1_game::time_of_day::CYCLE_TICKS;

/// Whole cycles per measurement: 21600 ticks each, enough to hide the timer's
/// own granularity and cheap in release.
const CYCLES: u64 = 200;

fn measure(label: &str, floor: f64, start_tick: u64, body: impl Fn(u64)) {
    let start = Instant::now();
    let mut absolute = start_tick;
    for _ in 0..CYCLES {
        for _tick in 0..CYCLE_TICKS {
            body(absolute);
            absolute += 1;
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
    let seed = black_box(42u64);

    // Parameters: the entry-point called once per frame by the render layer.
    measure("params (full LightingParams)", 0.2, 0, |tick| {
        black_box(params(black_box(tick), seed));
    });

    // Per-channel composition with tile-light: the shader-equivalent function
    // that individual surface pixels call. Inputs are black-boxed so the
    // compiler cannot fold constants.
    measure(
        "apply_channel (base 0.5, blend -0.7, lightning 0.1, light_color 0.2)",
        0.2,
        0,
        |_tick| {
            black_box(apply_channel(
                black_box(0.5f32),
                black_box(-0.7f32),
                black_box(0.1f32),
                black_box(0.2f32),
                black_box(false),
            ));
        },
    );
    measure("apply_channel blue (same args)", 0.2, 0, |_tick| {
        black_box(apply_channel(
            black_box(0.5f32),
            black_box(-0.7f32),
            black_box(0.1f32),
            black_box(0.2f32),
            black_box(true),
        ));
    });

    // Night palette selection: called once per cycle transition.
    // Uses non-zero start_tick so day is not always 0.
    measure(
        "night_palette (day index, seed → enum)",
        0.2,
        500_000,
        |tick| {
            let day = black_box(tick / CYCLE_TICKS);
            black_box(k1_game::lighting::night_palette(day, seed));
        },
    );

    // Light color variation: re-rolled every 16 ticks.
    measure("light_color (per-tile)", 0.2, 0, |tick| {
        black_box(k1_game::lighting::light_color(black_box(tick), seed));
    });

    // Combined full pipeline: params + channel application.
    // lightning is the mask brightness (0..1), light_color the draw-pass RGB.
    // They are independent in the shader; here we pass distinct values.
    measure("full pipeline (params + 3-channel apply)", 0.5, 0, |tick| {
        let lp = params(black_box(tick), seed);
        let base_r = black_box(0.5f32);
        let lightning = black_box(0.15f32); // mask brightness
        let light_color = k1_game::lighting::light_color(tick, seed); // RGB source
        let r = apply_channel(base_r, lp.ambient[0], lightning, light_color[0], false);
        let g = apply_channel(base_r, lp.ambient[1], lightning, light_color[1], false);
        let b = apply_channel(base_r, lp.ambient[2], lightning, light_color[2], true);
        black_box((r, g, b));
    });
}
