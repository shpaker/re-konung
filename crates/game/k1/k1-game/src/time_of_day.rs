//! K1 time-of-day model: the thirteen windows of a 21600-tick cycle.
//!
//! Evidence base: `docs/k1/time-of-day.md`. The cycle is the simulation's own tick
//! modulo 21600; no independent clock exists here.
//!
//! The model reports the ambient triple the original's light routine builds for a
//! cycle tick, ramp branches included, before the division by 100 on the way to the
//! shader. The arithmetic is the original's, read from the binaries — not the clone's
//! float reconstruction: the ramp halves the tick distance as an **integer** (the
//! routine's `sar …,1` / `idiv` idiom) and packs the channels as bytes, so a channel
//! that the clone would show as a half-integer is a whole integer here, and a ramp
//! reaches its end value one tick later than the clone's floats would.
//!
//! What remains in the lighting ticket (#96) is applying the triple: the blend, the
//! tile light, the two option switches and the per-map constant light. The windows
//! whose value is the per-day night palette are reported as the darker palette: the
//! original draws that choice as a coin flip at the cycle start, both palettes are dark,
//! and so the missing flip moves no boundary.
//!
//! #96 adds [`NightPalette`] so callers can select the variant explicitly; the default
//! still returns the deeper one to keep existing callers green.

/// Ticks in one full day/night cycle (confirmed: rus `0x5460` divisor).
pub const CYCLE_TICKS: u64 = 21600;

/// Which of the two confirmed night palettes the coin flip chooses at cycle tick 0.
///
/// Source: the coin flip (`System::rand() % 100 < 90`, Slavik `game.cpp:1768`) when the
/// cycle remainder wraps through zero (rus file offset `0x1943F` / `VA 0x42903F`).
/// Why the game alternates between them stays unknown — see the research page's
/// "Unknown" section; we do not invent a cause such as moonlight or lightning.
///
/// Our divergence from the original: the original rolls once per cycle from an unseeded
/// random source, making it a replay artifact. Ours is a pure function `(day, seed)` so
/// a scrubbed timeline shows the same night regardless of frame rate or replay order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NightPalette {
    /// The darker triple everywhere in the affected windows: `(-60, -70, -70)`.
    Deep,
    /// The lighter version where the green channel lifts to `-50` in plateaus of windows
    /// 9–10, with ramps on entry and exit (`t <= 40`: `-70 + half(t)`, then
    /// `-50 - half(t)`). Both palettes are dark so no boundary shifts.
    Green,
}

/// The twelve non-zero thresholds that delimit thirteen windows within a cycle.
/// Confirmed: contiguous int16 array at `VA 0x45AE0E` (rus), `0x45AE46` (eng),
/// `0x45AE52` (tutorial).
const THRESHOLDS: [u64; 12] = [
    1800, 1830, 3570, 3600, 5400, 8100, 11700, 11880, 11910, 16155, 16200, 18900,
];

/// The `dayTime >= threshold` index at which the original switches the ground-light
/// flag on, and the index of the last window it stays on for (rus `0x4291FF` writes
/// it at the top of the window-6 branch, `0x4294AD` clears it in the last branch).
const GROUND_LIGHT_FIRST_WINDOW: usize = 6;
const GROUND_LIGHT_LAST_WINDOW: usize = 11;

/// Ticks per displayed hour: 21600 / 24 = 900.
const TICKS_PER_HOUR: u64 = 900;
/// Ticks per displayed minute: 900 / 60 = 15.
const TICKS_PER_MINUTE: u64 = 15;
/// Cycle tick 0 reads 06:00 — our display convention, not a fact about the game.
const CLOCK_OFFSET_TICKS: u64 = 6 * TICKS_PER_HOUR;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ambient {
    Neutral,
    Warm,
    Dark,
}

