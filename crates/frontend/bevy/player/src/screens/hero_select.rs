//! Hero creation with original portraits, models, descriptions and rules.
//! A horizontal selector precedes the reusable three-column sheet. Narrow
//! windows stack its columns while retaining the ribbon and footer.
//! Only typed `HeroSelectAction`s leave the view; the reducer owns the draft.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::feathers::{controls::ButtonVariant, display::caption, font_styles::InheritableFont};
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::scene::{EntityScene, Scene};
use bevy::text::FontWeight;

use bevy::ui_widgets::Activate;
use kn_app::{Action, HeroSelectAction};
use kn_game::{HeroSummary, MapSource};
use kn_render::GameResource;
use kn_ui::{KonungFonts, palette, spacing, typo, widgets};

use crate::flow::{AppFlow, AppliedSettings, ScreenRoot};
use crate::i18n::{Text, points, t};
use crate::screens;
pub use crate::widgets::character_sheet::StatAction as AttributeButton;
use crate::widgets::character_sheet::{
    self, CharacterSheet, DetailSource, GroupPlacement, SheetImage, SheetInteraction, SheetScroll,
    Stat, StatEdit, StatGroup, StatOperation,
};

/// The centered creation block, including all three columns and their gaps.
const CREATION_WIDTH: f32 = 1040.0;

/// Which preview and constructor draft the on-screen view was built for: a
/// change in either means its captions are stale, and the view respawns
/// wholesale — the settings screen's `BuiltLanguage` pattern.
#[derive(Resource, Clone, PartialEq)]
struct HeroSelectView {
    preview: Option<u32>,
    draft: kn_app::CharacterCreationDraft,
}

/// The heroes' portraits, decoded once per process from the game's menu
/// images (`NEWH800R.RES`, any edition's — the portraits are identical);
/// keyed by record number. `None` — no game or an unreadable file: the cards
/// fall back to name-only buttons, the dossier to text alone.
#[derive(Resource, Default)]
struct HeroPortraits(Option<HashMap<u8, PortraitImage>>);

// The original's hover-hint lines: the `Texts::newgame` row each element
// points at (docs/research/k1/formats/executable.md, `Locale::NewGameMenu`).
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
/// lines. Empty descriptions leave the localized label and operation details available.
#[derive(Resource, Default, Clone)]
pub struct NewGameHints(pub Vec<String>);

/// The composed model previews, keyed by hero number; built once per
/// process together with the portraits.
#[derive(Resource, Default)]
struct HeroModels(HashMap<u32, PortraitImage>);

