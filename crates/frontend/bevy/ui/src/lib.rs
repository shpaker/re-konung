//! Shared Bevy UI mechanics for the «Князь» frontends: the Feathers theme,
//! checkbox mirroring and the pointer-capture check.
//!
//! Consumers are the frontends (`kn-viewer`, `kn-exec`). Nothing here knows
//! about panels, widget state, input bindings, rendering or game rules.

use bevy::{
    feathers::{dark_theme::create_dark_theme, theme::UiTheme},
    prelude::{Commands, Entity},
    ui::Checked,
};

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

/// The shared Feathers dark theme resource the frontends insert into their App.
pub fn dark_theme() -> UiTheme {
    UiTheme(create_dark_theme())
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
}
