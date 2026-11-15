//! The screen views: they render in the applied language, the settings
//! screen previews the draft live, and a failed save leaves its sentence on
//! the screen while the draft survives for a retry.
//!
//! Tests on real data skip when `original/` is absent (docs/research/k1/editions.md).

mod common;

use bevy::feathers::controls::ButtonVariant;
use bevy::feathers::controls::FeathersButton;
use bevy::prelude::*;
use bevy::ui::Checked;
use bevy::ui::widget::Text;
use bevy::ui_widgets::Checkbox;
use common::{all_heroes, first_hero, game_dir, player_app, player_app_with};
use kn_app::{Action, HeroSelectAction, MainMenuAction, SettingsAction, SettingsField};
use kn_fe_player::flow::{AppFlow, AppliedSettings};
use kn_fe_player::screens::{ConstructorButton, ConstructorKind};
use kn_fe_player::widgets::character_sheet::{
    DetailSource, DetailText, HistoryText, SheetInteraction,
};
use kn_game::Game;
use kn_settings::{Language, Settings};

/// Every interface string currently on screen.
fn strings(app: &mut App) -> Vec<String> {
    let mut texts = app.world_mut().query::<&Text>();
    texts.iter(app.world()).map(|text| text.0.clone()).collect()
}

/// The entity of the on-screen text containing `part`.
fn text_entity(app: &mut App, part: &str) -> Entity {
    let mut texts = app.world_mut().query_filtered::<Entity, With<Text>>();
    texts
        .iter(app.world())
        .find(|entity| {
            app.world()
                .get::<Text>(*entity)
                .is_some_and(|text| text.0.contains(part))
        })
        .unwrap_or_else(|| panic!("an on-screen text containing {part:?} must exist"))
}

/// The checkbox whose caption contains `part`: the nearest `Checkbox`
/// ancestor of that text — captions sit inside the control's own subtree,
/// not necessarily on a direct child.
fn checkbox_of_caption(app: &mut App, part: &str) -> Entity {
    let mut entity = text_entity(app, part);
    loop {
        let Some(child_of) = app.world().get::<ChildOf>(entity) else {
            panic!("no checkbox above the caption {part:?}");
        };
        entity = child_of.parent();
        if app.world().entity(entity).get::<Checkbox>().is_some() {
            return entity;
        }
    }
}

#[test]
fn the_menu_renders_in_the_applied_language() {
    let Some(dir) = game_dir() else {
        return;
    };
    let game = Game::open(&dir).unwrap();
    let hero = first_hero(&game);

    let mut app = player_app(game, vec![hero]);
    app.update();
    let shown = strings(&mut app);
    assert!(
        shown.iter().any(|s| s.contains("Новая игра")),
        "the default language is Russian, got {shown:?}"
    );

    // A fresh app born with English applied speaks English from the first
    // frame — the language is a startup setting, not a click. A second
    // opening of the game: the first one was moved into the Russian app.
    let english = Settings {
        language: Language::English,
        ..Settings::default()
    };
    let mut app = player_app_with(Game::open(&dir).unwrap(), vec![hero], english);
    app.update();
    let shown = strings(&mut app);
    assert!(
        shown.iter().any(|s| s.contains("New game")),
        "the applied language rules the menu, got {shown:?}"
    );
    assert!(
        !shown.iter().any(|s| s.contains("Новая игра")),
        "no stale Russian strings on the English menu"
    );
}

