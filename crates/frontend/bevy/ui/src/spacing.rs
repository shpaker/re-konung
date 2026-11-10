//! Spacing scale and control sizing, in pixels.
//!
//! Feathers fixes its own metrics per control (`bevy_feathers::constants::size`:
//! rows 24 px, checkboxes 18 px) and they are not themeable; these consts are
//! for everything kn-ui lays out itself. Control heights diverge upward from
//! Feathers on purpose — a 28 px minimum is the modern comfortable target.

use bevy::ui::Val;

/// Hairline gaps inside a control. 4 px.
pub const SPACE_4: Val = Val::Px(4.0);
/// Gap between a caption and its control, sibling widgets in a row. 8 px.
pub const SPACE_8: Val = Val::Px(8.0);
/// Compact group padding. 12 px.
pub const SPACE_12: Val = Val::Px(12.0);
/// Gap between sections. 16 px.
pub const SPACE_16: Val = Val::Px(16.0);
/// Panel margin from the window edge, roomy sections. 24 px.
pub const SPACE_24: Val = Val::Px(24.0);
/// Page-level separation. 32 px.
pub const SPACE_32: Val = Val::Px(32.0);
/// Minimum height of kn-ui controls (Feathers rows are 24 px). 28 px.
pub const CONTROL_MIN_HEIGHT: Val = Val::Px(28.0);
/// Panel padding on both axes. 10 px.
pub const PANEL_PADDING: Val = Val::Px(10.0);
