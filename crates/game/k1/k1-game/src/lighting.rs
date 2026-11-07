//! Day-phase ambient lighting computation — pure functions, no Bevy, no filesystem.
//!
//! Evidence base: `docs/k1/time-of-day.md` and Slavik reference (`game.cpp`,
//! `gfx.cpp`). The module reproduces the original's per-channel blend in the shader
//! (Slavik `gfx.cpp:56-84`) so callers can verify a CPU reference matches the GPU path.
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
//! ## Not yet implemented (out of scope for #96)
//!
//! - The two user option switches ("day/night" and "light effects"): ticket #97.
//!   Here we assume both are on.
//!
//! - Per-map constant light maps (42, 44, 45): their deciding flag `[0x81D5DC]`
//!   is unlocated at the binary level. We leave those out until it is confirmed.
//!
//! - Rounding of packed bytes and the `0.33` vs `1/3` coefficients: the research page
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

/// The ambient triple divided by 100, the way the original passes it to the shader
/// (`_ambientColor = CalculateLight() / 100.0`, game.cpp:924).
pub fn channels(cycle_tick: u64, night: NightPalette) -> [f32; 3] {
    let (r, g, b) = time_of_day::ambient_value_with(cycle_tick, night);
    [r / 100.0, g / 100.0, b / 100.0]
}

/// Per-channel composition reproduced from the original's fragment shader.
///
/// Source: Slavik `gfx.cpp:56-84` described in our own words. Each channel behaves
/// independently:
///
/// - Negative blend: multiply-darken, partially cancelled by tile-light brightness.
///   If `light >= 0.01`, first `base += (1.0 - base) * light` (light adds toward white),
///   then `base *= 1.0 + clamp(blend + light, -1.0, 0.0)` (multiply darkens).
/// - Non-negative blend: blend toward white — `base += (1.0 - base) * blend`.
/// - Blue channel never gets the light term — only the blend operation.
///
/// Alpha is never touched here; the caller sets alpha on the sprite itself.
pub fn apply_channel(base: f32, blend: f32, light: f32, is_blue: bool) -> f32 {
    if is_blue {
        // Blue never cancels darken with the tile-light source.
        if blend < 0.0 {
            base * (1.0 + blend.max(-1.0))
        } else {
            base + (1.0 - base) * blend
        }
    } else if blend < 0.0 {
        if light >= 0.01 {
            let lit = base + (1.0 - base) * light;
            lit * (1.0 + (blend + light).clamp(-1.0, 0.0))
        } else {
            base * (1.0 + blend.max(-1.0))
        }
    } else {
        base + (1.0 - base) * blend
    }
}

/// Which surface type the ambient blend is applied to.
///
/// Surfaces that fade do so whenever the ambient value is non-neutral and the window
/// condition permits; surfaces that don't fade always render at full brightness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// Ground tile and its upper layer. Faded only inside the ground-light window.
    Terrain,
    /// GRAPH.RES decorations. Same rules as terrain (same branch in the original).
    Decoration,
    /// Static map objects. Always faded unless the object carries two or more flames.
    StaticObject { self_lit: bool },
    /// Characters and dynamic objects: per-cell fine-block of `LIGHTS.N`.
    Dynamic,
}

/// Whether a piece on this surface should receive the ambient blend.
///
/// Confirmed sources:
/// - Terrain/Decoration: guarded by `_bLightGround && _bConfLightEffects`
///   (game.cpp:949: `if (!_bLightGround || !_bConfLightEffects) { SetFade(false) }`).
///   The user's "light effects" option belongs to ticket #97 — we assume it on.
/// - StaticObject: always fades except where `Flames.size() >= 2`
///   (game.cpp:1563-1580).
/// - Dynamic (characters/dynamic objects): fades where `LightMap2(tile) == 0`
///   (game.cpp:1089-1100, game.cpp:1611-1626); the fine cell's 0/≠0 distinction
///   decides whether the creature stands in a lit patch.
///
/// `fine_light_value` is the `u8` from [`k1_data::map::Lights::fine`]; only its
/// zero-ness matters. Only characters in ground-light window (windows 7–12) are
/// affected — the original calls `DrawGame()` → `SetFade()` on every pass inside that
/// window; outside it the fade flag stays false for all surfaces.
pub fn fades(surface: Surface, ground_window: bool, fine_light_value: u8) -> bool {
    match surface {
        Surface::Terrain | Surface::Decoration => ground_window,
        Surface::StaticObject { self_lit } => !self_lit,
        Surface::Dynamic => ground_window && fine_light_value == 0,
    }
}

