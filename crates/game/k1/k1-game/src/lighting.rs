//! Day-phase ambient lighting computation — pure functions, no Bevy, no filesystem.
//!
//! Evidence base: `docs/k1/time-of-day.md` — our own byte-level read of the
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
//! - **Float fade against integer tables**: the original fades in 5/6-bit integer
//!   lookup tables ([`fade_table_entry`]); our shader computes the same formulas in
//!   8-bit floats. The difference is bounded by the original's own output
//!   quantization (its display was 16-bit colour) and is not reproduced.
//!
//! ## Not yet implemented (out of scope for #96)
//!
//! - The two user option switches ("day/night" and "light effects"): ticket #97.
//!   Here we assume both are on. The light-effects global is pinned: `[0x81D604]`.
//!
//! - Per-map constant light maps (42, 44, 45): their deciding flag `[0x81D5DC]`
//!   is unlocated at the binary level. We leave those out until it is confirmed.
//!
//! - The `0.33` vs `1/3` coefficients of window 7's red: the research page
//!   leaves them unresolved (#96 does not invent them).

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

/// Tile-light colour for the ground pass (`/200`).
///
/// In the original the ground path receives half the draw pass value
/// (game.cpp:981: `SetLightColor(_lightColor / 200.0)`).
pub fn ground_light_color(tick: u64, seed: u64) -> [f32; 3] {
    let c = light_color(tick, seed);
    [c[0] / 2.0, c[1] / 2.0, c[2]]
}

/// The fade factor for a channel value, exactly as the original's table builder
/// computes it: a float32 accumulator taking |v| steps of the pool constant
/// `0.01f` (rus VA 0x4506C7), stored to memory after every step — so the value is
/// the f32 accumulation, not `v / 100` (at v = 12 it is 0.1199999898…, not the
/// f32 nearest to 0.12).
pub fn fade_factor(v: u32) -> f32 {
    // Each step adds in the x87 80-bit registers and stores to f32: an f64 sum
    // of two exact f32 values, rounded once on the store. A native f32 add
    // rounds once too, but from a narrower sum — at v = 6 it already disagrees
    // in the last bit, so the two-step form is the faithful one.
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
/// truncated. See docs/k1/time-of-day.md, "How the ambient value reaches the
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

/// Per-channel composition reproduced from the original's fragment shader.
///
/// Source: Slavik `gfx.cpp:56-84` described in our own words. Each channel behaves
/// independently:
///
/// - `lightning` is the mask brightness sampled from the light mask texture
///   (`texture2D(tex3, ...).r`), in `0..1`.
/// - `light_color` is the RGB tile-light colour (`cLight` in the original) that
///   adds toward white when the mask is bright.
/// - `blend` is the ambient blend (`cBlend`), negative for darken, positive for
///   lighten toward white.
///
/// Negative blend: the multiply always carries lightning —
/// `base *= 1.0 + clamp(blend + lightning, -1.0, 0.0)` (gfx.cpp:66) — while the
/// additive step toward white runs only when `lightning >= 0.01`
/// (`base += (1.0 - base) * light_color`, gfx.cpp:64-65); below the threshold the
/// multiplier still shifts by `lightning`.
/// Non-negative blend: blend toward white — `base += (1.0 - base) * blend`.
/// Blue channel never gets the light term, and its negative multiply is unclamped:
/// `base *= 1.0 + blend` (gfx.cpp:81).
///
/// Alpha is never touched here; the caller sets alpha on the sprite itself.
pub fn apply_channel(
    base: f32,
    blend: f32,
    lightning: f32,
    light_color: f32,
    is_blue: bool,
) -> f32 {
    if is_blue {
        // Blue never gets the light term, and the reference multiplies it raw.
        if blend < 0.0 {
            base * (1.0 + blend)
        } else {
            base + (1.0 - base) * blend
        }
    } else if blend < 0.0 {
        let lit = if lightning >= 0.01 {
            base + (1.0 - base) * light_color
        } else {
            base
        };
        lit * (1.0 + (blend + lightning).clamp(-1.0, 0.0))
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
/// docs/k1/time-of-day.md, "How the ambient value reaches the pixels").
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
/// No option switches (they belong to ticket #97), no per-map constant light
/// (unconfirmed flag `[0x81D5DC]`). All values come from the simulation tick and
/// the session seed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightingParams {
    /// The fade factors for every surface — ground, decorations, objects and
    /// characters alike: the fade follows the phase, window or not.
    pub ambient: [f32; 3],
    /// Tile-light colour / 200, added where mask brightness >= 0.01.
    pub ground_light: [f32; 3],
    /// The phase's ground-light window (ticks 8100..18900): gates only the
    /// tile light — masks and light colour — never the fade.
    pub ground_window: bool,
}

/// Full params for a simulation tick.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time_of_day;

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

        // Low lightning with high light_color → no white-add because lightning < 0.01,
        // but lightning still shifts the multiply:
        //   result = 0.6 * (1 + clamp(-0.7 + 0.005, -1, 0)) = 0.6 * 0.305 = 0.183
        let result2 = apply_channel(0.6, -0.7, 0.005, 0.5, false);
        let expected2 = 0.183;
        assert!(
            (result2 - expected2).abs() < 1e-6,
            "{result2} ~= {expected2}"
        );
    }

    #[test]
    fn apply_channel_light_below_threshold_has_no_add_but_multiplies() {
        // lightning = 0.009 is below the 0.01 gate: no additive step toward white,
        // yet lightning still enters the multiply (gfx.cpp:66 is unconditional).
        // 0.6 * (1 + clamp(-0.7 + 0.009, -1, 0)) = 0.6 * (1 - 0.691) = 0.1854
        let result = apply_channel(0.6, -0.7, 0.009, 0.5, false);
        let expected = 0.1854;
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
            apply_channel(0.5, 0.3, 0.2, 0.2, false)
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
    fn ground_light_color_is_half_of_draw_color() {
        for tick in 0..500 {
            let dc = light_color(tick, 42);
            let gc = ground_light_color(tick, 42);
            assert!((gc[0] - dc[0] / 2.0).abs() < 1e-9);
            assert!((gc[1] - dc[1] / 2.0).abs() < 1e-9);
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
}