#[test]
fn the_settings_screen_previews_the_draft_and_mirrors_it_onto_the_widgets() {
    let Some(dir) = game_dir() else {
        return;
    };
    let game = Game::open(&dir).unwrap();
    let hero = first_hero(&game);
    let mut app = player_app(game, vec![hero]);
    app.update();

    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::MainMenu(MainMenuAction::OpenSettings));
    app.update();
    assert!(
        strings(&mut app).iter().any(|s| s.contains("Настройки")),
        "the settings screen opened"
    );

    // The fullscreen checkbox mirrors the draft: off in the defaults, on
    // after the change action.
    let fullscreen = checkbox_of_caption(&mut app, "Полный экран");
    assert!(
        app.world().entity(fullscreen).get::<Checked>().is_none(),
        "fullscreen is off in the defaults"
    );
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::Settings(SettingsAction::Change(
            SettingsField::Fullscreen(true),
        )));
    app.update();
    assert!(
        app.world().entity(fullscreen).get::<Checked>().is_some(),
        "the checkbox mirrors the draft"
    );

    // The live language preview: switch the draft and the screen itself
    // respeaks, while the applied language has not moved yet.
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::Settings(SettingsAction::Change(
            SettingsField::Language(Language::English),
        )));
    app.update();
    assert!(
        strings(&mut app).iter().any(|s| s.contains("Fullscreen")),
        "the settings screen previews the draft language"
    );
    assert_eq!(
        app.world().resource::<AppliedSettings>().0.language,
        Language::Russian,
        "the applied language waits for a successful save"
    );
}

#[test]
fn a_failed_save_shows_its_sentence_and_keeps_the_draft() {
    let Some(dir) = game_dir() else {
        return;
    };
    let game = Game::open(&dir).unwrap();
    let hero = first_hero(&game);
    let mut app = player_app(game, vec![hero]);
    app.update();
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::MainMenu(MainMenuAction::OpenSettings));
    app.update();
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::Settings(SettingsAction::Change(
            SettingsField::Language(Language::English),
        )));
    app.update();

    // Save with no settings path — the typed Write failure the adapter
    // reports; the runtime rolls back, the draft survives, the sentence
    // appears in the draft's language.
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::Settings(SettingsAction::Save));
    app.update();
    assert!(
        strings(&mut app)
            .iter()
            .any(|s| s.contains("The settings could not be written")),
        "the save failure is on screen in the draft language"
    );
    assert_eq!(
        app.world().resource::<AppliedSettings>().0.language,
        Language::Russian,
        "the runtime rolled back to the applied settings"
    );

    // Cancel discards the draft; the menu is still in the applied language.
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::Settings(SettingsAction::Cancel));
    app.update();
    assert!(
        strings(&mut app).iter().any(|s| s.contains("Новая игра")),
        "back at the Russian menu"
    );
}

#[test]
fn the_loading_screen_shows_while_the_session_builds() {
    let Some(dir) = game_dir() else {
        return;
    };
    let game = Game::open(&dir).unwrap();
    let hero = first_hero(&game);
    let mut app = player_app(game, vec![hero]);
    app.update();
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::MainMenu(MainMenuAction::NewGame));
    for choice in [
        HeroSelectAction::Preview(hero),
        HeroSelectAction::SetClass(0),
        HeroSelectAction::SetCombatStyle(0),
        HeroSelectAction::SetSecondary(0),
    ] {
        app.world_mut()
            .resource_mut::<AppFlow>()
            .submit(Action::HeroSelect(choice));
    }
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::HeroSelect(HeroSelectAction::Choose(hero)));
    app.update();
    assert!(
        strings(&mut app).iter().any(|s| s.contains("Загрузка")),
        "the loading line shows while the session builds"
    );
}

