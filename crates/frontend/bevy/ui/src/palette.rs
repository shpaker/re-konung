//! Semantic colors of the konung design system.
//!
//! A dark warm neutral scale with a muted amber accent — the starting values,
//! tuned later at visual acceptance. These consts are the single source of
//! color truth: [`crate::theme`] maps Feathers semantic tokens onto them, BSN
//! styles (swatches, borders, overlays) read them directly. Feathers' own
//! `bevy::feathers::palette` stays untouched and unused.

use bevy::color::{Color, Srgba};

/// sRGB triplets from hex components; `Color::Srgba` is const-constructible.
const fn rgb(red: u8, green: u8, blue: u8) -> Color {
    Color::Srgba(Srgba::new(
        red as f32 / 255.0,
        green as f32 / 255.0,
        blue as f32 / 255.0,
        1.0,
    ))
}

/// Window background, the darkest layer. `#14120F`.
pub const WINDOW_BG: Color = rgb(0x14, 0x12, 0x0F);
/// Panels and pane bodies. `#1E1B17`.
pub const SURFACE: Color = rgb(0x1E, 0x1B, 0x17);
/// Raised surfaces: groups, dialogs, menus. `#26221D`.
pub const SURFACE_RAISED: Color = rgb(0x26, 0x22, 0x1D);
/// Borders and outlines. `#3A342C`.
pub const BORDER: Color = rgb(0x3A, 0x34, 0x2C);
/// Neutral control fill, visibly raised above the panel. `#373027`.
pub const CONTROL_BG: Color = rgb(0x37, 0x30, 0x27);
/// Neutral control hover fill. `#4A4033`.
pub const CONTROL_HOVER: Color = rgb(0x4A, 0x40, 0x33);
/// Control outline, distinct from the panel border. `#6D604E`.
pub const CONTROL_BORDER: Color = rgb(0x6D, 0x60, 0x4E);
/// Primary text. `#E8E2D6`.
pub const TEXT: Color = rgb(0xE8, 0xE2, 0xD6);
/// Secondary, dimmed text. `#A79E8E`.
pub const TEXT_MUTED: Color = rgb(0xA7, 0x9E, 0x8E);
/// Text on the accent fills. `#14120F`, the WINDOW_BG tone: the accent is a
/// light amber, so its text is dark — light-on-amber measured ~2.3:1, this
/// passes WCAG AA on both accent steps (see the tests below).
pub const TEXT_ON_ACCENT: Color = rgb(0x14, 0x12, 0x0F);
/// The accent: selection, checked states, primary buttons. `#C89B5A`.
pub const ACCENT: Color = rgb(0xC8, 0x9B, 0x5A);
/// The accent under the pointer. `#D9AF6F`.
pub const ACCENT_HOVER: Color = rgb(0xD9, 0xAF, 0x6F);
/// Destructive and error states. `#B3564D`.
pub const DANGER: Color = rgb(0xB3, 0x56, 0x4D);

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG relative luminance: sRGB → linear → the three fixed coefficients.
    fn relative_luminance(color: Color) -> f32 {
        let srgba = color.to_srgba();
        let channel = |c: f32| {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(srgba.red) + 0.7152 * channel(srgba.green) + 0.0722 * channel(srgba.blue)
    }

    /// WCAG contrast ratio of a foreground/background pair.
    fn contrast_ratio(foreground: Color, background: Color) -> f32 {
        let (a, b) = (
            relative_luminance(foreground),
            relative_luminance(background),
        );
        let (lighter, darker) = if a >= b { (a, b) } else { (b, a) };
        (lighter + 0.05) / (darker + 0.05)
    }

    const AA: f32 = 4.5;

    #[test]
    fn text_on_window_bg_passes_aa() {
        assert!(contrast_ratio(TEXT, WINDOW_BG) >= AA);
    }

    #[test]
    fn text_on_surface_passes_aa() {
        assert!(contrast_ratio(TEXT, SURFACE) >= AA);
    }

    #[test]
    fn text_on_surface_raised_passes_aa() {
        assert!(contrast_ratio(TEXT, SURFACE_RAISED) >= AA);
    }

    #[test]
    fn neutral_control_text_passes_aa() {
        for background in [CONTROL_BG, CONTROL_HOVER] {
            assert!(contrast_ratio(TEXT, background) >= AA);
        }
    }

    #[test]
    fn text_muted_on_window_bg_passes_aa() {
        assert!(contrast_ratio(TEXT_MUTED, WINDOW_BG) >= AA);
    }

    #[test]
    fn text_muted_on_surface_passes_aa() {
        assert!(contrast_ratio(TEXT_MUTED, SURFACE) >= AA);
    }

    #[test]
    fn text_muted_on_surface_raised_passes_aa() {
        assert!(contrast_ratio(TEXT_MUTED, SURFACE_RAISED) >= AA);
    }

    #[test]
    fn text_on_accent_on_accent_passes_aa() {
        assert!(contrast_ratio(TEXT_ON_ACCENT, ACCENT) >= AA);
    }

    #[test]
    fn text_on_accent_on_accent_hover_passes_aa() {
        assert!(contrast_ratio(TEXT_ON_ACCENT, ACCENT_HOVER) >= AA);
    }
}
