//! Minimal BSN composition helpers shared by kn-ui screens.
//!
//! Each helper is a plain function returning a scene, mirroring the viewer's
//! `panel` (viewer/src/lib.rs): compose, patch, no widget framework. Feathers
//! controls are used as-is with the konung identity patched on top — the theme
//! cannot carry fonts or metrics, so every control root gets its
//! `InheritableFont` overwritten with the bundled PT Sans and its height with
//! our spacing tokens. The interiors Feathers composes itself (the select's
//! trigger and rows, the slider's readout) are normalized by
//! [`crate::KonungUiPlugin`] instead: their own `InheritableFont` is a
//! propagation barrier no root patch can cross.

use bevy::feathers::{
    controls::{FeathersButton, FeathersCheckbox},
    display::caption,
    font_styles::InheritableFont,
    theme::{InheritableThemeTextColor, ThemeBackgroundColor, ThemeBorderColor},
    tokens,
};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::scene::EntityScene;
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

/// A captioned slider column. The slider is the caller's scene, composed
/// onto the FeathersSlider entity (`SliderStep`, `SliderValue`,
/// `InteractionDisabled`, markers, `on(...)` observers) — the helper wraps
/// it, so anything the caller adds lands on the control itself, not on the
/// column. The readout inside is normalized to PT Sans by
/// [`crate::KonungUiPlugin`], like every control interior.
pub fn slider_row(fonts: &KonungFonts, label: &str, slider: impl Scene) -> impl Scene {
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
            {EntityScene(slider)}
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::feathers::controls::FeathersSlider;
    use bevy::ui::widget::Text;
    use bevy::ui_widgets::{Slider, SliderStep};

    /// Marks the caller's slider scene in the placement test.
    #[derive(Component, Clone, Copy, Default)]
    struct ProbeMarker;

    /// The slider the caller composes — step, marker, value — keeps every
    /// component on the FeathersSlider entity, and the caption column wraps
    /// it as a separate ancestor. Spawned headless the way the viewer's
    /// slider tests do: assets for the embedded font, a theme to read.
    #[test]
    fn slider_row_composes_onto_the_slider_entity() {
        let mut app = crate::tests::headless_ui_app();
        let fonts = app.world().resource::<KonungFonts>().clone();
        let row = app
            .world_mut()
            .spawn_scene(slider_row(
                &fonts,
                "Громкость",
                bsn! {
                    @FeathersSlider { @min: 0.0, @max: 24.0 }
                    SliderStep(1.0)
                    ProbeMarker
                },
            ))
            .expect("the row spawns")
            .id();

        let mut sliders = app.world_mut().query_filtered::<Entity, With<Slider>>();
        let slider = sliders.single(app.world()).expect("one slider");
        assert_ne!(slider, row, "the control is not the column root");
        assert_eq!(
            app.world().entity(slider).get::<SliderStep>().unwrap().0,
            1.0,
            "the caller's step lands on the slider"
        );
        assert!(
            app.world().entity(slider).get::<ProbeMarker>().is_some(),
            "the caller's marker lands on the slider"
        );

        // The column is the slider's direct parent and carries the caption
        let mut parents = app.world_mut().query::<&ChildOf>();
        assert_eq!(
            parents.get(app.world(), slider).unwrap().parent(),
            row,
            "the column wraps the slider"
        );
        let caption_entities: Vec<Entity> = app
            .world()
            .entity(row)
            .get::<Children>()
            .unwrap()
            .iter()
            .collect();
        let mut texts = app.world_mut().query::<&Text>();
        let caption = caption_entities
            .iter()
            .find_map(|child| texts.get(app.world(), *child).ok())
            .expect("the column carries the caption");
        assert_eq!(caption.0, "Громкость");
    }
}