/// Flat parameters the render layer is handed once per frame.
///
/// No option switches (they belong to ticket #97), no per-map constant light
/// (unconfirmed flag `[0x81D5DC]`). All values come from the simulation tick and
/// the session seed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightingParams {
    /// Ambient / 100 for surfaces that are faded whenever the ambient is non-neutral.
    pub ambient: [f32; 3],
    /// Ambient / 100 for ground and decorations: present only inside the ground-light window.
    pub ground_ambient: Option<[f32; 3]>,
    /// Tile-light colour / 200, added where mask brightness >= 0.01.
    pub ground_light: [f32; 3],
    /// The phase's ground-light window (ticks 8100..18900).
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
    let gw = time_of_day::ground_window(cycle_tick);
    let ground_ambient = if gw { Some(ambient) } else { None };
    let ground_light = ground_light_color(tick, seed);
    LightingParams {
        ambient,
        ground_ambient,
        ground_light,
        ground_window: gw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time_of_day;

    #[test]
    fn ambient_divided_by_100() {
        // Window 3 plateau: (12, 6, 0) / 100
        assert_eq!(channels(2700, NightPalette::Deep), [0.12, 0.06, 0.0],);
        // Window 5 plateau: (10, -20, -50) / 100
        assert_eq!(channels(5501, NightPalette::Deep), [0.1, -0.2, -0.5],);
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
        // Verify actual values at known points.
        assert_eq!(channels(11950, NightPalette::Deep), [-0.6, -0.7, -0.7]);
        assert_eq!(channels(11950, NightPalette::Green), [-0.6, -0.5, -0.7]);
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
        // Negative blend, zero light: classic multiply-darken.
        // base=0.6, blend=-0.7, light=0.0 → 0.6 * (1 + clamp(-0.7, -1, 0)) = 0.6 * 0.3 = 0.18
        let result = apply_channel(0.6, -0.7, 0.0, false);
        let expected = 0.18;
        assert!((result - expected).abs() < 1e-6, "{result} ~= {expected}");
    }

    #[test]
    fn apply_channel_negative_blend_with_bright_light() {
        // Light adds toward white first, then multiply cancels.
        // base=0.6, blend=-0.7, light=0.1 → lit = 0.6 + 0.4*0.1 = 0.64;
        //   clamped part = clamp(-0.7+0.1, -1, 0) = -0.6;
        //   result = 0.64 * (1 + (-0.6)) = 0.64 * 0.4 = 0.256
        let result = apply_channel(0.6, -0.7, 0.1, false);
        let expected = 0.64 * 0.4;
        assert!((result - expected).abs() < 1e-6, "{result} ~= {expected}");
    }

    #[test]
    fn apply_channel_light_below_threshold_does_not_add() {
        // light = 0.009 → below 0.01, so light term is skipped entirely.
        // Should equal plain multiply-darken: 0.6 * (1 + clamp(-0.7, -1, 0)) = 0.18
        let result = apply_channel(0.6, -0.7, 0.009, false);
        let expected = 0.18;
        assert!((result - expected).abs() < 1e-6);
    }

    #[test]
    fn apply_channel_blue_no_light_cancellation() {
        // Blue channel never cancels darken with tile-light source.
        // Non-blue does: light adds toward white first, then multiply darkens.
        // base=0.5, blend=-0.5, light=0.3
        let base_not_blue = apply_channel(0.5, -0.5, 0.3, false);
        let base_blue = apply_channel(0.5, -0.5, 0.3, true);
        // They differ because non-blue gets the light additive step.
        assert_ne!(base_not_blue, base_blue);
        // Expected: blue ignores light entirely → 0.5 * (1 + clamp(-0.5, -1, 0)) = 0.25
        assert!((base_blue - 0.25).abs() < 1e-6);
    }

    #[test]
    fn apply_channel_blue_ignores_light_positive_blend() {
        // Positive blend: both channels do the same thing (blend toward white).
        assert_eq!(
            apply_channel(0.5, 0.3, 0.2, true),
            apply_channel(0.5, 0.3, 0.2, false)
        );
    }

    #[test]
    fn applies_dynamic_based_on_fine_cell_only_in_ground_window() {
        // In ground window: fine == 0 → fades; fine != 0 → no fade.
        assert!(fades(Surface::Dynamic, true, 0)); // not in lit cell → fades
        assert!(!fades(Surface::Dynamic, true, 5)); // in lit cell → no fade
        // Outside ground window: regardless of fine, ambient is not applied
        // because we won't call fades with ground_window=false unless a surface
        // type explicitly decides otherwise (none do for Dynamic here).
        // The caller checks ground_window before passing Fine values to fades.
        assert!(!fades(Surface::Dynamic, false, 0)); // ground off → no fade
    }

    #[test]
    fn apply_channel_positive_blend() {
        // Blend toward white: base += (1 - base) * blend.
        // base=0.4, blend=0.3 → 0.4 + 0.6*0.3 = 0.58
        let result = apply_channel(0.4, 0.3, 0.0, false);
        let expected = 0.4 + 0.6 * 0.3;
        assert!((result - expected).abs() < 1e-6);
    }

    #[test]
    fn apply_channel_clamp_prevents_overbrighten() {
        // blend + light > 0 → clamped to 0, so no extra brightening beyond light add.
        // base=0.6, blend=-0.3, light=0.5 → lit = 0.6+0.4*0.5 = 0.8;
        //   clamp(-0.3+0.5, -1, 0) = 0.0; result = 0.8 * 1.0 = 0.8
        let result = apply_channel(0.6, -0.3, 0.5, false);
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
    fn fades_ground_terrain_only_in_ground_window() {
        assert!(fades(Surface::Terrain, true, 0));
        assert!(!fades(Surface::Terrain, false, 0));
        assert!(fades(Surface::Decoration, true, 0));
        assert!(!fades(Surface::Decoration, false, 0));
    }

    #[test]
    fn fades_static_object_except_self_lit() {
        assert!(fades(Surface::StaticObject { self_lit: false }, true, 0));
        assert!(!fades(Surface::StaticObject { self_lit: true }, true, 0));
    }

    #[test]
    fn params_ground_ambient_none_outside_8100_to_18900() {
        let t = params(7800, 42);
        assert!(t.ground_ambient.is_none());
        assert!(!t.ground_window);

        let t = params(8100, 42);
        assert!(t.ground_ambient.is_some());
        assert!(t.ground_window);

        let t = params(18899, 42);
        assert!(t.ground_ambient.is_some());
        assert!(t.ground_window);

        let t = params(18900, 42);
        assert!(t.ground_ambient.is_none());
        assert!(!t.ground_window);
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
