//! The state side of the player's MVU loop: everything the frontend renders
//! from, updated exclusively by [`AppController`](crate::AppController).

use kn_settings::Settings;

/// A full-screen surface of the player application.
///
/// The screen is the only routing field: there is no nesting and no history,
/// because v1 has exactly one entry point per screen (settings open from the
/// main menu only; the pause menu is deferred).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    /// The title menu. Renders in `applied.language`; a failed session shows
    /// [`AppState::session_error`] here.
    MainMenu,
    /// The settings editor. Previews the `draft` of
    /// [`AppState::settings_screen`], while the rest of the UI keeps rendering
    /// in `applied.language`.
    Settings,
    /// Building the game session. Entered by `MainMenuAction::NewGame`, left by
    /// [`LoadingAction::Succeeded`] (to [`Playing`](Screen::Playing)) or
    /// [`LoadingAction::Failed`] (back to [`MainMenu`](Screen::MainMenu)).
    Loading,
    /// The running game session. No v1 action leaves it (exiting to the pause
    /// menu arrives with the pause menu itself).
    Playing,
}

/// Why a game session could not be built. Shown in the main menu until the
/// next `MainMenuAction::NewGame` clears it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SessionError {
    /// The start map could not be loaded or parsed.
    Map,
    /// The scene could not be assembled from the loaded data.
    Scene,
    /// The textures could not be decoded.
    Textures,
    /// The simulation could not be started.
    Simulation,
}

/// Persistence failure kind only — the io detail stays in the frontend's log.
///
/// The settings screen shows *that* saving failed, so the player can retry;
/// which syscall failed is diagnostics, not UI state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SaveError {
    /// Writing the new settings file failed.
    Write,
    /// The write finished, but replacing the previous file failed.
    Replace,
}

/// Per-save-attempt status of the settings screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SaveStatus {
    /// No save attempt is pending or failed; the initial status.
    Idle,
    /// The last `SettingsAction::Save` failed; the draft is kept for retry.
    Failed(SaveError),
}

/// The state of the settings screen while it is open.
#[derive(Clone, PartialEq, Debug)]
pub struct SettingsScreen {
    /// The edited, not yet persisted settings. Every `SettingsAction::Change`
    /// edits it; `ApplyAndPersist` carries it away on save; on
    /// `PersistOutcome::SaveFailed` it survives for another `Save`.
    pub draft: Settings,
    /// `Idle` until a save fails; reset never happens in-place (a successful
    /// save closes the screen, and reopening it starts at `Idle` again).
    pub status: SaveStatus,
    // No `return_to` in v1: settings opens from the main menu only (the pause
    // menu is deferred), so the return target is always MainMenu. The field
    // appears when a second entry point exists.
}

/// The whole application state of the player, minus the running game.
#[derive(Clone, PartialEq, Debug)]
pub struct AppState {
    pub screen: Screen,
    /// The last applied-and-persisted settings; the main menu and the whole UI
    /// render in `applied.language`, the settings screen previews
    /// `draft.language`.
    ///
    /// Advances only on `PersistOutcome::Saved`: while a save is in flight the
    /// UI keeps rendering in the old language, and on failure nothing moves.
    pub applied: Settings,
    /// `Some` exactly while `screen == Settings`.
    pub settings_screen: Option<SettingsScreen>,
    /// The failed session's error, shown in the main menu until the next
    /// `MainMenuAction::NewGame` clears it.
    pub session_error: Option<SessionError>,
}