type PortraitImage = SheetImage;

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
            state.new_game_draft.hero,
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
        let records = world
            .get_resource::<GameResource>()
            .and_then(|game| game.0.menu_portraits());
        let portraits = records.map(|records| {
            let mut images = world.resource_mut::<Assets<Image>>();
            records
                .into_iter()
                .map(|(record, image)| {
                    let portrait = PortraitImage {
                        handle: images.add(portrait_texture_raw(
                            image.width,
                            image.height,
                            image.data,
                        )),
                        width: image.width as f32,
                        height: image.height as f32,
                    };
                    (record, portrait)
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
    world
        .resource_mut::<SheetInteraction>()
        .select_character(preview);
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

/// Respawns the view when the previewed hero moved: the detail panel and the
/// card highlight both follow the state. Runs after the flow chain, like the
/// settings screen's own sync.
pub(crate) fn sync(world: &mut World) {
    let Some(built) = world.get_resource::<HeroSelectView>().cloned() else {
        return;
    };
    let state = world.resource::<AppFlow>().state();
    if state.screen != kn_app::Screen::HeroSelect
        || (state.new_game_draft.hero == built.preview && state.new_game_draft == built.draft)
    {
        return;
    }
    let identity_changed = state.new_game_draft.hero != built.preview;
    let allocation_reset = (built.draft.class.is_some()
        && state.new_game_draft.class != built.draft.class)
        || (built.draft.attributes.is_some() && state.new_game_draft.attributes.is_none());
    let scroll_positions = world
        .query::<(&SheetScroll, &ScrollPosition)>()
        .iter(world)
        .map(|(id, position)| (*id, position.0))
        .collect::<HashMap<_, _>>();
    let hero_scroll = world
        .query_filtered::<&ScrollPosition, With<HeroListScroll>>()
        .iter(world)
        .next()
        .cloned();
    let focused = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get());
    let focus_reset = focused.is_some_and(|entity| world.get::<ResetButton>(entity).is_some());
    let focus_detail = focused
        .and_then(|entity| world.get::<DetailSource>(entity))
        .copied();
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
    if allocation_reset {
        let language = world.resource::<AppliedSettings>().0.language;
        for (mut text, mut node) in world
            .query_filtered::<(&mut bevy::ui::widget::Text, &mut Node), With<ClassResetNotice>>()
            .iter_mut(world)
        {
            text.0 = t(language, Text::AllocationReset).to_string();
            node.display = Display::Flex;
        }
    }
    for (id, mut position) in world
        .query::<(&SheetScroll, &mut ScrollPosition)>()
        .iter_mut(world)
    {
        if identity_changed && id.0 == 4 {
            continue;
        }
        if let Some(previous) = scroll_positions.get(id) {
            position.0 = *previous;
        }
    }
    if let Some(previous) = hero_scroll {
        for mut position in world
            .query_filtered::<&mut ScrollPosition, With<HeroListScroll>>()
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
    } else if focus_reset {
        world
            .query_filtered::<Entity, With<ResetButton>>()
            .iter(world)
            .next()
    } else if let Some(detail) = focus_detail {
        world
            .query::<(Entity, &DetailSource)>()
            .iter(world)
            .find(|(_, source)| **source == detail)
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
    draft: kn_app::CharacterCreationDraft,
) -> impl Scene {
    let cards = heroes
        .iter()
        .enumerate()
        .map(|(row, &hero)| {
            let summary = summaries.iter().find(|summary| summary.hero == hero);
            let name = summary
                .map(|summary| summary.name.clone())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| format!("{} {}", t(language, Text::HeroSlot(0)), row + 1));
            let portrait = summary
                .map(|summary| {
                    if preview == Some(hero) {
                        summary.portrait_selected
                    } else {
                        summary.portrait
                    }
                })
                .and_then(|record| portraits.get(&record).cloned());
            hero_card(fonts, hero, &name, portrait, preview == Some(hero))
        })
        .collect::<Vec<_>>();
    let dossier = preview.and_then(|hero| summaries.iter().find(|summary| summary.hero == hero));
    let mut data = sheet_data(language, dossier, hints, models, draft);
    data.extra_details
        .extend(heroes.iter().enumerate().map(|(row, hero)| {
            let summary = summaries.iter().find(|summary| summary.hero == *hero);
            Stat {
                id: 1000 + *hero as usize,
                label: summary.map_or_else(
                    || format!("{} {}", t(language, Text::HeroSlot(0)), row + 1),
                    |summary| summary.name.clone(),
                ),
                value: String::new(),
                description: summary
                    .map_or_else(String::new, |summary| summary.description.clone()),
                edit: None,
            }
        }));
    if let Some(hero) = preview
        && dossier.is_none()
    {
        data.name = format!(
            "{} {}",
            t(language, Text::HeroSlot(0)),
            heroes.iter().position(|value| *value == hero).unwrap_or(0) + 1
        );
    }
    let ready = draft.complete().is_some();
    let missing = missing_choices(language, draft);
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            min_height: px(0),
            max_height: percent(100),
            min_width: px(0),
            max_width: percent(100),
            overflow: Overflow::clip(),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            padding: UiRect::all(px(24.0)),
            row_gap: spacing::SPACE_16,
        }
        BackgroundColor(palette::WINDOW_BG)
        ScreenRoot
        bevy::input_focus::tab_navigation::TabGroup { modal: true }
        Children [
            Node { width: percent(100), max_width: px(CREATION_WIDTH), min_width: px(0), height: px(100), min_height: px(100), column_gap: px(8), overflow: Overflow::scroll_x(), flex_shrink: 0.0 }
            HeroListScroll ScrollPosition::default() on(character_sheet::scroll)
            Children [{cards}]
            --
            Node { width: percent(100), max_width: px(CREATION_WIDTH), flex_grow: 1.0, flex_basis: px(0), min_height: px(0), min_width: px(0) }
            CreationContent
            Children [
                {EntityScene(character_sheet::sheet(fonts, data, bsn_list! {
                    {EntityScene(constructor_choices(fonts, language, draft))}
                }))}
            ]
            --
            Node {
                width: percent(100),
                max_width: px(CREATION_WIDTH),
                min_width: px(0),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                column_gap: spacing::SPACE_16,
                min_height: px(64),
                padding: UiRect::all(px(12.0)),
                flex_shrink: 0.0,
            }
            BackgroundColor(palette::SURFACE)
            Children [
                {EntityScene(back_button(fonts, language))}
                --
                Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: spacing::SPACE_4, min_width: px(0), flex_grow: 1.0, flex_basis: px(0) }
                Children [
                    {EntityScene(points_counter(fonts, language, draft))}
                    --
                    @screens::text(fonts.regular.clone(), typo::CAPTION, missing.as_str()) Node { width: percent(100) } TextLayout::justify(Justify::Center)
                ]
                --
                {EntityScene(begin_button(fonts, language, preview, ready))}
            ]
        ]
    }
}

