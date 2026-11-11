//! The intent side of the player's MVU loop: what the user (or a finished
//! background job) asks the application to do.
//!
//! An action names an intent, never a transition: `SettingsAction::Save` does
//! not say whether the screen closes — that depends on the draft and on the
//! [`PersistOutcome`](PersistOutcome) that follows. The transition table lives
//! in [`AppController`](crate::AppController).

use kn_settings::Language;

use crate::state::{SaveError, SessionError};

/// Main menu intents.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MainMenuAction {
    /// Start a new game: enter [`Loading`](crate::Screen::Loading) and wait
    /// for the session build, clearing any previous
    /// [`session_error`](crate::AppState::session_error). Emits
    /// [`StartSession`](crate::Effect::StartSession); a repeat while already
    /// loading is a no-op, so a double click cannot start two sessions.
    NewGame,
    /// Open the settings screen with a draft copied from the applied settings.
    OpenSettings,
    /// Quit the application. Emits [`ExitApp`](crate::Effect::ExitApp) and
    /// changes no state: whether the process really exits is the frontend's
    /// decision.
    Exit,
}

/// One settings field with its next value; `SettingsAction::Change` edits the
/// draft field by field.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingsField {
    Language(Language),
    Fullscreen(bool),
    Vsync(bool),
}

/// Settings screen intents.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingsAction {
    /// Edit the draft with one field assignment; `applied` is untouched.
    Change(SettingsField),
    /// Apply the draft and persist it. Emits
    /// [`ApplyAndPersist(draft)`](crate::Effect::ApplyAndPersist) and keeps
    /// the screen open until the frontend reports back:
    /// `Persisted(Saved)` applies the draft and returns to the main menu;
    /// `Persisted(SaveFailed(e))` keeps the draft for retry. An unchanged
    /// draft (draft == applied) skips the round trip and closes immediately —
    /// there is nothing to persist.
    Save,
    /// Discard the draft and return to the main menu; nothing is persisted.
    /// A `Persisted` result that arrives after `Cancel` is a no-op: the
    /// frontend may still deliver it, but the screen it belonged to is gone.
    Cancel,
}

/// Session build results, reported by the frontend that executes
/// [`StartSession`](crate::Effect::StartSession).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoadingAction {
    /// The session is built; enter [`Playing`](crate::Screen::Playing).
    Succeeded,
    /// The session failed; return to [`MainMenu`](crate::Screen::MainMenu)
    /// with [`session_error`](crate::AppState::session_error) set to `e`.
    Failed(SessionError),
}

/// Settings persistence results, reported by the frontend that executes
/// [`ApplyAndPersist`](crate::Effect::ApplyAndPersist).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PersistOutcome {
    /// The draft is persisted; it becomes `applied` and the screen closes.
    Saved,
    /// Persistence failed with this kind; the screen stays open, the draft is
    /// kept and `status` becomes `Failed(e)` so the player can retry `Save`.
    SaveFailed(SaveError),
}

/// An application action. The sub-enums carry the intents of one screen each;
/// an action submitted while its screen is not current is a no-op.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    MainMenu(MainMenuAction),
    Settings(SettingsAction),
    Loading(LoadingAction),
    Persisted(PersistOutcome),
}
