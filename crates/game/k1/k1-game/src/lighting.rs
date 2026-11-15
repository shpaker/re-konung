//! Day-phase ambient lighting computation — pure functions, no Bevy, no filesystem.
//!
//! Evidence base: `docs/research/k1/lighting.md` — our own byte-level read of the
//! original binaries (the palette-transform chain, the table builder and the
//! ground-pass gate), with the Slavik clone (`game.cpp`, `gfx.cpp`) as a
//! secondary source. The module reproduces the original's per-channel blend:
//! [`fade_table_entry`] is the byte-level arithmetic of the original's runtime
//! fade tables, and `apply_channel` is the float form the GPU path carries.
//!
//! ## Recorded divergences from the original
//!
//! - **Per-day palette**: the original flips one coin once per cycle when the counter
//!   wraps through zero (`rand() % 100 < 90`, game.cpp:1768). Ours is a pure function
//!   `(day, seed) → palette` so a scrubbed timeline shows the same night regardless of
//!   frame rate or replay order.
//!
//! - **Tile-light flicker**: the original re-rolls every draw (`rand() % 8 + 16`,
//!   `rand() % 4 + 8`, game.cpp:925), making colour a function of frame rate. Ours
//!   advances once every 16 ticks — an approximation we chose, NOT a period observed
//!   in the original. See docs/ if you need the exact citation.
//!
//! - **Float fade against integer tables**: the GPU uses display-encoded floats
//!   instead of 5/6-bit tables. Quantization and mask filtering remain intentional
//!   differences; no universal one-step error bound is claimed (see the audit).
//!
//! The light-effects option is available through [`params_for_map`]; frontends
//! currently keep it enabled and expose no additional menu control. Constant-map
//! flags and colors are confirmed in all three binaries (see the audit).

use crate::time_of_day::{self, NightPalette};

/// Mixing function that turns `(day, seed)` into a 0..100 value.
///
/// Uses a splitmix-style hash so small changes in either argument produce large,
/// unrelated changes in the output — the kind of distribution you want for a
/// pseudo-random bit without pulling in the full RNG machinery.
fn mix_day_seed(day: u64, seed: u64) -> u64 {
    let mut x = day.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(seed);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB);
    x ^ (x >> 31)
}

/// Returns the night palette for the given simulation day and session seed.
///
/// Deterministic: repeated calls with the same `(day, seed)` return the same value,
/// even after a timeline scrub forward and back. Our divergence from the original
/// (unseeded per-cycle flip) means this does NOT depend on frame rate or how many
/// real-time seconds elapsed between cycles.
pub fn night_palette(day: u64, seed: u64) -> NightPalette {
    if mix_day_seed(day, seed) % 100 < 90 {
        NightPalette::Green
    } else {
        NightPalette::Deep
    }
}

/// The tile-light colour `/100`, the draw-pass value.
///
/// Original re-rolls on every draw (game.cpp:925: `rand() % 8 + 16, rand() % 4 + 8, 0`);
/// ours re-rolls once every 16 ticks — an approximation, not a period observed in the
/// original. This avoids a frame-rate dependency while still providing variation.
pub fn light_color(tick: u64, seed: u64) -> [f32; 3] {
    let block = tick / 16;
    // Deterministic but varied values within each 16-tick block.
    let r_mix = mix_day_seed(block.saturating_sub(1), seed.wrapping_add(1)) % 8;
    let g_mix = mix_day_seed(block, seed.wrapping_add(2)) % 4;
    [
        ((r_mix as i32 + 16) as f32) / 100.0,
        ((g_mix as i32 + 8) as f32) / 100.0,
        0.0,
    ]
}

/// Tile-light table factors after integer halving in the ground pass.
///
/// Confirmed at rus `0x4254E6..0x42553C`: divide the integer levels by two,
/// then index the positive fade tables (not a floating division by 200).
pub fn ground_light_color(tick: u64, seed: u64) -> [f32; 3] {
    let c = light_color(tick, seed);
    [
        fade_factor((c[0] * 100.0).round() as u32 / 2),
        fade_factor((c[1] * 100.0).round() as u32 / 2),
        0.0,
    ]
}

