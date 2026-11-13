//! The hero selection of the new-game flow, in the spirit of the original's
//! new-game menu (Slavik `game_newgame.cpp:115-186`) and the classical CRPG
//! presentation: a card per offered hero on the left, the chosen hero's
//! dossier on the right — base attributes, health, carry limit, max party,
//! the seven skills, level, experience, gold, free points — every number the
//! game data carries and the original showed, nothing invented. The choice
//! leaves only as the typed `HeroSelectAction`; the reducer owns who starts
//! a session.

use bevy::feathers::{controls::ButtonVariant, display::caption, font_styles::InheritableFont};
use bevy::prelude::*;
use bevy::scene::{EntityScene, Scene, SceneList};
use bevy::text::FontWeight;
use bevy::ui_widgets::Activate;
use kn_app::{Action, HeroSelectAction};
use kn_game::{HeroSummary, MapSource};
use kn_render::GameResource;
use kn_ui::{KonungFonts, palette, spacing, typo, widgets};

use crate::flow::{AppFlow, AppliedSettings, ScreenRoot};
use crate::i18n::{Text, t};
use crate::screens;

/// Which preview the on-screen view was built for: a change means its
/// captions are stale, and the view respawns wholesale — the settings
/// screen's `BuiltLanguage` pattern.
#[derive(Resource)]
struct HeroSelectView(Option<u32>);

/// Which `Game.N` the clicked card stands for; the shared observer reads it
/// off the activated entity, so the card list stays data-driven.
#[derive(Component, Clone, Copy, Default)]
struct HeroCard(u32);

/// Spawns the hero-select root for the current state. Called by the flow
/// when the screen becomes `HeroSelect`; the whole tree dies with its
/// [`ScreenRoot`].
pub(crate) fn spawn(world: &mut World) {
    let fonts = screens::fonts(world);
    let language = world.resource::<AppliedSettings>().0.language;
    let (heroes, preview) = {
        let state = world.resource::<AppFlow>().state();
        (state.heroes.clone(), state.hero_preview)
    };
    // The dossier is game data: absent game (headless worlds) or texts the
    // exe does not carry fall back to localized slot labels — the card list
    // itself never depends on the data being readable.
    let summaries = world
        .get_resource::<GameResource>()
        .and_then(|game| game.0.hero_summaries(MapSource::Game).ok())
        .unwrap_or_default();
    world
        .spawn_scene(root(&fonts, language, &heroes, preview, &summaries))
        .expect("the hero-select root spawns");
    world.insert_resource(HeroSelectView(preview));
}

/// Respawns the view when the previewed hero moved: the detail panel and the
/// card highlight both follow the state. Runs after the flow chain, like the
/// settings screen's own sync.
pub(crate) fn sync(world: &mut World) {
    let Some(built) = world.get_resource::<HeroSelectView>().map(|view| view.0) else {
        return;
    };
    let state = world.resource::<AppFlow>().state();
    if state.screen != kn_app::Screen::HeroSelect || state.hero_preview == built {
        return;
    }
    let mut roots = world.query_filtered::<Entity, With<ScreenRoot>>();
    let old: Vec<Entity> = roots.iter(world).collect();
    for root in old {
        world.despawn(root);
    }
    spawn(world);
}

fn root(
    fonts: &KonungFonts,
    language: kn_settings::Language,
    heroes: &[u32],
    preview: Option<u32>,
    summaries: &[HeroSummary],
) -> impl Scene {
    let cards = heroes
        .iter()
        .enumerate()
        .map(|(row, &hero)| {
            let name = summaries
                .iter()
                .find(|summary| summary.hero == hero)
                .map(|summary| summary.name.as_str())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| t(language, Text::HeroSlot(row + 1)));
            hero_card(fonts, hero, name, preview == Some(hero))
        })
        .collect::<Vec<_>>();
    let dossier = preview.and_then(|hero| summaries.iter().find(|summary| summary.hero == hero));
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
            {EntityScene(dossier_row(
                fonts,
                language,
                cards,
                dossier,
            ))}
            --
            {EntityScene(action_row(fonts, language, preview))}
        ]
    }
}

/// The card list and the dossier of the previewed hero, side by side — the
/// comparison the classical reference puts on one screen.
fn dossier_row(
    fonts: &KonungFonts,
    language: kn_settings::Language,
    cards: Vec<impl Scene>,
    dossier: Option<&HeroSummary>,
) -> impl Scene {
    let dossier_list: Box<dyn SceneList> = Box::new(dossier_list(fonts, language, dossier));
    bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            column_gap: spacing::SPACE_24,
            align_items: AlignItems::Start,
        }
        Children [
            {EntityScene(widgets::panel(fonts, cards))}
            --
            {EntityScene(widgets::panel(fonts, dossier_list))}
        ]
    }
}

