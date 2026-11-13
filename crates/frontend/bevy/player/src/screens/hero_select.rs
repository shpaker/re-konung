//! The hero selection of the new-game flow: one button per hero the game
//! directory carries, captioned the way the original's new-game menu captions
//! them — with the name from the game data. The choice leaves only as the
//! typed `HeroSelectAction`; the reducer owns who starts a session.

use bevy::prelude::*;
use bevy::scene::{EntityScene, Scene};
use bevy::ui_widgets::Activate;
use kn_app::{Action, HeroSelectAction};
use kn_render::GameResource;
use kn_ui::{KonungFonts, palette, spacing, typo, widgets};

use crate::flow::{AppFlow, AppliedSettings, ScreenRoot};
use crate::i18n::{Text, t};
use crate::screens;

/// Which `Game.N` the clicked button stands for; the shared observer reads it
/// off the activated entity, so the button list stays data-driven.
#[derive(Component, Clone, Copy, Default)]
struct HeroButton(u32);

/// Spawns the hero-select root for the current state. Called by the flow when
/// the screen becomes `HeroSelect`; the whole tree dies with its
/// [`ScreenRoot`].
pub(crate) fn spawn(world: &mut World) {
    let fonts = screens::fonts(world);
    let language = world.resource::<AppliedSettings>().0.language;
    let heroes = world.resource::<AppFlow>().state().heroes.clone();
    // The names are game data: absent game (headless worlds) or an unnamed
    // record fall back to a localized slot label — the button list itself
    // never depends on the data being readable.
    let names = world
        .get_resource::<GameResource>()
        .and_then(|game| game.0.hero_names(kn_game::MapSource::Game).ok())
        .unwrap_or_default();
    world
        .spawn_scene(root(&fonts, language, &heroes, &names))
        .expect("the hero-select root spawns");
}

fn root(
    fonts: &KonungFonts,
    language: kn_settings::Language,
    heroes: &[u32],
    names: &[(u32, String)],
) -> impl Scene {
    let buttons = heroes
        .iter()
        .enumerate()
        .map(|(row, &hero)| {
            let name = names
                .iter()
                .find(|(number, _)| *number == hero)
                .map(|(_, name)| name.as_str())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| t(language, Text::HeroSlot(row + 1)));
            hero_button(fonts, hero, name)
        })
        .collect::<Vec<_>>();
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: spacing::SPACE_24,
        }
        BackgroundColor(palette::WINDOW_BG)
        ScreenRoot
        Children [
            {EntityScene(screens::text(
                fonts.bold.clone(),
                typo::DISPLAY,
                t(language, Text::HeroTitle),
            ))}
            --
            {EntityScene(widgets::panel(
                fonts,
                buttons,
            ))}
            --
            {EntityScene(back_button(fonts, language))}
        ]
    }
}

/// One hero option: the menu button of the panel plus the marker the shared
/// observer reads — a data-driven count of buttons cannot carry one `on()`
/// handler per row.
fn hero_button(fonts: &KonungFonts, hero: u32, label: &str) -> impl Scene {
    bsn! {
        @widgets::menu_button(fonts, label)
        HeroButton(hero)
        on(click_hero_button)
    }
}

fn back_button(fonts: &KonungFonts, language: kn_settings::Language) -> impl Scene {
    bsn! {
        @widgets::menu_button(fonts, t(language, Text::Back))
        on(click_back)
    }
}

fn click_hero_button(click: On<Activate>, buttons: Query<&HeroButton>, mut flow: ResMut<AppFlow>) {
    if let Ok(&HeroButton(hero)) = buttons.get(click.entity) {
        flow.submit(Action::HeroSelect(HeroSelectAction::Choose(hero)));
    }
}

fn click_back(_click: On<Activate>, mut flow: ResMut<AppFlow>) {
    flow.submit(Action::HeroSelect(HeroSelectAction::Cancel));
}
