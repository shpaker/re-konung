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
/// Primary text. `#E8E2D6`.
pub const TEXT: Color = rgb(0xE8, 0xE2, 0xD6);
/// Secondary, dimmed text. `#A79E8E`.
pub const TEXT_MUTED: Color = rgb(0xA7, 0x9E, 0x8E);
/// Text on the accent fills. `#FBF6EC`.
pub const TEXT_ON_ACCENT: Color = rgb(0xFB, 0xF6, 0xEC);
/// The accent: selection, checked states, primary buttons. `#C89B5A`.
pub const ACCENT: Color = rgb(0xC8, 0x9B, 0x5A);
/// The accent under the pointer. `#D9AF6F`.
pub const ACCENT_HOVER: Color = rgb(0xD9, 0xAF, 0x6F);
/// Destructive and error states. `#B3564D`.
pub const DANGER: Color = rgb(0xB3, 0x56, 0x4D);
