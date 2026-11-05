//! Per-creature animation state machine for K1.
//!
//! The machine owns no clock: the caller hands it ticks one by one (the
//! viewer's scene clock, the player's tick, a headless test's counter). That
//! keeps creature state transitions independent of the render frame rate and
//! lets the future simulation tick of #38 drive the same seam.
//!
//! K1 gives a dynamic creature three recorded animation states in a new game:
//! 0, 4 and 6 (docs/k1/creatures.md, "Our measurement"). State 4 for the three
//! classes 0x46, 0x4c and 0x55 is the creature's own appearance or
//! transformation — a skeleton rising from the ground, an Eechetik jumping out,
//! Ion the werewolf transforming — and per Slavik
//! [game_play.cpp:3607-3618](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3607-L3618)
//! it is a one-shot that ends in the idle chooser, not a loop. The frame
//! counter itself clamps and reports finished
//! ([game_play.cpp:3727-3735](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3727-L3735)),
//! so there is no modulo over a shared clock: the machine carries a per-creature
//! frame counter and hands the caller an explicit transition into idle.
//!
//! Our divergence, recorded in docs/k1/creatures.md: the original holds state-4
//! frame 0 until the monster decision's state-4 branch releases it
//! ([game_play.cpp:3226-3232](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3226-L3232)),
//! but the AI decision slot that runs that release does not exist here yet
//! (#38). Until it does, the temporary trigger is the explicit "the creature
//! entered the world" event — [Machine::enter_world] — so the special starts
//! on map load and cannot deadlock waiting for a trigger that never arrives.

use std::collections::HashMap;

/// The animation state K1 gives a dynamic creature's own appearance or
/// transformation (docs/k1/creatures.md, "State 4 — two meanings under one
/// number"). The only non-idle state the shipped `Game.N` files record.
pub const SPECIAL_STATE: u8 = 4;

/// Where one creature is in the state machine right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Playing the state-4 special. `frame` counts ticks since the sequence
    /// began, in `0 .. special_frames`; the last tick shows the last frame.
    Special { frame: u32 },
    /// Standing in the idle state (the caller overlays walk on top of this
    /// from the simulation's `WalkState` when the creature is moving).
    Idle,
}

impl Stage {
    /// True while the state-4 sequence is running.
    pub fn is_special(self) -> bool {
        matches!(self, Stage::Special { .. })
    }
}

/// Seed for [`Machine::enter_world`]: one creature as the record describes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seed {
    /// The stable identifier — `Creature::id`, the character record slot.
    pub id: usize,
    /// Byte 0x00 of the character record: the state the file recorded.
    pub recorded_state: u8,
    /// Frames of the state-4 sequence at the creature's direction, or `0` when
    /// the record has no such sequence (a human, or a dynamic creature whose
    /// sprite set carries no state-4 row): such a creature is Idle from the
    /// start and never plays a special.
    pub special_frames: u32,
}

/// One creature's slot inside a [`Machine`].
#[derive(Debug, Clone, Copy)]
struct Entry {
    /// The state the file recorded (0, 4 or 6 in the shipped data).
    recorded_state: u8,
    /// Frames of the state-4 sequence, or 0 when there is none.
    special_frames: u32,
    /// Current stage.
    stage: Stage,
    /// Stage as last reported by [`Machine::drain_changed`]; starts at Idle so
    /// a fresh Special is reported on the first drain.
    reported: Stage,
}

/// The machine over a map's roster: one entry per creature the caller seeded,
/// keyed by `Creature::id`.
///
/// [`Machine::tick`] advances every entry by one tick. A Special whose frame
/// counter reaches its sequence length becomes Idle; that transition is the
/// only event the render layer reacts to with a pose rebuild, so it is the
/// only transition counted (issue #70 AC6).
#[derive(Debug, Default)]
pub struct Machine {
    entries: Vec<Entry>,
    /// Maps `Creature::id` (a character record slot, sparse) into `entries`.
    index: HashMap<usize, usize>,
    /// Number of stage changes since the machine was created. Ordinary frame
    /// advances do not count. Reset on every [`Machine::enter_world`].
    pose_rebuilds: u64,
}

