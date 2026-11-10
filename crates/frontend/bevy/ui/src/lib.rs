//! Shared Bevy UI design system for the «Князь» frontends.
//!
//! The pieces: the konung theme and design tokens ([`palette`], [`spacing`],
//! [`typo`]), the bundled PT Sans fonts, BSN widget helpers ([`widgets`]) and
//! [`KonungUiPlugin`] to bring it all up in one call. The pointer-capture
//! check and checkbox mirroring stay from the pre-design-system era.
//!
//! Consumers are the frontends (`kn-fe-viewer`, `kn-fe-player`). Nothing here
//! knows about input bindings, rendering or game rules.

pub mod palette;
pub mod spacing;
pub mod theme;
pub mod typo;
pub mod widgets;

pub use theme::konung_theme;
pub use typo::KonungFonts;

use bevy::{
    app::{App, Plugin},
    asset::Assets,
    feathers::FeathersPlugins,
    prelude::{Commands, Entity},
    text::Font,
    ui::Checked,
};

/// Brings up the konung UI: Feathers, the konung [`UiTheme`](bevy::feathers::
/// theme::UiTheme) and the bundled PT Sans fonts as [`KonungFonts`].
///
/// Requires the app to run `DefaultPlugins` (or at least initialize
/// `Assets<Font>`) before its first frame; the frontends always do.
pub struct KonungUiPlugin;

impl Plugin for KonungUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeathersPlugins);
        let mut font_assets = app.world_mut().resource_mut::<Assets<Font>>();
        let regular = font_assets.add(Font::from_bytes(typo::PT_SANS_REGULAR_BYTES.to_vec()));
        let bold = font_assets.add(Font::from_bytes(typo::PT_SANS_BOLD_BYTES.to_vec()));
        app.insert_resource(KonungFonts { regular, bold })
            .insert_resource(konung_theme());
    }
}

/// Cursor over the interface.
///
/// The value, not the presence of the component: `Hovered` in Bevy is a
/// boolean component, and the hover plugin only updates it on entities that
/// already have it. Bevy Feathers attaches `Hovered` to every one of its
/// controls at spawn, so "`Hovered` is on a UI node" means "the panel has a
/// widget", not "the cursor is over the panel" — with that test the map did
/// not move or zoom at all.
pub fn pointer_over_ui(mut hovers: impl Iterator<Item = bool>) -> bool {
    hovers.any(|hovered| hovered)
}

/// Mirrors a checkbox state back onto the widget.
///
/// Bevy's `Checkbox` computes the new value as `!Has<Checked>` and never
/// writes `Checked` itself (`bevy_ui_widgets::checkbox_self_update`) —
/// without this every click would send `true`.
pub fn sync_checked(commands: &mut Commands, widget: Entity, checked: bool) {
    let mut checkbox = commands.entity(widget);
    if checked {
        checkbox.insert(Checked);
    } else {
        checkbox.remove::<Checked>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widgets_without_the_cursor_are_not_the_ui_under_it() {
        // Feathers attaches Hovered to every one of its controls, and away
        // from the cursor it is false: by the mere presence of the component
        // the map lost both dragging and the wheel
        assert!(!pointer_over_ui([false, false, false].into_iter()));
        assert!(!pointer_over_ui(std::iter::empty()));
        assert!(pointer_over_ui([false, true, false].into_iter()));
    }

    // KonungUiPlugin itself is not headless-testable: Feathers embeds a Shader
    // asset, whose registration needs the render asset registry
    // (`init_asset::<Shader>()`), i.e. the render plugin graph. The showcase
    // example exercises the plugin under a real app.
}
