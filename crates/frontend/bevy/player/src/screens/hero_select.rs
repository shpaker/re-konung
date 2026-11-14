//! The hero selection of the new-game flow, in the spirit of the original's
//! new-game menu (Slavik `game_newgame.cpp:115-186`) and the classical CRPG
//! presentation: a portrait card per offered hero on the left, the chosen
//! hero's dossier on the right — the description from the game's own lines,
//! the menu numbers (base attributes, health, carry limit, max party, the
//! seven skills, level, experience, gold, free points) and the hero's
//! selected portrait. The layout is compact so the whole offer reads at once
//! at 800×600 and up; nothing is invented. The choice leaves only as the
//! typed `HeroSelectAction`; the reducer owns who starts a session.

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
/// The width of the description column inside the dossier.
const DESCRIPTION_WIDTH: f32 = 520.0;

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
    let mut roots = world.query_filtered::<Entity, With<ScreenRoot>>();
    let old: Vec<Entity> = roots.iter(world).collect();
    for root in old {
        world.despawn(root);
    }
    spawn(world);
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
                .map(|summary| summary.portrait)
                .and_then(|record| portraits.get(&record).cloned());
            hero_card(fonts, hero, name, portrait, preview == Some(hero))
        })
        .collect::<Vec<_>>();
    let dossier = preview.and_then(|hero| summaries.iter().find(|summary| summary.hero == hero));
    let _dossier_portrait = dossier
        .map(|summary| summary.portrait_selected)
        .and_then(|record| portraits.get(&record).cloned());
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
    }
}

/// The card column and the dossier, side by side — the comparison the
/// classical reference puts on one screen.
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
    let dossier_list: Box<dyn SceneList> =
        Box::new(dossier_list(fonts, language, dossier, &model, hints, draft));
    bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            column_gap: spacing::SPACE_16,
            align_items: AlignItems::Start,
        }
        Children [
            {EntityScene(card_column(cards))}
            --
            {EntityScene(widgets::panel(fonts, dossier_list))}
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
        }
        Children [{cards}]
    }
}

/// One hero card of the list: the plain portrait above the name button, the
/// previewed one reading as primary — exactly like the settings screen's
/// active language. The portrait lives outside the button on purpose: an
/// image inside a `FeathersButton` caption breaks the caption's font
/// inheritance (the name renders in the theme's default face, without
/// Cyrillic). Two blocks: the BSN grammar only takes component constructors,
/// not value expressions.
fn hero_card(
    fonts: &KonungFonts,
    hero: u32,
    label: &str,
    portrait: Option<PortraitImage>,
    selected: bool,
) -> Box<dyn Scene> {
    if selected {
        Box::new(bsn! {
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: spacing::SPACE_8,
                width: percent(100),
            }
            Children [
                {EntityScene(portrait_view(portrait))}
                --
                {EntityScene(bsn! {
                    @widgets::menu_button(fonts, label)
                    HeroCard(hero)
                    ButtonVariant::Primary
                    on(click_hero_card)
                })}
            ]
        })
    } else {
        Box::new(bsn! {
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: spacing::SPACE_8,
                width: percent(100),
            }
            Children [
                {EntityScene(portrait_view(portrait))}
                --
                {EntityScene(bsn! {
                    @widgets::menu_button(fonts, label)
                    HeroCard(hero)
                    on(click_hero_card)
                })}
            ]
        })
    }
}

