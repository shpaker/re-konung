//! Measurement of what a creature pose costs to assemble (issue #36, criterion 10).
//!
//! An `#[ignore]`d test: it measures in release and prints a table, so it does not
//! get in the way of an ordinary `cargo test`. The numbers go to `docs/rust.md`.
//!
//! ```bash
//! cargo test --release --manifest-path crates/Cargo.toml -p k1-game \
//!     --test pose_cost -- --ignored --nocapture
//! ```
//!
//! Skipped when `original/` is absent (docs/k1/editions.md).

use std::hint::black_box;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use k1_data::{characters, game_state, repo::GAME_FILES, repo::GameDir};
use k1_game::{resources::Resources, scene};

/// The measurement runs on the map with the most creatures of the first hero.
const MAP: u32 = 1;
/// Repeats per measurement: enough to smooth the allocator, cheap in release.
const REPEATS: u32 = 200;

fn editions() -> Vec<(&'static str, GameDir)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../original/k1");
    ["rus", "eng"]
        .into_iter()
        .filter_map(|edition| {
            let game = GameDir::new(root.join(edition).join("game"));
            game.root().is_dir().then_some((edition, game))
        })
        .collect()
}

/// Microseconds per iteration: what one call of the measured thing costs.
fn report(label: &str, iterations: u32, total: Duration) {
    let per = total.as_secs_f64() * 1e6 / f64::from(iterations);
    println!("{label:<44} {per:>10.1} us  ({iterations} iterations)");
}

/// Milliseconds for a single call, when one call is the measurement.
fn report_once(label: &str, total: Duration) {
    println!(
        "{label:<44} {:>10.1} ms  (1 iteration)",
        total.as_secs_f64() * 1e3
    );
}

#[test]
#[ignore]
fn pose_assembly_cost() {
    for (edition, game) in editions() {
        let resources = Resources::load(&game).unwrap();
        let data = game.state_bytes(0, GAME_FILES).unwrap();
        let state = game_state::parse(&data).unwrap();
        let entities = game_state::on_map(&state, MAP as usize, Some(0));
        println!("\n{edition} konung.{MAP}");

        let start = Instant::now();
        let blocks = resources
            .with_graphics(|graph, objects| black_box((graph.models.len(), objects.dynamic.len())))
            .unwrap();
        report_once("parsing GRAPH.RES and Objects.res", start.elapsed());
        black_box(blocks);

        resources
            .with_graphics(|graph, objects| {
                let graphics = scene::Graphics::new(graph, objects, &resources.tables);
                let placed = characters::placed(&entities, &state);
                let sample = placed
                    .iter()
                    .find(|(character, _, _)| !character.has_dynamic_sprite())
                    .or_else(|| placed.first())
                    .expect("konung.1 holds no creature");
                let character = sample.0;
                let (px, py) = game_state::character_position(sample.1, sample.2);

                // One pose: the layers and frames of one state and direction.
                let clock = Instant::now();
                for _ in 0..REPEATS {
                    let pose = scene::pose_in(character, px, py, &state, &graphics, 1, 4).unwrap();
                    black_box(pose.expect("the creature has no pose"));
                }
                report("one pose (state 1, direction 4)", REPEATS, clock.elapsed());

                // A creature that turns its head or starts to walk: the pose the
                // display asks for when state or direction changes. All sixteen of
                // them, so the number is what one change costs on average.
                let clock = Instant::now();
                for _ in 0..REPEATS / 16 {
                    for direction in 0..8u8 {
                        for state_value in [0u8, 1] {
                            let pose = scene::pose_in(
                                character,
                                px,
                                py,
                                &state,
                                &graphics,
                                state_value,
                                direction,
                            )
                            .unwrap();
                            black_box(pose);
                        }
                    }
                }
                report(
                    "a pose per state and direction change",
                    REPEATS,
                    clock.elapsed(),
                );

                // The whole map once: what a scene build pays for creatures.
                let clock = Instant::now();
                for _ in 0..REPEATS / 20 {
                    let mut pieces = 0;
                    for (creature, x, y) in &placed {
                        let (px, py) = game_state::character_position(*x, *y);
                        let creature = *creature;
                        if let Some(pose) = scene::pose_in(
                            creature,
                            px,
                            py,
                            &state,
                            &graphics,
                            0,
                            creature.direction,
                        )
                        .unwrap()
                        {
                            pieces += pose.pieces.len();
                        }
                    }
                    black_box(pieces);
                }
                report(
                    &format!("all {} creatures of the map", placed.len()),
                    REPEATS / 20,
                    clock.elapsed(),
                );
            })
            .unwrap();
    }
}