/// Our display reading of the cycle, not a named property of the original.
/// The game stores only the threshold array, the ambient triple and the ground
/// light; `DayPhase` is derived from the two classifications below, so every
/// boundary in it is an interval boundary of the confirmed model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayPhase {
    Night,
    Dusk,
    Dawn,
    Day,
}

/// All time-of-day state derived from a single simulation tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameTime {
    pub day: u64,
    pub cycle_tick: u64,
    pub ambient: Ambient,
    /// The ground tile light of the phase — not the final on-screen state:
    /// whether a pixel is lit also depends on the two option switches and on
    /// the per-map constant light, which the lighting tickets (#96, #97) apply.
    pub ground_window: bool,
    pub phase: DayPhase,
    pub clock_hour: u8,
    pub clock_minute: u8,
    pub cycle_progress_percent: u8,
}

/// Returns the window index (0..12) for a cycle tick.
fn window_index(cycle_tick: u64) -> usize {
    THRESHOLDS.iter().filter(|&&t| cycle_tick >= t).count()
}

/// The distance of a cycle tick from its window's start, `dayTime - threshold[w]`.
fn since_start(cycle_tick: u64, window: usize) -> f32 {
    (cycle_tick - THRESHOLDS[window - 1]) as f32
}

/// The original's ramp halving: the signed `sar …,1` idiom on the routine's stack
/// local, applied to a tick distance that is never negative here.
fn half(t: f32) -> f32 {
    (t as i32 / 2) as f32
}

/// The ambient triple before the division by 100, for a cycle tick.
///
/// Every branch — the ramp condition, the halving and the returned triple — mirrors
/// a branch of the original's light routine as recorded, per build, in
/// `docs/k1/time-of-day.md`. The ramp value `u` is the integer half of the distance
/// from the window's start; the channels that the routine scales by the pool
/// constants (`{0.4, 0.2, 1/3, 10.0, 0.66, -50.0, 0.5988, -60.0}`) are shown as the
/// unrounded product — the original packs the channels as bytes before the shader
/// divides them by 100, and #96 will pin the exact rounding and the two coefficients
/// the pool leaves unresolved (`0.33` and `1/3`).
///
/// The two windows whose value is the per-day night palette are reported as the
/// darker palette; see the module docs.
///
/// The ambient triple before the division by 100, for a cycle tick — using the
/// darker night palette. This is what every existing caller gets and the value
/// tested throughout the module so all existing tests stay green.
pub fn ambient_value(cycle_tick: u64) -> (f32, f32, f32) {
    ambient_value_with(cycle_tick, NightPalette::Deep)
}

