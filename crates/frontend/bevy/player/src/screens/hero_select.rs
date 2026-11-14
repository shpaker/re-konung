//! Hero creation with original portraits, models, descriptions and rules.
//! The centered layout separates selection, story and configuration into
//! three columns; narrow windows wrap the columns inside a scrolling body.
//! Only typed `HeroSelectAction`s leave the view; the reducer owns the draft.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::feathers::{controls::ButtonVariant, display::caption, font_styles::InheritableFont};
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::scene::{EntityScene, Scene, SceneList};
use bevy::text::FontWeight;
use bevy::ui::widget::ImageNode;

use bevy::picking::hover::Hovered;
use bevy::ui_widgets::Activate;
use kn_app::{Action, HeroSelectAction};
use kn_game::{HeroSummary, MapSource};
use kn_render::GameResource;
use kn_ui::{KonungFonts, palette, spacing, typo, widgets};

use crate::flow::{AppFlow, AppliedSettings, ScreenRoot};
use crate::i18n::{Text, t};
use crate::screens;

/// The width of the portrait-card column, and the natural size of the menu
/// portraits (`NEWH800*.RES` records 3–8).
const CARD_COLUMN_WIDTH: f32 = 148.0;
const PORTRAIT_WIDTH: f32 = 76.0;
const PORTRAIT_HEIGHT: f32 = 87.0;
const DESCRIPTION_WIDTH: f32 = 280.0;
const CREATION_WIDTH: f32 = 1120.0;

/// Which preview and constructor draft the on-screen view was built for: a
/// change in either means its captions are stale, and the view respawns
/// wholesale — the settings screen's `BuiltLanguage` pattern.
#[derive(Resource, Clone, PartialEq)]
struct HeroSelectView {
    preview: Option<u32>,
    draft: kn_app::NewGameDraft,
}

/// The heroes' portraits, decoded once per process from the game's menu
/// images (`NEWH800R.RES`, any edition's — the portraits are identical);
/// keyed by record number. `None` — no game or an unreadable file: the cards
/// fall back to name-only buttons, the dossier to text alone.
#[derive(Resource, Default)]
struct HeroPortraits(Option<HashMap<u8, PortraitImage>>);

// The original's hover-hint lines: the `Texts::newgame` row each element
// points at (docs/k1/formats/executable.md, `Locale::NewGameMenu`).
const HINT_CLASS_WARRIOR: usize = 6;
const HINT_CLASS_HUNTER: usize = 4;
const HINT_CLASS_MERCHANT: usize = 3;
const HINT_CLASS_CHIEF: usize = 5;
const HINT_CHARISMA: usize = 7;
const HINT_STRENGTH: usize = 8;
const HINT_AGILITY: usize = 9;
const HINT_ENDURANCE: usize = 10;
const HINT_CARRY: usize = 11;
const HINT_PARTY: usize = 12;
const HINT_SMITHERY: usize = 13;
const HINT_CARPENTRY: usize = 14;
const HINT_FENCING: usize = 15;
const HINT_ACCURACY: usize = 16;
const HINT_IDENTIFICATION: usize = 17;
const HINT_TRADING: usize = 18;
const HINT_MEDICINE: usize = 19;

/// The hints of the hero-select elements: the trimmed `Texts::newgame`
/// lines. Empty — the edition carries no texts and the popups stay off.
#[derive(Resource, Default, Clone)]
pub struct NewGameHints(pub Vec<String>);

/// The composed model previews, keyed by hero number; built once per
/// process together with the portraits.
#[derive(Resource, Default)]
struct HeroModels(HashMap<u32, PortraitImage>);