/// The hero selection in full: one named card per offered hero, the dossier
/// of the previewed hero on the right (attributes, skills, the rest), and
/// the two actions — preview moves the dossier, Back returns to the menu
/// (#121).
#[test]
fn the_hero_select_presents_every_hero_and_previews_the_details() {
    let Some(dir) = game_dir() else {
        return;
    };
    let game = Game::open(&dir).unwrap();
    let heroes = all_heroes(&game);
    let summaries = game.hero_summaries(kn_game::MapSource::Game).unwrap();
    let mut app = player_app(game, heroes);
    app.update();

    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::MainMenu(MainMenuAction::NewGame));
    app.update();
    let shown = strings(&mut app);
    assert!(
        shown.iter().any(|s| s.contains("Характеристики")),
        "the hero-select screen opened with the dossier, got {shown:?}"
    );
    for summary in &summaries {
        assert!(
            !summary.name.is_empty(),
            "the game data must name every hero for this test to be meaningful"
        );
        assert!(
            shown.iter().any(|s| s.contains(summary.name.as_str())),
            "the card of hero {:?} is on screen, got {shown:?}",
            summary.name
        );
    }
    // The dossier of the default preview (the first hero), sectioned.
    for part in ["Характеристики", "Навыки", "Сила", "Фехтование", "Начать"]
    {
        assert!(
            shown.iter().any(|s| s.contains(part)),
            "the dossier holds {part:?}, got {shown:?}"
        );
    }
    let first_name = summaries[0].name.clone();
    let occurrences =
        |app: &mut App, name: &str| strings(app).iter().filter(|s| s.as_str() == name).count();
    let second_name = summaries[1].name.clone();
    assert_eq!(
        occurrences(&mut app, &second_name),
        1,
        "the second hero's name sits on its card alone"
    );

    // A biography can mention another hero; select the exact card caption.
    let mut target = app
        .world_mut()
        .query::<(Entity, &Text)>()
        .iter(app.world())
        .find(|(_, text)| text.0 == second_name)
        .expect("the second hero has a card caption")
        .0;
    while app
        .world()
        .get::<bevy::ui_widgets::Button>(target)
        .is_none()
    {
        target = app
            .world()
            .get::<ChildOf>(target)
            .expect("caption belongs to a button")
            .parent();
    }
    let portrait_inside = app
        .world_mut()
        .query_filtered::<Entity, With<bevy::ui::widget::ImageNode>>()
        .iter(app.world())
        .any(|mut image| {
            while let Some(parent) = app.world().get::<ChildOf>(image) {
                image = parent.parent();
                if image == target {
                    return true;
                }
            }
            false
        });
    assert!(
        portrait_inside,
        "the portrait must be inside the clickable card"
    );
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity: target });
    app.update();
    let shown = strings(&mut app);
    assert_eq!(
        occurrences(&mut app, &second_name),
        2,
        "the dossier follows the preview, got {shown:?}"
    );
    assert_eq!(
        occurrences(&mut app, &first_name),
        1,
        "the first hero is back to its card alone"
    );

    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::HeroSelect(HeroSelectAction::Cancel));
    app.update();
    assert!(
        strings(&mut app).iter().any(|s| s.contains("Новая игра")),
        "Cancel returned to the main menu"
    );
}

#[test]
fn history_is_readable_on_hover_without_selection_and_persists_after_selection() {
    let Some(dir) = game_dir() else {
        return;
    };
    let game = Game::open(&dir).unwrap();
    let heroes = all_heroes(&game);
    let summaries = game.hero_summaries(kn_game::MapSource::Game).unwrap();
    let mut app = player_app(game, heroes.clone());
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::MainMenu(MainMenuAction::NewGame));
    app.update();
    assert!(
        app.world()
            .resource::<AppFlow>()
            .state()
            .new_game_draft
            .hero
            .is_none()
    );
    assert!(
        strings(&mut app)
            .iter()
            .any(|text| text == "Выберите персонажа")
    );
    let target = app
        .world_mut()
        .query::<(Entity, &DetailSource)>()
        .iter(app.world())
        .find(|(_, source)| source.0 == 1000 + heroes[1] as usize)
        .unwrap()
        .0;
    app.world_mut()
        .entity_mut(target)
        .insert(bevy::picking::hover::Hovered(true));
    app.update();
    let details = |app: &mut App| {
        app.world_mut()
            .query_filtered::<&Text, With<DetailText>>()
            .single(app.world())
            .unwrap()
            .0
            .clone()
    };
    assert!(details(&mut app).contains(&summaries[1].description));
    assert!(
        app.world()
            .resource::<AppFlow>()
            .state()
            .new_game_draft
            .hero
            .is_none()
    );
    app.world_mut()
        .entity_mut(target)
        .insert(bevy::picking::hover::Hovered(false));
    app.update();
    assert!(details(&mut app).contains(&summaries[1].description));
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity: target });
    app.update();
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::HeroSelect(HeroSelectAction::SetClass(0)));
    app.update();
    assert_eq!(
        app.world_mut()
            .query_filtered::<&Text, With<HistoryText>>()
            .single(app.world())
            .unwrap()
            .0,
        summaries[1].description
    );
    app.init_resource::<ButtonInput<KeyCode>>();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Escape);
    app.update();
    assert_eq!(
        app.world().resource::<AppFlow>().state().screen,
        kn_app::Screen::MainMenu
    );
}

