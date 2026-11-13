//! The visual side of each screen: BSN scenes spawned and despawned by the
//! flow's `sync_screens`, plus the state→widget mirroring of the settings
//! screen. Views read [`crate::flow::AppFlow`] state and the applied
//! settings; user intents leave them only as typed actions.

pub(crate) mod hero_select;
pub(crate) mod loading;
pub(crate) mod main_menu;
pub(crate) mod settings;

use bevy::asset::Handle;
use bevy::feathers::display::caption;
use bevy::feathers::font_styles::InheritableFont;
use bevy::prelude::*;
use bevy::scene::Scene;
use bevy::text::{FontSize, FontWeight};
use kn_ui::KonungFonts;

/// One line of interface text: a wrapper node carrying the face and size — a
/// `TextFont` written directly on the span would reset the font to Bevy's
/// default, the propagation travels over `ThemedText` spans instead (the
/// showcase's `text_block` finding).
pub(crate) fn text(font: Handle<Font>, size: FontSize, string: &str) -> impl Scene {
    bsn! {
        Node
        InheritableFont {
            font: {font},
            font_size: {size},
            weight: FontWeight::NORMAL,
        }
        Children [@caption(string)]
    }
}

/// The fonts every view composes with; the resource is present in any app
/// that runs [`kn_ui::KonungUiPlugin`] (or the headless equivalent).
pub(crate) fn fonts(world: &World) -> KonungFonts {
    world.resource::<KonungFonts>().clone()
}
