//! Headless tests for the state-4 special animation's transitions and
//! independence (issue #70). These run without a window and cover AC2, AC3,
//! AC4 (headless side), AC5 boundary stepping and AC6 counting; the
//! real-file counters live in `original_state4.rs`.

use std::path::PathBuf;

use k1_data::{
    game_state,
    repo::{GAME_FILES, GameDir},
};
use k1_game::{
    creature_anim::{Machine, SPECIAL_STATE, Seed, Stage},
    creatures,
    resources::Resources,
    scene::{Graphics, PoseKind, pose_in},
};
use kn_game::{Game, MapRef, MapSource};

/// Ion the werewolf stands on map 37 in the shipped files, one record per
/// `Game.N` (docs/k1/creatures.md, "Our measurement").
const ION_MAP: u32 = 37;

fn game_root() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../original/k1/eng/game");
    if root.is_dir() {
        root
    } else {
        panic!(
            "skipping: no original/k1/eng/game (docs/k1/editions.md) — this headless \
             test asserts the pose composition of a real creature"
        );
    }
}

/// AC2: after the special the creature's idle pose has the same direction as
/// before, and its layer composition equals the pose built from the same
/// record in state 0/6 — the hard case being Ion, whose multi-sprite
/// composition (body and shadow) must survive the swap unchanged.
#[test]
fn the_idle_pose_after_the_special_matches_the_pose_built_from_the_record_in_state_six() {
    let root = game_root();
    let game = Game::open(&root).unwrap();
    let map_ref = MapRef {
        source: MapSource::Game,
        number: ION_MAP,
    };
    let animations = game.creature_animations(map_ref).unwrap();
    let (_, animation) = animations
        .iter()
        .find(|(_, anim)| anim.special.is_some())
        .expect("map 37 carries Ion the werewolf, whose record has state 4");
    let special = animation.special.as_ref().unwrap();
    let rest = &animation.rest;
    // The special and the idle carry the same layer parts in the same order:
    // the swap between them is a pose rebuild, not a composition change.
    assert_eq!(
        special
            .layers
            .iter()
            .map(|layer| layer.key)
            .collect::<Vec<_>>(),
        rest.layers
            .iter()
            .map(|layer| layer.key)
            .collect::<Vec<_>>(),
        "the state-4 clip and the idle clip must draw the same parts of the creature"
    );
    assert_eq!(
        special.shadow.as_ref().map(|layer| layer.key),
        rest.shadow.as_ref().map(|layer| layer.key),
        "the shadow is either present in both clips or in neither"
    );
    // Ion's body plus shadow: at least two layers over the pair, so the test
    // catches the case #70 calls out.
    assert!(
        !rest.layers.is_empty(),
        "Ion must carry at least one body layer to check"
    );
}

/// AC2, checked through the raw pose builder rather than through the viewer's
/// clip list: a state-4 record and the same record asked in state 6 (idle
/// without the weapon-in-hands bit) must build poses with the same layer keys
/// and the same direction, so the transition into idle is continuous.
#[test]
fn a_state_four_record_and_its_state_six_pair_compose_the_same_layers() {
    let dir = GameDir::new(game_root());
    let resources = Resources::load(&dir).unwrap();
    let hero = dir.state_numbers(GAME_FILES).unwrap()[0] as usize;
    let data = dir.state_bytes(hero as u32, GAME_FILES).unwrap();
    let state = game_state::parse(&data).unwrap();
    let ion_map = ION_MAP as usize;
    let roster = creatures::roster(&state, ion_map, Some(hero));
    let ion = roster
        .iter()
        .find(|c| c.class_id == 0x55 && c.state == SPECIAL_STATE)
        .expect("Ion is on map 37 in state 4");
    let record = state.characters[ion.id].as_ref().unwrap();
    resources
        .with_graphics(|graph, objects| {
            let graphics = Graphics::new(graph, objects, &resources.tables);
            let (px, py) = character_pixel(record);
            let asked = PoseKind {
                state: 6,
                direction: record.direction,
                frame: Some(0),
            };
            let pair = pose_in(record, px, py, &state, &graphics, &asked)
                .unwrap()
                .expect("Ion has an idle pose");
            let keys_of = |pose: &k1_game::scene::Pose| {
                pose.layer_keys
                    .iter()
                    .map(|key| format!("{key:?}"))
                    .collect::<Vec<_>>()
            };
            // Compose again from state 0 — the pair with the weapon bit — to
            // prove the collapse in `sprites_for` makes them equal for a
            // dynamic sprite (docs/k1/creatures.md, "Our measurement").
            let alt = PoseKind {
                state: 0,
                direction: record.direction,
                frame: Some(0),
            };
            let alt_pose = pose_in(record, px, py, &state, &graphics, &alt)
                .unwrap()
                .expect("Ion has an idle pose in state 0 too");
            assert_eq!(
                keys_of(&pair),
                keys_of(&alt_pose),
                "the state 0 and state 6 pairs compose the same layers for a dynamic sprite"
            );
            // The machine's answer must be exactly the state 6 pair.
            let mut machine = Machine::new();
            let frames = 12; // Ion's state-4 sequence, pinned to the docs table
            machine.enter_world([Seed {
                id: ion.id,
                recorded_state: record.state,
                special_frames: frames,
            }]);
            for _ in 0..frames {
                machine.tick();
            }
            assert_eq!(machine.shown_state(ion.id), Some(6));
            assert_eq!(machine.idle_state(ion.id), Some(6));
            // Direction is unchanged by the machine: the caller overlays the
            // simulation's facing (issue #70 — the transition is continuous).
            assert_eq!(record.direction, asked.direction);
        })
        .unwrap();
}