/// One hero card of the list; the previewed one reads as primary, exactly
/// like the settings screen's active language. Two blocks: the BSN grammar
/// only takes component constructors, not value expressions.
fn hero_card(fonts: &KonungFonts, hero: u32, label: &str, selected: bool) -> Box<dyn Scene> {
    if selected {
        Box::new(bsn! {
            @widgets::menu_button(fonts, label)
            HeroCard(hero)
            ButtonVariant::Primary
            on(click_hero_card)
        })
    } else {
        Box::new(bsn! {
            @widgets::menu_button(fonts, label)
            HeroCard(hero)
            on(click_hero_card)
        })
    }
}

/// The dossier: the chosen hero's facts, sectioned the way the original's
/// menu grouped them. Without the game's texts the panel holds the slot
/// label alone — nothing invented in place of numbers we do not have.
fn dossier_list(
    fonts: &KonungFonts,
    language: kn_settings::Language,
    dossier: Option<&HeroSummary>,
) -> Box<dyn SceneList> {
    let Some(summary) = dossier else {
        return Box::new(bsn_list! {
            @caption(t(language, Text::HeroSlot(1)))
        });
    };
    Box::new(bsn_list! {
        {EntityScene(screens::text(
            fonts.bold.clone(),
            typo::TITLE,
            &summary.name,
        ))}
        --
        @caption(t(language, Text::HeroAttributes))
        --
        {EntityScene(row(fonts, t(language, Text::LabelAgility), &summary.agility.to_string()))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelCharisma), &summary.charisma.to_string()))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelStrength), &summary.strength.to_string()))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelEndurance), &summary.endurance.to_string()))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelHealth), &summary.health.to_string()))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelCarrying), &format!("{:.1}", summary.carrying)))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelMaxParty), &summary.max_party.to_string()))}
        --
        @caption(t(language, Text::HeroSkills))
        --
        {EntityScene(row(fonts, t(language, Text::LabelFencing), &summary.fencing.to_string()))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelTrading), &summary.trading.to_string()))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelAccuracy), &summary.accuracy.to_string()))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelCarpentry), &summary.carpentry.to_string()))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelMedicine), &summary.medicine.to_string()))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelIdentification), &summary.identification.to_string()))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelSmithery), &summary.smithery.to_string()))}
        --
        @caption(t(language, Text::HeroOther))
        --
        {EntityScene(row(fonts, t(language, Text::LabelLevel), &summary.level.to_string()))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelExperience), &summary.experience.to_string()))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelGold), &summary.gold.to_string()))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelFreePoints), &summary.free_points.to_string()))}
    })
}

/// One dossier line: the dim label on the left, the value on the right.
fn row(fonts: &KonungFonts, label: &str, value: &str) -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            column_gap: spacing::SPACE_24,
            min_width: px(220.0),
        }
        InheritableFont {
            font: {fonts.regular.clone()},
            font_size: typo::BODY,
            weight: FontWeight::NORMAL,
        }
        Children [
            @caption(label) TextColor(palette::TEXT_MUTED)
            --
            @caption(value) TextColor(palette::TEXT)
        ]
    }
}

/// Begin with the previewed hero, or return to the main menu. Begin carries
/// the previewed number — the reducer still validates it against the offer.
fn action_row(
    fonts: &KonungFonts,
    language: kn_settings::Language,
    preview: Option<u32>,
) -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            column_gap: spacing::SPACE_16,
        }
        Children [
            {EntityScene(begin_button(fonts, language, preview))}
            --
            {EntityScene(back_button(fonts, language))}
        ]
    }
}

fn begin_button(
    fonts: &KonungFonts,
    language: kn_settings::Language,
    preview: Option<u32>,
) -> impl Scene {
    bsn! {
        @widgets::menu_button(fonts, t(language, Text::Start))
        BeginButton(preview)
        on(click_begin)
    }
}

fn back_button(fonts: &KonungFonts, language: kn_settings::Language) -> impl Scene {
    bsn! {
        @widgets::menu_button(fonts, t(language, Text::Back))
        on(click_back)
    }
}

/// Which hero the Begin button starts with, resolved at spawn time from the
/// preview; a stale button (an empty offer) carries `None` and does nothing.
#[derive(Component, Clone, Copy, Default)]
struct BeginButton(Option<u32>);

fn click_hero_card(click: On<Activate>, cards: Query<&HeroCard>, mut flow: ResMut<AppFlow>) {
    if let Ok(&HeroCard(hero)) = cards.get(click.entity) {
        flow.submit(Action::HeroSelect(HeroSelectAction::Preview(hero)));
    }
}

fn click_begin(click: On<Activate>, buttons: Query<&BeginButton>, mut flow: ResMut<AppFlow>) {
    if let Ok(&BeginButton(hero)) = buttons.get(click.entity)
        && let Some(hero) = hero
    {
        flow.submit(Action::HeroSelect(HeroSelectAction::Choose(hero)));
    }
}

fn click_back(_click: On<Activate>, mut flow: ResMut<AppFlow>) {
    flow.submit(Action::HeroSelect(HeroSelectAction::Cancel));
}