/// Variant of [`ambient_value`] that selects a specific night palette.
///
/// When the palette is [`NightPalette::Green`] windows 8–10 (cycle ticks
/// 11880..11910, 11910..16155, 16155..16200) change as follows:
/// - index 8: flat `(-60,-70,-70)` → ramp `t <= 40`:
///   `(-60, -70 + half(t), -70)`, else `(-60,-50,-70)`.
/// - index 9: `(-60,-70,-70)` → `(-60,-50,-70)`.
/// - index 10: flat `(-60,-70,-70)` → ramp `t <= 40`:
///   `(-60, -50 - half(t), -70)`, else `(-60,-70,-70)`.
///   All arithmetic stays the original's integer-halved kind; boundaries do not
///   move because both palettes are dark.
pub fn ambient_value_with(cycle_tick: u64, night: NightPalette) -> (f32, f32, f32) {
    let d = cycle_tick % CYCLE_TICKS;
    let t = |w: usize| since_start(d, w);
    match window_index(d) {
        0 => (0.0, 0.0, 0.0),
        1 if t(1) < 25.0 => {
            let u = half(t(1));
            (u, half(u), 0.0)
        }
        1 | 2 => (12.0, 6.0, 0.0),
        3 if t(3) < 25.0 => {
            let u = half(24.0 - t(3));
            (u, half(u), 0.0)
        }
        3 | 4 => (0.0, 0.0, 0.0),
        5 if t(5) < 101.0 => {
            let u = half(t(5));
            (0.2 * u, -0.4 * u, -u)
        }
        5 => (10.0, -20.0, -50.0),
        6 if t(6) < 61.0 => {
            let u = half(t(6));
            (10.0 - 0.33 * u, -20.0 - u, -50.0 - 0.66 * u)
        }
        6 => (0.0, -50.0, -70.0),
        7 if t(7) < 121.0 => {
            let u = half(t(7));
            (-u, -50.0 - u / 3.0, -70.0)
        }
        7 => (-60.0, -70.0, -70.0),
        // Window 9/10: the night palette. Index 8 is all-ramp. The plateau
        // differs by palette; green also has a down-ramp on its exit (index 10).
        8 if t(8) < 41.0 => (-60.0, -70.0 + half(t(8)), -70.0),
        8 | 9 => {
            if matches!(night, NightPalette::Green) {
                (-60.0, -50.0, -70.0)
            } else {
                (-60.0, -70.0, -70.0)
            }
        }
        10 if t(10) < 41.0 => {
            if matches!(night, NightPalette::Green) {
                (-60.0, -50.0 - half(t(10)), -70.0)
            } else {
                (-60.0, -70.0, -70.0)
            }
        }
        10 => (-60.0, -70.0, -70.0),
        11 if t(11) < 101.0 => {
            let u = half(t(11));
            (0.5988 * u - 60.0, u / 5.0 - 70.0, u - 70.0)
        }
        11 => (-30.0, -60.0, -20.0),
        // The byte-packed branch read off the binaries (rus `0x4294D1`): the
        // channels are `(u/2 - 30, u - 60, u/3 - 20)` with integer divisions.
        12 if t(12) < 121.0 => {
            let u = half(t(12));
            (half(u) - 30.0, u - 60.0, (u as i32 / 3) as f32 - 20.0)
        }
        _ => (0.0, 0.0, 0.0),
    }
}

/// The ambient classification for a cycle tick, from the triple's channels:
/// blue is the channel that darkens without the tile-light cancellation, so its
/// sign decides between Dark and the others.
pub fn ambient_kind(cycle_tick: u64) -> Ambient {
    let (r, g, b) = ambient_value(cycle_tick);
    if b < 0.0 {
        Ambient::Dark
    } else if r == 0.0 && g == 0.0 && b == 0.0 {
        Ambient::Neutral
    } else {
        Ambient::Warm
    }
}

/// Whether the phase's ground tile light is on at this cycle tick: windows 7 to
/// 12, `[8100, 18900)`. The name is deliberate — this is the phase window, not
/// the final on-screen light, which the option switches and the per-map constant
/// light further gate (#96, #97).
pub fn ground_window(cycle_tick: u64) -> bool {
    let window = window_index(cycle_tick % CYCLE_TICKS);
    (GROUND_LIGHT_FIRST_WINDOW..=GROUND_LIGHT_LAST_WINDOW).contains(&window)
}

/// Our four-name reading of the cycle — the original names no phase. The words
/// are labels over the two classifications, and nothing outside a display may
/// use them as a game rule.
pub fn day_phase(cycle_tick: u64) -> DayPhase {
    let d = cycle_tick % CYCLE_TICKS;
    if ground_window(d) {
        DayPhase::Night
    } else if ambient_kind(d) == Ambient::Dark && d < THRESHOLDS[GROUND_LIGHT_FIRST_WINDOW - 1] {
        DayPhase::Dusk
    } else if ambient_kind(d) == Ambient::Dark {
        DayPhase::Dawn
    } else {
        DayPhase::Day
    }
}

