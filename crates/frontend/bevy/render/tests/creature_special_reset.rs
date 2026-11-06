//! "Reset positions" and "Reset animation" are separate operations (review of
//! #101, finding P2-#3).
//!
//! * **Reset positions** returns the walk world to the map's load cells but
//!   keeps the creature animation machine: a state-4 creature mid-special stays
//!   mid-special, an idle creature stays idle. It must NOT reseed the special to
//!   frame 0.
//! * **Reset animation** restarts every state-4 special from frame 0 while the
//!   walk world and the simulation clock stay put — the button is no longer the
//!   `Play → Off → Play` full reset (review of #101, P2-#3). The restart
//!   re-anchors the [`Animator`] on the same [`Sequence`]: a frame regression the
//!   Special↔Idle kind flip cannot see, so the swap must notice it
//!   (`CreatureAnim::observe_specials`).
//!
//! The pre-reset lockstep assertion is what makes each falsifiable: the animator
//! must be showing frame `K` before the reset. Tests on real data skip when
//! `original/` is absent (docs/k1/editions.md).

use std::path::PathBuf;
use std::sync::Arc;

use bevy::prelude::*;
use kn_game::{Game, MapRef, SceneOptions, TICK_MS, simulation::CreatureStage};
use kn_render::{
    AnimPlugin, Clock, CreatureAnimationPlugin, CreatureMotion, CreatureSim, CreatureSprite,
    GameResource, PlayState, Sequence, TextureCache, reset_creature_animations,
    reset_creature_positions,
};

const SEED: u64 = 0x5EED_0171;
/// How many frames of the special to reach before a reset.
const K: u64 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reset {
    Positions,
    Animation,
}

/// A pending reset the test sets, applied once by [`reset_once`].
#[derive(Resource, Default)]
struct PendingReset(Option<(MapRef, u64, Reset)>);

fn reset_once(
    game: Res<GameResource>,
    mut sim: ResMut<CreatureSim>,
    mut pending: ResMut<PendingReset>,
) {
    if let Some((map_ref, clock_ticks, kind)) = pending.0.take() {
        match kind {
            Reset::Positions => {
                reset_creature_positions(&game.0, map_ref, SEED, clock_ticks, &mut sim)
            }
            Reset::Animation => reset_creature_animations(&mut sim),
        }
    }
}

fn game_dir() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../original/k1");
    for edition in ["rus", "eng"] {
        let dir = root.join(edition).join("game");
        if dir.is_dir() {
            return Some(dir);
        }
    }
    eprintln!("skipping: no original/k1/{{rus,eng}}/game (docs/k1/editions.md)");
    None
}

fn app(game: Game, motion: CreatureMotion) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AnimPlugin::new(TICK_MS as f32),
        CreatureAnimationPlugin,
    ))
    .insert_resource(GameResource(game))
    .init_resource::<TextureCache>()
    .insert_resource(Assets::<Image>::default())
    .init_resource::<PendingReset>()
    .add_systems(Update, reset_once)
    .insert_resource(motion);
    app.world_mut()
        .resource_mut::<Clock>()
        .goto(PlayState::Paused);
    app
}

/// First map whose simulation seeds a creature into its special, and that id.
fn special_creature(game: &Game) -> Option<(MapRef, usize)> {
    for &map_ref in game.maps().ok()?.iter() {
        let Ok(simulation) = game.simulation(map_ref, SEED) else {
            continue;
        };
        for state in simulation.states() {
            if simulation
                .creature_stage(state.id)
                .is_some_and(|stage| stage.is_special())
            {
                return Some((map_ref, state.id));
            }
        }
    }
    None
}

/// First map that BOTH seeds a state-4 special AND has a creature that has
/// walked off its load cell within `ticks`. The animation-reset test needs a real
/// displacement so "positions preserved" is falsifiable (a reseed would snap
/// everyone back to their load cells). The probe runs the same `(map, seed, ticks)`
/// the app will, so the app reproduces the displacement exactly.
fn special_and_moving_creature(game: &Game, ticks: u64) -> Option<(MapRef, usize)> {
    for &map_ref in game.maps().ok()?.iter() {
        let Ok(mut simulation) = game.simulation(map_ref, SEED) else {
            continue;
        };
        let load: Vec<(usize, (i32, i32))> = simulation
            .states()
            .iter()
            .map(|state| (state.id, state.pixel))
            .collect();
        let special = load.iter().map(|(id, _)| *id).find(|id| {
            simulation
                .creature_stage(*id)
                .is_some_and(|stage| stage.is_special())
        })?;
        for _ in 0..ticks {
            simulation.tick();
        }
        let walked: Vec<(usize, (i32, i32))> = simulation
            .states()
            .iter()
            .map(|state| (state.id, state.pixel))
            .collect();
        if load
            .iter()
            .zip(walked.iter())
            .any(|((_, from), (_, to))| from != to)
        {
            return Some((map_ref, special));
        }
    }
    None
}

