//! The controller: a pure reducer over [`AppState`] driven by queued
//! [`Action`]s and returning [`Effect`]s for the frontend to execute.

use std::collections::VecDeque;

use kn_settings::Settings;

use crate::action::{
    Action, HeroSelectAction, LoadingAction, MainMenuAction, PersistOutcome, SettingsAction,
    SettingsField,
};
use crate::effect::Effect;
use crate::state::{AppState, NewGameDraft, SaveStatus, Screen, SettingsScreen};

/// The table sizes the game layer resolves for the character constructor
/// (`newgame_classes()`, the combat styles, the secondary sets).
const NEWGAME_CLASSES: usize = 4;
const NEWGAME_COMBAT_STYLES: usize = 2;
const NEWGAME_SECONDARY_SETS: usize = 3;

/// The MVU controller of the player application.
///
/// Actions are enqueued by [`submit`](AppController::submit) and applied in
/// order by the next [`flush`](AppController::flush), which returns every
/// effect emitted along the way. Nothing else mutates [`AppState`].
pub struct AppController {
    state: AppState,
    queue: VecDeque<Action>,
}

impl AppController {
    /// Creates the controller at the main menu with the given heroes the
    /// new-game flow offers and the given applied-and-persisted settings (the
    /// ones the startup loaded from disk).
    pub fn new(heroes: Vec<u32>, applied: Settings) -> Self {
        Self {
            state: AppState {
                screen: Screen::MainMenu,
                heroes: heroes.clone(),
                // The screen opens on the first offered hero; an empty offer
                // stays `None`.
                hero_preview: heroes.first().copied(),
                new_game_draft: NewGameDraft::default(),
                applied,
                settings_screen: None,
                saving: None,
                session_error: None,
            },
            queue: VecDeque::new(),
        }
    }

    /// The current state; the frontend renders from this and nothing else.
    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// Enqueues an action; it is applied by the next
    /// [`flush`](AppController::flush).
    pub fn submit(&mut self, action: Action) {
        self.queue.push_back(action);
    }

    /// Applies every queued action in order and returns the emitted effects.
    ///
    /// Pure: no clock, no io, no randomness. The queue is drained fully, so a
    /// second `flush` without a `submit` in between returns an empty `Vec`.
    pub fn flush(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        while let Some(action) = self.queue.pop_front() {
            self.apply(action, &mut effects);
        }
        effects
    }

