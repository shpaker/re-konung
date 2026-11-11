//! Shared Bevy UI design system for the «Князь» frontends.
//!
//! The pieces: the konung theme and design tokens ([`palette`], [`spacing`],
//! [`typo`]), the bundled PT Sans fonts, BSN widget helpers ([`widgets`]) and
//! [`KonungUiPlugin`] to bring it all up in one call. The pointer-capture
//! check and checkbox mirroring stay from the pre-design-system era.
//!
//! One typeface everywhere: Feathers controls hardcode their embedded
//! FiraSans on interior nodes of their own scenes, and every
//! `InheritableFont` is a propagation barrier, so the plugin rewrites every
//! added `InheritableFont`'s handle to the bundled PT Sans
//! ([`normalize_control_fonts`]) — including the composites' interiors the
//! per-control patches of [`widgets`] cannot reach. Icons are `ImageNode`s
//! and never ride `InheritableFont`, so their glyphs survive.
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
    app::{App, Plugin, PostUpdate, PropagateSet},
    asset::Assets,
    ecs::schedule::IntoScheduleConfigs,
    feathers::{FeathersPlugins, font_styles::InheritableFont},
    prelude::{Added, Commands, Entity, Query, Res},
    text::{Font, FontWeight, TextFont},
    ui::Checked,
};

/// Brings up the konung UI: Feathers, the konung [`UiTheme`](bevy::feathers::
/// theme::UiTheme), the bundled PT Sans fonts as [`KonungFonts`] and the
/// font normalization that keeps every control interior in PT Sans.
///
/// Requires the app to run `DefaultPlugins` (or at least initialize
/// `Assets<Font>`) before its first frame; the frontends always do.
pub struct KonungUiPlugin;

impl Plugin for KonungUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeathersPlugins);
        let fonts = {
            let mut font_assets = app.world_mut().resource_mut::<Assets<Font>>();
            konung_fonts(&mut font_assets)
        };
        app.insert_resource(fonts)
            .insert_resource(konung_theme())
            // Before the propagation set, so the rewritten handle reaches the
            // text spans in the same frame it is spawned.
            .add_systems(
                PostUpdate,
                normalize_control_fonts.before(PropagateSet::<TextFont>::default()),
            );
    }
}

/// The bundled PT Sans faces added to the asset store and returned as
/// [`KonungFonts`]. [`KonungUiPlugin`] uses this; headless setups that cannot
/// run the plugin (its Shader asset needs the render registry) build the same
/// resource with it.
pub fn konung_fonts(fonts: &mut Assets<Font>) -> KonungFonts {
    KonungFonts {
        regular: fonts.add(Font::from_bytes(typo::PT_SANS_REGULAR_BYTES.to_vec())),
        bold: fonts.add(Font::from_bytes(typo::PT_SANS_BOLD_BYTES.to_vec())),
    }
}

/// Rewrites the font handle of every added [`InheritableFont`] to the bundled
/// PT Sans, picked by the component's weight; size and weight stay untouched.
///
/// Feathers composites keep their own `InheritableFont` on interior nodes
/// (the select's trigger and list rows, the slider's numeric readout), and
/// the component requires `PropagateOver<TextFont>` — a barrier no patch
/// composed on the control root can cross. Re-inserting the component here
/// re-fires Feathers' own insert observer, which re-propagates the PT Sans
/// handle into that node's subtree; propagation is change-driven, so the
/// rewritten value is what survives. Handles already pointing at PT Sans
/// (the kn-ui patches) are left alone.
fn normalize_control_fonts(
    fonts: Res<KonungFonts>,
    added: Query<(Entity, &InheritableFont), Added<InheritableFont>>,
    mut commands: Commands,
) {
    for (entity, font) in &added {
        if font.font == fonts.regular || font.font == fonts.bold {
            continue;
        }
        let face = if font.weight >= FontWeight::MEDIUM {
            fonts.bold.clone()
        } else {
            fonts.regular.clone()
        };
        commands.entity(entity).insert(InheritableFont {
            font: face,
            ..font.clone()
        });
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
    use super::{
        InheritableFont, KonungFonts, konung_theme, normalize_control_fonts, pointer_over_ui, typo,
    };
    use bevy::feathers::controls::FeathersSlider;
    use bevy::prelude::*;
    use bevy::scene::{ScenePlugin, WorldSceneExt};

    #[test]
    fn widgets_without_the_cursor_are_not_the_ui_under_it() {
        // Feathers attaches Hovered to every one of its controls, and away
        // from the cursor it is false: by the mere presence of the component
        // the map lost both dragging and the wheel
        assert!(!pointer_over_ui([false, false, false].into_iter()));
        assert!(!pointer_over_ui(std::iter::empty()));
        assert!(pointer_over_ui([false, true, false].into_iter()));
    }

    /// A headless app that can spawn Feathers scenes: assets for the embedded
    /// font paths, the scene machinery and a theme the controls read at
    /// spawn. `KonungUiPlugin` itself needs the render registry (Feathers
    /// registers a Shader asset), so the normalization system is added by
    /// hand — it has no such requirement.
    pub(crate) fn headless_ui_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins((AssetPlugin::default(), ScenePlugin))
            .init_asset::<Font>()
            .insert_resource(konung_theme());
        let mut font_assets = app.world_mut().resource_mut::<Assets<Font>>();
        let regular = font_assets.add(Font::from_bytes(typo::PT_SANS_REGULAR_BYTES.to_vec()));
        let bold = font_assets.add(Font::from_bytes(typo::PT_SANS_BOLD_BYTES.to_vec()));
        app.insert_resource(KonungFonts { regular, bold })
            .add_systems(PostUpdate, normalize_control_fonts);
        app
    }

    /// The composite interior Feathers hides from outside patching — the
    /// slider's readout node with its embedded FiraMono — comes out in PT
    /// Sans, size and weight untouched.
    #[test]
    fn feathers_interiors_are_normalized_to_pt_sans() {
        let mut app = headless_ui_app();
        let fonts = app.world().resource::<KonungFonts>().clone();
        app.world_mut()
            .spawn_scene_list(bsn! {
                @FeathersSlider { @min: 0.0, @max: 100.0 }
            })
            .expect("the slider scene spawns");
        app.update();

        // The readout container is the only InheritableFont in the subtree
        let mut inheritable = app.world_mut().query::<&InheritableFont>();
        let font = inheritable.single(app.world()).unwrap().clone();
        assert_eq!(
            font.font, fonts.regular,
            "the readout's FiraMono is rewritten, not left to tofu on Cyrillic"
        );
        assert_eq!(
            font.font_size,
            bevy::text::FontSize::Px(12.0),
            "the readout keeps its own size"
        );
        assert_eq!(font.weight, FontWeight::NORMAL);
    }

    /// A weight the family answers with its bold face; everything Feathers
    /// ships today is NORMAL, this pins the mapping for the day one is not.
    #[test]
    fn bold_weight_normalizes_to_the_bold_face() {
        let mut app = headless_ui_app();
        let fonts = app.world().resource::<KonungFonts>().clone();
        app.world_mut().spawn(InheritableFont {
            font: Default::default(),
            font_size: typo::BODY,
            weight: FontWeight::BOLD,
        });
        app.update();

        let mut inheritable = app.world_mut().query::<&InheritableFont>();
        let font = inheritable.single(app.world()).unwrap();
        assert_eq!(font.font, fonts.bold);
    }
}
