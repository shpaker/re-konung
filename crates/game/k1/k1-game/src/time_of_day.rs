//! K1 time-of-day model: the thirteen windows of a 21600-tick cycle.
//!
//! Evidence base: `docs/k1/time-of-day.md`. The cycle is the simulation's own tick
//! modulo 21600; no independent clock exists here.
//!
//! Plateau ambient values are modelled; per-tick interpolation within ramp windows
//! belongs to a later ticket (#96).

/// Ticks in one full day/night cycle (confirmed: rus `0x5460` divisor).
pub const CYCLE_TICKS: u64 = 21600;

/// The twelve thresholds that delimit thirteen windows within a cycle.
/// Confirmed: contiguous int16 array at `VA 0x45AE0E` (rus), `0x45AE46` (eng).
const THRESHOLDS: [u64; 12] = [
    1800, 1830, 3570, 3600, 5400, 8100, 11700, 11880, 11910, 16155, 16200, 18900,
];

/// Plateau ambient `(r, g, b)` per window, before the /100 the shader applies.
const AMBIENT_PLATEAU: [(i32, i32, i32); 13] = [
    (0, 0, 0),       // 1: [0, 1800)
    (12, 6, 0),      // 2: [1800, 1830)
    (12, 6, 0),      // 3: [1830, 3570)
    (0, 0, 0),       // 4: [3570, 3600)
    (0, 0, 0),       // 5: [3600, 5400)
    (10, -20, -50),  // 6: [5400, 8100)
    (0, -50, -70),   // 7: [8100, 11700)
    (-60, -70, -70), // 8: [11700, 11880)
    (-60, -50, -70), // 9: [11880, 11910)
    (-60, -50, -70), // 10: [11910, 16155)
    (-60, -70, -70), // 11: [16155, 16200)
    (-30, -60, -20), // 12: [16200, 18900)
    (0, 0, 0),       // 13: [19021, 21600) — plateau; [18900, 19021) is the ramp back
];

/// Cycle tick where window 13's ramp ends and its neutral plateau begins.
const WINDOW_13_NEUTRAL_START: u64 = 19021;

/// Ground light (the original's `_bLightGround`) is on exactly on `[8100, 18900)`.
const GROUND_LIGHT_START: u64 = 8100;
const GROUND_LIGHT_END: u64 = 18900;

/// Ticks per displayed hour: 21600 / 24 = 900.
const TICKS_PER_HOUR: u64 = 900;
/// Ticks per displayed minute: 900 / 60 = 15.
const TICKS_PER_MINUTE: u64 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ambient {
    Neutral,
    Warm,
    Dark,
}

/// Our display reading of the cycle, not a named property of the original.
/// The game only stores the threshold array, the ambient triple, and `_bLightGround`.
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
    pub ground_lit: bool,
    pub phase: DayPhase,
    pub clock_hour: u8,
    pub clock_minute: u8,
    pub cycle_progress_percent: u8,
}

/// Returns the window index (0..12) for a cycle tick.
fn window_index(cycle_tick: u64) -> usize {
    THRESHOLDS.iter().filter(|&&t| cycle_tick >= t).count()
}

/// The ambient classification for a cycle tick, derived from the plateau triple.
///
/// Blue is the channel that darkens without the tile-light cancellation, so its
/// sign decides between Dark and the others.
pub fn ambient_kind(cycle_tick: u64) -> Ambient {
    let w = window_index(cycle_tick);
    let (r, g, b) = ambient_for_window(w, cycle_tick);
    if b < 0 {
        Ambient::Dark
    } else if r == 0 && g == 0 && b == 0 {
        Ambient::Neutral
    } else {
        Ambient::Warm
    }
}

/// Whether the ground tile-light is active at this cycle tick.
pub fn ground_lit(cycle_tick: u64) -> bool {
    (GROUND_LIGHT_START..GROUND_LIGHT_END).contains(&cycle_tick)
}

/// Our four-name reading of the cycle — the original names no phase.
pub fn day_phase(cycle_tick: u64) -> DayPhase {
    if ground_lit(cycle_tick) {
        DayPhase::Night
    } else if ambient_kind(cycle_tick) == Ambient::Dark && cycle_tick < GROUND_LIGHT_START {
        DayPhase::Dusk
    } else if ambient_kind(cycle_tick) == Ambient::Dark {
        DayPhase::Dawn
    } else {
        DayPhase::Day
    }
}