/// A decoded portrait and its natural pixel size, so the layout can reserve
/// the exact box before the texture is in.
#[derive(Clone)]
struct PortraitImage {
    handle: Handle<Image>,
    width: f32,
    height: f32,
}

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
    let (heroes, preview, draft) = {
        let state = world.resource::<AppFlow>().state();
        (
            state.heroes.clone(),
            state.hero_preview,
            state.new_game_draft,
        )
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
                .map(|(record, image)| {
                    let portrait = PortraitImage {
                        handle: images.add(portrait_texture(&image)),
                        width: image.width as f32,
                        height: image.height as f32,
                    };
                    (record as u8, portrait)
                })
                .collect::<HashMap<u8, PortraitImage>>()
        });
        world.insert_resource(HeroPortraits(portraits));
    }
    if !world.contains_resource::<HeroModels>() {
        let mut models = HashMap::new();
        let summaries_by_hero = heroes.clone();
        if world.get_resource::<GameResource>().is_some() {
            for hero in &summaries_by_hero {
                let Ok(preview) = world.resource::<GameResource>().0.hero_preview(*hero) else {
                    continue;
                };
                let mut images = world.resource_mut::<Assets<Image>>();
                let handle = images.add(portrait_texture_raw(
                    preview.width,
                    preview.height,
                    preview.rgba,
                ));
                models.insert(
                    *hero,
                    PortraitImage {
                        handle,
                        width: preview.width as f32,
                        height: preview.height as f32,
                    },
                );
            }
        }
        world.insert_resource(HeroModels(models));
    }
    if !world.contains_resource::<NewGameHints>() {
        let hints = world
            .get_resource::<GameResource>()
            .map(|game| NewGameHints(game.0.newgame_texts()))
            .unwrap_or_default();
        world.insert_resource(hints);
    }
    let hints = world.resource::<NewGameHints>().0.clone();
    let portraits = world
        .resource::<HeroPortraits>()
        .0
        .clone()
        .unwrap_or_default();
    let models = world.resource::<HeroModels>().0.clone();
    world
        .spawn_scene(root(
            &fonts, language, &heroes, preview, &summaries, &portraits, &hints, &models, draft,
        ))
        .expect("the hero-select root spawns");
    world.insert_resource(HeroSelectView { preview, draft });
}

/// The decoded menu image as a texture: straight-alpha RGBA.
fn portrait_texture_raw(width: u32, height: u32, rgba: Vec<u8>) -> Image {
    Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn portrait_texture(image: &k1_data::newh_res::Image) -> Image {
    Image::new(
        Extent3d {
            width: u32::from(image.width),
            height: u32::from(image.height),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        image.rgba.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

/// Respawns the view when the previewed hero moved: the detail panel and the
/// card highlight both follow the state. Runs after the flow chain, like the
/// settings screen's own sync.
pub(crate) fn sync(world: &mut World) {
    let Some(built) = world.get_resource::<HeroSelectView>().cloned() else {
        return;
    };
    let state = world.resource::<AppFlow>().state();
    if state.screen != kn_app::Screen::HeroSelect
        || (state.hero_preview == built.preview && state.new_game_draft == built.draft)
    {
        return;
    }
    let scroll_position = world
        .query_filtered::<&ScrollPosition, With<CreationContent>>()
        .iter(world)
        .next()
        .cloned();
    let focused = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get());
    let focus_attribute = focused
        .and_then(|entity| world.get::<AttributeButton>(entity))
        .copied();
    let focus_constructor = focused
        .and_then(|entity| world.get::<ConstructorButton>(entity))
        .copied();
    let focus_hero = focused
        .and_then(|entity| world.get::<HeroCard>(entity))
        .map(|card| card.0);
    let mut roots = world.query_filtered::<Entity, With<ScreenRoot>>();
    let old: Vec<Entity> = roots.iter(world).collect();
    for root in old {
        world.despawn(root);
    }
    spawn(world);
    if let Some(previous) = scroll_position {
        for mut position in world
            .query_filtered::<&mut ScrollPosition, With<CreationContent>>()
            .iter_mut(world)
        {
            position.0 = previous.0;
        }
    }
    let next_focus = if let Some(button) = focus_attribute {
        world
            .query::<(Entity, &AttributeButton)>()
            .iter(world)
            .find(|(_, value)| **value == button)
            .map(|(entity, _)| entity)
    } else if let Some(button) = focus_constructor {
        world
            .query::<(Entity, &ConstructorButton)>()
            .iter(world)
            .find(|(_, value)| **value == button)
            .map(|(entity, _)| entity)
    } else if let Some(hero) = focus_hero {
        world
            .query::<(Entity, &HeroCard)>()
            .iter(world)
            .find(|(_, value)| value.0 == hero)
            .map(|(entity, _)| entity)
    } else {
        None
    };
    if let Some(entity) = next_focus
        && let Some(mut focus) = world.get_resource_mut::<bevy::input_focus::InputFocus>()
    {
        focus.set(entity, bevy::input_focus::FocusCause::Navigated);
    }
}

#[allow(clippy::too_many_arguments)]
fn root(
    fonts: &KonungFonts,
    language: kn_settings::Language,
    heroes: &[u32],
    preview: Option<u32>,
    summaries: &[HeroSummary],
    portraits: &HashMap<u8, PortraitImage>,
    hints: &[String],
    models: &HashMap<u32, PortraitImage>,
    draft: kn_app::NewGameDraft,
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
                .map(|summary| {
                    if preview == Some(hero) {
                        summary.portrait_selected
                    } else {
                        summary.portrait
                    }
                })
                .and_then(|record| portraits.get(&record).cloned());
            hero_card(fonts, hero, name, portrait, preview == Some(hero))
        })
        .collect::<Vec<_>>();
    let dossier = preview.and_then(|hero| summaries.iter().find(|summary| summary.hero == hero));
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            padding: UiRect::all(px(24.0)),
            row_gap: spacing::SPACE_16,
        }
        BackgroundColor(palette::WINDOW_BG)
        ScreenRoot
        Children [
            Node {
                width: percent(100),
                flex_grow: 1.0,
                min_height: px(0),
                overflow: Overflow::scroll_y(),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
            }
            CreationContent
            ScrollPosition::default()
            on(scroll_content)
            Children [
            {EntityScene(dossier_row(
                fonts,
                language,
                cards,
                dossier,
                hints,
                models,
                draft,
            ))}
            ]
            --
            Node {
                width: percent(100),
                max_width: px(CREATION_WIDTH),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::all(px(12.0)),
                flex_shrink: 0.0,
            }
            BackgroundColor(palette::SURFACE)
            Children [
                {EntityScene(back_button(fonts, language))}
                --
                {EntityScene(begin_button(fonts, language, preview))}
            ]
        ]
    }
}