fn missing_choices(
    language: kn_settings::Language,
    draft: kn_app::CharacterCreationDraft,
) -> String {
    let missing = [
        (draft.hero.is_none(), Text::HeroSlot(0)),
        (draft.class.is_none(), Text::HeroClass),
        (draft.combat_style.is_none(), Text::HeroSpec),
        (draft.secondary.is_none(), Text::HeroSecondary),
    ]
    .into_iter()
    .filter(|(missing, _)| *missing)
    .map(|(_, label)| t(language, label))
    .collect::<Vec<_>>();
    if missing.is_empty() {
        t(language, Text::UnspentPointsHelp).into()
    } else {
        format!(
            "{}: {}",
            t(language, Text::RequiredChoices),
            missing.join(", ")
        )
    }
}

#[derive(Component, Clone, Default)]
struct HeroListScroll;

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
    let portrait = portrait.map(|mut image| {
        let scale = (64.0 / image.height).min(64.0 / image.width).min(1.0);
        image.width *= scale;
        image.height *= scale;
        image
    });
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
                    flex_direction: FlexDirection::Row,
                    column_gap: px(8),
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
            width: px(184),
            height: px(96),
            min_height: px(96),
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
        HeroCard(hero) DetailSource({1000 + hero as usize})
        on(click_hero_card)
    }
}