/// Clock face as `(hour, minute)`: cycle tick 0 reads 06:00.
/// This is our display convention; no rule reads the hour.
pub fn clock_face(cycle_tick: u64) -> (u8, u8) {
    let offset = (cycle_tick + 6 * TICKS_PER_HOUR) % CYCLE_TICKS;
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
        ground_lit: ground_lit(cycle_tick),
        phase: day_phase(cycle_tick),
        clock_hour: hour,
        clock_minute: minute,
        cycle_progress_percent: percent,
    }
}

/// Returns the effective ambient triple, accounting for window 13's ramp
/// (ticks [18900, 19021) still carry the previous window's dark value).
fn ambient_for_window(window: usize, cycle_tick: u64) -> (i32, i32, i32) {
    if window == 12 && cycle_tick < WINDOW_13_NEUTRAL_START {
        return AMBIENT_PLATEAU[11];
    }
    AMBIENT_PLATEAU[window]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_1_is_neutral() {
        assert_eq!(ambient_kind(0), Ambient::Neutral);
        assert_eq!(ambient_kind(1799), Ambient::Neutral);
        assert!(!ground_lit(0));
    }

    #[test]
    fn window_2_is_warm() {
        assert_eq!(ambient_kind(1800), Ambient::Warm);
        assert_eq!(ambient_kind(1829), Ambient::Warm);
        assert!(!ground_lit(1800));
    }

    #[test]
    fn window_3_is_warm() {
        assert_eq!(ambient_kind(1830), Ambient::Warm);
        assert_eq!(ambient_kind(3569), Ambient::Warm);
    }

    #[test]
    fn window_4_is_neutral() {
        assert_eq!(ambient_kind(3570), Ambient::Neutral);
        assert_eq!(ambient_kind(3599), Ambient::Neutral);
    }

    #[test]
    fn window_5_is_neutral() {
        assert_eq!(ambient_kind(3600), Ambient::Neutral);
        assert_eq!(ambient_kind(5399), Ambient::Neutral);
    }

    #[test]
    fn window_6_is_dark() {
        assert_eq!(ambient_kind(5400), Ambient::Dark);
        assert_eq!(ambient_kind(8099), Ambient::Dark);
        assert!(!ground_lit(5400));
    }

    #[test]
    fn window_7_is_dark_ground_lit() {
        assert_eq!(ambient_kind(8100), Ambient::Dark);
        assert_eq!(ambient_kind(11699), Ambient::Dark);
        assert!(ground_lit(8100));
    }

    #[test]
    fn window_8_is_dark_ground_lit() {
        assert_eq!(ambient_kind(11700), Ambient::Dark);
        assert_eq!(ambient_kind(11879), Ambient::Dark);
        assert!(ground_lit(11700));
    }

    #[test]
    fn window_9_is_dark_ground_lit() {
        assert_eq!(ambient_kind(11880), Ambient::Dark);
        assert_eq!(ambient_kind(11909), Ambient::Dark);
        assert!(ground_lit(11880));
    }

    #[test]
    fn window_10_is_dark_ground_lit() {
        assert_eq!(ambient_kind(11910), Ambient::Dark);
        assert_eq!(ambient_kind(16154), Ambient::Dark);
        assert!(ground_lit(11910));
    }

    #[test]
    fn window_11_is_dark_ground_lit() {
        assert_eq!(ambient_kind(16155), Ambient::Dark);
        assert_eq!(ambient_kind(16199), Ambient::Dark);
        assert!(ground_lit(16155));
    }

    #[test]
    fn window_12_is_dark_ground_lit() {
        assert_eq!(ambient_kind(16200), Ambient::Dark);
        assert_eq!(ambient_kind(18899), Ambient::Dark);
        assert!(ground_lit(16200));
    }

    #[test]
    fn window_13_ramp_then_neutral() {
        assert_eq!(ambient_kind(18900), Ambient::Dark);
        assert_eq!(ambient_kind(19020), Ambient::Dark);
        assert_eq!(ambient_kind(19021), Ambient::Neutral);
        assert_eq!(ambient_kind(21599), Ambient::Neutral);
        assert!(!ground_lit(18900));
        assert!(!ground_lit(19021));
    }

    #[test]
    fn ground_light_opens_at_8100_closes_at_18900() {
        assert!(!ground_lit(8099));
        assert!(ground_lit(8100));
        assert!(ground_lit(18899));
        assert!(!ground_lit(18900));
    }

    /// Every threshold opens a new row of the table, even where the plateau values
    /// repeat: the six that change nothing observable are ramp starts, and a ramp
    /// is what the lighting ticket models, not this one.
    #[test]
    fn every_threshold_starts_a_new_window() {
        for (index, &threshold) in THRESHOLDS.iter().enumerate() {
            assert_eq!(window_index(threshold), index + 1, "at {threshold}");
            assert_eq!(window_index(threshold - 1), index, "below {threshold}");
        }
    }

    /// The state this model reports changes at exactly the boundaries where a
    /// plateau or the ground light changes. 19021 is not a threshold: it is where
    /// window 13's ramp reaches neutral.
    #[test]
    fn observable_state_changes_at_the_recorded_boundaries() {
        let mut changed: Vec<u64> = Vec::new();
        let mut previous = (ambient_kind(0), ground_lit(0));
        for tick in 1..CYCLE_TICKS {
            let now = (ambient_kind(tick), ground_lit(tick));
            if now != previous {
                changed.push(tick);
                previous = now;
            }
        }
        assert_eq!(changed, vec![1800, 3570, 5400, 8100, 18900, 19021]);
    }

    /// Mid-window the reported state is constant. Window 13 is compared from
    /// 19021 because its first 121 ticks are the ramp down from window 12.
    #[test]
    fn nothing_changes_mid_window() {
        let mut spans: Vec<(u64, u64)> = Vec::new();
        let mut start = 0u64;
        for &threshold in &THRESHOLDS {
            spans.push((start, threshold));
            start = threshold;
        }
        spans.push((start, CYCLE_TICKS));

        for (index, &(start, end)) in spans.iter().enumerate() {
            let plateau_start = if index + 1 == spans.len() {
                WINDOW_13_NEUTRAL_START
            } else {
                start
            };
            let middle = (plateau_start + end) / 2;
            assert_eq!(
                (ambient_kind(plateau_start), ground_lit(plateau_start)),
                (ambient_kind(middle), ground_lit(middle)),
                "window {index}: {middle} differs from {plateau_start}"
            );
        }
    }

    #[test]
    fn clock_face_at_known_points() {
        assert_eq!(clock_face(0), (6, 0));
        assert_eq!(clock_face(900), (7, 0));
        assert_eq!(clock_face(15), (6, 1));
        assert_eq!(clock_face(21600), (6, 0));
    }

    #[test]
    fn day_phase_covers_expected_spans() {
        assert_eq!(day_phase(900), DayPhase::Day);
        assert_eq!(day_phase(5400), DayPhase::Dusk);
        assert_eq!(day_phase(8099), DayPhase::Dusk);
        assert_eq!(day_phase(8100), DayPhase::Night);
        assert_eq!(day_phase(18899), DayPhase::Night);
        assert_eq!(day_phase(18900), DayPhase::Dawn);
        assert_eq!(day_phase(19020), DayPhase::Dawn);
        assert_eq!(day_phase(19021), DayPhase::Day);
    }

    #[test]
    fn phase_trace_day_dusk_night_dawn_day() {
        let mut phases: Vec<DayPhase> = Vec::new();
        let mut prev = day_phase(0);
        phases.push(prev);
        for t in 1..CYCLE_TICKS {
            let p = day_phase(t);
            if p != prev {
                phases.push(p);
                prev = p;
            }
        }
        // Our convention: Day → Dusk → Night → Dawn → Day. The original names no phase.
        assert_eq!(
            phases,
            vec![
                DayPhase::Day,
                DayPhase::Dusk,
                DayPhase::Night,
                DayPhase::Dawn,
                DayPhase::Day
            ]
        );
    }

    #[test]
    fn game_time_at_tick_zero() {
        let gt = game_time(0);
        assert_eq!(gt.day, 0);
        assert_eq!(gt.cycle_tick, 0);
        assert_eq!(gt.ambient, Ambient::Neutral);
        assert!(!gt.ground_lit);
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