/// Three columns: hero selection, identity and story, then editable statistics.
#[allow(clippy::too_many_arguments)]
fn dossier_row(
    fonts: &KonungFonts,
    language: kn_settings::Language,
    cards: Vec<impl Scene>,
    dossier: Option<&HeroSummary>,
    hints: &[String],
    models: &HashMap<u32, PortraitImage>,
    draft: kn_app::NewGameDraft,
) -> impl Scene {
    let model = dossier
        .map(|summary| summary.hero)
        .and_then(|hero| models.get(&hero).cloned());
    let dossier_list = dossier_list(fonts, language, dossier, hints, draft);
    bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            width: percent(100),
            max_width: px(CREATION_WIDTH),
            flex_shrink: 0.0,
            margin: UiRect::vertical(Val::Auto),
            column_gap: spacing::SPACE_16,
            row_gap: spacing::SPACE_16,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Start,
        }
        Children [
            {EntityScene(card_column(cards))}
            --
            {EntityScene(story_column(fonts, language, dossier, model))}
            --
            @widgets::panel(fonts, dossier_list)
            Node {
                width: px(540),
                flex_basis: px(440),
                flex_grow: 1.0,
                min_width: px(440),
                max_width: percent(100),
                padding: UiRect::all(px(16)),
                row_gap: spacing::SPACE_16,
            }
            bevy::feathers::theme::ThemeBackgroundColor(bevy::feathers::tokens::PANE_BODY_BG)
        ]
    }
}

/// The vertical stack of hero cards.
fn card_column(cards: Vec<impl Scene>) -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: spacing::SPACE_8,
            width: px(CARD_COLUMN_WIDTH),
            flex_shrink: 0.0,
        }
        Children [{cards}]
    }
}

/// The whole card is one control; decorative children never intercept picking.
fn hero_card(
    fonts: &KonungFonts,
    hero: u32,
    label: &str,
    portrait: Option<PortraitImage>,
    selected: bool,
) -> Box<dyn Scene> {
    if selected {
        Box::new(
            bsn! { @hero_card_control(fonts, hero, label, portrait, selected) ButtonVariant::Primary },
        )
    } else {
        Box::new(bsn! { @hero_card_control(fonts, hero, label, portrait, selected) })
    }
}