fn sheet_data(
    language: kn_settings::Language,
    summary: Option<&HeroSummary>,
    hints: &[String],
    models: &HashMap<u32, PortraitImage>,
    draft: kn_app::CharacterCreationDraft,
) -> CharacterSheet {
    let allocation = draft
        .class
        .and_then(|class| kn_game::newgame_allocation(class, draft.attributes));
    let baseline = draft
        .class
        .and_then(|class| kn_game::newgame_allocation(class, None));
    let profile = draft
        .class
        .and_then(|class| kn_game::newgame_profile(class, draft.attributes));
    let style = if draft.combat_style == Some(1) {
        kn_game::COMBAT_ACCURACY
    } else {
        kn_game::COMBAT_FENCING
    };
    let secondary = match draft.secondary {
        Some(1) => kn_game::SECONDARY_TRADING,
        Some(2) => kn_game::SECONDARY_MEDICINE,
        _ => kn_game::SECONDARY_DEFAULT,
    };
    let hint = |index: usize| hints.get(index).cloned().unwrap_or_default();
    let operation = |attribute, delta| {
        let Some((allocation, class)) = allocation.zip(draft.class) else {
            return StatOperation {
                summary: String::new(),
                blocked: Some(t(language, Text::ChooseClass).into()),
            };
        };
        let change = allocation
            .change(class, attribute, delta)
            .expect("valid attribute change");
        StatOperation {
            summary: format!(
                "{}: {}{}",
                t(
                    language,
                    if delta < 0 {
                        Text::Decrease
                    } else {
                        Text::Increase
                    }
                ),
                if delta < 0 { "+" } else { "" },
                points(language, change.cost.abs())
            ),
            blocked: change.blocked.map(|reason| {
                t(
                    language,
                    match reason {
                        kn_game::NewGameChangeBlocked::Minimum => Text::AttributeMinimum,
                        kn_game::NewGameChangeBlocked::Maximum => Text::AttributeMaximum,
                        kn_game::NewGameChangeBlocked::InsufficientPoints => {
                            Text::InsufficientPoints
                        }
                    },
                )
                .to_string()
            }),
        }
    };
    let attributes = [
        (Text::LabelAgility, HINT_AGILITY),
        (Text::LabelCharisma, HINT_CHARISMA),
        (Text::LabelStrength, HINT_STRENGTH),
        (Text::LabelEndurance, HINT_ENDURANCE),
    ]
    .into_iter()
    .enumerate()
    .map(|(id, (label, line))| Stat {
        id,
        label: t(language, label).to_string(),
        value: allocation.map_or_else(
            || "—".into(),
            |allocation| allocation.values[id].to_string(),
        ),
        description: hint(line),
        edit: Some(StatEdit {
            baseline: baseline
                .map_or_else(|| "—".into(), |baseline| baseline.values[id].to_string()),
            decrease: operation(id, -1),
            increase: operation(id, 1),
        }),
    })
    .collect();
    let facts = [
        (
            Text::LabelHealth,
            summary.map_or_else(|| "—".into(), |hero| hero.health.to_string()),
            usize::MAX,
        ),
        (
            Text::LabelCarrying,
            profile.map_or_else(|| "—".into(), |profile| format!("{:.1}", profile.carrying)),
            HINT_CARRY,
        ),
        (
            Text::LabelMaxParty,
            profile.map_or_else(|| "—".into(), |profile| profile.max_party.to_string()),
            HINT_PARTY,
        ),
        (Text::LabelLevel, "1".to_string(), usize::MAX),
        (
            Text::LabelExperience,
            summary.map_or_else(|| "—".into(), |hero| hero.experience.to_string()),
            usize::MAX,
        ),
        (
            Text::LabelGold,
            summary.map_or_else(|| "—".into(), |hero| hero.gold.to_string()),
            usize::MAX,
        ),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (label, value, line))| Stat {
        id: 20 + index,
        label: t(language, label).to_string(),
        value,
        description: hint(line),
        edit: None,
    })
    .collect();
    let skills = [
        (Text::LabelFencing, style.fencing, HINT_FENCING),
        (Text::LabelTrading, secondary.trading, HINT_TRADING),
        (Text::LabelAccuracy, style.accuracy, HINT_ACCURACY),
        (Text::LabelCarpentry, style.carpentry, HINT_CARPENTRY),
        (Text::LabelMedicine, secondary.medicine, HINT_MEDICINE),
        (
            Text::LabelIdentification,
            secondary.identification,
            HINT_IDENTIFICATION,
        ),
        (Text::LabelSmithery, 10, HINT_SMITHERY),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (label, value, line))| Stat {
        id: 40 + index,
        label: t(language, label).to_string(),
        value: if (matches!(index, 0 | 2 | 3) && draft.combat_style.is_none())
            || (matches!(index, 1 | 4 | 5) && draft.secondary.is_none())
        {
            "—".into()
        } else {
            value.to_string()
        },
        description: hint(line),
        edit: None,
    })
    .collect();
    let detail_labels = [
        (HINT_CLASS_WARRIOR, Text::ClassWarrior),
        (HINT_CLASS_HUNTER, Text::ClassHunter),
        (HINT_CLASS_MERCHANT, Text::ClassMerchant),
        (HINT_CLASS_CHIEF, Text::ClassChief),
        (HINT_FENCING, Text::LabelFencing),
        (HINT_ACCURACY, Text::LabelAccuracy),
        (HINT_IDENTIFICATION, Text::LabelIdentification),
        (HINT_TRADING, Text::LabelTrading),
        (HINT_MEDICINE, Text::LabelMedicine),
    ];
    CharacterSheet {
        character: draft.hero.unwrap_or_default(),
        name: summary.map_or_else(
            || t(language, Text::ChooseHero).to_string(),
            |hero| hero.name.clone(),
        ),
        image: summary.and_then(|hero| models.get(&hero.hero).cloned()),
        history: summary.map_or_else(String::new, |hero| hero.description.clone()),
        baseline_label: t(language, Text::Baseline).to_string(),
        guidance: t(language, Text::AllocationHelp).to_string(),
        groups: vec![
            StatGroup {
                placement: GroupPlacement::Statistics,
                label: t(language, Text::HeroAttributes).to_string(),
                stats: attributes,
            },
            StatGroup {
                placement: GroupPlacement::BesideImage,
                label: String::new(),
                stats: facts,
            },
            StatGroup {
                placement: GroupPlacement::BelowHistory,
                label: t(language, Text::HeroSkills).to_string(),
                stats: skills,
            },
        ],
        extra_details: hints
            .iter()
            .enumerate()
            .map(|(id, description)| Stat {
                id: 100 + id,
                label: detail_labels
                    .iter()
                    .find(|(line, _)| *line == id)
                    .map_or_else(String::new, |(_, label)| t(language, *label).to_string()),
                value: String::new(),
                description: description.clone(),
                edit: None,
            })
            .collect(),
    }
}