impl Machine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Explicit "the creature entered the world" event. Idempotent per id:
    /// calling it again with the same set re-seeds those creatures to the
    /// state the file describes and counts one pose rebuild per creature whose
    /// stage actually changed. A state-4 seed starts at `Special { frame: 0 }`;
    /// every other seed starts at Idle.
    ///
    /// The seeds are the file's records, so this is also the reset the viewer's
    /// `Play → Off → Play` path needs: recreate the machine and reseed from the
    /// file. The clock is the caller's; the machine only tracks stages.
    pub fn enter_world(&mut self, seeds: impl IntoIterator<Item = Seed>) {
        for seed in seeds {
            let stage = initial_stage(&seed);
            let changed = match self.index.get(&seed.id).copied() {
                Some(position) => {
                    let entry = &mut self.entries[position];
                    // Only a stage-kind change (Special ↔ Idle) means the
                    // render has to swap clips; re-seeding inside the same
                    // kind is a plain re-anchor, not a rebuild.
                    let changed = entry.stage.is_special() != stage.is_special();
                    entry.recorded_state = seed.recorded_state;
                    entry.special_frames = seed.special_frames;
                    entry.stage = stage;
                    changed
                }
                None => {
                    self.index.insert(seed.id, self.entries.len());
                    self.entries.push(Entry {
                        recorded_state: seed.recorded_state,
                        special_frames: seed.special_frames,
                        stage,
                        reported: Stage::Idle,
                    });
                    // A freshly seeded state-4 creature had no stage before, so
                    // "changed" here means "will be drawn by the special clip
                    // rather than the resting clip" — the same rebuild event.
                    stage.is_special()
                }
            };
            if changed {
                self.pose_rebuilds += 1;
            }
        }
    }

    /// Advance every entry by one tick. A Special that reaches its last frame
    /// (its counter equals `special_frames`) transitions to Idle and counts one
    /// pose rebuild; every other tick is a plain frame advance and counts
    /// nothing.
    pub fn tick(&mut self) {
        for entry in &mut self.entries {
            if let Stage::Special { frame } = entry.stage {
                let next = frame + 1;
                if next >= entry.special_frames {
                    entry.stage = Stage::Idle;
                    self.pose_rebuilds += 1;
                } else {
                    entry.stage = Stage::Special { frame: next };
                }
            }
        }
    }

    /// Restart one creature's special from frame 0. This is the seam a game
    /// event drives (issue #70 AC3): a new appearance is allowed only through
    /// here, never by sequence exhaustion — the machine never re-enters
    /// Special on its own.
    ///
    /// `None` when the id is unknown or its sprite has no state-4 sequence,
    /// because neither has anything to play; the caller reports the miss
    /// rather than treating it as a successful restart.
    pub fn restart_special(&mut self, id: usize) -> bool {
        let Some(&position) = self.index.get(&id) else {
            return false;
        };
        let entry = &mut self.entries[position];
        if entry.special_frames == 0 {
            return false;
        }
        let previous = entry.stage;
        entry.stage = Stage::Special { frame: 0 };
        if previous.is_special() != entry.stage.is_special() {
            self.pose_rebuilds += 1;
        }
        true
    }

    /// The stage of one creature; `None` when the machine has never seeded it.
    pub fn stage(&self, id: usize) -> Option<Stage> {
        self.index
            .get(&id)
            .map(|&position| self.entries[position].stage)
    }

    /// Which animation state a renderer should show right now: the special
    /// state while a creature plays it, and the recorded state otherwise. The
    /// record's own state is what the file says, so a state-4 creature that
    /// finished its special continues in the state its record carries next to
    /// `state 4` (docs/k1/creatures.md — a dynamic creature's `state % 6` is
    /// idle). Callers wanting the pair without the weapon-in-hands bit pass
    /// [`Machine::idle_state`].
    pub fn shown_state(&self, id: usize) -> Option<u8> {
        let &position = self.index.get(&id)?;
        let entry = &self.entries[position];
        Some(match entry.stage {
            Stage::Special { .. } => SPECIAL_STATE,
            Stage::Idle => idle_state(entry.recorded_state),
        })
    }

    /// The idle state the record's pair describes, with the special resolved.
    /// For a dynamic creature, 4 and 6 both yield the idle animation row
    /// (`sprites_for` collapses 6–9 to 0–3, docs/k1/creatures.md); we settle
    /// on 6 so a multi-sprite dynamic creature keeps the same weapon-in-hands
    /// bit it carries in the file next to state 4.
    pub fn idle_state(&self, id: usize) -> Option<u8> {
        self.index
            .get(&id)
            .map(|&position| idle_state(self.entries[position].recorded_state))
    }

    /// Direction the record carries. The machine never changes direction on
    /// its own — that stays with the simulation and the rules.
    pub fn recorded_state(&self, id: usize) -> Option<u8> {
        self.index
            .get(&id)
            .map(|&position| self.entries[position].recorded_state)
    }

    /// Frames of the creature's state-4 sequence, or 0 when it has none.
    pub fn special_frames(&self, id: usize) -> Option<u32> {
        self.index
            .get(&id)
            .map(|&position| self.entries[position].special_frames)
    }

    /// Counted pose rebuilds since the machine was created: one per creature
    /// whose stage actually changed. Proves the AC6 bound of issue #70 — an
    /// ordinary tick must not touch a pose.
    pub fn pose_rebuilds(&self) -> u64 {
        self.pose_rebuilds
    }

    /// Creatures currently in Special, keyed by `Creature::id`.
    pub fn in_special(&self) -> impl Iterator<Item = (usize, u32)> + '_ {
        self.index
            .iter()
            .filter_map(|(&id, &position)| match self.entries[position].stage {
                Stage::Special { frame } => Some((id, frame)),
                Stage::Idle => None,
            })
    }

    /// Every seeded id, in the order they were first seen.
    pub fn ids(&self) -> impl Iterator<Item = usize> + '_ {
        self.index.keys().copied()
    }

    /// Creatures whose stage kind (Special vs Idle) differs from what was last
    /// reported, and updates the tracker. The viewer uses this to rebuild a
    /// pose at most once per creature per stage change (issue #70 AC6); a
    /// frame advance within a running Special must not appear here.
    ///
    /// On the first call after [`Machine::enter_world`], returns the creatures
    /// that entered Special at world load (their stage is Special, and the
    /// last-reported stage — from before — was "nothing"). Ids are sorted so
    /// a caller can compare a deterministic list.
    pub fn drain_changed(&mut self) -> Vec<usize> {
        let mut changed: Vec<usize> = self
            .index
            .iter()
            .filter_map(|(&id, &position)| {
                let entry = &self.entries[position];
                (entry.stage.is_special() != entry.reported.is_special()).then_some(id)
            })
            .collect();
        changed.sort_unstable();
        for &id in &changed {
            let position = self.index[&id];
            self.entries[position].reported = self.entries[position].stage;
        }
        changed
    }
}

