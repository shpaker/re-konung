//! The screen views: they render in the applied language, the settings
//! screen previews the draft live, and a failed save leaves its sentence on
//! the screen while the draft survives for a retry.
//!
//! Tests on real data skip when `original/` is absent (docs/k1/editions.md).

mod common;

use bevy::prelude::*;
use bevy::ui::Checked;
use bevy::ui::widget::Text;
use bevy::ui_widgets::Checkbox;
use common::{all_heroes, first_hero, game_dir, player_app, player_app_with};
use kn_app::{Action, HeroSelectAction, MainMenuAction, SettingsAction, SettingsField};
use kn_fe_player::flow::{AppFlow, AppliedSettings};
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

/// The hero selection: one named button per offered hero, the names coming
/// from the game data — and Back returns to the menu (#121).
#[test]
fn the_hero_select_offers_every_hero_by_name_and_back_returns() {
    let Some(dir) = game_dir() else {
        return;
    };
    let game = Game::open(&dir).unwrap();
    let heroes = all_heroes(&game);
    let names = game.hero_names(kn_game::MapSource::Game).unwrap();
    let mut app = player_app(game, heroes);
    app.update();

    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::MainMenu(MainMenuAction::NewGame));
    app.update();
    assert!(
        strings(&mut app).iter().any(|s| s.contains("Выбор героя")),
        "the hero-select screen opened"
    );
    let shown = strings(&mut app);
    for (_, name) in &names {
        assert!(
            !name.is_empty(),
            "the game data must name every hero for this test to be meaningful"
        );
        assert!(
            shown.iter().any(|s| s.contains(name.as_str())),
            "the button of hero {name:?} is on screen, got {shown:?}"
        );
    }
    assert!(
        shown.iter().any(|s| s.contains("Назад")),
        "the way back is on screen"
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
