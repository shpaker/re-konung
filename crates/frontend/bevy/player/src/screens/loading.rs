//! The loading screen: one honest line while the session builds. The build
//! itself is usually one frame, so this rarely shows — it exists for the
//! frames it honestly takes.

use bevy::prelude::*;
use bevy::scene::{EntityScene, Scene};
use kn_ui::{KonungFonts, palette, spacing, typo};

use crate::flow::{AppliedSettings, ScreenRoot};
use crate::i18n::{Text, t};
use crate::screens;

pub(crate) fn spawn(world: &mut World) {
    let fonts = screens::fonts(world);
    let language = world.resource::<AppliedSettings>().0.language;
    world
        .spawn_scene(root(&fonts, language))
        .expect("the loading root spawns");
}

fn root(fonts: &KonungFonts, language: kn_settings::Language) -> impl Scene {
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: spacing::SPACE_16,
        }
        BackgroundColor(palette::WINDOW_BG)
        ScreenRoot
        Children [
            {EntityScene(screens::text(
                fonts.regular.clone(),
                typo::BODY,
                t(language, Text::Loading),
            ))}
        ]
    }
}