fn hero_card_control(
    fonts: &KonungFonts,
    hero: u32,
    label: &str,
    portrait: Option<PortraitImage>,
    selected: bool,
) -> impl Scene {
    let label_color = if selected {
        palette::TEXT_ON_ACCENT
    } else {
        palette::TEXT
    };
    let border_color = if selected {
        palette::ACCENT
    } else {
        palette::CONTROL_BORDER
    };
    bsn! {
        @bevy::feathers::controls::FeathersButton {
            @caption: bsn! {
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: spacing::SPACE_8,
                }
                bevy::picking::Pickable::IGNORE
                InheritableFont {
                    font: {fonts.regular.clone()},
                    font_size: typo::BODY,
                    weight: FontWeight::NORMAL,
                }
                Children [
                    {EntityScene(portrait_view(portrait))}
                    --
                    @caption(label)
                    TextColor(label_color)
                    bevy::picking::Pickable::IGNORE
                ]
            }
        }
        Node {
            width: percent(100),
            height: px(132),
            min_height: px(132),
            flex_shrink: 0.0,
            padding: UiRect::all(px(10)),
            border: px(1),
            overflow: Overflow::clip(),
        }
        BorderColor::all(border_color)
        InheritableFont {
            font: {fonts.regular.clone()},
            font_size: typo::BODY,
            weight: FontWeight::NORMAL,
        }
        HeroCard(hero)
        on(click_hero_card)
    }
}

fn story_column(
    fonts: &KonungFonts,
    language: kn_settings::Language,
    dossier: Option<&HeroSummary>,
    model: Option<PortraitImage>,
) -> impl Scene {
    let name = dossier.map_or(t(language, Text::HeroSlot(1)), |hero| hero.name.as_str());
    let story = dossier.map_or("", |hero| hero.description.as_str());
    bsn! {
        @widgets::panel(fonts, bsn_list! {
            {EntityScene(screens::text(fonts.bold.clone(), typo::TITLE, name))}
            --
            Node {
                width: percent(100),
                height: px(164),
                flex_shrink: 0.0,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border_radius: px(6),
            }
            BackgroundColor(palette::WINDOW_BG)
            Children [{EntityScene(portrait_view(model))}]
            --
            {EntityScene(description(fonts, story))}
        })
        Node {
            width: px(300),
            max_width: percent(100),
            flex_shrink: 0.0,
            padding: UiRect::all(px(16)),
            row_gap: spacing::SPACE_16,
        }
        bevy::feathers::theme::ThemeBackgroundColor(bevy::feathers::tokens::PANE_BODY_BG)
    }
}