#[derive(Component, Default, Clone)]
struct ClassResetNotice;

fn constructor_choices(
    fonts: &KonungFonts,
    language: kn_settings::Language,
    draft: kn_app::CharacterCreationDraft,
) -> impl Scene {
    let reset: Box<dyn Scene> = if draft.class.is_some() {
        Box::new(
            bsn! { @widgets::menu_button(fonts, t(language, Text::Reset)) ResetButton on(reset_attributes) Node { align_self: AlignSelf::End } },
        )
    } else {
        Box::new(
            bsn! { @widgets::menu_button(fonts, t(language, Text::Reset)) ResetButton bevy::ui::InteractionDisabled Node { align_self: AlignSelf::End } },
        )
    };
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
            @caption(t(language, Text::HeroClass)) Node { min_width: px(72) } TextColor(palette::TEXT_MUTED)
            --
            {EntityScene(choice_button(fonts, t(language, Text::ClassWarrior), draft.class == Some(0), ConstructorKind::Class, 0, HINT_CLASS_WARRIOR))}
            --
            {EntityScene(choice_button(fonts, t(language, Text::ClassHunter), draft.class == Some(1), ConstructorKind::Class, 1, HINT_CLASS_HUNTER))}
            --
            {EntityScene(choice_button(fonts, t(language, Text::ClassMerchant), draft.class == Some(2), ConstructorKind::Class, 2, HINT_CLASS_MERCHANT))}
            --
            {EntityScene(choice_button(fonts, t(language, Text::ClassChief), draft.class == Some(3), ConstructorKind::Class, 3, HINT_CLASS_CHIEF))}
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
            @caption(t(language, Text::HeroSpec)) Node { min_width: px(72) } TextColor(palette::TEXT_MUTED)
            --
            {EntityScene(choice_button(fonts, t(language, Text::LabelFencing), draft.combat_style == Some(0), ConstructorKind::Combat, 0, HINT_FENCING))}
            --
            {EntityScene(choice_button(fonts, t(language, Text::LabelAccuracy), draft.combat_style == Some(1), ConstructorKind::Combat, 1, HINT_ACCURACY))}
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
            @caption(t(language, Text::HeroSecondary)) Node { min_width: px(72) } TextColor(palette::TEXT_MUTED)
            --
            {EntityScene(choice_button(fonts, t(language, Text::LabelIdentification), draft.secondary == Some(0), ConstructorKind::Secondary, 0, HINT_IDENTIFICATION))}
            --
            {EntityScene(choice_button(fonts, t(language, Text::LabelTrading), draft.secondary == Some(1), ConstructorKind::Secondary, 1, HINT_TRADING))}
            --
            {EntityScene(choice_button(fonts, t(language, Text::LabelMedicine), draft.secondary == Some(2), ConstructorKind::Secondary, 2, HINT_MEDICINE))}
        ]
    };
    bsn! {
        Node { flex_direction: FlexDirection::Column, row_gap: spacing::SPACE_8 }
        InheritableFont { font: {fonts.regular.clone()}, font_size: typo::CAPTION, weight: FontWeight::NORMAL }
        Children [
            {EntityScene(reset)}
            --
            {EntityScene(class_row)}
            --
            {EntityScene(screens::text(fonts.regular.clone(), typo::CAPTION, t(language, Text::AllocationHelp)))}
            --
            {EntityScene(screens::text(fonts.regular.clone(), typo::CAPTION, t(language, Text::ClassResetHelp)))}
            --
            @caption("") ClassResetNotice Node { display: Display::None } TextColor(palette::ACCENT)
            --
            {EntityScene(style_row)}
            --
            {EntityScene(secondary_row)}
        ]
    }
}

fn portrait_view(portrait: Option<PortraitImage>) -> Box<dyn Scene> {
    character_sheet::image_view(portrait)
}