/// The fade factor for a channel value, exactly as the original's table builder
/// computes it: a float32 accumulator taking |v| steps of the pool constant
/// `0.01f` (rus VA 0x4506C7), stored to memory after every step — so the value is
/// the f32 accumulation, not `v / 100` (at v = 12 it is 0.1199999898…, not the
/// f32 nearest to 0.12).
pub fn fade_factor(v: u32) -> f32 {
    // Each x87 addition is stored back to f32; the wider sum preserves the
    // exact intermediate before that store rounds it.
    let mut k = 0.0f32;
    for _ in 0..v {
        k = (f64::from(k) + f64::from(0.01f32)) as f32;
    }
    k
}

/// One entry of the original's runtime fade tables (the builder at rus
/// 0x439B08-0x439D18), the byte-level arithmetic our float render approximates.
///
/// The window formula is truncated toward zero to a signed byte first (the x87
/// `fistp` with RC=chop at the packing sites), then:
/// negative `v`: `trunc(c · (1 − k))`; positive `v`: `c + trunc((MAX − c) · k)`
/// with `k = fade_factor(v)` and `MAX` = 31 for the 5-bit channels, 63 for green.
/// The intermediates are exact in the x87 80-bit registers (`c ≤ 63`, `k` a
/// 24-bit mantissa), so they are computed in f64 here; only the final result is
/// truncated. See docs/research/k1/lighting.md, "How the ambient value reaches the
/// pixels".
pub fn fade_table_entry(component: u32, value: i32, bits: u32) -> u32 {
    let max = (1u32 << bits) - 1;
    let c = component.min(max);
    if value < 0 {
        let k = fade_factor(value.unsigned_abs()) as f64;
        (f64::from(c) * (1.0 - k)).trunc() as u32
    } else {
        let k = fade_factor(value.unsigned_abs()) as f64;
        c + ((f64::from(max) - f64::from(c)) * k).trunc() as u32
    }
}

/// The ambient triple as sign-carrying fade factors: the window value truncated
/// toward zero to the packed byte the original stores (rus 0x42866D), then the
/// table factor of that byte's magnitude. A channel of 0 carries 0.0 — the
/// original's transform no-ops when the whole triple is zero (rus 0x43D971).
pub fn channels(cycle_tick: u64, night: NightPalette) -> [f32; 3] {
    let (r, g, b) = time_of_day::ambient_value_with(cycle_tick, night);
    let factor = |raw: f32| {
        let value = raw.trunc() as i32;
        if value < 0 {
            -fade_factor(value.unsigned_abs())
        } else {
            fade_factor(value.unsigned_abs())
        }
    };
    [factor(r), factor(g), factor(b)]
}

/// Display-encoded channel composition of the original palette and lit blit.
///
/// With a nonzero mask the red/green path brightens first, then applies only
/// negative ambient after adding the mask level (rus `0x43E7FA`, `0x43CC49`).
/// Positive ambient is ignored on that path. Blue and zero-mask pixels use the
/// plain palette transform. GPU filtering uses a one-percent mask threshold.
/// Alpha is untouched; the renderer converts sRGB around this operation.
pub fn apply_channel(
    base: f32,
    blend: f32,
    lightning: f32,
    light_color: f32,
    is_blue: bool,
) -> f32 {
    if !is_blue && lightning >= 0.01 {
        let lit = base + (1.0 - base) * light_color;
        lit * (1.0 + (blend + lightning).clamp(-1.0, 0.0))
    } else if blend < 0.0 {
        base * (1.0 + blend)
    } else {
        base + (1.0 - base) * blend
    }
}

/// Which surface type the ambient blend is applied to.
///
/// Every surface fades with the ambient whenever the triple is non-zero: the
/// original resolves each drawn sprite's palette through one shared transform
/// (rus 0x43D971) with no per-pass state, so the ground fades at dusk exactly
/// like the objects on it. The ground-light window gates only the tile-light
/// mode — masks and light colour — not the fade (our own byte-level read,
/// docs/research/k1/lighting.md, "How the ambient value reaches the pixels").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// Ground tile and its upper layer. Faded with the ambient, like everything.
    Terrain,
    /// GRAPH.RES decorations. Same rules as terrain (same transform).
    Decoration,
    /// Static map objects. Faded unless the object carries two or more flames.
    StaticObject { self_lit: bool },
    /// Characters and dynamic objects: per-cell fine-block of `LIGHTS.N`.
    Dynamic,
}

