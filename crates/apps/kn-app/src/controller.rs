//! The controller: a pure reducer over [`AppState`] driven by queued
//! [`Action`]s and returning [`Effect`]s for the frontend to execute.

use std::collections::VecDeque;

use kn_settings::Settings;

use crate::action::{
    Action, LoadingAction, MainMenuAction, PersistOutcome, SettingsAction, SettingsField,
};
use crate::effect::Effect;
use crate::state::{AppState, SaveStatus, Screen, SettingsScreen};

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
    /// Creates the controller at the main menu with the given
    /// applied-and-persisted settings (the ones the startup loaded from disk).
    pub fn new(applied: Settings) -> Self {
        Self {
            state: AppState {
                screen: Screen::MainMenu,
                applied,
                settings_screen: None,
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
                self.state.screen = Screen::Loading;
                effects.push(Effect::StartSession);
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
                        // The screen stays open until the Persisted report.
                        effects.push(Effect::ApplyAndPersist(settings_screen.draft));
                    } else {
                        self.close_settings();
                    }
                }
            }
            (Screen::Settings, Action::Settings(SettingsAction::Cancel)) => {
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
                if let Some(settings_screen) = self.state.settings_screen.take() {
                    self.state.applied = settings_screen.draft;
                    self.state.screen = Screen::MainMenu;
                }
            }
            (Screen::Settings, Action::Persisted(PersistOutcome::SaveFailed(error))) => {
                if let Some(settings_screen) = self.state.settings_screen.as_mut() {
                    settings_screen.status = SaveStatus::Failed(error);
                }
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
}

#[cfg(test)]
mod tests {
    use kn_settings::{Language, Settings, VideoSettings};

    use super::AppController;
    use crate::action::{
        Action, LoadingAction, MainMenuAction, PersistOutcome, SettingsAction, SettingsField,
    };
    use crate::effect::Effect;
    use crate::state::{SaveError, SaveStatus, Screen, SessionError};

    fn controller() -> AppController {
        AppController::new(Settings::default())
    }

    /// A controller driven to `screen` through legal transitions only.
    fn controller_at(screen: Screen) -> AppController {
        let mut controller = controller();
        match screen {
            Screen::MainMenu => {}
            Screen::Settings => {
                controller.submit(Action::MainMenu(MainMenuAction::OpenSettings));
            }
            Screen::Loading => {
                controller.submit(Action::MainMenu(MainMenuAction::NewGame));
            }
            Screen::Playing => {
                controller.submit(Action::MainMenu(MainMenuAction::NewGame));
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
        let controller = AppController::new(applied);
        assert_eq!(controller.state().screen, Screen::MainMenu);
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
    fn main_menu_new_game_enters_loading_and_starts_session() {
        let mut controller = controller();
        let effects = submit_all(&mut controller, [Action::MainMenu(MainMenuAction::NewGame)]);
        assert_eq!(effects, vec![Effect::StartSession]);
        assert_eq!(controller.state().screen, Screen::Loading);
        assert_eq!(controller.state().session_error, None);
    }

    #[test]
    fn main_menu_new_game_clears_previous_session_error() {
        let mut controller = controller();
        submit_all(
            &mut controller,
            [
                Action::MainMenu(MainMenuAction::NewGame),
                Action::Loading(LoadingAction::Failed(SessionError::Map)),
            ],
        );
        assert_eq!(controller.state().session_error, Some(SessionError::Map));
        let effects = submit_all(&mut controller, [Action::MainMenu(MainMenuAction::NewGame)]);
        assert_eq!(effects, vec![Effect::StartSession]);
        assert_eq!(controller.state().screen, Screen::Loading);
        assert_eq!(controller.state().session_error, None);
    }

    #[test]
    fn main_menu_open_settings_copies_applied_into_draft() {
        let mut controller = AppController::new(english_fullscreen());
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
        // The only rows that emit: NewGame and Exit from MainMenu, Save from
        // Settings with a draft differing from applied.
        for screen in [
            Screen::MainMenu,
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
                        (Screen::MainMenu, Action::MainMenu(MainMenuAction::NewGame))
                        | (Screen::MainMenu, Action::MainMenu(MainMenuAction::Exit)) => true,
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
        let effects = submit_all(&mut controller, [Action::MainMenu(MainMenuAction::NewGame)]);
        assert_eq!(effects, vec![Effect::StartSession]);
        let effects = submit_all(
            &mut controller,
            [Action::Loading(LoadingAction::Failed(SessionError::Map))],
        );
        assert!(effects.is_empty());
        assert_eq!(controller.state().screen, Screen::MainMenu);
        assert_eq!(controller.state().session_error, Some(SessionError::Map));
        let effects = submit_all(&mut controller, [Action::MainMenu(MainMenuAction::NewGame)]);
        assert_eq!(effects, vec![Effect::StartSession]);
        assert_eq!(controller.state().screen, Screen::Loading);
        assert_eq!(controller.state().session_error, None);
    }
}