    /// The transition table: a plain match on `(screen, action)`. Every pair
    /// not listed is a no-op, including result actions (`Loading`, `Persisted`)
    /// — they only consume reports, never emit effects.
    fn apply(&mut self, action: Action, effects: &mut Vec<Effect>) {
        match (self.state.screen, action) {
            (Screen::MainMenu, Action::MainMenu(MainMenuAction::NewGame)) => {
                self.state.session_error = None;
                self.state.screen = Screen::HeroSelect;
                self.state.new_game_draft = NewGameDraft::default();
            }
            (Screen::HeroSelect, Action::HeroSelect(HeroSelectAction::Preview(hero))) => {
                if self.state.heroes.contains(&hero) {
                    self.state.hero_preview = Some(hero);
                }
            }
            (Screen::HeroSelect, Action::HeroSelect(HeroSelectAction::SetClass(index))) => {
                if usize::from(index) < NEWGAME_CLASSES {
                    self.state.new_game_draft.class = usize::from(index);
                    self.state.new_game_draft.attributes = None;
                }
            }
            (Screen::HeroSelect, Action::HeroSelect(HeroSelectAction::SetCombatStyle(index))) => {
                if usize::from(index) < NEWGAME_COMBAT_STYLES {
                    self.state.new_game_draft.combat_style = usize::from(index);
                }
            }
            (Screen::HeroSelect, Action::HeroSelect(HeroSelectAction::SetSecondary(index))) => {
                if usize::from(index) < NEWGAME_SECONDARY_SETS {
                    self.state.new_game_draft.secondary = usize::from(index);
                }
            }
            (
                Screen::HeroSelect,
                Action::HeroSelect(HeroSelectAction::SetAttributes { class, values }),
            ) => {
                if class == self.state.new_game_draft.class {
                    self.state.new_game_draft.attributes = Some(values);
                }
            }
            (Screen::HeroSelect, Action::HeroSelect(HeroSelectAction::Choose(hero))) => {
                // The offered list is the only source of valid numbers: a
                // stale view cannot start a session for a hero this game
                // directory does not carry.
                if self.state.heroes.contains(&hero) {
                    self.state.session_error = None;
                    self.state.screen = Screen::Loading;
                    effects.push(Effect::StartSession {
                        hero,
                        draft: self.state.new_game_draft,
                    });
                }
            }
            (Screen::HeroSelect, Action::HeroSelect(HeroSelectAction::Cancel)) => {
                self.state.screen = Screen::MainMenu;
            }
            (Screen::MainMenu, Action::MainMenu(MainMenuAction::OpenSettings)) => {
                self.state.screen = Screen::Settings;
                self.state.settings_screen = Some(SettingsScreen {
                    draft: self.state.applied,
                    status: SaveStatus::Idle,
                });
            }
            (Screen::MainMenu, Action::MainMenu(MainMenuAction::Exit)) => {
                effects.push(Effect::ExitApp);
            }
            (Screen::Settings, Action::Settings(SettingsAction::Change(field))) => {
                if let Some(settings_screen) = self.state.settings_screen.as_mut() {
                    match field {
                        SettingsField::Language(language) => {
                            settings_screen.draft.language = language;
                        }
                        SettingsField::Fullscreen(fullscreen) => {
                            settings_screen.draft.video.fullscreen = fullscreen;
                        }
                        SettingsField::Vsync(vsync) => {
                            settings_screen.draft.video.vsync = vsync;
                        }
                    }
                }
            }
            (Screen::Settings, Action::Settings(SettingsAction::Save)) => {
                if let Some(settings_screen) = self.state.settings_screen.as_ref() {
                    if settings_screen.draft != self.state.applied {
                        if self.state.saving.is_none() {
                            // Freeze the snapshot the effect carries: by the
                            // time the report arrives the draft may have
                            // moved on, and `applied` must become what the
                            // file actually holds — never the newer draft.
                            let snapshot = settings_screen.draft;
                            self.state.saving = Some(snapshot);
                            // The screen stays open until the Persisted report.
                            effects.push(Effect::ApplyAndPersist(snapshot));
                        }
                        // A save is already in flight: a second Save is a
                        // no-op until its report lands.
                    } else {
                        self.close_settings();
                    }
                }
            }
            (Screen::Settings, Action::Settings(SettingsAction::Cancel)) => {
                // The draft is discarded, but a save already in flight keeps
                // going: its report still advances `applied` to what the file
                // will hold, wherever the screen is by then.
                self.close_settings();
            }
            (Screen::Loading, Action::Loading(LoadingAction::Succeeded)) => {
                self.state.screen = Screen::Playing;
            }
            (Screen::Loading, Action::Loading(LoadingAction::Failed(error))) => {
                self.state.screen = Screen::MainMenu;
                self.state.session_error = Some(error);
            }
            (Screen::Settings, Action::Persisted(PersistOutcome::Saved)) => {
                self.apply_persisted_saved();
            }
            (Screen::MainMenu, Action::Persisted(PersistOutcome::Saved)) => {
                // The screen was cancelled (or closed) while the save was in
                // flight; the file still holds the snapshot, so `applied`
                // follows it — without this, the file and the state would
                // disagree until the next successful save.
                self.apply_persisted_saved();
            }
            (Screen::Settings, Action::Persisted(PersistOutcome::SaveFailed(error))) => {
                self.state.saving = None;
                if let Some(settings_screen) = self.state.settings_screen.as_mut() {
                    settings_screen.status = SaveStatus::Failed(error);
                }
            }
            (Screen::MainMenu, Action::Persisted(PersistOutcome::SaveFailed(_))) => {
                // A cancelled screen's failed save: nowhere to show it — the
                // frontend has already logged the io detail and rolled the
                // runtime back. Clear the flight so a future Save can go.
                self.state.saving = None;
            }
            _ => {}
        }
    }

    /// Discards the draft and returns to the main menu (Cancel, and Save with
    /// an unchanged draft: there is nothing to persist).
    fn close_settings(&mut self) {
        self.state.screen = Screen::MainMenu;
        self.state.settings_screen = None;
    }