fn special_arc(app: &App, creature: usize) -> (Arc<Sequence>, usize) {
    let anim = app.world().resource::<kn_render::CreatureAnim>();
    let layer = anim
        .creatures
        .get(&creature)
        .unwrap_or_else(|| panic!("creature #{creature} must have sequences"))
        .body
        .values()
        .find(|layer| layer.special.is_some())
        .unwrap_or_else(|| panic!("creature #{creature} must have a special clip"));
    let clip = layer.special.as_ref().unwrap();
    (Arc::clone(&clip.sequence), clip.base)
}

fn shown(app: &mut App, creature: usize) -> (Option<usize>, Arc<Sequence>) {
    let mut query = app
        .world_mut()
        .query::<(&CreatureSprite, &kn_render::Animator)>();
    let world = app.world();
    let (_, animator) = query
        .iter(world)
        .find(|(tag, _)| tag.creature_id == creature && !tag.is_shadow && tag.layer == 0)
        .unwrap_or_else(|| panic!("creature #{creature} must have a body sprite"));
    (animator.shown(), Arc::clone(&animator.sequence))
}

fn machine_frame(app: &App, creature: usize) -> u32 {
    let sim = app.world().resource::<CreatureSim>();
    let simulation = sim.sim.as_ref().expect("the map simulates");
    match simulation.creature_stage(creature) {
        Some(CreatureStage::Special { frame }) => frame,
        other => panic!("creature #{creature} must be playing its special, got {other:?}"),
    }
}

fn machine_idle(app: &App, creature: usize) -> bool {
    let sim = app.world().resource::<CreatureSim>();
    let simulation = sim.sim.as_ref().expect("the map simulates");
    !simulation
        .creature_stage(creature)
        .is_some_and(|stage| stage.is_special())
}

/// The simulation's own game-time counter (#95): the clock a reset must not rewind.
fn sim_tick(app: &App) -> u64 {
    app.world()
        .resource::<CreatureSim>()
        .sim
        .as_ref()
        .expect("the map simulates")
        .tick_number()
}

/// Every creature's world pixel, in deterministic roster order — the snapshot a
/// reset must leave untouched when it only rewinds the animation.
fn snapshot_pixels(app: &App) -> Vec<(usize, (i32, i32))> {
    let sim = app.world().resource::<CreatureSim>();
    let simulation = sim.sim.as_ref().expect("the map simulates");
    simulation
        .states()
        .iter()
        .map(|state| (state.id, state.pixel))
        .collect()
}

/// Opens `creature`'s special at frame `K`: a fresh app, drive the clock to `K`,
/// assert the machine and the animator are in lockstep on frame `K` (the
/// precondition that makes the reset below meaningful). Returns the length.
fn run_to_frame_k(app: &mut App, creature: usize) -> usize {
    app.update();
    let (special, base) = special_arc(app, creature);
    let length = special.frames.len();
    assert!(
        length >= usize::try_from(K + 2).unwrap(),
        "the special must run at least {} frames to drive it partway, got {length}",
        K + 2
    );
    app.world_mut().resource_mut::<Clock>().ticks = K;
    app.update();
    assert_eq!(
        machine_frame(app, creature),
        u32::try_from(K).unwrap(),
        "the machine ran {K} ticks"
    );
    let (shown_frame, shown_sequence) = shown(app, creature);
    assert!(
        Arc::ptr_eq(&shown_sequence, &special),
        "the body shows the special"
    );
    assert_eq!(
        shown_frame,
        Some((base + usize::try_from(K).unwrap()) % length),
        "the animator shows frame {K} of the special before the reset"
    );
    length
}

/// AC: "Reset positions" does NOT restart an in-flight special.
#[test]
fn reset_positions_keeps_a_mid_special_playing() {
    let Some(dir) = game_dir() else {
        return;
    };
    let game = Game::open(&dir).unwrap();
    let (map_ref, creature) =
        special_creature(&game).expect("an edition map must have state-4 creatures");

    let mut motion = CreatureMotion::default();
    motion.request(
        map_ref,
        SEED,
        None,
        game.scene(map_ref, &SceneOptions::ALL).unwrap(),
    );
    let mut app = app(game, motion);
    let length = run_to_frame_k(&mut app, creature);
    // Read the clip base after the first update: before it the animation machine
    // is empty, and the base is stable across the positions reset (same Arc).
    let base = special_arc(&app, creature).1;

    // The clock stays put (positions reset does not rewind the timeline).
    app.world_mut().resource_mut::<PendingReset>().0 = Some((map_ref, K, Reset::Positions));
    app.update();
    app.update();

    assert_eq!(
        machine_frame(&app, creature),
        u32::try_from(K).unwrap(),
        "Reset positions must NOT reseed the special: the machine stays at frame {K}"
    );
    let (shown_frame, shown_sequence) = shown(&mut app, creature);
    assert!(Arc::ptr_eq(&shown_sequence, &special_arc(&app, creature).0));
    assert_eq!(
        shown_frame,
        Some((base + usize::try_from(K).unwrap()) % length),
        "the shown frame continues on frame {K}; the special was not restarted"
    );
}

