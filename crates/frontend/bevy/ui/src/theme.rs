//! The konung theme over Bevy Feathers 0.20, and what a Feathers theme can be.
//!
//! What Feathers 0.20 offers:
//! - `FeathersPlugins` is a plugin group (`TabNavigationPlugin` +
//!   `FeathersCorePlugin`). The core plugin embeds the FiraSans/FiraMono fonts,
//!   icons and shaders with `embedded_asset!`, registers the color observers
//!   and the propagation of `TextFont`/`TextColor`/`ThemeContext` down to
//!   `ThemedText` spans, and adds the focus-outline plugin.
//! - [`ThemeProps`] carries **colors only**: `token_assignments` (design token
//!   → semantic token), `semantic_base` and per-[`SurfaceLevel`]
//!   `semantic_overrides` (semantic token → color). Fonts, font sizes, corner
//!   radii and control metrics are not themeable — every Feathers control
//!   hardcodes `InheritableFont` with its embedded FiraSans path and its own
//!   literal sizes.
//!
//! So the konung theme is `create_dark_theme()` with the semantic colors
//! swapped for [`crate::palette`]: surfaces, border, text, accent, focus ring,
//! and the neutral button/field/item fills re-mapped onto warm palette steps
//! (the cool Feathers grays would clash with the warm surfaces). Fonts travel
//! outside the theme: [`crate::KonungUiPlugin`] embeds PT Sans, and the
//! [`crate::widgets`] helpers patch `InheritableFont` over each control — BSN
//! applies later components on the same entity as field-level patches over the
//! scene's own.
//!
//! What is deliberately not fought: border radii (4/6 px literals inside
//! Feathers controls), row heights and paddings. Identity in those dimensions
//! comes from [`crate::palette`], [`crate::spacing`] and [`crate::typo`]
//! applied in our own BSN styles — the same pattern as the viewer's
//! `ThemeBackgroundColor(tokens::WINDOW_BG)`.

use bevy::color::{Alpha, Color, Luminance};
use bevy::feathers::{
    dark_theme::create_dark_theme,
    theme::{SemanticToken, SurfaceLevel, ThemeProps, ThemeToken, UiTheme},
    tokens,
};

use crate::palette;

/// Design token for the border of kn-ui panels. Feathers has no generic panel
/// border token (only dialog/menu ones), so the konung theme registers its own
/// and maps it to the shared `border.default` semantic.
pub const PANEL_BORDER: ThemeToken = ThemeToken::new_static("konung.panel.border");