/// The editable statistics and occupation/specialization choices.
fn dossier_list(
    fonts: &KonungFonts,
    language: kn_settings::Language,
    dossier: Option<&HeroSummary>,
    _hints: &[String],
    draft: kn_app::NewGameDraft,
) -> Box<dyn SceneList> {
    let Some(summary) = dossier else {
        return Box::new(bsn_list! {
            @caption(t(language, Text::HeroSlot(1)))
        });
    };
    let class =
        kn_game::newgame_profile(draft.class, draft.attributes).expect("validated allocation");
    let allocation =
        kn_game::newgame_allocation(draft.class, draft.attributes).expect("validated allocation");
    let style = if draft.combat_style == 1 {
        kn_game::COMBAT_ACCURACY
    } else {
        kn_game::COMBAT_FENCING
    };
    let secondary = match draft.secondary {
        1 => kn_game::SECONDARY_TRADING,
        2 => kn_game::SECONDARY_MEDICINE,
        _ => kn_game::SECONDARY_DEFAULT,
    };
    let attributes: Box<dyn SceneList> = Box::new(bsn_list! {
        {EntityScene(screens::text(fonts.bold.clone(), typo::BODY, t(language, Text::HeroAttributes)))}
        --
        {EntityScene(attribute_row(fonts, t(language, Text::LabelAgility), draft, 0, HINT_AGILITY))}
        --
        {EntityScene(attribute_row(fonts, t(language, Text::LabelCharisma), draft, 1, HINT_CHARISMA))}
        --
        {EntityScene(attribute_row(fonts, t(language, Text::LabelStrength), draft, 2, HINT_STRENGTH))}
        --
        {EntityScene(attribute_row(fonts, t(language, Text::LabelEndurance), draft, 3, HINT_ENDURANCE))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelHealth), &summary.health.to_string(), usize::MAX))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelCarrying), &format!("{:.1}", class.carrying), HINT_CARRY))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelMaxParty), &class.max_party.to_string(), HINT_PARTY))}
    });
    let others: Box<dyn SceneList> = Box::new(bsn_list! {
        {EntityScene(screens::text(fonts.bold.clone(), typo::BODY, t(language, Text::HeroOther)))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelLevel), &1.to_string(), usize::MAX))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelExperience), &summary.experience.to_string(), usize::MAX))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelGold), &summary.gold.to_string(), usize::MAX))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelFreePoints), &allocation.free_points.to_string(), usize::MAX))}
    });
    let skills: Box<dyn SceneList> = Box::new(bsn_list! {
        {EntityScene(screens::text(fonts.bold.clone(), typo::BODY, t(language, Text::HeroSkills)))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelFencing), &style.fencing.to_string(), HINT_FENCING))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelTrading), &secondary.trading.to_string(), HINT_TRADING))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelAccuracy), &style.accuracy.to_string(), HINT_ACCURACY))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelCarpentry), &style.carpentry.to_string(), HINT_CARPENTRY))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelMedicine), &secondary.medicine.to_string(), HINT_MEDICINE))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelIdentification), &secondary.identification.to_string(), HINT_IDENTIFICATION))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelSmithery), &style.smithery.to_string(), HINT_SMITHERY))}
    });
    let class_row = bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            column_gap: spacing::SPACE_8,
            align_items: AlignItems::Center,
            row_gap: spacing::SPACE_8,
        }
        // The row label is a bare `@caption`: the theme font has no
        // Cyrillic, the bundled face is set on the row.
        InheritableFont {
            font: {fonts.regular.clone()},
            font_size: typo::BODY,
            weight: FontWeight::NORMAL,
        }
        Children [
            @caption(t(language, Text::HeroClass)) Node { width: percent(100) } TextColor(palette::TEXT_MUTED)
            --
            {EntityScene(choice_button(fonts, t(language, Text::ClassWarrior), draft.class == 0, ConstructorKind::Class, 0, HINT_CLASS_WARRIOR))}
            --
            {EntityScene(choice_button(fonts, t(language, Text::ClassHunter), draft.class == 1, ConstructorKind::Class, 1, HINT_CLASS_HUNTER))}
            --
            {EntityScene(choice_button(fonts, t(language, Text::ClassMerchant), draft.class == 2, ConstructorKind::Class, 2, HINT_CLASS_MERCHANT))}
            --
            {EntityScene(choice_button(fonts, t(language, Text::ClassChief), draft.class == 3, ConstructorKind::Class, 3, HINT_CLASS_CHIEF))}
        ]
    };
    let style_row = bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            column_gap: spacing::SPACE_8,
            align_items: AlignItems::Center,
            row_gap: spacing::SPACE_8,
        }
        // The row label is a bare `@caption`: the theme font has no
        // Cyrillic, the bundled face is set on the row.
        InheritableFont {
            font: {fonts.regular.clone()},
            font_size: typo::BODY,
            weight: FontWeight::NORMAL,
        }
        Children [
            @caption(t(language, Text::HeroSpec)) Node { width: percent(100) } TextColor(palette::TEXT_MUTED)
            --
            {EntityScene(choice_button(fonts, t(language, Text::LabelFencing), draft.combat_style == 0, ConstructorKind::Combat, 0, HINT_FENCING))}
            --
            {EntityScene(choice_button(fonts, t(language, Text::LabelAccuracy), draft.combat_style == 1, ConstructorKind::Combat, 1, HINT_ACCURACY))}
        ]
    };
    let secondary_row = bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            column_gap: spacing::SPACE_8,
            align_items: AlignItems::Center,
            row_gap: spacing::SPACE_8,
        }
        // The row label is a bare `@caption`: the theme font has no
        // Cyrillic, the bundled face is set on the row.
        InheritableFont {
            font: {fonts.regular.clone()},
            font_size: typo::BODY,
            weight: FontWeight::NORMAL,
        }
        Children [
            @caption(t(language, Text::HeroSecondary)) Node { width: percent(100) } TextColor(palette::TEXT_MUTED)
            --
            {EntityScene(choice_button(fonts, t(language, Text::LabelIdentification), draft.secondary == 0, ConstructorKind::Secondary, 0, HINT_IDENTIFICATION))}
            --
            {EntityScene(choice_button(fonts, t(language, Text::LabelTrading), draft.secondary == 1, ConstructorKind::Secondary, 1, HINT_TRADING))}
            --
            {EntityScene(choice_button(fonts, t(language, Text::LabelMedicine), draft.secondary == 2, ConstructorKind::Secondary, 2, HINT_MEDICINE))}
        ]
    };
    Box::new(bsn_list! {
        {EntityScene(bsn! {
            Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                column_gap: spacing::SPACE_24,
                row_gap: spacing::SPACE_16,
                align_items: AlignItems::Start,
            }
            Children [
                {EntityScene(bsn! {
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: spacing::SPACE_4,
                    }
                    Children [{attributes}]
                })}
                --
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: spacing::SPACE_16,
                    flex_grow: 1.0,
                }
                Children [
                    Node { flex_direction: FlexDirection::Column, row_gap: spacing::SPACE_4 }
                    Children [{skills}]
                    --
                    Node { flex_direction: FlexDirection::Column, row_gap: spacing::SPACE_4 }
                    Children [{others}]
                ]
            ]
        })}
        --
        {EntityScene(class_row)}
        --
        {EntityScene(style_row)}
        --
        {EntityScene(secondary_row)}

    })
}