/// AC: "Reset animation" returns a mid-special creature to frame 0 and re-anchors
/// the animator on the same clip (the P2-#3 visual/logical consistency fix).
#[test]
fn reset_animation_returns_the_shown_frame_to_zero() {
    let Some(dir) = game_dir() else {
        return;
    };
    let game = Game::open(&dir).unwrap();
    let (map_ref, creature) =
        special_creature(&game).expect("an edition map must have state-4 creatures");

    let mut motion = CreatureMotion::default();
    motion.request(
        map_ref,
        SEED,
        None,
        game.scene(map_ref, &SceneOptions::ALL).unwrap(),
    );
    let mut app = app(game, motion);
    let length = run_to_frame_k(&mut app, creature);

    // The animation reset leaves the clock and the creature where they are and
    // only rewinds the machine: the special opens again at frame 0.
    app.world_mut().resource_mut::<PendingReset>().0 = Some((map_ref, 0, Reset::Animation));
    app.update();
    app.update();

    assert_eq!(
        machine_frame(&app, creature),
        0,
        "the restarted machine opens the special at frame 0"
    );
    let (shown_frame, shown_sequence) = shown(&mut app, creature);
    assert!(
        Arc::ptr_eq(&shown_sequence, &special_arc(&app, creature).0),
        "the reset re-anchors the same special clip, it does not swap off it"
    );
    assert_eq!(
        shown_frame,
        Some(special_arc(&app, creature).1 % length),
        "the shown frame returns to frame 0 of the new simulation, not the {K} it had reached"
    );
    assert_eq!(
        machine_frame(&app, creature),
        0,
        "the machine stayed at frame 0"
    );
}

/// AC: after the special ends the two buttons diverge. "Reset positions" leaves
/// the idle creature idle — it must not reseed a phantom special from the load
/// cells. "Reset animation" reseeds, which re-opens the state-4 special at frame 0.
#[test]
fn reset_after_the_special_ends_stays_idle() {
    let Some(dir) = game_dir() else {
        return;
    };
    let game = Game::open(&dir).unwrap();
    let (map_ref, creature) =
        special_creature(&game).expect("an edition map must have state-4 creatures");

    let mut motion = CreatureMotion::default();
    motion.request(
        map_ref,
        SEED,
        None,
        game.scene(map_ref, &SceneOptions::ALL).unwrap(),
    );
    let mut app = app(game, motion);
    app.update();
    let frames = special_arc(&app, creature).0.frames.len();

    // Run past the end of the special so the creature lands in Idle.
    app.world_mut().resource_mut::<Clock>().ticks = u64::try_from(frames + 3).unwrap();
    app.update();
    assert!(
        machine_idle(&app, creature),
        "the special ended and the machine is Idle"
    );

    // "Reset positions" must not re-enter the special from Idle.
    let ticks = app.world().resource::<Clock>().ticks;
    app.world_mut().resource_mut::<PendingReset>().0 = Some((map_ref, ticks, Reset::Positions));
    app.update();
    app.update();
    assert!(
        machine_idle(&app, creature),
        "a positions reset must not re-enter the special from Idle"
    );

    // "Reset animation" reopens the state-4 special at frame 0 from Idle without
    // touching the clock: paused, no tick is fed, so the machine holds the frame.
    app.world_mut().resource_mut::<PendingReset>().0 = Some((map_ref, 0, Reset::Animation));
    app.update();
    app.update();
    assert_eq!(
        machine_frame(&app, creature),
        0,
        "an animation reset re-opens the special at frame 0 from Idle"
    );
}

