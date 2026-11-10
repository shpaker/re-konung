//! Minimal BSN composition helpers shared by kn-ui screens.
//!
//! Each helper is a plain function returning a scene, mirroring the viewer's
//! `panel` (viewer/src/lib.rs): compose, patch, no widget framework. Feathers
//! controls are used as-is with the konung identity patched on top — the theme
//! cannot carry fonts or metrics, so every control gets its
//! `InheritableFont` overwritten with the bundled PT Sans and its height with
//! our spacing tokens.

use bevy::feathers::{
    controls::{FeathersButton, FeathersCheckbox, FeathersSlider},
    display::caption,
    font_styles::InheritableFont,
    theme::{InheritableThemeTextColor, ThemeBackgroundColor, ThemeBorderColor},
    tokens,
};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::text::FontWeight;

use crate::{spacing, theme::PANEL_BORDER, typo, typo::KonungFonts};

/// A panel: the base container of kn-ui screens.
///
/// Positioning is left to the caller — in flow by default, floating when
/// patched with `Node { position_type: PositionType::Absolute, .. }`.
pub fn panel(fonts: &KonungFonts, children: impl SceneList) -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: spacing::SPACE_8,
            padding: UiRect::axes(spacing::PANEL_PADDING, spacing::PANEL_PADDING),
            border: px(1.0),
            border_radius: px(6.0),
        }
        // The whole panel area must read as "over the UI" for camera input,
        // not only the widgets: Bevy updates `Hovered` only on entities that
        // already have it, and Feathers puts it on its controls alone — the
        // empty gaps would leak drags and the wheel to the camera
        // (see [`crate::pointer_over_ui`]).
        Hovered
        ThemeBackgroundColor(tokens::WINDOW_BG)
        ThemeBorderColor(PANEL_BORDER)
        InheritableFont {
            font: {fonts.regular.clone()},
            font_size: typo::BODY,
            weight: FontWeight::NORMAL,
        }
        InheritableThemeTextColor(tokens::TEXT_MAIN)
        Children [{children}]
    }
}

/// A menu button in the konung typography and height.
pub fn menu_button(fonts: &KonungFonts, label: &str) -> impl Scene {
    bsn! {
        @FeathersButton {
            @caption: bsn! { @caption(label) }
        }
        Node {
            height: spacing::CONTROL_MIN_HEIGHT,
        }
        InheritableFont {
            font: {fonts.regular.clone()},
            font_size: typo::BODY,
            weight: FontWeight::NORMAL,
        }
    }
}

/// A checkbox with a caption.
///
/// State and the `ValueChange<bool>` observer are the caller's: compose them
/// onto the returned scene, as the viewer does with its `OverlayToggle`.
pub fn checkbox_row(fonts: &KonungFonts, label: &str) -> impl Scene {
    bsn! {
        @FeathersCheckbox {
            @caption: bsn! { @caption(label) }
        }
        InheritableFont {
            font: {fonts.regular.clone()},
            font_size: typo::BODY,
            weight: FontWeight::NORMAL,
        }
    }
}

/// A captioned slider column.
///
/// The numeric readout inside FeathersSlider keeps its embedded monospace
/// font: it hangs off an inner node of the control's own scene, out of reach
/// of patching from outside.
pub fn slider_row(fonts: &KonungFonts, label: &str, minimum: f32, maximum: f32) -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: spacing::SPACE_4,
        }
        InheritableFont {
            font: {fonts.regular.clone()},
            font_size: typo::BODY,
            weight: FontWeight::NORMAL,
        }
        InheritableThemeTextColor(tokens::TEXT_DIM)
        Children [
            @caption(label)
            --
            @FeathersSlider { @min: {minimum}, @max: {maximum} }
        ]
    }
}