/// Begin with the previewed hero, or return to the main menu. Begin carries
/// the previewed number — the reducer still validates it against the offer.
fn begin_button(
    fonts: &KonungFonts,
    language: kn_settings::Language,
    preview: Option<u32>,
    ready: bool,
) -> Box<dyn Scene> {
    if ready {
        Box::new(
            bsn! { @widgets::menu_button(fonts, t(language, Text::Start)) ButtonVariant::Primary BeginButton(preview) on(click_begin) },
        )
    } else {
        Box::new(
            bsn! { @widgets::menu_button(fonts, t(language, Text::Start)) bevy::ui::InteractionDisabled BeginButton(preview) on(click_begin) },
        )
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
            DetailSource({100 + hint_line})
            ButtonVariant::Primary
            on(click_constructor)
        })
    } else {
        Box::new(bsn! {
            @widgets::menu_button(fonts, label.as_str())
            ConstructorButton { kind, index }
            DetailSource({100 + hint_line})
            on(click_constructor)
        })
    }
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
    buttons: Query<(&ConstructorButton, &DetailSource)>,
    mut sheet: ResMut<SheetInteraction>,
    mut flow: ResMut<AppFlow>,
) {
    let Ok((&button, detail)) = buttons.get(click.entity) else {
        return;
    };
    sheet.selected = Some(detail.0);
    let action = match button.kind {
        ConstructorKind::Class => HeroSelectAction::SetClass(button.index),
        ConstructorKind::Combat => HeroSelectAction::SetCombatStyle(button.index),
        ConstructorKind::Secondary => HeroSelectAction::SetSecondary(button.index),
    };
    flow.submit(Action::HeroSelect(action));
}

fn click_hero_card(
    click: On<Activate>,
    cards: Query<&HeroCard>,
    mut flow: ResMut<AppFlow>,
    mut sheet: ResMut<SheetInteraction>,
) {
    if let Ok(&HeroCard(hero)) = cards.get(click.entity) {
        sheet.selected = Some(1000 + hero as usize);
        flow.submit(Action::HeroSelect(HeroSelectAction::Preview(hero)));
    }
}

fn click_begin(
    click: On<Activate>,
    buttons: Query<&BeginButton, Without<bevy::ui::InteractionDisabled>>,
    mut flow: ResMut<AppFlow>,
) {
    if let Ok(&BeginButton(hero)) = buttons.get(click.entity)
        && let Some(hero) = hero
    {
        flow.submit(Action::HeroSelect(HeroSelectAction::Choose(hero)));
    }
}

fn click_back(_click: On<Activate>, mut flow: ResMut<AppFlow>) {
    flow.submit(Action::HeroSelect(HeroSelectAction::Cancel));
}

/// Enter/Space activate the focused control; Escape returns to the menu.
pub(crate) fn keyboard(keys: Option<Res<ButtonInput<KeyCode>>>, mut flow: ResMut<AppFlow>) {
    if flow.state().screen == kn_app::Screen::HeroSelect
        && keys.is_some_and(|keys| keys.just_pressed(KeyCode::Escape))
    {
        flow.submit(Action::HeroSelect(HeroSelectAction::Cancel));
    }
}

fn points_counter(
    fonts: &KonungFonts,
    language: kn_settings::Language,
    draft: kn_app::CharacterCreationDraft,
) -> impl Scene {
    let allocation = draft
        .class
        .and_then(|class| kn_game::newgame_allocation(class, draft.attributes));
    let label = format!(
        "{}: {}",
        t(language, Text::LabelFreePoints),
        allocation.map_or_else(
            || "—".into(),
            |allocation| allocation.free_points.to_string()
        )
    );
    bsn! {
        Node
        InheritableFont { font: {fonts.bold.clone()}, font_size: typo::TITLE, weight: FontWeight::NORMAL }
        Children [@caption(label.as_str()) TextColor(palette::ACCENT)]
    }
}

pub(crate) fn adjust_attribute(event: On<character_sheet::AdjustStat>, mut flow: ResMut<AppFlow>) {
    if flow.state().screen != kn_app::Screen::HeroSelect {
        return;
    }
    let button = event.0;
    let draft = flow.state().new_game_draft;
    if let Some(class) = draft.class
        && let Some(next) = kn_game::newgame_allocation(class, draft.attributes)
            .and_then(|allocation| allocation.adjusted(class, button.attribute, button.delta))
    {
        flow.submit(Action::HeroSelect(HeroSelectAction::SetAttributes {
            class,
            values: next.values,
        }));
    }
}

#[derive(Component, Clone, Default)]
struct ResetButton;

fn reset_attributes(_: On<Activate>, mut flow: ResMut<AppFlow>) {
    let draft = flow.state().new_game_draft;
    if let Some(class) = draft.class {
        flow.submit(Action::HeroSelect(HeroSelectAction::SetClass(class as u8)));
    }
}

#[derive(Component, Clone, Default)]
struct CreationContent;