/// The chosen hero's dossier: the header (selected portrait, name, the
/// game's description), the constructor — the occupation, the combat style
/// and the secondary set, the numbers the original's menu showed (computed
/// from the choices exactly like the original's click branches), and the
/// actions. Everything visible at once.
fn dossier_list(
    fonts: &KonungFonts,
    language: kn_settings::Language,
    dossier: Option<&HeroSummary>,
    model: &Option<PortraitImage>,
    _hints: &[String],
    draft: kn_app::NewGameDraft,
) -> Box<dyn SceneList> {
    let Some(summary) = dossier else {
        return Box::new(bsn_list! {
            @caption(t(language, Text::HeroSlot(1)))
        });
    };
    let classes = kn_game::newgame_classes();
    let class = &classes[draft.class.min(classes.len() - 1)];
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
    let header = bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            column_gap: spacing::SPACE_16,
            align_items: AlignItems::Start,
            height: px(130.0),
        }
        Children [
            {EntityScene(portrait_view(model.clone()))}
            --
            {EntityScene(bsn! {
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: spacing::SPACE_8,
                    width: px(DESCRIPTION_WIDTH),
                }
                Children [
                    {EntityScene(screens::text(
                        fonts.bold.clone(),
                        typo::TITLE,
                        &summary.name,
                    ))}
                    --
                    {EntityScene(description(fonts, &summary.description))}
                ]
            })}
        ]
    };
    let attributes: Box<dyn SceneList> = Box::new(bsn_list! {
        {EntityScene(screens::text(fonts.bold.clone(), typo::BODY, t(language, Text::HeroAttributes)))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelAgility), &class.agility.to_string(), HINT_AGILITY))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelCharisma), &class.charisma.to_string(), HINT_CHARISMA))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelStrength), &class.strength.to_string(), HINT_STRENGTH))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelEndurance), &class.endurance.to_string(), HINT_ENDURANCE))}
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
        {EntityScene(row(fonts, t(language, Text::LabelLevel), &summary.level.to_string(), usize::MAX))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelExperience), &summary.experience.to_string(), usize::MAX))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelGold), &summary.gold.to_string(), usize::MAX))}
        --
        {EntityScene(row(fonts, t(language, Text::LabelFreePoints), &summary.free_points.to_string(), usize::MAX))}
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
    let other_skills = bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            column_gap: spacing::SPACE_32,
            align_items: AlignItems::Start,
        }
        Children [
            {EntityScene(bsn! {
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: spacing::SPACE_4,
                }
                Children [{others}]
            })}
            --
            {EntityScene(bsn! {
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: spacing::SPACE_4,
                }
                Children [{skills}]
            })}
        ]
    };
    let class_row = bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            column_gap: spacing::SPACE_8,
            align_items: AlignItems::Center,
            height: px(44.0),
        }
        // The row label is a bare `@caption`: the theme font has no
        // Cyrillic, the bundled face is set on the row.
        InheritableFont {
            font: {fonts.regular.clone()},
            font_size: typo::BODY,
            weight: FontWeight::NORMAL,
        }
        Children [
            @caption(t(language, Text::HeroClass)) TextColor(palette::TEXT_MUTED)
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
            column_gap: spacing::SPACE_8,
            align_items: AlignItems::Center,
            height: px(44.0),
        }
        // The row label is a bare `@caption`: the theme font has no
        // Cyrillic, the bundled face is set on the row.
        InheritableFont {
            font: {fonts.regular.clone()},
            font_size: typo::BODY,
            weight: FontWeight::NORMAL,
        }
        Children [
            @caption(t(language, Text::HeroSpec)) TextColor(palette::TEXT_MUTED)
            --
            {EntityScene(choice_button(fonts, t(language, Text::LabelFencing), draft.combat_style == 0, ConstructorKind::Combat, 0, HINT_FENCING))}
            --
            {EntityScene(choice_button(fonts, t(language, Text::LabelAccuracy), draft.combat_style == 1, ConstructorKind::Combat, 1, HINT_ACCURACY))}
        ]
    };
    let secondary_row = bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            column_gap: spacing::SPACE_8,
            align_items: AlignItems::Center,
            height: px(44.0),
        }
        // The row label is a bare `@caption`: the theme font has no
        // Cyrillic, the bundled face is set on the row.
        InheritableFont {
            font: {fonts.regular.clone()},
            font_size: typo::BODY,
            weight: FontWeight::NORMAL,
        }
        Children [
            @caption(t(language, Text::HeroSecondary)) TextColor(palette::TEXT_MUTED)
            --
            {EntityScene(choice_button(fonts, t(language, Text::LabelIdentification), draft.secondary == 0, ConstructorKind::Secondary, 0, HINT_IDENTIFICATION))}
            --
            {EntityScene(choice_button(fonts, t(language, Text::LabelTrading), draft.secondary == 1, ConstructorKind::Secondary, 1, HINT_TRADING))}
            --
            {EntityScene(choice_button(fonts, t(language, Text::LabelMedicine), draft.secondary == 2, ConstructorKind::Secondary, 2, HINT_MEDICINE))}
        ]
    };
    let actions: Box<dyn SceneList> = Box::new(bsn_list! {
        {EntityScene(begin_button(fonts, language, Some(summary.hero)))}
        --
        {EntityScene(back_button(fonts, language))}
    });
    Box::new(bsn_list! {
        {EntityScene(header)}
        --
        {EntityScene(bsn! {
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: spacing::SPACE_32,
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
                {EntityScene(other_skills)}
            ]
        })}
        --
        {EntityScene(class_row)}
        --
        {EntityScene(style_row)}
        --
        {EntityScene(secondary_row)}
        --
        {EntityScene(bsn! {
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: spacing::SPACE_16,
                margin: UiRect::top(px(8.0)),
            }
            Children [{actions}]
        })}
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
        }
        ImageNode { image: {portrait.handle} }
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
            width: px(DESCRIPTION_WIDTH),
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