/// The hero's portrait at an explicit pixel box: the layout reserves the
/// exact cell, whatever the texture pipeline reports. Without the menu
/// images — an empty placeholder of the same size.
fn portrait_view(portrait: Option<PortraitImage>) -> impl Scene {
    let Some(portrait) = portrait else {
        return Box::new(bsn! {
            Node {
                width: px(PORTRAIT_WIDTH),
                height: px(PORTRAIT_HEIGHT),
            }
        }) as Box<dyn Scene>;
    };
    let (width, height) = (portrait.width, portrait.height);
    Box::new(bsn! {
        Node {
            width: px(width),
            height: px(height),
            flex_shrink: 0.0,
        }
        ImageNode { image: {portrait.handle} }
        bevy::picking::Pickable::IGNORE
    }) as Box<dyn Scene>
}

/// The hero's description: the game's own line for this hero, wrapped inside
/// a fixed-width column. Game data, not interface strings — it shows only
/// when the exe's texts were readable. The bundled font is set explicitly:
/// the theme default carries no Cyrillic glyphs (the tofu row of #125's
/// screenshots).
fn description(fonts: &KonungFonts, text: &str) -> impl Scene {
    let text = text.to_string();
    bsn! {
        Node {
            width: percent(100),
            max_width: px(DESCRIPTION_WIDTH),
        }
        InheritableFont {
            font: {fonts.regular.clone()},
            font_size: typo::BODY,
            weight: FontWeight::NORMAL,
        }
        Children [
            @caption(text.as_str()) TextColor(palette::TEXT_MUTED)
        ]
    }
}

/// One dossier line: the dim label on the left, the value on the right.
/// `hint_line` — the `Texts::newgame` row the original showed on hover
/// (`HINT_*`); hovering the line opens the hint popup.
fn row(fonts: &KonungFonts, label: &str, value: &str, hint_line: usize) -> impl Scene {
    let label = label.to_string();
    let value = value.to_string();
    bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            column_gap: spacing::SPACE_24,
            min_width: px(150.0),
        }
        InheritableFont {
            font: {fonts.regular.clone()},
            font_size: typo::BODY,
            weight: FontWeight::NORMAL,
        }
        HintSource(hint_line)
        Hovered
        Children [
            @caption(label.as_str()) TextColor(palette::TEXT_MUTED)
            --
            @caption(value.as_str()) TextColor(palette::TEXT)
        ]
    }
}

/// Begin with the previewed hero, or return to the main menu. Begin carries
/// the previewed number — the reducer still validates it against the offer.
fn begin_button(
    fonts: &KonungFonts,
    language: kn_settings::Language,
    preview: Option<u32>,
) -> impl Scene {
    bsn! {
        @widgets::menu_button(fonts, t(language, Text::Start))
        ButtonVariant::Primary
        BeginButton(preview)
        on(click_begin)
    }
}

fn back_button(fonts: &KonungFonts, language: kn_settings::Language) -> Box<dyn Scene> {
    let label = format!("{} · Esc", t(language, Text::Back));
    Box::new(bsn! {
        @widgets::menu_button(fonts, label.as_str())
        on(click_back)
    })
}

/// Which hero the Begin button starts with, resolved at spawn time from the
/// preview; a stale button (an empty offer) carries `None` and does nothing.
#[derive(Component, Clone, Copy, Default)]
struct BeginButton(Option<u32>);