/// The state a dynamic creature shows when idle: the record's state collapsed
/// by the pair rule, so a state-4 record falls back to state 6 — the idle row
/// without the weapon-in-hands bit.
fn idle_state(recorded_state: u8) -> u8 {
    if recorded_state == SPECIAL_STATE {
        6
    } else {
        recorded_state
    }
}

/// Initial stage of one seed: Special at frame 0 for a state-4 creature that
/// has a sequence, Idle otherwise.
fn initial_stage(seed: &Seed) -> Stage {
    if seed.recorded_state == SPECIAL_STATE && seed.special_frames > 0 {
        Stage::Special { frame: 0 }
    } else {
        Stage::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(id: usize, recorded_state: u8, special_frames: u32) -> Seed {
        Seed {
            id,
            recorded_state,
            special_frames,
        }
    }

    #[test]
    fn a_state_four_creature_enters_special_at_frame_zero() {
        let mut machine = Machine::new();
        machine.enter_world([seed(1, 4, 12)]);
        assert_eq!(machine.stage(1), Some(Stage::Special { frame: 0 }));
        assert_eq!(machine.shown_state(1), Some(SPECIAL_STATE));
        // The rebuild the render will pay on the next frame's swap.
        assert_eq!(machine.pose_rebuilds(), 1);
    }

    #[test]
    fn an_idle_record_never_enters_special() {
        let mut machine = Machine::new();
        machine.enter_world([seed(1, 6, 12), seed(2, 0, 12), seed(3, 4, 0)]);
        for id in [1, 2, 3] {
            assert_eq!(machine.stage(id), Some(Stage::Idle), "id {id}");
        }
        assert_eq!(machine.shown_state(1), Some(6));
        assert_eq!(machine.shown_state(2), Some(0));
        // A state-4 record whose sprite has no state-4 row is Idle from the
        // start: it has nothing to play.
        assert_eq!(machine.shown_state(3), Some(6));
        assert_eq!(machine.pose_rebuilds(), 0);
    }

    #[test]
    fn special_plays_every_frame_then_lands_in_idle_once() {
        let mut machine = Machine::new();
        machine.enter_world([seed(7, 4, 5)]);
        for frame in 0..5 {
            assert_eq!(machine.stage(7), Some(Stage::Special { frame }));
            machine.tick();
        }
        // Frame 5 was never shown: the last tick of the sequence showed frame
        // 4 and the next tick entered Idle.
        assert_eq!(machine.stage(7), Some(Stage::Idle));
        assert_eq!(machine.shown_state(7), Some(6));
    }

    #[test]
    fn idle_does_not_advance_no_matter_how_many_ticks() {
        let mut machine = Machine::new();
        machine.enter_world([seed(9, 0, 0)]);
        let before = machine.pose_rebuilds();
        for _ in 0..1_000 {
            machine.tick();
        }
        assert_eq!(machine.stage(9), Some(Stage::Idle));
        assert_eq!(machine.pose_rebuilds(), before);
    }

    #[test]
    fn restart_special_replays_the_sequence_from_zero() {
        let mut machine = Machine::new();
        machine.enter_world([seed(1, 4, 3)]);
        machine.tick(); // frame 0 → 1
        machine.tick(); // frame 1 → 2
        machine.tick(); // frame 2 → 3, 3 ≥ 3 → Idle
        assert_eq!(machine.stage(1), Some(Stage::Idle));
        assert!(machine.restart_special(1));
        assert_eq!(machine.stage(1), Some(Stage::Special { frame: 0 }));
        machine.tick();
        assert_eq!(machine.stage(1), Some(Stage::Special { frame: 1 }));
    }

    #[test]
    fn restart_special_never_reenters_special_from_idle_on_its_own() {
        // Sequence exhaustion is not an event; the machine does not re-enter
        // without a call to restart_special.
        let mut machine = Machine::new();
        machine.enter_world([seed(1, 4, 4)]);
        for _ in 0..4 {
            machine.tick();
        }
        for _ in 0..1_000 {
            machine.tick();
        }
        assert_eq!(machine.stage(1), Some(Stage::Idle));
    }

    #[test]
    fn restart_special_reports_no_for_creatures_without_a_sequence() {
        let mut machine = Machine::new();
        machine.enter_world([seed(1, 4, 0), seed(2, 0, 4)]);
        // id 3 was never seeded.
        assert!(!machine.restart_special(1), "no state-4 sequence");
        // id 2 was not a state-4 record — it can still be re-triggered if the
        // caller asks, but it does not count as a fresh Special on restart.
        assert!(machine.restart_special(2), "restart of an idle record");
        assert!(!machine.restart_special(3), "unknown id");
    }

    #[test]
    fn two_creatures_run_independently() {
        let mut machine = Machine::new();
        machine.enter_world([seed(1, 4, 3), seed(2, 4, 3)]);
        machine.tick();
        assert_eq!(machine.stage(1), Some(Stage::Special { frame: 1 }));
        assert_eq!(machine.stage(2), Some(Stage::Special { frame: 1 }));
        machine.restart_special(1);
        assert_eq!(
            machine.stage(1),
            Some(Stage::Special { frame: 0 }),
            "restart hit only the targeted creature"
        );
        assert_eq!(
            machine.stage(2),
            Some(Stage::Special { frame: 1 }),
            "the other creature is untouched"
        );
    }

    #[test]
    fn a_reseed_with_the_same_stage_does_not_count_a_rebuild() {
        let mut machine = Machine::new();
        machine.enter_world([seed(1, 6, 0)]);
        let before = machine.pose_rebuilds();
        machine.enter_world([seed(1, 6, 0)]);
        assert_eq!(machine.pose_rebuilds(), before);
    }

    #[test]
    fn a_reseed_after_the_special_ended_counts_one_rebuild() {
        // Off → Play on a state-4 creature: the fresh seed brings the stage
        // from Idle back to Special, which the render pays as one rebuild.
        let mut machine = Machine::new();
        machine.enter_world([seed(1, 4, 2)]);
        machine.tick();
        machine.tick();
        assert_eq!(machine.stage(1), Some(Stage::Idle));
        let before = machine.pose_rebuilds();
        machine.enter_world([seed(1, 4, 2)]);
        assert_eq!(machine.pose_rebuilds(), before + 1);
    }

    #[test]
    fn drain_changed_reports_each_stage_change_exactly_once() {
        let mut machine = Machine::new();
        machine.enter_world([seed(1, 4, 3), seed(2, 0, 0)]);
        // Special { frame: 0 } differs from the default (Idle reported), so
        // only the state-4 creature is reported.
        assert_eq!(machine.drain_changed(), vec![1]);
        assert!(machine.drain_changed().is_empty(), "nothing else changed");
        machine.tick();
        // Same stage still, only its frame advanced — the render must not pay.
        assert!(machine.drain_changed().is_empty());
        machine.tick();
        machine.tick(); // this tick lands in Idle
        assert_eq!(machine.drain_changed(), vec![1], "the Special → Idle edge");
        assert!(machine.drain_changed().is_empty());
    }

    #[test]
    fn a_tick_after_the_special_lands_in_the_record_pair_for_idle() {
        // A state-4 record's pair without the weapon-in-hands bit is state 6
        // (docs/k1/creatures.md), and that is what the machine reports.
        let mut machine = Machine::new();
        machine.enter_world([seed(1, 4, 1)]);
        assert_eq!(machine.shown_state(1), Some(SPECIAL_STATE));
        machine.tick();
        assert_eq!(machine.shown_state(1), Some(6));
        assert_eq!(machine.idle_state(1), Some(6));
    }
}