    /// A persisted save: `applied` advances to the frozen snapshot — the one
    /// the file actually received — and an open settings screen closes. A
    /// report with no save in flight is a stray and changes nothing.
    fn apply_persisted_saved(&mut self) {
        if let Some(snapshot) = self.state.saving.take() {
            self.state.applied = snapshot;
            if self.state.settings_screen.take().is_some() {
                self.state.screen = Screen::MainMenu;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use kn_settings::{Language, Settings, VideoSettings};

    use super::AppController;
    use crate::action::{
        Action, HeroSelectAction, LoadingAction, MainMenuAction, PersistOutcome, SettingsAction,
        SettingsField,
    };
    use crate::effect::Effect;
    use crate::state::{NewGameDraft, SaveError, SaveStatus, Screen, SessionError};

    /// The heroes the test game directory offers — the same numbers the real
    /// data resolves (`Game.0`–`Game.2`).
    const HEROES: [u32; 3] = [0, 1, 2];

    fn controller() -> AppController {
        AppController::new(HEROES.into(), Settings::default())
    }

    /// A controller driven to `screen` through legal transitions only.
    fn controller_at(screen: Screen) -> AppController {
        let mut controller = controller();
        match screen {
            Screen::MainMenu => {}
            Screen::HeroSelect => {
                controller.submit(Action::MainMenu(MainMenuAction::NewGame));
            }
            Screen::Settings => {
                controller.submit(Action::MainMenu(MainMenuAction::OpenSettings));
            }
            Screen::Loading => {
                controller.submit(Action::MainMenu(MainMenuAction::NewGame));
                controller.submit(Action::HeroSelect(HeroSelectAction::Choose(HEROES[0])));
            }
            Screen::Playing => {
                controller.submit(Action::MainMenu(MainMenuAction::NewGame));
                controller.submit(Action::HeroSelect(HeroSelectAction::Choose(HEROES[0])));
                controller.submit(Action::Loading(LoadingAction::Succeeded));
            }
        }
        controller.flush();
        controller
    }

    fn submit_all(
        controller: &mut AppController,
        actions: impl IntoIterator<Item = Action>,
    ) -> Vec<Effect> {
        for action in actions {
            controller.submit(action);
        }
        controller.flush()
    }

    /// The defaults with only the language changed.
    fn english_only() -> Settings {
        Settings {
            language: Language::English,
            ..Settings::default()
        }
    }

    /// The defaults with the language and fullscreen changed.
    fn english_fullscreen() -> Settings {
        Settings {
            language: Language::English,
            video: VideoSettings {
                fullscreen: true,
                vsync: true,
            },
        }
    }

    #[test]
    fn new_starts_at_main_menu_clean() {
        let applied = english_fullscreen();
        let controller = AppController::new(HEROES.into(), applied);
        assert_eq!(controller.state().screen, Screen::MainMenu);
        assert_eq!(controller.state().heroes, HEROES);
        assert_eq!(controller.state().hero_preview, Some(HEROES[0]));
        assert_eq!(controller.state().applied, applied);
        assert_eq!(controller.state().settings_screen, None);
        assert_eq!(controller.state().session_error, None);
    }

    #[test]
    fn flush_without_submissions_is_empty() {
        assert!(controller().flush().is_empty());
    }

    // Transition table, one test per row.

    #[test]
    fn main_menu_new_game_enters_hero_select_without_effects() {
        let mut controller = controller();
        let effects = submit_all(&mut controller, [Action::MainMenu(MainMenuAction::NewGame)]);
        assert!(effects.is_empty());
        assert_eq!(controller.state().screen, Screen::HeroSelect);
        assert_eq!(controller.state().session_error, None);
    }

    #[test]
    fn hero_select_choose_enters_loading_and_emits_the_chosen_hero() {
        for &hero in &HEROES {
            let mut controller = controller_at(Screen::HeroSelect);
            let effects = submit_all(
                &mut controller,
                [Action::HeroSelect(HeroSelectAction::Choose(hero))],
            );
            assert_eq!(
                effects,
                vec![Effect::StartSession {
                    hero,
                    draft: NewGameDraft::default()
                }]
            );
            assert_eq!(controller.state().screen, Screen::Loading);
            assert_eq!(controller.state().session_error, None);
        }
    }

    #[test]
    fn hero_select_preview_moves_the_detail_panel_target() {
        let mut controller = controller_at(Screen::HeroSelect);
        let effects = submit_all(
            &mut controller,
            [Action::HeroSelect(HeroSelectAction::Preview(HEROES[2]))],
        );
        assert!(effects.is_empty());
        assert_eq!(controller.state().screen, Screen::HeroSelect);
        assert_eq!(controller.state().hero_preview, Some(HEROES[2]));
    }

    #[test]
    fn hero_select_preview_of_an_unoffered_hero_is_a_no_op() {
        let mut controller = controller_at(Screen::HeroSelect);
        let before = controller.state().clone();
        let effects = submit_all(
            &mut controller,
            [Action::HeroSelect(HeroSelectAction::Preview(
                HEROES[HEROES.len() - 1] + 1,
            ))],
        );
        assert!(effects.is_empty());
        assert_eq!(controller.state(), &before);
    }

    #[test]
    fn hero_select_choose_of_an_unoffered_hero_is_a_no_op() {
        let mut controller = controller_at(Screen::HeroSelect);
        let before = controller.state().clone();
        let effects = submit_all(
            &mut controller,
            [Action::HeroSelect(HeroSelectAction::Choose(
                HEROES[HEROES.len() - 1] + 1,
            ))],
        );
        assert!(effects.is_empty());
        assert_eq!(controller.state(), &before);
    }

    #[test]
    fn hero_select_set_actions_shape_the_constructor_draft() {
        let mut controller = controller_at(Screen::HeroSelect);
        let effects = submit_all(
            &mut controller,
            [
                Action::HeroSelect(HeroSelectAction::SetClass(2)),
                Action::HeroSelect(HeroSelectAction::SetCombatStyle(1)),
                Action::HeroSelect(HeroSelectAction::SetSecondary(2)),
            ],
        );
        assert!(effects.is_empty());
        let draft = &controller.state().new_game_draft;
        assert_eq!(
            (draft.class, draft.combat_style, draft.secondary),
            (2, 1, 2)
        );
        // The choice rides into the session: Start carries the drafted row.
        let effects = submit_all(
            &mut controller,
            [Action::HeroSelect(HeroSelectAction::Choose(HEROES[0]))],
        );
        assert_eq!(
            effects,
            vec![Effect::StartSession {
                hero: HEROES[0],
                draft: NewGameDraft {
                    class: 2,
                    combat_style: 1,
                    secondary: 2,
                    attributes: None,
                }
            }]
        );
    }

    #[test]
    fn hero_select_set_actions_out_of_range_are_no_ops() {
        let mut controller = controller_at(Screen::HeroSelect);
        let before = controller.state().clone();
        let effects = submit_all(
            &mut controller,
            [
                Action::HeroSelect(HeroSelectAction::SetClass(4)),
                Action::HeroSelect(HeroSelectAction::SetCombatStyle(2)),
                Action::HeroSelect(HeroSelectAction::SetSecondary(3)),
            ],
        );
        assert!(effects.is_empty());
        assert_eq!(controller.state(), &before);
    }

    #[test]
    fn hero_select_cancel_returns_to_main_menu() {
        let mut controller = controller_at(Screen::HeroSelect);
        let effects = submit_all(
            &mut controller,
            [Action::HeroSelect(HeroSelectAction::Cancel)],
        );
        assert!(effects.is_empty());
        assert_eq!(controller.state().screen, Screen::MainMenu);
    }

    #[test]
    fn the_new_game_seed_reaches_loading_in_one_flush_without_a_hero_select_frame() {
        // What `--new-game` seeds: the whole new-game flow queued before the
        // first frame. One flush reduces both actions, so exactly one session
        // effect exists and the screen is already `Loading` — the hero select
        // root can never spawn.
        let mut controller = controller();
        controller.submit(Action::MainMenu(MainMenuAction::NewGame));
        controller.submit(Action::HeroSelect(HeroSelectAction::Choose(HEROES[1])));
        let effects = controller.flush();
        assert_eq!(
            effects,
            vec![Effect::StartSession {
                hero: HEROES[1],
                draft: NewGameDraft::default()
            }]
        );
        assert_eq!(controller.state().screen, Screen::Loading);
        assert!(controller.flush().is_empty());
    }

    #[test]
    fn main_menu_new_game_clears_previous_session_error() {
        let mut controller = controller();
        submit_all(
            &mut controller,
            [
                Action::MainMenu(MainMenuAction::NewGame),
                Action::HeroSelect(HeroSelectAction::Choose(HEROES[0])),
                Action::Loading(LoadingAction::Failed(SessionError::Map)),
            ],
        );
        assert_eq!(controller.state().session_error, Some(SessionError::Map));
        let effects = submit_all(&mut controller, [Action::MainMenu(MainMenuAction::NewGame)]);
        assert!(effects.is_empty());
        assert_eq!(controller.state().screen, Screen::HeroSelect);
        assert_eq!(controller.state().session_error, None);
    }

    #[test]
    fn main_menu_open_settings_copies_applied_into_draft() {
        let mut controller = AppController::new(HEROES.into(), english_fullscreen());
        let effects = submit_all(
            &mut controller,
            [Action::MainMenu(MainMenuAction::OpenSettings)],
        );
        assert!(effects.is_empty());
        assert_eq!(controller.state().screen, Screen::Settings);
        let settings_screen = controller.state().settings_screen.as_ref().unwrap();
        assert_eq!(settings_screen.draft, english_fullscreen());
        assert_eq!(settings_screen.status, SaveStatus::Idle);
    }

    #[test]
    fn main_menu_exit_emits_exit_app_and_changes_nothing() {
        let mut controller = controller();
        let before = controller.state().clone();
        let effects = submit_all(&mut controller, [Action::MainMenu(MainMenuAction::Exit)]);
        assert_eq!(effects, vec![Effect::ExitApp]);
        assert_eq!(controller.state(), &before);
    }

    #[test]
    fn settings_change_updates_only_the_named_draft_field() {
        let mut controller = controller();
        submit_all(
            &mut controller,
            [Action::MainMenu(MainMenuAction::OpenSettings)],
        );
        let effects = submit_all(
            &mut controller,
            [
                Action::Settings(SettingsAction::Change(SettingsField::Language(
                    Language::English,
                ))),
                Action::Settings(SettingsAction::Change(SettingsField::Fullscreen(true))),
            ],
        );
        assert!(effects.is_empty());
        assert_eq!(controller.state().screen, Screen::Settings);
        let draft = controller.state().settings_screen.as_ref().unwrap().draft;
        assert_eq!(draft.language, Language::English);
        assert!(draft.video.fullscreen);
        // Only the two named fields moved; the rest keeps the applied value.
        assert_eq!(draft.video.vsync, Settings::default().video.vsync);
        assert_eq!(controller.state().applied, Settings::default());
    }

    #[test]
    fn settings_change_vsync_updates_draft() {
        let mut controller = controller();
        submit_all(
            &mut controller,
            [Action::MainMenu(MainMenuAction::OpenSettings)],
        );
        let effects = submit_all(
            &mut controller,
            [Action::Settings(SettingsAction::Change(
                SettingsField::Vsync(false),
            ))],
        );
        assert!(effects.is_empty());
        let draft = controller.state().settings_screen.as_ref().unwrap().draft;
        assert!(!draft.video.vsync);
        assert!(controller.state().applied.video.vsync);
    }

    #[test]
    fn settings_save_with_changed_draft_waits_for_persisted() {
        let mut controller = controller();
        submit_all(
            &mut controller,
            [
                Action::MainMenu(MainMenuAction::OpenSettings),
                Action::Settings(SettingsAction::Change(SettingsField::Language(
                    Language::English,
                ))),
            ],
        );
        let effects = submit_all(&mut controller, [Action::Settings(SettingsAction::Save)]);
        assert_eq!(effects, vec![Effect::ApplyAndPersist(english_only())]);
        assert_eq!(controller.state().screen, Screen::Settings);
        assert_eq!(
            controller.state().settings_screen.as_ref().unwrap().status,
            SaveStatus::Idle
        );
        assert_eq!(controller.state().applied, Settings::default());
    }

    #[test]
    fn settings_save_with_unchanged_draft_closes_without_persisting() {
        let mut controller = controller();
        submit_all(
            &mut controller,
            [Action::MainMenu(MainMenuAction::OpenSettings)],
        );
        let effects = submit_all(&mut controller, [Action::Settings(SettingsAction::Save)]);
        assert!(effects.is_empty());
        assert_eq!(controller.state().screen, Screen::MainMenu);
        assert_eq!(controller.state().settings_screen, None);
    }

    #[test]
    fn settings_cancel_discards_draft() {
        let mut controller = controller();
        submit_all(
            &mut controller,
            [
                Action::MainMenu(MainMenuAction::OpenSettings),
                Action::Settings(SettingsAction::Change(SettingsField::Fullscreen(true))),
            ],
        );
        let effects = submit_all(&mut controller, [Action::Settings(SettingsAction::Cancel)]);
        assert!(effects.is_empty());
        assert_eq!(controller.state().screen, Screen::MainMenu);
        assert_eq!(controller.state().settings_screen, None);
        assert_eq!(controller.state().applied, Settings::default());
    }

    #[test]
    fn loading_succeeded_enters_playing() {
        let mut controller = controller_at(Screen::Loading);
        let effects = submit_all(&mut controller, [Action::Loading(LoadingAction::Succeeded)]);
        assert!(effects.is_empty());
        assert_eq!(controller.state().screen, Screen::Playing);
    }

    #[test]
    fn loading_failed_returns_to_menu_with_error() {
        let mut controller = controller_at(Screen::Loading);
        let effects = submit_all(
            &mut controller,
            [Action::Loading(LoadingAction::Failed(
                SessionError::Textures,
            ))],
        );
        assert!(effects.is_empty());
        assert_eq!(controller.state().screen, Screen::MainMenu);
        assert_eq!(
            controller.state().session_error,
            Some(SessionError::Textures)
        );
    }

    #[test]
    fn persisted_saved_applies_draft_and_closes() {
        let mut controller = controller();
        submit_all(
            &mut controller,
            [
                Action::MainMenu(MainMenuAction::OpenSettings),
                Action::Settings(SettingsAction::Change(SettingsField::Language(
                    Language::English,
                ))),
                Action::Settings(SettingsAction::Save),
            ],
        );
        let effects = submit_all(&mut controller, [Action::Persisted(PersistOutcome::Saved)]);
        assert!(effects.is_empty());
        assert_eq!(controller.state().screen, Screen::MainMenu);
        assert_eq!(controller.state().applied, english_only());
        assert_eq!(controller.state().settings_screen, None);
    }

    #[test]
    fn persisted_save_failed_keeps_draft_for_retry() {
        let mut controller = controller();
        submit_all(
            &mut controller,
            [
                Action::MainMenu(MainMenuAction::OpenSettings),
                Action::Settings(SettingsAction::Change(SettingsField::Language(
                    Language::English,
                ))),
                Action::Settings(SettingsAction::Save),
            ],
        );
        let effects = submit_all(
            &mut controller,
            [Action::Persisted(PersistOutcome::SaveFailed(
                SaveError::Replace,
            ))],
        );
        assert!(effects.is_empty());
        assert_eq!(controller.state().screen, Screen::Settings);
        let settings_screen = controller.state().settings_screen.as_ref().unwrap();
        assert_eq!(settings_screen.draft, english_only());
        assert_eq!(
            settings_screen.status,
            SaveStatus::Failed(SaveError::Replace)
        );
    }

    // Structural invariants.

    #[test]
    fn result_actions_never_emit_effects_on_any_screen() {
        let result_actions = [
            Action::Loading(LoadingAction::Succeeded),
            Action::Loading(LoadingAction::Failed(SessionError::Map)),
            Action::Persisted(PersistOutcome::Saved),
            Action::Persisted(PersistOutcome::SaveFailed(SaveError::Write)),
        ];
        for screen in [
            Screen::MainMenu,
            Screen::HeroSelect,
            Screen::Settings,
            Screen::Loading,
            Screen::Playing,
        ] {
            for &action in &result_actions {
                let mut controller = controller_at(screen);
                assert!(
                    submit_all(&mut controller, [action]).is_empty(),
                    "{action:?} on {screen:?} emitted effects"
                );
            }
        }
    }

    #[test]
    fn every_screen_action_pair_matches_the_table() {
        let all_actions = [
            Action::MainMenu(MainMenuAction::NewGame),
            Action::MainMenu(MainMenuAction::OpenSettings),
            Action::MainMenu(MainMenuAction::Exit),
            Action::HeroSelect(HeroSelectAction::Choose(HEROES[0])),
            Action::HeroSelect(HeroSelectAction::Choose(HEROES[2] + 1)),
            Action::HeroSelect(HeroSelectAction::Preview(HEROES[1])),
            Action::HeroSelect(HeroSelectAction::Preview(HEROES[2] + 1)),
            Action::HeroSelect(HeroSelectAction::Cancel),
            Action::Settings(SettingsAction::Change(SettingsField::Language(
                Language::English,
            ))),
            Action::Settings(SettingsAction::Save),
            Action::Settings(SettingsAction::Cancel),
            Action::Loading(LoadingAction::Succeeded),
            Action::Loading(LoadingAction::Failed(SessionError::Scene)),
            Action::Persisted(PersistOutcome::Saved),
            Action::Persisted(PersistOutcome::SaveFailed(SaveError::Write)),
        ];
        // The only rows that emit: Exit from MainMenu, a Choose of an offered
        // hero from HeroSelect, Save from Settings with a draft differing
        // from applied.
        for screen in [
            Screen::MainMenu,
            Screen::HeroSelect,
            Screen::Settings,
            Screen::Loading,
            Screen::Playing,
        ] {
            for draft_changed in [false, true] {
                for &action in &all_actions {
                    let mut controller = controller_at(screen);
                    if draft_changed && screen == Screen::Settings {
                        submit_all(
                            &mut controller,
                            [Action::Settings(SettingsAction::Change(
                                SettingsField::Language(Language::English),
                            ))],
                        );
                    }
                    let emits = match (screen, action) {
                        (Screen::MainMenu, Action::MainMenu(MainMenuAction::Exit)) => true,
                        (
                            Screen::HeroSelect,
                            Action::HeroSelect(HeroSelectAction::Choose(hero)),
                        ) => controller.state().heroes.contains(&hero),
                        (Screen::Settings, Action::Settings(SettingsAction::Save)) => draft_changed,
                        _ => false,
                    };
                    let effects = submit_all(&mut controller, [action]);
                    assert_eq!(
                        effects.len(),
                        usize::from(emits),
                        "{action:?} on {screen:?} (draft_changed={draft_changed})"
                    );
                }
            }
        }
    }

    #[test]
    fn second_new_game_while_loading_is_a_no_op() {
        let mut controller = controller_at(Screen::Loading);
        let effects = submit_all(&mut controller, [Action::MainMenu(MainMenuAction::NewGame)]);
        assert!(effects.is_empty());
        assert_eq!(controller.state().screen, Screen::Loading);
    }

    #[test]
    fn a_choose_while_loading_is_a_no_op() {
        // The double-click protection of the new-game flow: the screen left
        // HeroSelect, so a late or repeated Choose cannot start a second
        // session.
        let mut controller = controller_at(Screen::Loading);
        let effects = submit_all(
            &mut controller,
            [Action::HeroSelect(HeroSelectAction::Choose(HEROES[0]))],
        );
        assert!(effects.is_empty());
        assert_eq!(controller.state().screen, Screen::Loading);
    }

    #[test]
    fn flush_applies_actions_in_order_and_drains_fully() {
        let mut controller = controller();
        controller.submit(Action::MainMenu(MainMenuAction::OpenSettings));
        controller.submit(Action::Settings(SettingsAction::Change(
            SettingsField::Vsync(false),
        )));
        controller.submit(Action::Settings(SettingsAction::Cancel));
        // Valid only because Cancel has already returned to MainMenu by the
        // time this action is applied; out of order it would be a no-op.
        controller.submit(Action::MainMenu(MainMenuAction::Exit));
        assert_eq!(controller.flush(), vec![Effect::ExitApp]);
        assert_eq!(controller.state().screen, Screen::MainMenu);
        assert_eq!(controller.state().applied, Settings::default());
        assert!(controller.flush().is_empty());
    }

    // End-to-end reducer sequences.

    #[test]
    fn settings_round_trip_persists_new_settings() {
        let mut controller = controller();
        submit_all(
            &mut controller,
            [Action::MainMenu(MainMenuAction::OpenSettings)],
        );
        // One flush batch: the save keeps the screen open on the draft while
        // the frontend persists.
        let effects = submit_all(
            &mut controller,
            [
                Action::Settings(SettingsAction::Change(SettingsField::Language(
                    Language::English,
                ))),
                Action::Settings(SettingsAction::Change(SettingsField::Fullscreen(true))),
                Action::Settings(SettingsAction::Save),
            ],
        );
        assert_eq!(effects, vec![Effect::ApplyAndPersist(english_fullscreen())]);
        assert_eq!(controller.state().screen, Screen::Settings);
        submit_all(&mut controller, [Action::Persisted(PersistOutcome::Saved)]);
        assert_eq!(controller.state().screen, Screen::MainMenu);
        assert_eq!(controller.state().applied, english_fullscreen());
        assert_eq!(controller.state().settings_screen, None);
    }

    #[test]
    fn failed_save_keeps_draft_until_a_retry_succeeds() {
        let mut controller = controller();
        submit_all(
            &mut controller,
            [
                Action::MainMenu(MainMenuAction::OpenSettings),
                Action::Settings(SettingsAction::Change(SettingsField::Language(
                    Language::English,
                ))),
                Action::Settings(SettingsAction::Save),
            ],
        );
        submit_all(
            &mut controller,
            [Action::Persisted(PersistOutcome::SaveFailed(
                SaveError::Replace,
            ))],
        );
        // Draft intact, failure shown, still on the settings screen.
        assert_eq!(controller.state().screen, Screen::Settings);
        let settings_screen = controller.state().settings_screen.as_ref().unwrap();
        assert_eq!(settings_screen.draft, english_only());
        assert_eq!(
            settings_screen.status,
            SaveStatus::Failed(SaveError::Replace)
        );
        // Retry: Save again emits ApplyAndPersist again.
        let effects = submit_all(&mut controller, [Action::Settings(SettingsAction::Save)]);
        assert_eq!(effects, vec![Effect::ApplyAndPersist(english_only())]);
        submit_all(&mut controller, [Action::Persisted(PersistOutcome::Saved)]);
        assert_eq!(controller.state().screen, Screen::MainMenu);
        assert_eq!(controller.state().applied, english_only());
        assert_eq!(controller.state().settings_screen, None);
    }

    #[test]
    fn failed_session_returns_to_menu_and_retry_clears_the_error() {
        let mut controller = controller();
        let effects = submit_all(
            &mut controller,
            [
                Action::MainMenu(MainMenuAction::NewGame),
                Action::HeroSelect(HeroSelectAction::Choose(HEROES[0])),
            ],
        );
        assert_eq!(
            effects,
            vec![Effect::StartSession {
                hero: HEROES[0],
                draft: NewGameDraft::default()
            }]
        );
        let effects = submit_all(
            &mut controller,
            [Action::Loading(LoadingAction::Failed(SessionError::Map))],
        );
        assert!(effects.is_empty());
        assert_eq!(controller.state().screen, Screen::MainMenu);
        assert_eq!(controller.state().session_error, Some(SessionError::Map));
        let effects = submit_all(
            &mut controller,
            [
                Action::MainMenu(MainMenuAction::NewGame),
                Action::HeroSelect(HeroSelectAction::Choose(HEROES[2])),
            ],
        );
        assert_eq!(
            effects,
            vec![Effect::StartSession {
                hero: HEROES[2],
                draft: NewGameDraft::default()
            }]
        );
        assert_eq!(controller.state().screen, Screen::Loading);
        assert_eq!(controller.state().session_error, None);
    }

    // Save-sequencing: `applied` must become exactly what the file received —
    // the frozen snapshot, never the draft of a later moment.

    /// Save → Change → Saved: the edit made while the save was in flight
    /// stays unsaved; `applied` is the snapshot the effect carried.
    #[test]
    fn save_change_saved_applies_the_persisted_snapshot_not_the_later_draft() {
        let mut controller = controller();
        submit_all(
            &mut controller,
            [
                Action::MainMenu(MainMenuAction::OpenSettings),
                Action::Settings(SettingsAction::Change(SettingsField::Language(
                    Language::English,
                ))),
                Action::Settings(SettingsAction::Save),
            ],
        );
        // The draft moves on while the save flies.
        submit_all(
            &mut controller,
            [Action::Settings(SettingsAction::Change(
                SettingsField::Fullscreen(true),
            ))],
        );
        submit_all(&mut controller, [Action::Persisted(PersistOutcome::Saved)]);
        assert_eq!(controller.state().applied, english_only());
        assert_eq!(controller.state().saving, None);
        assert_eq!(controller.state().screen, Screen::MainMenu);
        assert_eq!(controller.state().settings_screen, None);
    }

    /// Save → Cancel → Saved: the write is already in flight, so the report
    /// still advances `applied` — the file and the state must not disagree.
    #[test]
    fn save_cancel_saved_still_applies_the_in_flight_snapshot() {
        let mut controller = controller();
        submit_all(
            &mut controller,
            [
                Action::MainMenu(MainMenuAction::OpenSettings),
                Action::Settings(SettingsAction::Change(SettingsField::Language(
                    Language::English,
                ))),
                Action::Settings(SettingsAction::Save),
                Action::Settings(SettingsAction::Cancel),
            ],
        );
        assert_eq!(controller.state().screen, Screen::MainMenu);
        assert_eq!(controller.state().applied, Settings::default());
        let effects = submit_all(&mut controller, [Action::Persisted(PersistOutcome::Saved)]);
        assert!(effects.is_empty());
        assert_eq!(controller.state().applied, english_only());
        assert_eq!(controller.state().saving, None);
        assert_eq!(controller.state().screen, Screen::MainMenu);
    }

    /// Save → Save: one save in flight; the second press emits nothing.
    #[test]
    fn double_save_while_in_flight_emits_one_persist() {
        let mut controller = controller();
        submit_all(
            &mut controller,
            [
                Action::MainMenu(MainMenuAction::OpenSettings),
                Action::Settings(SettingsAction::Change(SettingsField::Language(
                    Language::English,
                ))),
            ],
        );
        let effects = submit_all(
            &mut controller,
            [
                Action::Settings(SettingsAction::Save),
                Action::Settings(SettingsAction::Save),
            ],
        );
        assert_eq!(
            effects,
            vec![Effect::ApplyAndPersist(english_only())],
            "the second Save must be a no-op while the first is in flight"
        );
        // And the report resolves the flight: the next Save can go again.
        submit_all(&mut controller, [Action::Persisted(PersistOutcome::Saved)]);
        assert_eq!(controller.state().saving, None);
    }

    /// Save → Failed → Save: the retry freezes a fresh snapshot of the
    /// current draft and its report applies that one.
    #[test]
    fn save_failed_retry_resaves_with_a_fresh_snapshot() {
        let mut controller = controller();
        submit_all(
            &mut controller,
            [
                Action::MainMenu(MainMenuAction::OpenSettings),
                Action::Settings(SettingsAction::Change(SettingsField::Language(
                    Language::English,
                ))),
                Action::Settings(SettingsAction::Save),
            ],
        );
        // The player edits further, then learns the save failed.
        submit_all(
            &mut controller,
            [Action::Settings(SettingsAction::Change(
                SettingsField::Fullscreen(true),
            ))],
        );
        submit_all(
            &mut controller,
            [Action::Persisted(PersistOutcome::SaveFailed(
                SaveError::Write,
            ))],
        );
        assert_eq!(controller.state().saving, None);
        // The retry carries the full current draft, not the stale snapshot.
        let effects = submit_all(&mut controller, [Action::Settings(SettingsAction::Save)]);
        assert_eq!(effects, vec![Effect::ApplyAndPersist(english_fullscreen())]);
        submit_all(&mut controller, [Action::Persisted(PersistOutcome::Saved)]);
        assert_eq!(controller.state().applied, english_fullscreen());
        assert_eq!(controller.state().screen, Screen::MainMenu);
    }

    /// A Saved report with no save in flight changes nothing.
    #[test]
    fn a_stray_saved_report_is_a_no_op() {
        let mut controller = controller();
        submit_all(
            &mut controller,
            [Action::MainMenu(MainMenuAction::OpenSettings)],
        );
        let applied = controller.state().applied;
        submit_all(&mut controller, [Action::Persisted(PersistOutcome::Saved)]);
        assert_eq!(controller.state().applied, applied);
        assert_eq!(controller.state().screen, Screen::Settings);
        assert_eq!(
            controller.state().settings_screen.as_ref().unwrap().draft,
            applied
        );
    }

    /// Reopening settings while a cancelled save is still in flight: the
    /// blocked Save protects the one-flight invariant, and the late report
    /// closes the reopened screen with the snapshot applied.
    #[test]
    fn reopen_during_a_flight_blocks_save_until_the_report_lands() {
        let mut controller = controller();
        submit_all(
            &mut controller,
            [
                Action::MainMenu(MainMenuAction::OpenSettings),
                Action::Settings(SettingsAction::Change(SettingsField::Language(
                    Language::English,
                ))),
                Action::Settings(SettingsAction::Save),
                Action::Settings(SettingsAction::Cancel),
                Action::MainMenu(MainMenuAction::OpenSettings),
            ],
        );
        // The reopened draft is the applied settings; Save must not fire a
        // second persist while the first is unresolved.
        let effects = submit_all(&mut controller, [Action::Settings(SettingsAction::Save)]);
        assert!(effects.is_empty());
        assert!(controller.state().saving.is_some());
        // The report lands and closes the reopened screen too.
        submit_all(&mut controller, [Action::Persisted(PersistOutcome::Saved)]);
        assert_eq!(controller.state().applied, english_only());
        assert_eq!(controller.state().screen, Screen::MainMenu);
        assert_eq!(controller.state().settings_screen, None);
    }
}