/// Clicking an occupation redraws the constructor: the drafted button reads
/// primary and the numbers follow the class (Воин: Сила 15 → Охотник:
/// Сила 3).
#[test]
fn choosing_an_occupation_redraws_the_numbers_and_the_highlight() {
    let Some(dir) = game_dir() else {
        return;
    };
    let heroes = all_heroes(&Game::open(&dir).unwrap());
    let mut app = player_app(Game::open(&dir).unwrap(), heroes);
    app.update();

    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::MainMenu(MainMenuAction::NewGame));
    app.update();
    // Rows render as two captions (label, then value): Воин has Сила 15,
    // Охотник has Сила 3 — the pair is the discriminator.
    let strength_value = |app: &mut App| {
        let shown = strings(app);
        let index = shown
            .iter()
            .position(|s| s == "Сила")
            .expect("the Сила row stands in the dossier");
        shown[index + 1].clone()
    };
    assert_eq!(
        strength_value(&mut app),
        "—",
        "an unselected occupation must not expose hidden defaults"
    );
    // The Охотник button is not primary before the click.
    let mut buttons = app
        .world_mut()
        .query_filtered::<(Entity, &ConstructorButton, &ButtonVariant), With<FeathersButton>>();
    let hunter = buttons
        .iter(app.world())
        .find(|(_, button, _)| {
            **button
                == ConstructorButton {
                    kind: ConstructorKind::Class,
                    index: 1,
                }
        })
        .map(|(entity, _, _)| entity)
        .expect("the Охотник button stands on screen");
    assert!(
        app.world()
            .get::<ButtonVariant>(hunter)
            .is_none_or(|v| *v != ButtonVariant::Primary),
        "Охотник is not primary before the click"
    );

    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::HeroSelect(HeroSelectAction::SetClass(1)));
    app.update();

    assert_eq!(strength_value(&mut app), "3", "the numbers follow Охотник");
    let (_, _, variant) = buttons
        .iter(app.world())
        .find(|(_, button, _)| {
            **button
                == ConstructorButton {
                    kind: ConstructorKind::Class,
                    index: 1,
                }
        })
        .expect("the Охотник button stands on screen");
    assert_eq!(variant, &ButtonVariant::Primary, "Охотник reads as drafted");
}

