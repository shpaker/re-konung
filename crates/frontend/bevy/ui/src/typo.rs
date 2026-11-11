//! Typography scale and the bundled fonts.
//!
//! The scale aligns with Feathers where one exists
//! (`bevy_feathers::constants::size`): captions 12 = its `SMALL_FONT`, body
//! 14 = its `MEDIUM_FONT`. Titles diverge: Feathers uses 16 for dialog
//! headers, screens here use 18; display 24 has no Feathers counterpart.
//!
//! The bundled family is PT Sans (SIL OFL, license in `assets/fonts/OFL.txt`):
//! full Cyrillic and Latin coverage, unlike the embedded Feathers fonts and
//! Bevy's default. The bytes ship inside the crate; [`crate::KonungUiPlugin`]
//! turns them into [`KonungFonts`] at startup.

use bevy::asset::Handle;
use bevy::ecs::resource::Resource;
use bevy::text::{Font, FontSize};

/// Field captions and secondary labels. 12 px.
pub const CAPTION: FontSize = FontSize::Px(12.0);
/// Body text and widget labels. 14 px.
pub const BODY: FontSize = FontSize::Px(14.0);
/// Section and screen titles. 18 px.
pub const TITLE: FontSize = FontSize::Px(18.0);
/// Display text, the screen heading of the showcase. 24 px.
pub const DISPLAY: FontSize = FontSize::Px(24.0);

/// Bundled PT Sans Regular, embedded at compile time.
pub(crate) const PT_SANS_REGULAR_BYTES: &[u8] =
    include_bytes!("../assets/fonts/PT_Sans-Web-Regular.ttf");
/// Bundled PT Sans Bold, embedded at compile time.
pub(crate) const PT_SANS_BOLD_BYTES: &[u8] = include_bytes!("../assets/fonts/PT_Sans-Web-Bold.ttf");

/// Handles of the bundled PT Sans faces, inserted by [`crate::KonungUiPlugin`].
///
/// Scene-building code (BSN helpers, screens) reads this resource and passes
/// the handles into [`bevy::feathers::font_styles::InheritableFont`] — Feathers
/// themes cannot carry fonts, so the kn-ui helpers patch every control root
/// with these handles, and the plugin's font normalization rewrites the
/// composites' interiors that no root patch reaches.
#[derive(Clone, Resource)]
pub struct KonungFonts {
    /// PT Sans Regular.
    pub regular: Handle<Font>,
    /// PT Sans Bold.
    pub bold: Handle<Font>,
}