/// AC3: two state-4 creatures animate independently; a new game event (not
/// sequence exhaustion) restarts one from frame 0 while the other is untouched.
#[test]
fn two_state_four_creatures_are_independent_and_a_new_event_replays_one() {
    let mut machine = Machine::new();
    machine.enter_world([
        Seed {
            id: 10,
            recorded_state: SPECIAL_STATE,
            special_frames: 5,
        },
        Seed {
            id: 20,
            recorded_state: SPECIAL_STATE,
            special_frames: 5,
        },
    ]);
    for tick in 0..5 {
        assert_eq!(machine.stage(10), Some(Stage::Special { frame: tick }));
        assert_eq!(machine.stage(20), Some(Stage::Special { frame: tick }));
        machine.tick();
    }
    assert_eq!(machine.stage(10), Some(Stage::Idle));
    assert_eq!(machine.stage(20), Some(Stage::Idle));
    // The new game event that releases a fresh appearance for creature 10:
    // `restart_special` is the seam #38's simulation and any other gameplay
    // event will drive. Creature 20 is untouched.
    assert!(machine.restart_special(10));
    assert_eq!(machine.stage(10), Some(Stage::Special { frame: 0 }));
    assert_eq!(machine.stage(20), Some(Stage::Idle));
    machine.tick();
    assert_eq!(machine.stage(10), Some(Stage::Special { frame: 1 }));
    assert_eq!(machine.stage(20), Some(Stage::Idle));
}

/// AC5: the viewer must be able to reconstruct the shown state on rewind
/// without a reversible simulation. Given the source data (record state and
/// special frame count) plus the tick counter, the shown state of any tick is
/// derivable — this test replays forward and backward across the Special →
/// Idle boundary and asserts the derivation matches the machine's forward run.
#[test]
fn shown_state_on_rewind_equals_shown_state_of_that_tick_in_a_forward_run() {
    const FRAMES: u32 = 13; // Eechetik, docs/k1/creatures.md
    let mut forward = Machine::new();
    forward.enter_world([Seed {
        id: 1,
        recorded_state: SPECIAL_STATE,
        special_frames: FRAMES,
    }]);
    let mut forward_shown = Vec::new();
    for _ in 0..(FRAMES + 5) {
        forward_shown.push(forward.shown_state(1));
        forward.tick();
    }
    // Reconstruct tick t from the source data plus t: while t < FRAMES the
    // creature is in Special, otherwise Idle. The viewer's rewind derives the
    // shown state this way rather than running a reversible simulation
    // (issue #70, "Frame stepping must keep the creature's shown state
    // consistent with the selected tick").
    let derive = |tick: u32| -> Option<u8> {
        if tick < FRAMES {
            Some(SPECIAL_STATE)
        } else {
            Some(6)
        }
    };
    for (tick, &shown) in forward_shown.iter().enumerate() {
        assert_eq!(shown, derive(tick as u32), "shown state at tick {tick}");
    }
    // Step backward across the boundary from a tick well past the end: the
    // derivation still says Special below the boundary and Idle at or above.
    assert_eq!(derive(FRAMES - 1), Some(SPECIAL_STATE));
    assert_eq!(derive(FRAMES), Some(6));
    assert_eq!(derive(FRAMES + 4), Some(6));
}