/// Whether a piece on this surface should receive the ambient blend.
///
/// Confirmed sources (our read of the binary, confirmed against Slavik where
/// marked):
/// - Terrain/Decoration: always — the plain ground path calls the shared
///   palette transform for every tile outside the window too (rus 0x42541D,
///   0x425472); the clone's `SetFade(false)` there is its own transcription.
/// - StaticObject: always fades except where `Flames.size() >= 2`
///   (**by Slavik** game.cpp:1563-1580).
/// - Dynamic (characters/dynamic objects): fades where the fine cell is 0
///   (**by Slavik** game.cpp:1089-1100, 1611-1626); the fine block's 0/≠0
///   distinction decides whether the creature stands in a lit patch, window
///   or not.
///
/// `fine_light_value` is the `u8` from [`k1_data::map::Lights::fine`]; only its
/// zero-ness matters.
pub fn fades(surface: Surface, fine_light_value: u8) -> bool {
    match surface {
        Surface::Terrain | Surface::Decoration => true,
        Surface::StaticObject { self_lit } => !self_lit,
        Surface::Dynamic => fine_light_value == 0,
    }
}

/// Flat parameters the render layer is handed once per frame.
///
/// The day/night and light-effects gates and the constant-map override are
/// applied by [`params_for_map`] before the render layer receives these values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightingParams {
    /// The fade factors for every surface — ground, decorations, objects and
    /// characters alike: the fade follows the phase, window or not.
    pub ambient: [f32; 3],
    /// Positive table factors for integer-halved light levels, added on lit pixels.
    pub ground_light: [f32; 3],
    /// The phase's ground-light window (ticks 8100..18900): gates only the
    /// tile light — masks and light colour — never the fade.
    pub ground_window: bool,
}

/// Full params for a simulation tick — the day/night cycle on.
///
/// Derives `day` and `cycle_tick` the same way `time_of_day::game_time` does, so a
/// caller can hand any absolute tick without normalising it first.
pub fn params(tick: u64, seed: u64) -> LightingParams {
    let day = tick / time_of_day::CYCLE_TICKS;
    let cycle_tick = tick % time_of_day::CYCLE_TICKS;
    let night = night_palette(day, seed);
    let ambient = channels(cycle_tick, night);
    let ground_light = ground_light_color(tick, seed);
    LightingParams {
        ambient,
        ground_light,
        ground_window: time_of_day::ground_window(cycle_tick),
    }
}

/// A map's fixed signed-byte color, before either user-option gate.
///
/// Confirmed table lookup: rus `0x43ADFC` reads the flag at `0x45AE2F + map`.
/// Slots 42..55 are flagged, including sentinel-colored slots. The color table
/// is indexed by map number, not map number minus one; see the lighting audit.
pub fn constant_map_color(map: u32) -> Option<[i32; 3]> {
    match map {
        42 => Some([-70, 0, 0]),
        44 => Some([-20, -1, -70]),
        45 => Some([0, -50, -50]),
        43 | 46..=55 => Some([-1; 3]),
        _ => None,
    }
}

/// Lighting with K1 map rules and the two independent original options.
/// Constant-map lighting takes precedence over day/night and never lights tiles.
/// Disabling light effects closes the tile-light gate without changing ambient.
pub fn params_for_map(
    tick: u64,
    seed: u64,
    map: u32,
    day_night: bool,
    light_effects: bool,
) -> LightingParams {
    if let Some(color) = constant_map_color(map) {
        return LightingParams {
            ambient: color.map(|v| v.signum() as f32 * fade_factor(v.unsigned_abs())),
            ground_light: [0.0; 3],
            ground_window: false,
        };
    }
    let mut result = params_gated(tick, seed, day_night);
    if !light_effects {
        result.ground_window = false;
        result.ground_light = [0.0; 3];
    }
    result
}

