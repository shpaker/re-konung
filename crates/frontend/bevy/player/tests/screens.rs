//! The screen views: they render in the applied language, the settings
//! screen previews the draft live, and a failed save leaves its sentence on
//! the screen while the draft survives for a retry.
//!
//! Tests on real data skip when `original/` is absent (docs/k1/editions.md).

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
use kn_fe_player::screens::{ConstructorButton, ConstructorKind, HintPopup, HintSource};
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
        shown.iter().any(|s| s.contains("Атрибуты")),
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
    for part in [
        "Атрибуты",
        "Навыки",
        "Прочее",
        "Сила",
        "Фехтование",
        "Начать",
    ] {
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

    // Preview moves the dossier: the second hero's name now reads on its
    // card and again as the dossier heading.
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::HeroSelect(HeroSelectAction::Preview(
            summaries[1].hero,
        )));
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

/// The hint popups: hovering a dossier line opens the original's hover hint
/// (`Texts::newgame`), leaving closes it. `Hovered` is inserted by hand —
/// the headless world has no pointer.
#[test]
fn a_hovered_line_opens_the_hint_popup_and_leaving_closes_it() {
    let Some(dir) = game_dir() else {
        return;
    };
    let game = Game::open(&dir).unwrap();
    let heroes = all_heroes(&game);
    let hints = game.newgame_texts();
    assert!(
        hints.iter().all(|text| !text.is_empty()),
        "the game's hint lines must resolve for this test to be meaningful"
    );
    let mut app = player_app(game, heroes);
    app.update();

    // Open the hero select, then hover the first dossier row (its label is
    // Ловкость — hint line 9, the original's agility hint).
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::MainMenu(MainMenuAction::NewGame));
    app.update();
    let mut popups = app.world_mut().query_filtered::<Entity, With<HintPopup>>();
    assert!(
        popups.iter(app.world()).next().is_none(),
        "no hover — no popup yet"
    );
    let target = {
        let mut sources = app.world_mut().query_filtered::<Entity, With<HintSource>>();
        sources
            .iter(app.world())
            .find(|entity| {
                app.world()
                    .get::<HintSource>(*entity)
                    .is_some_and(|hint| hint.0 == 9)
            })
            .expect("the agility row carries the hint source")
    };
    app.world_mut()
        .entity_mut(target)
        .insert(bevy::picking::hover::Hovered(true));
    app.update();
    let mut popups = app.world_mut().query_filtered::<Entity, With<HintPopup>>();
    let popup = popups
        .iter(app.world())
        .next()
        .expect("the hint popup opened");
    let text = app
        .world()
        .get::<bevy::ui::widget::Text>(popup)
        .expect("the popup carries the hint text")
        .0
        .clone();
    assert_eq!(text, hints[9], "the popup shows the hovered line's hint");

    // Leaving closes it.
    app.world_mut().despawn(target);
    app.update();
    let mut popups = app.world_mut().query_filtered::<Entity, With<HintPopup>>();
    assert!(
        popups.iter(app.world()).next().is_none(),
        "no hover — no popup"
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
        "15",
        "the default occupation is Воин"
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