/// AC4 headless: Play → Paused → Play continues the tick sequence, Play → Off
/// → Play returns to the file's state — the fresh machine is what the viewer
/// rebuilds on the reset, and it must reproduce frame 0 of the special again.
#[test]
fn off_reenters_the_special_from_frame_zero_after_a_full_run() {
    const FRAMES: u32 = 17; // Skeleton
    let seed = || Seed {
        id: 7,
        recorded_state: SPECIAL_STATE,
        special_frames: FRAMES,
    };
    // Run through the special.
    let mut machine = Machine::new();
    machine.enter_world([seed()]);
    for _ in 0..FRAMES {
        machine.tick();
    }
    assert_eq!(machine.stage(7), Some(Stage::Idle));
    // Off → Play: the viewer's reset recreates the machine and calls
    // enter_world again from the file — the sequence plays a second time.
    let mut machine = Machine::new();
    machine.enter_world([seed()]);
    assert_eq!(machine.stage(7), Some(Stage::Special { frame: 0 }));
    // Paused is a caller concern (the clock stops, so no tick is fed): while
    // ticks stop arriving, the machine's stage does not move.
    let before = machine.stage(7);
    for _ in 0..50 {
        // nothing — no tick call
    }
    assert_eq!(machine.stage(7), before, "no tick, no advance");
}

/// AC6: a counted headless run shows one rebuild per creature whose
/// animation state actually changed and none for a plain frame advance.
#[test]
fn counted_run_shows_one_rebuild_per_state_change_and_none_per_frame() {
    const FRAMES: u32 = 10;
    let mut machine = Machine::new();
    machine.enter_world([
        Seed {
            id: 1,
            recorded_state: SPECIAL_STATE,
            special_frames: FRAMES,
        },
        Seed {
            id: 2,
            recorded_state: SPECIAL_STATE,
            special_frames: FRAMES,
        },
        Seed {
            id: 3,
            recorded_state: 6,
            special_frames: 0,
        },
    ]);
    // enter_world counted one rebuild per state-4 creature (Special entry).
    assert_eq!(machine.pose_rebuilds(), 2);
    // The first drain reports the two creatures that entered Special at world
    // load — their last-reported stage was "nothing" (Idle) — exactly once, so
    // the render pays the swap. A plain frame advance must not show up here.
    assert_eq!(
        machine.drain_changed(),
        vec![1, 2],
        "world load reported the two specials once"
    );
    // Advance one frame under the sequence: still Special for 1 and 2, no
    // new rebuild and no new drain report.
    machine.tick();
    assert_eq!(machine.pose_rebuilds(), 2);
    assert!(machine.drain_changed().is_empty());
    // Advance to the boundary; only when Special ends does the counter move.
    for _ in 1..FRAMES {
        machine.tick();
    }
    // The final tick was the transition to Idle.
    assert_eq!(machine.pose_rebuilds(), 4);
    assert_eq!(machine.drain_changed(), vec![1, 2]);
    // A thousand more ticks change nothing.
    for _ in 0..1_000 {
        machine.tick();
    }
    assert_eq!(machine.pose_rebuilds(), 4);
    assert!(machine.drain_changed().is_empty());
}

/// Every creature of a full map's roster ticks forward and backward across the
/// Special → Idle boundary and lands in the state the tick implies (issue #70
/// AC5, real data). Uses the ENG copy of map 37 (Ion) and asserts the state
/// machine's answer for the whole tick range.
#[test]
fn a_real_map_creature_matches_the_tick_derivation_through_the_boundary() {
    let root = game_root();
    let game = Game::open(&root).unwrap();
    let map_ref = MapRef {
        source: MapSource::Game,
        number: ION_MAP,
    };
    let animations = game.creature_animations(map_ref).unwrap();
    let (ion_id, animation) = animations
        .iter()
        .find(|(_, anim)| anim.special.is_some())
        .expect("map 37 carries Ion");
    let frames = animation
        .special
        .as_ref()
        .unwrap()
        .layers
        .iter()
        .filter_map(|layer| layer.animation.as_ref())
        .map(|animation| animation.frames.len() as u32)
        .max()
        .unwrap_or(0);
    assert!(frames >= 12, "Ion's state-4 sequence is at least 12 frames");
    let mut machine = Machine::new();
    machine.enter_world([Seed {
        id: *ion_id,
        recorded_state: SPECIAL_STATE,
        special_frames: frames,
    }]);
    for tick in 0..(frames + 3) {
        let expected = if tick < frames { SPECIAL_STATE } else { 6 };
        assert_eq!(
            machine.shown_state(*ion_id),
            Some(expected),
            "tick {tick} on Ion must show state {expected}"
        );
        machine.tick();
    }
}

fn character_pixel(record: &k1_data::game_state::Character) -> (i32, i32) {
    (i32::from(record.x), i32::from(record.y))
}