/// Clock face as `(hour, minute)`: cycle tick 0 reads 06:00. This is our display
/// convention; no rule reads the hour. The argument is normalized, so a caller
/// may hand in the absolute simulation tick as well as a cycle tick.
pub fn clock_face(cycle_tick: u64) -> (u8, u8) {
    let offset = (cycle_tick % CYCLE_TICKS + CLOCK_OFFSET_TICKS) % CYCLE_TICKS;
    let hour = (offset / TICKS_PER_HOUR) as u8;
    let minute = ((offset % TICKS_PER_HOUR) / TICKS_PER_MINUTE) as u8;
    (hour, minute)
}

/// Computes the full game-time state from the simulation's absolute tick.
pub fn game_time(tick: u64) -> GameTime {
    let day = tick / CYCLE_TICKS;
    let cycle_tick = tick % CYCLE_TICKS;
    let (hour, minute) = clock_face(cycle_tick);
    let percent = ((cycle_tick * 100) / CYCLE_TICKS) as u8;
    GameTime {
        day,
        cycle_tick,
        ambient: ambient_kind(cycle_tick),
        ground_window: ground_window(cycle_tick),
        phase: day_phase(cycle_tick),
        clock_hour: hour,
        clock_minute: minute,
        cycle_progress_percent: percent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The observable interval trace of one cycle: every tick at which
    /// `(ambient_kind, ground_window)` changes, and the state each window starts
    /// in. This — not any phase name — is the contract of the model
    /// (`docs/k1/time-of-day.md`). The ramp ticks come from the original's
    /// integer halving: a ramp leaves the value it comes from on the tick whose
    /// integer half first turns non-zero.
    fn observable_transitions() -> Vec<(u64, Ambient, bool)> {
        let mut changes: Vec<(u64, Ambient, bool)> = Vec::new();
        let mut previous = (ambient_kind(0), ground_window(0));
        for tick in 1..CYCLE_TICKS {
            let now = (ambient_kind(tick), ground_window(tick));
            if now != previous {
                changes.push((tick, now.0, now.1));
                previous = now;
            }
        }
        changes
    }

    #[test]
    fn the_cycle_reports_the_confirmed_interval_trace() {
        assert_eq!(
            observable_transitions(),
            vec![
                (1802, Ambient::Warm, false),
                (3593, Ambient::Neutral, false),
                (5402, Ambient::Dark, false),
                (8100, Ambient::Dark, true),
                (18900, Ambient::Dark, false),
                (19020, Ambient::Neutral, false),
            ]
        );
    }

    #[test]
    fn window_1_is_neutral() {
        assert_eq!(ambient_kind(0), Ambient::Neutral);
        assert_eq!(ambient_kind(1799), Ambient::Neutral);
        assert!(!ground_window(0));
    }

    /// The ramp halves the distance as an integer, so its first two ticks still
    /// carry the neutral `(0, 0, 0)` the ramp leaves: the picture turns warm at
    /// 1802, where `u = 1`.
    #[test]
    fn window_2_ramps_from_neutral_to_warm() {
        assert_eq!(ambient_value(1800), (0.0, 0.0, 0.0));
        assert_eq!(ambient_value(1801), (0.0, 0.0, 0.0));
        assert_eq!(ambient_kind(1801), Ambient::Neutral);
        assert_eq!(ambient_value(1802), (1.0, 0.0, 0.0));
        assert_eq!(ambient_kind(1802), Ambient::Warm);
        assert_eq!(ambient_value(1824), (12.0, 6.0, 0.0));
        assert_eq!(ambient_kind(1829), Ambient::Warm);
        assert!(!ground_window(1800));
    }

    #[test]
    fn window_3_is_the_warm_plateau() {
        assert_eq!(ambient_kind(1830), Ambient::Warm);
        assert_eq!(ambient_value(3569), (12.0, 6.0, 0.0));
    }

    #[test]
    fn window_4_ramps_warm_to_neutral() {
        assert_eq!(ambient_kind(3570), Ambient::Warm);
        assert_eq!(ambient_value(3592), (1.0, 0.0, 0.0));
        assert_eq!(ambient_value(3593), (0.0, 0.0, 0.0));
        assert_eq!(ambient_kind(3593), Ambient::Neutral);
        assert_eq!(ambient_kind(3599), Ambient::Neutral);
    }

    #[test]
    fn window_5_is_neutral() {
        assert_eq!(ambient_kind(3600), Ambient::Neutral);
        assert_eq!(ambient_kind(5399), Ambient::Neutral);
    }

    #[test]
    fn window_6_ramps_neutral_to_dark() {
        assert_eq!(ambient_value(5400), (0.0, 0.0, 0.0));
        assert_eq!(ambient_kind(5400), Ambient::Neutral);
        assert_eq!(ambient_value(5401), (0.0, 0.0, 0.0));
        assert_eq!(ambient_kind(5401), Ambient::Neutral);
        assert_eq!(ambient_value(5402), (0.2, -0.4, -1.0));
        assert_eq!(ambient_kind(5402), Ambient::Dark);
        assert_eq!(ambient_value(5501), (10.0, -20.0, -50.0));
        assert_eq!(ambient_kind(8099), Ambient::Dark);
        assert!(!ground_window(5400));
    }

    #[test]
    fn window_7_is_dark_with_the_ground_window_open() {
        assert_eq!(ambient_value(8100), (10.0, -20.0, -50.0));
        assert_eq!(ambient_value(8161), (0.0, -50.0, -70.0));
        assert!(ground_window(8100));
        assert_eq!(ambient_kind(11699), Ambient::Dark);
    }

    #[test]
    fn window_8_ramps_deeper_and_stays_dark() {
        assert_eq!(ambient_value(11700), (0.0, -50.0, -70.0));
        assert_eq!(ambient_value(11821), (-60.0, -70.0, -70.0));
        assert_eq!(ambient_kind(11700), Ambient::Dark);
        assert_eq!(ambient_kind(11879), Ambient::Dark);
        assert!(ground_window(11700));
    }

    /// Windows 9 to 11 hold one of the two night palettes, chosen by the per-day
    /// coin flip that #96 will draw. Both palettes are dark, so no boundary of
    /// this model can depend on the choice.
    #[test]
    fn the_night_palette_windows_are_dark_whatever_the_palette() {
        for tick in [11880, 11900, 11909, 11910, 16154, 16155, 16175, 16199] {
            assert_eq!(ambient_kind(tick), Ambient::Dark, "at {tick}");
            assert!(ground_window(tick), "at {tick}");
        }
        // The window-9 and window-11 ramps connect the two palettes through
        // dark values only: the green channel travels between -70 and -50.
        for tick in 11880..11910 {
            let (r, g, b) = ambient_value(tick);
            assert_eq!((r, b), (-60.0, -70.0), "at {tick}");
            assert!((-70.0..=-50.0).contains(&g), "at {tick}: {g}");
        }
    }

    #[test]
    fn window_12_ramps_up_to_the_last_dark_plateau() {
        assert_eq!(ambient_value(16200), (-60.0, -70.0, -70.0));
        assert_eq!(ambient_value(16301), (-30.0, -60.0, -20.0));
        assert!(ground_window(16200));
        assert!(ground_window(18899));
    }

    /// The ground window closes while the picture is still dark: the byte-packed
    /// ramp branch (rus `0x4294D1`) is `(u/2 - 30, u - 60, u/3 - 20)` with integer
    /// divisions, so it reaches `(0, 0, 0)` at t = 120 — tick 19020 — while the
    /// branch itself runs to t = 120 and only the branch after it returns the
    /// plateau.
    #[test]
    fn window_13_closes_the_ground_window_then_reaches_neutral() {
        assert_eq!(ambient_value(18900), (-30.0, -60.0, -20.0));
        assert_eq!(ambient_kind(18900), Ambient::Dark);
        assert!(!ground_window(18900));
        assert_eq!(ambient_value(19018), (-1.0, -1.0, -1.0));
        assert_eq!(ambient_value(19019), (-1.0, -1.0, -1.0));
        assert_eq!(ambient_kind(19019), Ambient::Dark);
        assert_eq!(ambient_value(19020), (0.0, 0.0, 0.0));
        assert_eq!(ambient_kind(19020), Ambient::Neutral);
        assert_eq!(ambient_kind(21599), Ambient::Neutral);
    }

    #[test]
    fn the_ground_window_opens_at_8100_and_closes_at_18900() {
        assert!(!ground_window(8099));
        assert!(ground_window(8100));
        assert!(ground_window(18899));
        assert!(!ground_window(18900));
    }

    /// Every threshold opens a new window of the table, even where the reported
    /// state does not change: the thresholds that move no boundary are ramp
    /// starts, and their ramps are what the windows are for.
    #[test]
    fn every_threshold_starts_a_new_window() {
        for (index, &threshold) in THRESHOLDS.iter().enumerate() {
            assert_eq!(window_index(threshold), index + 1, "at {threshold}");
            assert_eq!(window_index(threshold - 1), index, "below {threshold}");
        }
    }

    /// The twelve windows whose length outlives their ramp condition hold a
    /// plateau — a run of ticks on the recorded triple. The thirteenth, window
    /// 9, is the exception (see `window_9_is_all_ramp_and_reaches_no_plateau`),
    /// so together the two tests cover all thirteen windows. Each checked tick
    /// is re-derived back to its window, so a tick that drifted into a
    /// neighbouring window cannot pass.
    #[test]
    fn each_window_with_a_plateau_holds_the_recorded_triple() {
        let checked: [(usize, u64, (f32, f32, f32)); 12] = [
            (0, 0, (0.0, 0.0, 0.0)),
            (1, 1825, (12.0, 6.0, 0.0)),
            (2, 1830, (12.0, 6.0, 0.0)),
            (3, 3595, (0.0, 0.0, 0.0)),
            (4, 3600, (0.0, 0.0, 0.0)),
            (5, 5501, (10.0, -20.0, -50.0)),
            (6, 8161, (0.0, -50.0, -70.0)),
            (7, 11821, (-60.0, -70.0, -70.0)),
            (9, 11910, (-60.0, -70.0, -70.0)),
            (10, 16196, (-60.0, -70.0, -70.0)),
            (11, 16301, (-30.0, -60.0, -20.0)),
            (12, 19021, (0.0, 0.0, 0.0)),
        ];
        for (window, tick, want) in checked {
            assert_eq!(
                window_index(tick),
                window,
                "tick {tick} is not in window {window}"
            );
            assert_eq!(ambient_value(tick), want, "plateau of window {window}");
        }
    }

    /// Window 9 is thirty ticks long and its ramp condition is `t <= 40`, so
    /// the whole window is the ramp: it leaves the previous plateau rising and
    /// the window ends mid-climb — there is no run of ticks at a settled
    /// triple. The flat `(-60, -70, -70)` belongs to window 10, not here, even
    /// though the ramp's first two ticks carry the same value on their way out
    /// of window 8.
    #[test]
    fn window_9_is_all_ramp_and_reaches_no_plateau() {
        for tick in 11880..11910 {
            let t = (tick - 11880) as f32;
            assert_eq!(window_index(tick), 8, "at {tick}");
            assert_eq!(
                ambient_value(tick),
                (-60.0, -70.0 + half(t), -70.0),
                "at {tick}: still on the ramp"
            );
        }
        // The climb peaks at the window's last tick fourteen units above the
        // triple it left with, and turns back down there — it settles nowhere.
        assert_eq!(ambient_value(11909), (-60.0, -56.0, -70.0));
    }

    #[test]
    fn clock_face_at_known_points() {
        assert_eq!(clock_face(0), (6, 0));
        assert_eq!(clock_face(900), (7, 0));
        assert_eq!(clock_face(15), (6, 1));
        assert_eq!(clock_face(21600), (6, 0));
    }

    /// The clock face normalizes its argument before shifting it, so the
    /// addition cannot overflow on any u64 a caller might hand in.
    #[test]
    fn clock_face_normalizes_a_non_cycle_argument() {
        assert_eq!(clock_face(CYCLE_TICKS - 1), clock_face(2 * CYCLE_TICKS - 1));
        assert_eq!(
            clock_face(u64::MAX),
            clock_face(u64::MAX % CYCLE_TICKS),
            "an unnormalized tick must not overflow"
        );
    }

    #[test]
    fn day_phase_labels_the_intervals() {
        assert_eq!(day_phase(900), DayPhase::Day);
        // The ramp's first two ticks are still neutral under the integer
        // halving, so Dusk opens at 5402.
        assert_eq!(day_phase(5401), DayPhase::Day);
        assert_eq!(day_phase(5402), DayPhase::Dusk);
        assert_eq!(day_phase(8099), DayPhase::Dusk);
        assert_eq!(day_phase(8100), DayPhase::Night);
        assert_eq!(day_phase(18899), DayPhase::Night);
        assert_eq!(day_phase(18900), DayPhase::Dawn);
        assert_eq!(day_phase(19019), DayPhase::Dawn);
        assert_eq!(day_phase(19020), DayPhase::Day);
    }

    /// The phase word is a label over the two confirmed classifications, so it
    /// may not cut the cycle anywhere the intervals do not: if two consecutive
    /// ticks observe the same state, they must read the same word. Which words
    /// the cycle spells is not asserted here — the original names no phase, and
    /// `docs/k1/time-of-day.md` refuses the four-name trace as a property of the
    /// game; `day_phase_labels_the_intervals` pins the words at a few cycle
    /// points as the display rule they are.
    #[test]
    fn the_phase_word_adds_no_boundary_of_its_own() {
        // (The map from state to word is not injective — `(Dark, false)` is
        // Dusk on one side of the ground window and Dawn on the other, which
        // is the cycle position speaking, not the state.)
        for tick in 1..CYCLE_TICKS {
            if ambient_kind(tick) == ambient_kind(tick - 1)
                && ground_window(tick) == ground_window(tick - 1)
            {
                assert_eq!(
                    day_phase(tick),
                    day_phase(tick - 1),
                    "tick {tick} changes the word without an interval change"
                );
            }
        }
    }

    #[test]
    fn game_time_at_tick_zero() {
        let gt = game_time(0);
        assert_eq!(gt.day, 0);
        assert_eq!(gt.cycle_tick, 0);
        assert_eq!(gt.ambient, Ambient::Neutral);
        assert!(!gt.ground_window);
        assert_eq!(gt.phase, DayPhase::Day);
        assert_eq!(gt.clock_hour, 6);
        assert_eq!(gt.clock_minute, 0);
        assert_eq!(gt.cycle_progress_percent, 0);
    }

    #[test]
    fn game_time_crosses_into_day_two() {
        let gt = game_time(21600);
        assert_eq!(gt.day, 1);
        assert_eq!(gt.cycle_tick, 0);
        assert_eq!(gt.phase, DayPhase::Day);
        assert_eq!(gt.clock_hour, 6);
    }

    #[test]
    fn window_index_at_each_threshold() {
        assert_eq!(window_index(0), 0);
        assert_eq!(window_index(1800), 1);
        assert_eq!(window_index(1830), 2);
        assert_eq!(window_index(3570), 3);
        assert_eq!(window_index(3600), 4);
        assert_eq!(window_index(5400), 5);
        assert_eq!(window_index(8100), 6);
        assert_eq!(window_index(11700), 7);
        assert_eq!(window_index(11880), 8);
        assert_eq!(window_index(11910), 9);
        assert_eq!(window_index(16155), 10);
        assert_eq!(window_index(16200), 11);
        assert_eq!(window_index(18900), 12);
        assert_eq!(window_index(21599), 12);
    }
}