#[test]
fn attribute_buttons_refund_spend_reset_and_preserve_keyboard_focus() {
    use bevy::input_focus::InputFocus;
    use kn_fe_player::screens::AttributeButton;
    let Some(dir) = game_dir() else {
        return;
    };
    let game = Game::open(&dir).unwrap();
    let heroes = all_heroes(&game);
    let mut app = player_app(game, heroes);
    app.init_resource::<InputFocus>();
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::MainMenu(MainMenuAction::NewGame));
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::HeroSelect(HeroSelectAction::SetClass(0)));
    app.update();
    let button = |app: &mut App, attribute, delta| {
        app.world_mut()
            .query::<(Entity, &AttributeButton)>()
            .iter(app.world())
            .find(|(_, button)| button.attribute == attribute && button.delta == delta)
            .unwrap()
            .0
    };
    let captions = strings(&mut app);
    assert_eq!(
        captions
            .iter()
            .filter(|text| text.as_str() == "Свободные очки: 0")
            .count(),
        1
    );
    assert!(
        captions
            .iter()
            .any(|text| text.contains("Повысить: 2 очка"))
    );
    assert!(
        captions
            .iter()
            .any(|text| text.contains("Понизить: +2 очка"))
    );
    assert!(
        captions
            .iter()
            .any(|text| text.contains("Не хватает очков"))
    );
    assert!(
        !captions
            .iter()
            .any(|text| text.starts_with("− ") || text.starts_with("+ "))
    );
    let unavailable = button(&mut app, 0, 1);
    app.world_mut().trigger(bevy::ui_widgets::Activate {
        entity: unavailable,
    });
    app.update();
    assert_eq!(
        app.world()
            .resource::<AppFlow>()
            .state()
            .new_game_draft
            .attributes,
        None
    );
    let minus = button(&mut app, 2, -1);
    app.world_mut()
        .insert_resource(InputFocus::from_entity(minus));
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity: minus });
    app.update();
    let draft = app.world().resource::<AppFlow>().state().new_game_draft;
    assert_eq!(draft.attributes, Some([3, 3, 14, 10]));
    assert!(
        strings(&mut app)
            .iter()
            .any(|text| text == "Свободные очки: 5")
    );
    assert!(
        strings(&mut app)
            .iter()
            .any(|text| text.contains("Исходное: 15"))
    );
    let focused = app.world().resource::<InputFocus>().get().unwrap();
    assert_eq!(
        app.world().get::<AttributeButton>(focused).unwrap().delta,
        -1
    );
    app.insert_resource(bevy::input_focus::InputFocusVisible(true));
    app.update();
    assert_eq!(
        app.world().resource::<SheetInteraction>().selected,
        Some(2),
        "keyboard focus selects strength without hovering"
    );
    // This headless app omits the renderer's visibility propagation.
    for mut visibility in app
        .world_mut()
        .query::<&mut InheritedVisibility>()
        .iter_mut(app.world_mut())
    {
        *visibility = InheritedVisibility::VISIBLE;
    }
    let strength_plus = button(&mut app, 2, 1);
    let mut navigation = bevy::ecs::system::SystemState::<
        bevy::input_focus::tab_navigation::TabNavigation,
    >::new(app.world_mut());
    let next = navigation
        .get(app.world())
        .expect("tab navigation resources exist")
        .navigate(
            app.world().resource::<InputFocus>(),
            bevy::input_focus::tab_navigation::NavAction::Next,
        )
        .expect("the creation screen provides a working tab group");
    assert_eq!(next, strength_plus);
    let plus = button(&mut app, 0, 1);
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity: plus });
    app.update();
    let draft = app.world().resource::<AppFlow>().state().new_game_draft;
    assert_eq!(draft.attributes, Some([4, 3, 14, 10]));
    assert_eq!(
        kn_game::newgame_allocation(draft.class.unwrap(), draft.attributes)
            .unwrap()
            .free_points,
        3
    );
    let before = app
        .world()
        .resource::<AppFlow>()
        .state()
        .new_game_draft
        .attributes;
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::HeroSelect(HeroSelectAction::SetCombatStyle(1)));
    app.update();
    assert_eq!(
        app.world()
            .resource::<AppFlow>()
            .state()
            .new_game_draft
            .attributes,
        before
    );
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::HeroSelect(HeroSelectAction::SetClass(1)));
    app.update();
    assert!(
        strings(&mut app)
            .iter()
            .any(|text| text == "Распределение сброшено к параметрам класса.")
    );
    assert_eq!(
        app.world()
            .resource::<AppFlow>()
            .state()
            .new_game_draft
            .attributes,
        None
    );
    let draft = app.world().resource::<AppFlow>().state().new_game_draft;
    assert_eq!(
        kn_game::newgame_allocation(draft.class.unwrap(), draft.attributes)
            .unwrap()
            .free_points,
        0
    );
}