/// The Feathers dark theme recolored to the konung palette.
pub fn konung_theme() -> UiTheme {
    let mut props = create_dark_theme();

    // Surfaces: the three Feathers surface steps collapse onto the konung
    // dark scale — window darkest, panes on SURFACE, raised containers on
    // SURFACE_RAISED.
    recolor(
        &mut props,
        &tokens::semantic::SURFACE_WINDOW,
        palette::WINDOW_BG,
    );
    recolor(
        &mut props,
        &tokens::semantic::SURFACE_PANE_BODY,
        palette::SURFACE,
    );
    recolor(
        &mut props,
        &tokens::semantic::SURFACE_PANE_HEADER,
        palette::SURFACE,
    );
    recolor(
        &mut props,
        &tokens::semantic::SURFACE_SUBPANE_BODY,
        palette::SURFACE,
    );
    recolor(
        &mut props,
        &tokens::semantic::SURFACE_SUBPANE_HEADER,
        palette::SURFACE_RAISED,
    );
    recolor(
        &mut props,
        &tokens::semantic::SURFACE_GROUP,
        palette::SURFACE_RAISED,
    );
    recolor(
        &mut props,
        &tokens::semantic::SURFACE_DIALOG,
        palette::SURFACE_RAISED,
    );
    recolor(
        &mut props,
        &tokens::semantic::SURFACE_MENU,
        palette::SURFACE_RAISED,
    );
    recolor(
        &mut props,
        &tokens::semantic::BORDER_DEFAULT,
        palette::BORDER,
    );

    // Text.
    recolor(&mut props, &tokens::semantic::TEXT_DEFAULT, palette::TEXT);
    recolor(&mut props, &tokens::semantic::TEXT_DIM, palette::TEXT_MUTED);
    recolor(
        &mut props,
        &tokens::semantic::TEXT_DISABLED,
        palette::TEXT.with_alpha(0.4),
    );
    recolor(
        &mut props,
        &tokens::semantic::TEXT_ON_ACCENT,
        palette::TEXT_ON_ACCENT,
    );

    // Accent: checked checkboxes, slider bars, primary buttons, focus ring.
    recolor(
        &mut props,
        &tokens::semantic::FILL_ACCENT_DEFAULT,
        palette::ACCENT,
    );
    recolor(
        &mut props,
        &tokens::semantic::FILL_ACCENT_HOVER,
        palette::ACCENT_HOVER,
    );
    recolor(
        &mut props,
        &tokens::semantic::FILL_ACCENT_PRESSED,
        palette::ACCENT_HOVER.lighter(0.05),
    );
    recolor(
        &mut props,
        &tokens::semantic::FOCUS_RING,
        palette::ACCENT.with_alpha(0.5),
    );

    // Neutral fills re-mapped onto warm steps: solid = the button body, field
    // = slider tracks and inputs, item = list rows and plain buttons.
    recolor(
        &mut props,
        &tokens::semantic::FILL_SOLID_DEFAULT,
        palette::SURFACE_RAISED,
    );
    recolor(
        &mut props,
        &tokens::semantic::FILL_SOLID_HOVER,
        palette::BORDER,
    );
    recolor(
        &mut props,
        &tokens::semantic::FILL_SOLID_PRESSED,
        palette::BORDER.lighter(0.05),
    );
    recolor(
        &mut props,
        &tokens::semantic::FILL_SOLID_DISABLED,
        palette::SURFACE_RAISED.with_alpha(0.7),
    );
    recolor(
        &mut props,
        &tokens::semantic::FILL_ITEM_HOVER,
        palette::SURFACE_RAISED,
    );
    recolor(
        &mut props,
        &tokens::semantic::FILL_ITEM_PRESSED,
        palette::BORDER,
    );
    recolor(
        &mut props,
        &tokens::semantic::FILL_FIELD_DEFAULT,
        palette::BORDER,
    );
    recolor(
        &mut props,
        &tokens::semantic::FILL_FIELD_HOVER,
        palette::BORDER.lighter(0.03),
    );
    recolor(
        &mut props,
        &tokens::semantic::FILL_FIELD_PRESSED,
        palette::BORDER.lighter(0.06),
    );

    // Feathers lifts fills on raised surfaces (its `semantic_overrides` use
    // cool grays); the same correction with the warm steps keeps controls
    // readable on SURFACE_RAISED groups and dialogs.
    for level in [
        SurfaceLevel::Highest,
        SurfaceLevel::Higher,
        SurfaceLevel::Floating,
    ] {
        let overrides = props.semantic_overrides.get_mut(&level).unwrap();
        overrides.insert(
            tokens::semantic::FILL_SOLID_DEFAULT.clone(),
            palette::BORDER,
        );
        overrides.insert(
            tokens::semantic::FILL_SOLID_HOVER.clone(),
            palette::BORDER.lighter(0.05),
        );
        overrides.insert(
            tokens::semantic::FILL_SOLID_PRESSED.clone(),
            palette::BORDER.lighter(0.1),
        );
        overrides.insert(
            tokens::semantic::FILL_FIELD_DEFAULT.clone(),
            palette::BORDER,
        );
        overrides.insert(
            tokens::semantic::FILL_ITEM_HOVER.clone(),
            palette::SURFACE_RAISED,
        );
        overrides.insert(tokens::semantic::FILL_ITEM_PRESSED.clone(), palette::BORDER);
    }

    // The kn-ui panel border rides the shared border semantic.
    props
        .token_assignments
        .insert(PANEL_BORDER, tokens::semantic::BORDER_DEFAULT.clone());

    UiTheme(props)
}

fn recolor(props: &mut ThemeProps, token: &SemanticToken, color: Color) {
    props.semantic_base.insert(token.clone(), color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feathers_tokens_answer_with_the_konung_palette() {
        let theme = konung_theme();
        assert_eq!(theme.color(&tokens::WINDOW_BG), palette::WINDOW_BG);
        assert_eq!(theme.color(&tokens::TEXT_MAIN), palette::TEXT);
        assert_eq!(theme.color(&tokens::TEXT_DIM), palette::TEXT_MUTED);
        assert_eq!(theme.color(&tokens::BUTTON_PRIMARY_BG), palette::ACCENT);
        assert_eq!(
            theme.color(&tokens::BUTTON_PRIMARY_BG_HOVER),
            palette::ACCENT_HOVER
        );
    }

    #[test]
    fn the_panel_border_token_resolves_to_the_konung_border() {
        assert_eq!(konung_theme().color(&PANEL_BORDER), palette::BORDER);
    }
}