/// A constructor choice button: the kind and the row it selects; the drafted
/// row reads as primary.
fn choice_button(
    fonts: &KonungFonts,
    label: &str,
    selected: bool,
    kind: ConstructorKind,
    index: u8,
    hint_line: usize,
) -> Box<dyn Scene> {
    let label = label.to_string();
    if selected {
        Box::new(bsn! {
            @widgets::menu_button(fonts, label.as_str())
            ConstructorButton { kind, index }
            HintSource(hint_line)
            ButtonVariant::Primary
            on(click_constructor)
        })
    } else {
        Box::new(bsn! {
            @widgets::menu_button(fonts, label.as_str())
            ConstructorButton { kind, index }
            HintSource(hint_line)
            on(click_constructor)
        })
    }
}

/// The `Texts::newgame` line a hoverable element shows in the popup;
/// `usize::MAX` — no hint.
#[derive(Component, Clone, Copy, Default)]
pub struct HintSource(pub usize);

/// The visible hint popup and the line it currently shows.
#[derive(Component)]
pub struct HintPopup(pub usize);

/// Shows the hint popup of the hovered element near the cursor; hides it
/// when nothing is hovered. Screen-local, runs after the flow chain like
/// the other sync systems.
pub(crate) fn hint_popup(world: &mut World) {
    let hint_lines = world
        .get_resource::<NewGameHints>()
        .map(|hints| hints.0.clone())
        .unwrap_or_default();
    let current = {
        let mut query = world.query_filtered::<(&HintSource, &Hovered), ()>();
        query
            .iter(world)
            .find(|(_, hovered)| hovered.0)
            .map(|(source, _)| source.0)
    };
    let cursor = world
        .query::<&Window>()
        .single(world)
        .ok()
        .and_then(|window| window.cursor_position());
    let existing = world
        .query_filtered::<Entity, With<HintPopup>>()
        .iter(world)
        .next();
    match current.filter(|&line| hint_lines.get(line).is_some_and(|text| !text.is_empty())) {
        None => {
            if let Some(entity) = existing {
                world.despawn(entity);
            }
        }
        Some(line) => {
            if let Some(entity) = existing {
                if world
                    .get::<HintPopup>(entity)
                    .is_some_and(|popup| popup.0 == line)
                {
                    return;
                }
                world.despawn(entity);
            }
            let text = hint_lines[line].clone();
            spawn_hint_popup(world, &text, cursor.map(|c| (c.x, c.y)), line);
        }
    }
}

/// Spawns the popup: a bordered panel near the cursor (or at a fallback
/// point, when there is no window cursor — headless), non-pickable so it
/// never blocks the row under it.
pub fn spawn_hint_popup(world: &mut World, text: &str, at: Option<(f32, f32)>, line: usize) {
    let (left, top) = at.map_or((240.0, 250.0), |(x, y)| (x + 16.0, y + 16.0));
    let fonts = crate::screens::fonts(world);
    let text = text.to_string();
    // Plain Bevy UI, not a bsn scene: the popup spawns outside the flow
    // chain and nothing reads it back. The bundled font is explicit — the
    // theme default carries no Cyrillic glyphs.
    let entity = world
        .spawn((
            Name::new("hint popup"),
            HintPopup(line),
            Node {
                position_type: PositionType::Absolute,
                left: px(left),
                top: px(top),
                width: px(320.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(10.0)),
                border: UiRect::all(px(1.0)),
                border_radius: BorderRadius::all(px(6.0)),
                ..default()
            },
            bevy::picking::Pickable::IGNORE,
            BackgroundColor(palette::SURFACE_RAISED),
            // BorderColor is a four-field struct in Bevy 0.20, not a newtype
            bevy::ui::BorderColor {
                top: palette::BORDER,
                right: palette::BORDER,
                bottom: palette::BORDER,
                left: palette::BORDER,
            },
            bevy::ui::widget::Text::new(text),
            bevy::text::TextFont {
                font: fonts.regular.clone().into(),
                font_size: typo::BODY,
                ..default()
            },
            bevy::text::TextColor(palette::TEXT),
        ))
        .id();
    // The scene spawn above returns an entity; the markers ride after, so
    // the popup never counts as part of the screen root.
    let _ = entity;
}

/// Which constructor choice the clicked button stands for.
#[derive(Component, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConstructorButton {
    pub kind: ConstructorKind,
    pub index: u8,
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConstructorKind {
    #[default]
    Class,
    Combat,
    Secondary,
}

