//! The hero selection of the new-game flow, in the spirit of the original's
//! new-game menu (Slavik `game_newgame.cpp:115-186`) and the classical CRPG
//! presentation: a portrait card per offered hero on the left, the chosen
//! hero's dossier on the right — the description from the game's own lines,
//! the menu numbers (base attributes, health, carry limit, max party, the
//! seven skills, level, experience, gold, free points) and the hero's
//! selected portrait. Every number is the original's own; nothing invented.
//! The choice leaves only as the typed `HeroSelectAction`; the reducer owns
//! who starts a session.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::feathers::{
    controls::{ButtonVariant, FeathersButton},
    display::caption,
    font_styles::InheritableFont,
};
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::scene::{EntityScene, Scene, SceneList};
use bevy::text::FontWeight;
use bevy::ui::widget::ImageNode;
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

/// The heroes' portraits, decoded once per process from the game's menu
/// images (`NEWH800R.RES`, any edition's — the portraits are identical);
/// keyed by record number. `None` — no game or an unreadable file: the cards
/// fall back to name-only buttons, the dossier to text alone.
#[derive(Resource, Default)]
struct HeroPortraits(Option<HashMap<u8, Handle<Image>>>);

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
    if !world.contains_resource::<HeroPortraits>() {
        // The game's root first, so the world borrow ends before the assets
        // borrow starts.
        let game_root = world
            .get_resource::<GameResource>()
            .map(|game| game.0.root().to_path_buf());
        let records = game_root.and_then(|root| {
            ["NEWH800R.RES", "NEWH800E.RES"].iter().find_map(|name| {
                let bytes = std::fs::read(root.join(name)).ok()?;
                k1_data::newh_res::parse(&bytes).ok()
            })
        });
        let portraits = records.map(|records| {
            let mut images = world.resource_mut::<Assets<Image>>();
            records
                .images
                .into_iter()
                .enumerate()
                .skip(3)
                .take(6)
                .map(|(record, image)| (record as u8, images.add(portrait_image(image))))
                .collect::<HashMap<u8, Handle<Image>>>()
        });
        world.insert_resource(HeroPortraits(portraits));
    }
    let portraits = world
        .resource::<HeroPortraits>()
        .0
        .clone()
        .unwrap_or_default();
    world
        .spawn_scene(root(
            &fonts, language, &heroes, preview, &summaries, &portraits,
        ))
        .expect("the hero-select root spawns");
    world.insert_resource(HeroSelectView(preview));
}

/// The decoded menu image as a texture: straight-alpha RGBA.
fn portrait_image(image: k1_data::newh_res::Image) -> Image {
    Image::new(
        Extent3d {
            width: u32::from(image.width),
            height: u32::from(image.height),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        image.rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
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
    portraits: &HashMap<u8, Handle<Image>>,
) -> impl Scene {
    let cards = heroes
        .iter()
        .enumerate()
        .map(|(row, &hero)| {
            let summary = summaries.iter().find(|summary| summary.hero == hero);
            let name = summary
                .map(|summary| summary.name.as_str())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| t(language, Text::HeroSlot(row + 1)));
            let portrait = summary
                .map(|summary| summary.portrait)
                .and_then(|record| portraits.get(&record).cloned());
            hero_card(hero, name, portrait, preview == Some(hero))
        })
        .collect::<Vec<_>>();
    let dossier = preview.and_then(|hero| summaries.iter().find(|summary| summary.hero == hero));
    let dossier_portrait = dossier
        .map(|summary| summary.portrait_selected)
        .and_then(|record| portraits.get(&record).cloned());
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
                dossier_portrait,
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
    dossier_portrait: Option<Handle<Image>>,
) -> impl Scene {
    let dossier_list: Box<dyn SceneList> =
        Box::new(dossier_list(fonts, language, dossier, dossier_portrait));
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

/// One hero card of the list: the plain portrait over the name, the
/// previewed one reading as primary — exactly like the settings screen's
/// active language. Two blocks: the BSN grammar only takes component
/// constructors, not value expressions.
fn hero_card(
    hero: u32,
    label: &str,
    portrait: Option<Handle<Image>>,
    selected: bool,
) -> Box<dyn Scene> {
    let body = bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: spacing::SPACE_8,
        }
        Children [
            {EntityScene(portrait_view(portrait))}
            --
            @caption(label)
        ]
    };
    if selected {
        Box::new(bsn! {
            @FeathersButton { @caption: {body} }
            HeroCard(hero)
            ButtonVariant::Primary
            on(click_hero_card)
        })
    } else {
        Box::new(bsn! {
            @FeathersButton { @caption: {body} }
            HeroCard(hero)
            on(click_hero_card)
        })
    }
}

fn dossier_list(
    fonts: &KonungFonts,
    language: kn_settings::Language,
    dossier: Option<&HeroSummary>,
    dossier_portrait: Option<Handle<Image>>,
) -> Box<dyn SceneList> {
    let Some(summary) = dossier else {
        return Box::new(bsn_list! {
            @caption(t(language, Text::HeroSlot(1)))
        });
    };
    Box::new(bsn_list! {
        Node {
            flex_direction: FlexDirection::Row,
            column_gap: spacing::SPACE_16,
            align_items: AlignItems::Start,
        }
        Children [
            {EntityScene(portrait_view(dossier_portrait))}
            --
            {EntityScene(screens::text(
                fonts.bold.clone(),
                typo::TITLE,
                &summary.name,
            ))}
        ]
        --
        {EntityScene(description(&summary.description))}
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

/// The hero's portrait at its natural size; without the menu images — nothing.
fn portrait_view(portrait: Option<Handle<Image>>) -> impl Scene {
    let Some(handle) = portrait else {
        return Box::new(bsn! { Node {} }) as Box<dyn Scene>;
    };
    Box::new(bsn! {
        Node {}
        ImageNode { image: {handle} }
    }) as Box<dyn Scene>
}

/// The hero's description: the game's own line for this hero, wrapped inside
/// a fixed-width column. Game data, not interface strings — it shows only
/// when the exe's texts were readable.
fn description(text: &str) -> impl Scene {
    let text = text.to_string();
    bsn! {
        Node {
            width: px(430.0),
        }
        Children [
            @caption(text.as_str()) TextColor(palette::TEXT_MUTED)
        ]
    }
}

/// One dossier line: the dim label on the left, the value on the right.
fn row(fonts: &KonungFonts, label: &str, value: &str) -> impl Scene {
    let label = label.to_string();
    let value = value.to_string();
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
            @caption(label.as_str()) TextColor(palette::TEXT_MUTED)
            --
            @caption(value.as_str()) TextColor(palette::TEXT)
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