/// [`params`] with the original's day/night option gate applied (`#97`).
///
/// `day_night` mirrors the user option `[0x81D5EC]` (the clone's
/// `_bConfDayNight`) exactly where the original reads it: with it off the light
/// routine returns the neutral triple and the ground-light flag stays cleared —
/// the routine clears the flag at its head and the stores that set it live in
/// the window branches the gate skips (`docs/research/k1/lighting.md`, "The order of
/// the checks"). So the gate-off result is the constant day value: neutral
/// ambient, no tile light.
///
/// The option gates the lighting computation only — the original's counter is
/// incremented at two sites neither of which sits inside a branch on the option,
/// and ours is not either: `tick` keeps advancing through the cycle whatever
/// the flag is, and turning the gate back on resumes the visible cycle at the
/// current tick. Deliberate divergence, recorded next to the fact: the
/// original's flag travels with the saved options, ours is a viewer session
/// setting handed to this function every frame.
pub fn params_gated(tick: u64, seed: u64, day_night: bool) -> LightingParams {
    if day_night {
        return params(tick, seed);
    }
    LightingParams {
        ambient: [0.0; 3],
        ground_light: [0.0; 3],
        ground_window: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time_of_day;

    #[test]
    fn constant_maps_precede_both_option_gates() {
        for map in 0..=56 {
            for tick in [0, 8100, 12000, 18900, 21600] {
                for day_night in [false, true] {
                    let on = params_for_map(tick, 42, map, day_night, true);
                    let off = params_for_map(tick, 42, map, day_night, false);
                    assert_eq!(on.ambient, off.ambient);
                    assert!(!off.ground_window);
                    assert_eq!(off.ground_light, [0.0; 3]);
                    if (42..=55).contains(&map) {
                        assert_eq!(on, params_for_map(0, 42, map, !day_night, false));
                        assert!(!on.ground_window);
                    } else {
                        assert_eq!(on, params_gated(tick, 42, day_night));
                    }
                }
            }
        }
        assert_eq!(constant_map_color(42), Some([-70, 0, 0]));
        assert_eq!(constant_map_color(44), Some([-20, -1, -70]));
        assert_eq!(constant_map_color(45), Some([0, -50, -50]));
        assert_eq!(constant_map_color(43), Some([-1; 3]));
        assert_eq!(constant_map_color(56), None);
    }

    #[test]
    fn masked_pixels_use_light_even_when_ambient_is_neutral_or_positive() {
        for ambient in [0.0, 0.1] {
            assert!((apply_channel(0.5, ambient, 0.2, 0.08, false) - 0.54).abs() < 1e-6);
        }
        assert_eq!(apply_channel(0.5, 0.1, 0.2, 0.08, true), 0.55);
    }

    #[test]
    fn ambient_factors_are_the_original_accumulation() {
        // Window 3 plateau: the packed bytes are (12, 6, 0) and the factors are
        // the f32 accumulations of the pool constant 0.01f (0.009999999776482582),
        // not v/100 — golden values generated by replicating the builder.
        assert_eq!(
            channels(2700, NightPalette::Deep),
            [0.11999998, 0.059999995, 0.0],
        );
        // Window 6 plateau: bytes (10, -20, -50).
        assert_eq!(
            channels(5501, NightPalette::Deep),
            [0.09999999, -0.20000002, -0.4999998],
        );
    }

    #[test]
    fn fade_factor_matches_the_accumulated_steps() {
        assert_eq!(fade_factor(0), 0.0);
        assert_eq!(fade_factor(1), 0.01);
        assert_eq!(fade_factor(12), 0.11999998);
        assert_eq!(fade_factor(50), 0.4999998);
        assert_eq!(fade_factor(70), 0.69999963);
    }

    #[test]
    fn ramps_step_in_whole_bytes_like_the_original() {
        // Window 6 ramp, red = 0.2·u: the original packs whole bytes, so the
        // factor steps 0,0,…,1 and never holds a fraction.
        for (tick, expect_byte) in [(5402u64, 0i32), (5410, 1), (5412, 1), (5420, 2)] {
            let (r, _, _) = time_of_day::ambient_value_with(tick, NightPalette::Deep);
            assert_eq!(r.trunc() as i32, expect_byte, "tick {tick}");
        }
        let [r, _, _] = channels(5412, NightPalette::Deep);
        assert_eq!(r, fade_factor(1));
    }

    /// Golden rows of the original's runtime fade tables, generated by
    /// replicating the builder at rus 0x439B08-0x439D18: f32 0.01f
    /// accumulation, exact intermediates (x87 80-bit), trunc toward zero.
    #[test]
    fn fade_table_matches_the_original_builder() {
        let red_pos_12: Vec<u32> = (0..32).map(|c| fade_table_entry(c, 12, 5)).collect();
        assert_eq!(
            red_pos_12,
            [
                3, 4, 5, 6, 7, 8, 8, 9, 10, 11, 12, 13, 14, 15, 16, 16, 17, 18, 19, 20, 21, 22, 23,
                23, 24, 25, 26, 27, 28, 29, 30, 31
            ],
            "red positive v=12 — the warm band brightens toward the maximum"
        );
        let green_pos_6: Vec<u32> = (0..64).map(|c| fade_table_entry(c, 6, 6)).collect();
        assert_eq!(
            green_pos_6,
            [
                3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 15, 16, 17, 18, 19, 20, 21, 22, 23,
                24, 25, 26, 27, 28, 29, 30, 31, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43,
                44, 45, 46, 47, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63
            ],
        );
        let blue_neg_50: Vec<u32> = (0..32).map(|c| fade_table_entry(c, -50, 5)).collect();
        assert_eq!(
            blue_neg_50,
            [
                0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12,
                13, 13, 14, 14, 15, 15
            ],
        );
        let green_neg_70: Vec<u32> = (0..64).map(|c| fade_table_entry(c, -70, 6)).collect();
        assert_eq!(
            green_neg_70,
            [
                0, 0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 5, 5, 5, 6, 6, 6, 6, 7, 7, 7, 8,
                8, 8, 9, 9, 9, 9, 10, 10, 10, 11, 11, 11, 12, 12, 12, 12, 13, 13, 13, 14, 14, 14,
                15, 15, 15, 15, 16, 16, 16, 17, 17, 17, 18, 18, 18, 18
            ],
        );
        let red_neg_30: Vec<u32> = (0..32).map(|c| fade_table_entry(c, -30, 5)).collect();
        assert_eq!(
            red_neg_30,
            [
                0, 0, 1, 2, 2, 3, 4, 4, 5, 6, 7, 7, 8, 9, 9, 10, 11, 11, 12, 13, 14, 14, 15, 16,
                16, 17, 18, 18, 19, 20, 21, 21
            ],
        );
        // Positive v saturates at the component maximum, negative floors at 0.
        assert_eq!(fade_table_entry(31, 12, 5), 31);
        assert_eq!(fade_table_entry(0, -70, 6), 0);
    }

    #[test]
    fn ambient_is_continuous_across_the_window_boundaries() {
        // The window table itself is ramp-connected at 8100 and 18900, and the
        // fade now follows the ambient for every surface — nothing steps there.
        assert_eq!(
            channels(8099, NightPalette::Green),
            channels(8100, NightPalette::Green)
        );
        assert_eq!(
            channels(18899, NightPalette::Green),
            channels(18900, NightPalette::Green)
        );
        assert!(!params(8099, 42).ground_window);
        assert!(params(8100, 42).ground_window);
        assert!(params(18899, 42).ground_window);
        assert!(!params(18900, 42).ground_window);
    }

    #[test]
    fn green_deep_differ_only_in_windows_8_through_10() {
        // Tick 11879: window 7 → identical (pre-night-palette windows).
        assert_eq!(
            channels(11879, NightPalette::Deep),
            channels(11879, NightPalette::Green),
        );
        // Tick 11950: inside window 9 plateau → differ (Deep=`-70`, Green=`-50`).
        assert_ne!(
            channels(11950, NightPalette::Deep),
            channels(11950, NightPalette::Green)
        );
        // Verify actual values at known points: the packed bytes (−60,−70,−70)
        // and (−60,−50,−70) as accumulated fade factors.
        assert_eq!(
            channels(11950, NightPalette::Deep),
            [-fade_factor(60), -fade_factor(70), -fade_factor(70)]
        );
        assert_eq!(
            channels(11950, NightPalette::Green),
            [-fade_factor(60), -fade_factor(50), -fade_factor(70)]
        );
        // Tick 16245: past window 10 plateau → identical.
        assert_eq!(
            channels(16245, NightPalette::Deep),
            channels(16245, NightPalette::Green)
        );
    }

    #[test]
    fn boundary_trace_unchanged_by_palette_choice() {
        // Both palettes are dark (blue < 0), so both produce Ambient::Dark throughout
        // windows 8–10. We compare the raw triples to confirm they stay in the same
        // classification without needing an `ambient_kind_with` wrapper.
        for tick in 11880..16200 {
            let (_, _, db) = time_of_day::ambient_value(tick);
            let (_, _, gb) = time_of_day::ambient_value_with(tick, NightPalette::Green);
            assert!(
                db < 0.0 && gb < 0.0,
                "at {tick}: Deep={db} Green={gb} must be negative blue"
            );
        }
    }

    #[test]
    fn apply_channel_negative_blend_no_light() {
        // Negative blend, zero lightning/light_color: classic multiply-darken.
        // base=0.6, blend=-0.7, lightning=0.0, light_color=0.0 → 0.6 * (1 + clamp(-0.7, -1, 0)) = 0.6 * 0.3 = 0.18
        let result = apply_channel(0.6, -0.7, 0.0, 0.0, false);
        let expected = 0.18;
        assert!((result - expected).abs() < 1e-6, "{result} ~= {expected}");
    }

    #[test]
    fn apply_channel_negative_blend_with_bright_light() {
        // Lightning adds toward white first (using light_color), then multiply cancels with blend+lightning.
        // base=0.6, blend=-0.7, lightning=0.1, light_color=0.1 → lit = 0.6 + 0.4*0.1 = 0.64;
        //   clamped part = clamp(-0.7+0.1, -1, 0) = -0.6;
        //   result = 0.64 * (1 + (-0.6)) = 0.64 * 0.4 = 0.256
        let result = apply_channel(0.6, -0.7, 0.1, 0.1, false);
        let expected = 0.64 * 0.4;
        assert!((result - expected).abs() < 1e-6, "{result} ~= {expected}");
    }

    #[test]
    fn apply_channel_lightning_and_light_color_differ() {
        // Regression: lightning (mask brightness) and light_color (RGB source) are separate.
        // High lightning with low light_color → strong multiply cancel but weak white-add.
        // base=0.6, blend=-0.7, lightning=0.5, light_color=0.05
        //   lit = 0.6 + 0.4*0.05 = 0.62
        //   clamp = clamp(-0.7+0.5, -1, 0) = -0.2
        //   result = 0.62 * 0.8 = 0.496
        let result = apply_channel(0.6, -0.7, 0.5, 0.05, false);
        let expected = 0.62 * 0.8;
        assert!((result - expected).abs() < 1e-6, "{result} ~= {expected}");

        // A filtered sample below one percent takes the plain-palette path.
        let result2 = apply_channel(0.6, -0.7, 0.005, 0.5, false);
        let expected2 = 0.18;
        assert!(
            (result2 - expected2).abs() < 1e-6,
            "{result2} ~= {expected2}"
        );
    }

    #[test]
    fn filtered_mask_below_one_percent_uses_the_plain_palette() {
        let result = apply_channel(0.6, -0.7, 0.009, 0.5, false);
        let expected = 0.18;
        assert!((result - expected).abs() < 1e-6, "{result} ~= {expected}");
    }

    #[test]
    fn apply_channel_blue_negative_blend_has_no_clamp() {
        // The reference multiplies blue raw, without the -1 clamp (gfx.cpp:81).
        // blend = -1.5 is outside real data but pins the formula:
        // 0.6 * (1 + (-1.5)) = -0.3, while a clamp would give 0.0.
        let result = apply_channel(0.6, -1.5, 0.0, 0.0, true);
        let expected = -0.3;
        assert!((result - expected).abs() < 1e-6, "{result} ~= {expected}");
    }

    #[test]
    fn apply_channel_blue_no_light_cancellation() {
        // Blue channel never cancels darken with tile-light source.
        // Non-blue does: lightning adds toward white (via light_color) first, then multiply darkens.
        // base=0.5, blend=-0.5, lightning=0.3, light_color=0.3
        let base_not_blue = apply_channel(0.5, -0.5, 0.3, 0.3, false);
        let base_blue = apply_channel(0.5, -0.5, 0.3, 0.3, true);
        // They differ because non-blue gets the light additive step.
        assert_ne!(base_not_blue, base_blue);
        // Expected: blue ignores light entirely → 0.5 * (1 + clamp(-0.5, -1, 0)) = 0.25
        assert!((base_blue - 0.25).abs() < 1e-6);
    }

    #[test]
    fn apply_channel_blue_ignores_light_positive_blend() {
        // Positive blend: both channels do the same thing (blend toward white).
        assert_eq!(
            apply_channel(0.5, 0.3, 0.2, 0.2, true),
            apply_channel(0.5, 0.3, 0.0, 0.2, false)
        );
    }

    #[test]
    fn applies_dynamic_based_on_fine_cell_only() {
        // fine == 0 → fades; fine != 0 → no fade, window or not: characters
        // stand in lit patches even when the ground-light window is closed.
        assert!(fades(Surface::Dynamic, 0));
        assert!(!fades(Surface::Dynamic, 5));
    }

    #[test]
    fn apply_channel_positive_blend() {
        // Blend toward white: base += (1 - base) * blend.
        // base=0.4, blend=0.3 → 0.4 + 0.6*0.3 = 0.58
        let result = apply_channel(0.4, 0.3, 0.0, 0.0, false);
        let expected = 0.4 + 0.6 * 0.3;
        assert!((result - expected).abs() < 1e-6);
    }

    #[test]
    fn apply_channel_clamp_prevents_overbrighten() {
        // blend + lightning > 0 → clamped to 0, so no extra brightening beyond light_color add.
        // base=0.6, blend=-0.3, lightning=0.5, light_color=0.5 → lit = 0.6+0.4*0.5 = 0.8;
        //   clamp(-0.3+0.5, -1, 0) = 0.0; result = 0.8 * 1.0 = 0.8
        let result = apply_channel(0.6, -0.3, 0.5, 0.5, false);
        let expected = 0.8;
        assert!((result - expected).abs() < 1e-6);
    }

    #[test]
    fn light_color_within_expected_ranges() {
        // Ranges must be 16..23, 8..11, 0 (divided by 100).
        for tick in 0..1000 {
            let c = light_color(tick, 42);
            assert!(
                (16.0..=23.0).contains(&(c[0] * 100.0)),
                "red at {tick}: {}",
                c[0]
            );
            assert!(
                (8.0..=11.0).contains(&(c[1] * 100.0)),
                "green at {tick}: {}",
                c[1]
            );
            assert_eq!(c[2], 0.0, "blue at {tick}: {}", c[2]);
        }
    }

    #[test]
    fn light_color_stable_within_block_different_across_blocks() {
        let seed = 999;
        // Same block: stable.
        let c1 = light_color(50, seed); // block 3
        let c2 = light_color(65, seed); // block 4
        // Different blocks: different colors.
        assert_ne!(c1, c2, "colors at blocks 3 and 4 must differ");
        // Same block: identical.
        let c3 = light_color(51, seed); // block 3
        let c4 = light_color(63, seed); // block 3
        assert_eq!(c3, c4, "colors within block 3 must be identical");
    }

    #[test]
    fn night_palette_deterministic_and_round_trip_consistent() {
        let seed = 12345;
        let d = 7;
        // Two calls must agree.
        let p1 = night_palette(d, seed);
        let p2 = night_palette(d, seed);
        assert_eq!(p1, p2);
        // Count Green over days 0..1000 for this fixed seed.
        let green_count: u64 = (0..1000)
            .map(|d| night_palette(d, seed))
            .filter(|p| matches!(p, NightPalette::Green))
            .count() as u64;
        // The exact count depends on the mixing function but should be close to 90% (±5%).
        assert!(
            (850..=950).contains(&green_count),
            "{green_count} Green days in 1000, expected ~900"
        );
        // And it cannot be trivially constant (not all Deep or all Green).
        assert!(green_count > 0 && green_count < 1000);
    }

    #[test]
    fn night_palette_identical_after_scrub_forward_back() {
        // Simulate a forward-and-back scrub of a timeline: same (day, seed) → same palette.
        let seed = 777;
        for day in 0..50 {
            assert_eq!(
                night_palette(day, seed),
                night_palette(day, seed),
                "day {day}"
            );
        }
    }

    #[test]
    fn ground_light_uses_integer_halves_before_the_table_factor() {
        for tick in 0..500 {
            let dc = light_color(tick, 42);
            let gc = ground_light_color(tick, 42);
            assert_eq!(gc[0], fade_factor((dc[0] * 100.0).round() as u32 / 2));
            assert_eq!(gc[1], fade_factor((dc[1] * 100.0).round() as u32 / 2));
            assert_eq!(gc[2], dc[2]);
        }
    }

    #[test]
    fn fades_every_surface_with_the_ambient() {
        // The fade follows the phase for every surface: the original's plain
        // ground path calls the shared palette transform per tile outside the
        // window too (rus 0x42541D), so dusk darkens the ground together with
        // the objects standing on it.
        assert!(fades(Surface::Terrain, 0));
        assert!(fades(Surface::Decoration, 0));
    }

    #[test]
    fn fades_static_object_except_self_lit() {
        assert!(fades(Surface::StaticObject { self_lit: false }, 0));
        assert!(!fades(Surface::StaticObject { self_lit: true }, 0));
    }

    #[test]
    fn params_pure_function_of_tick_seed() {
        let t1 = params(11000, 42);
        let t2 = params(11000, 42);
        assert_eq!(t1, t2);
        // Same tick, different seed: ground_light may differ because light_color depends on seed.
        let t3 = params(11000, 43);
        assert_ne!(
            t3.ground_light, t1.ground_light,
            "ground_light should vary with seed"
        );
    }

    /// #97 AC2: with the day/night gate off the ambient snapshot at a day
    /// boundary and at a night boundary are identical — the constant day value.
    /// The boundary ticks are the interval trace of
    /// `docs/research/k1/lighting.md`, plus the ground-lit plateau between them.
    #[test]
    fn the_gate_off_ambient_is_identical_at_day_and_night_boundaries() {
        let boundaries = [0u64, 1802, 5402, 8100, 11000, 11700, 18900, 19020, 21599];
        for tick in boundaries {
            assert_eq!(
                params_gated(tick, 42, false).ambient,
                [0.0, 0.0, 0.0],
                "ambient at {tick} must hold the day value"
            );
        }
        // The two snapshots the ticket names: a day boundary against a night
        // boundary — identical under the gate, different without it.
        assert_eq!(
            params_gated(900, 42, false).ambient,
            params_gated(11000, 42, false).ambient,
            "day and night snapshots under the gate"
        );
        assert_ne!(
            params(900, 42).ambient,
            params(11000, 42).ambient,
            "the gate-on run must actually change with the phase"
        );
    }

    /// The gate-off run closes the ground-light window and drops the tile-light
    /// colour: the original clears the ground flag at the routine's head and
    /// the gate skips the branches that set it, so no tile light either.
    #[test]
    fn the_gate_off_closes_the_ground_window_and_the_tile_light() {
        let off = params_gated(11000, 42, false);
        assert!(!off.ground_window);
        assert_eq!(off.ground_light, [0.0; 3]);
        let on = params(11000, 42);
        assert!(on.ground_window, "the gate-on run at the same tick is lit");
        assert_ne!(on.ground_light, [0.0; 3]);
    }

    /// The flag is the only thing the gate changes: the gate-on call is the
    /// legacy entry unchanged across the cycle, and the gate-off result at an
    /// absolute tick two cycles away equals the one at the same cycle position —
    /// nothing about the gate freezes, shifts or rewinds the clock (#97 AC2:
    /// the phase trace matches the flag-on run).
    #[test]
    fn the_gate_changes_only_the_lighting_input() {
        for tick in (0..time_of_day::CYCLE_TICKS).step_by(7) {
            assert_eq!(
                params(tick, 42),
                params_gated(tick, 42, true),
                "gate-on must equal the legacy params at {tick}"
            );
            assert_eq!(
                params_gated(tick, 42, false),
                params_gated(2 * time_of_day::CYCLE_TICKS + tick, 42, false),
                "the gate sees the same cycle position at {tick}"
            );
        }
    }
}