fn click_constructor(
    click: On<Activate>,
    buttons: Query<&ConstructorButton>,
    mut flow: ResMut<AppFlow>,
) {
    let Ok(&button) = buttons.get(click.entity) else {
        return;
    };
    let action = match button.kind {
        ConstructorKind::Class => HeroSelectAction::SetClass(button.index),
        ConstructorKind::Combat => HeroSelectAction::SetCombatStyle(button.index),
        ConstructorKind::Secondary => HeroSelectAction::SetSecondary(button.index),
    };
    flow.submit(Action::HeroSelect(action));
}

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

/// Escape cancels; Enter/Space remain scoped to the focused control.
pub(crate) fn keyboard(keys: Option<Res<ButtonInput<KeyCode>>>, mut flow: ResMut<AppFlow>) {
    if flow.state().screen == kn_app::Screen::HeroSelect
        && keys.is_some_and(|keys| keys.just_pressed(KeyCode::Escape))
    {
        flow.submit(Action::HeroSelect(HeroSelectAction::Cancel));
    }
}

/// A typed attribute operation, used by pointer and focused keyboard activation.
#[derive(Component, Clone, Copy, Default, PartialEq, Eq)]
pub struct AttributeButton {
    pub attribute: usize,
    pub delta: i16,
}

fn attribute_row(
    fonts: &KonungFonts,
    label: &str,
    draft: kn_app::NewGameDraft,
    attribute: usize,
    hint: usize,
) -> impl Scene {
    let allocation =
        kn_game::newgame_allocation(draft.class, draft.attributes).expect("validated allocation");
    bsn! {
        Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: spacing::SPACE_8 }
        Children [
            {EntityScene(row(fonts, label, &allocation.values[attribute].to_string(), hint))}
            --
            {EntityScene(attribute_button(fonts, draft, attribute, -1))}
            --
            {EntityScene(attribute_button(fonts, draft, attribute, 1))}
        ]
    }
}

fn attribute_button(
    fonts: &KonungFonts,
    draft: kn_app::NewGameDraft,
    attribute: usize,
    delta: i16,
) -> Box<dyn Scene> {
    let allocation =
        kn_game::newgame_allocation(draft.class, draft.attributes).expect("validated allocation");
    let next = allocation.adjusted(draft.class, attribute, delta);
    let label = match next {
        Some(next) => format!(
            "{} {}",
            if delta < 0 { "−" } else { "+" },
            (allocation.free_points - next.free_points).abs()
        ),
        None => if delta < 0 { "−" } else { "+" }.to_string(),
    };
    if next.is_some() {
        Box::new(
            bsn! { @widgets::menu_button(fonts, label.as_str()) Node { width: px(56) } AttributeButton { attribute, delta } on(adjust_attribute) },
        )
    } else {
        Box::new(
            bsn! { @widgets::menu_button(fonts, label.as_str()) Node { width: px(56) } AttributeButton { attribute, delta } bevy::ui::InteractionDisabled on(adjust_attribute) },
        )
    }
}

fn adjust_attribute(
    click: On<Activate>,
    buttons: Query<&AttributeButton>,
    mut flow: ResMut<AppFlow>,
) {
    let Ok(button) = buttons.get(click.entity) else {
        return;
    };
    let draft = flow.state().new_game_draft;
    if let Some(next) = kn_game::newgame_allocation(draft.class, draft.attributes)
        .and_then(|allocation| allocation.adjusted(draft.class, button.attribute, button.delta))
    {
        flow.submit(Action::HeroSelect(HeroSelectAction::SetAttributes {
            class: draft.class,
            values: next.values,
        }));
    }
}

#[derive(Component, Clone, Default)]
struct CreationContent;

fn scroll_content(
    event: On<bevy::picking::events::PointerScroll>,
    mut query: Query<(&mut ScrollPosition, &ComputedNode), With<CreationContent>>,
) {
    if let Ok((mut position, node)) = query.single_mut() {
        let factor = match event.unit {
            bevy::input::mouse::MouseScrollUnit::Line => 32.0,
            bevy::input::mouse::MouseScrollUnit::Pixel => 1.0,
        };
        let maximum = (node.content_size.y - node.size.y).max(0.0) * node.inverse_scale_factor;
        position.y = (position.y - event.y * factor).clamp(0.0, maximum);
    }
}
