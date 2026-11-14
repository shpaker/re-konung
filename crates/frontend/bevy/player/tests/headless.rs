//! The flow without game data: everything that needs no `original/` runs on
//! CI — the watcher's failure path, the menu hiding as the screen moves to
//! loading, and the whole save lifecycle. Tests on real session data stay in
//! `creatures.rs` (they skip without `original/`); nothing key is covered
//! only there.

use bevy::asset::Assets;
use bevy::prelude::*;
use bevy::scene::ScenePlugin;
use bevy::ui::widget::Text;
use kn_app::{Action, HeroSelectAction, MainMenuAction, SettingsAction, SettingsField};
use kn_fe_player::flow::{AppFlow, AppliedSettings};
use kn_fe_player::session::SessionInit;
use kn_render::{CreatureMotion, CreatureSim, KonungLightMaterial};

/// The pipeline headless: the plugin, its screen views (which need assets,
/// the scene machinery, a theme and the bundled fonts — everything
/// `KonungUiPlugin` adds except Feathers itself, whose Shader asset needs the
/// render registry) and the creature resources the watcher reads, empty.
fn headless_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        bevy::asset::AssetPlugin::default(),
        ScenePlugin,
    ))
    .init_asset::<Font>()
    .insert_resource(kn_ui::konung_theme())
    .insert_resource(Assets::<Image>::default())
    .insert_resource(Assets::<KonungLightMaterial>::default())
    .init_resource::<CreatureSim>()
    .init_resource::<CreatureMotion>()
    .add_plugins(kn_fe_player::flow::PlayerFlowPlugin {
        // The offer is reducer state; no game data is needed to reduce it.
        heroes: vec![0, 1, 2],
        settings: kn_settings::defaults(),
        settings_path: None,
    });
    let fonts = kn_ui::konung_fonts(&mut app.world_mut().resource_mut::<Assets<Font>>());
    app.insert_resource(fonts);
    app
}

/// Every interface string currently on screen.
fn strings(app: &mut App) -> Vec<String> {
    let mut texts = app.world_mut().query::<&Text>();
    texts.iter(app.world()).map(|text| text.0.clone()).collect()
}

/// The dead-build state the world is in when the creature subsystem consumed
/// the request and failed: the screen is `Loading`, the marker stands, the
/// simulation never appeared, no request is pending. The half-built state is
/// assembled by hand because the real road needs game data — through the
/// controller's own legal transition, so the report lands like the window's
/// would — while the watcher runs through the real `app.update()`.
#[test]
fn the_watcher_reports_a_dead_build_and_the_menu_shows_the_reason() {
    let mut app = headless_app();
    app.update();

    // To Loading through the reducer's whole new-game flow; the StartSession
    // effect is dropped on the floor — this test owns the build's aftermath,
    // not the build.
    let mut loading = kn_app::AppController::new(vec![0, 1, 2], kn_settings::defaults());
    loading.submit(Action::MainMenu(MainMenuAction::NewGame));
    loading.submit(Action::HeroSelect(HeroSelectAction::Choose(0)));
    let _effects = loading.flush();
    app.world_mut().resource_mut::<AppFlow>().0 = loading;
    // One honest Loading frame first — the menu root leaves, exactly like
    // the window would show — then the synchronous half of the build has
    // "succeeded" and the subsystem dies with the request consumed.
    app.update();
    assert!(strings(&mut app).iter().any(|s| s.contains("Загрузка…")));
    app.world_mut().insert_resource(SessionInit);
    app.update();

    let flow = app.world().resource::<AppFlow>();
    assert_eq!(flow.state().screen, kn_app::Screen::MainMenu);
    assert_eq!(
        flow.state().session_error,
        Some(kn_app::SessionError::Simulation),
        "the watcher's failure report must reach the reducer in the same frame"
    );
    assert!(
        !app.world().contains_resource::<SessionInit>(),
        "the report tears the half-session down"
    );
    assert!(
        strings(&mut app)
            .iter()
            .any(|s| s.contains("Не удалось запустить симуляцию")),
        "the menu names the typed reason"
    );
}

/// The menu's buttons are gone the frame the screen moves on: the root tree
/// is despawned by the same `sync_screens` that spawns the loading view.
#[test]
fn the_menu_hides_when_the_screen_moves_to_loading() {
    let mut app = headless_app();
    app.update();
    assert!(strings(&mut app).iter().any(|s| s.contains("Новая игра")));

    // The controller is driven through the legal transitions alone; the
    // StartSession effect is dropped on the floor here — this test is about
    // the view swap, not the session build.
    let mut loading = kn_app::AppController::new(vec![0, 1, 2], kn_settings::defaults());
    loading.submit(Action::MainMenu(MainMenuAction::NewGame));
    loading.submit(Action::HeroSelect(HeroSelectAction::Choose(0)));
    let _effects = loading.flush();
    app.world_mut().resource_mut::<AppFlow>().0 = loading;

    app.update();
    let shown = strings(&mut app);
    assert!(
        !shown.iter().any(|s| s.contains("Новая игра")),
        "the menu buttons must not survive into loading, got {shown:?}"
    );
    assert!(shown.iter().any(|s| s.contains("Загрузка…")));
}

/// The save lifecycle end to end without game data and without a settings
/// path: the typed failure lands on the screen in the draft language, the
/// runtime stays on the applied settings, cancel returns to the menu.
#[test]
fn the_save_lifecycle_runs_without_game_data() {
    let mut app = headless_app();
    app.update();
    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::MainMenu(MainMenuAction::OpenSettings));
    app.update();
    assert!(strings(&mut app).iter().any(|s| s.contains("Настройки")));

    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::Settings(SettingsAction::Change(
            SettingsField::Language(kn_settings::Language::English),
        )));
    app.update();
    assert!(
        strings(&mut app).iter().any(|s| s.contains("Fullscreen")),
        "the draft language previews live"
    );

    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::Settings(SettingsAction::Save));
    app.update();
    assert!(
        strings(&mut app)
            .iter()
            .any(|s| s.contains("The settings could not be written")),
        "the typed save failure is on screen"
    );
    assert_eq!(
        app.world().resource::<AppliedSettings>().0.language,
        kn_settings::Language::Russian,
        "the applied settings rolled back"
    );

    app.world_mut()
        .resource_mut::<AppFlow>()
        .submit(Action::Settings(SettingsAction::Cancel));
    app.update();
    assert!(
        strings(&mut app).iter().any(|s| s.contains("Новая игра")),
        "back at the menu in the applied language"
    );
}