/// AC: repeated reset is stable, and a paused clock (no ticks) does not drift the
/// special while it is being reset.
#[test]
fn repeated_reset_is_stable_and_paused_does_not_advance() {
    let Some(dir) = game_dir() else {
        return;
    };
    let game = Game::open(&dir).unwrap();
    let (map_ref, creature) =
        special_creature(&game).expect("an edition map must have state-4 creatures");

    let mut motion = CreatureMotion::default();
    motion.request(
        map_ref,
        SEED,
        None,
        game.scene(map_ref, &SceneOptions::ALL).unwrap(),
    );
    let mut app = app(game, motion);
    let length = run_to_frame_k(&mut app, creature);

    // Paused at the clock the special reached: each reset rewinds only the
    // machine, so the shown frame holds 0 and no tick drifts it forward.
    for _ in 0..3 {
        app.world_mut().resource_mut::<PendingReset>().0 = Some((map_ref, 0, Reset::Animation));
        app.update();
        app.update();
        // Paused at the reset epoch: no tick is fed, so the machine must hold 0.
        assert_eq!(
            machine_frame(&app, creature),
            0,
            "repeated Reset animation holds frame 0"
        );
        let (shown_frame, _) = shown(&mut app, creature);
        assert_eq!(
            shown_frame,
            Some(special_arc(&app, creature).1 % length),
            "the shown frame holds frame 0 across repeated resets under Pause"
        );
    }
}

/// AC (P2-#3a): "Reset positions" only moves the walk world — it must NOT rewind
/// the simulation's game clock (`tick`, the #95 time source) nor restart an
/// in-flight special. A full reseed (the old shared handler) tripped both.
#[test]
fn reset_positions_preserves_the_game_clock_and_the_special() {
    let Some(dir) = game_dir() else {
        return;
    };
    let game = Game::open(&dir).unwrap();
    let (map_ref, creature) =
        special_creature(&game).expect("an edition map must have state-4 creatures");

    let mut motion = CreatureMotion::default();
    motion.request(
        map_ref,
        SEED,
        None,
        game.scene(map_ref, &SceneOptions::ALL).unwrap(),
    );
    let mut app = app(game, motion);
    let length = run_to_frame_k(&mut app, creature);
    let base = special_arc(&app, creature).1;
    let tick_before = sim_tick(&app);
    assert_eq!(
        tick_before, K,
        "the sim ran exactly {K} ticks to reach the special frame"
    );

    app.world_mut().resource_mut::<PendingReset>().0 = Some((map_ref, K, Reset::Positions));
    app.update();
    app.update();

    assert_eq!(
        sim_tick(&app),
        tick_before,
        "Reset positions must NOT rewind the game clock: tick stays at {K}"
    );
    assert_eq!(
        machine_frame(&app, creature),
        u32::try_from(K).unwrap(),
        "Reset positions must NOT restart the special"
    );
    let (shown_frame, _) = shown(&mut app, creature);
    assert_eq!(
        shown_frame,
        Some((base + usize::try_from(K).unwrap()) % length),
        "the shown special frame is preserved across a positions reset"
    );
}

/// AC (P2-#3b): "Reset animation" restarts the special from frame 0 while every
/// creature keeps its world position and the simulation keeps its clock — unlike
/// the `Play → Off → Play` full reset, it moves nobody and rewinds no time.
#[test]
fn reset_animation_keeps_positions_and_clock_but_restarts_the_special() {
    const WALK: u64 = 240;
    let Some(dir) = game_dir() else {
        return;
    };
    let game = Game::open(&dir).unwrap();
    let Some((map_ref, creature)) = special_and_moving_creature(&game, WALK) else {
        eprintln!("skipping: no map both seeds a state-4 special and walks within {WALK} ticks");
        return;
    };

    let mut motion = CreatureMotion::default();
    motion.request(
        map_ref,
        SEED,
        None,
        game.scene(map_ref, &SceneOptions::ALL).unwrap(),
    );
    let mut app = app(game, motion);
    app.update(); // the map simulates at clock 0: everyone stands on their load cell
    let load = snapshot_pixels(&app);

    // Drive the walk the discovery probe promised: this displacement is what makes
    // "positions preserved" falsifiable — a reseed would snap them all back.
    app.world_mut().resource_mut::<Clock>().ticks = WALK;
    app.update();
    let walked = snapshot_pixels(&app);
    assert!(
        load.iter()
            .zip(walked.iter())
            .any(|((_, from), (_, to))| from != to),
        "the probe promised a mover on this map; none walked within {WALK} ticks"
    );

    let tick_before = sim_tick(&app);
    let before = snapshot_pixels(&app);
    app.world_mut().resource_mut::<PendingReset>().0 = Some((map_ref, WALK, Reset::Animation));
    app.update();
    app.update();

    assert_eq!(
        snapshot_pixels(&app),
        before,
        "Reset animation must NOT move any creature: it is not the full reset"
    );
    assert_eq!(
        sim_tick(&app),
        tick_before,
        "Reset animation must NOT rewind the game clock"
    );
    assert_eq!(
        machine_frame(&app, creature),
        0,
        "Reset animation restarts the state-4 special at frame 0"
    );
}
