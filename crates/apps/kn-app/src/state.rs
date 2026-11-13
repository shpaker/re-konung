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
    /// The hero selection of the new-game flow: one option per
    /// [`AppState::heroes`] entry, entered by `MainMenuAction::NewGame`, left
    /// by [`HeroSelectAction::Choose`] (to [`Loading`](Screen::Loading)) or
    /// [`HeroSelectAction::Cancel`] (back to [`MainMenu`](Screen::MainMenu)).
    HeroSelect,
    /// The settings editor. Previews the `draft` of
    /// [`AppState::settings_screen`], while the rest of the UI keeps rendering
    /// in `applied.language`.
    Settings,
    /// Building the game session. Entered by
    /// `HeroSelectAction::Choose`, left by
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
    /// The heroes the new-game flow offers, as the game layer resolves them
    /// (`Game.N` starting-state numbers). Fixed for the process lifetime: the
    /// list comes from the opened game directory at startup. An empty list is
    /// legal state (nothing to offer); `HeroSelectAction::Choose` of a number
    /// outside it is a no-op.
    pub heroes: Vec<u32>,
    /// The hero the hero-select screen shows the details of: the first
    /// offered hero at startup, [`HeroSelectAction::Preview`] afterwards —
    /// the offer persists across re-entries of the screen. `Some` exactly
    /// while it is a hero of [`AppState::heroes`]; `None` — an empty offer.
    /// The session still starts from `HeroSelectAction::Choose`, which
    /// carries its own number.
    pub hero_preview: Option<u32>,
    /// The last applied-and-persisted settings; the main menu and the whole UI
    /// render in `applied.language`, the settings screen previews
    /// `draft.language`.
    ///
    /// Advances only on `PersistOutcome::Saved`, and only to the snapshot a
    /// `Save` froze — never to the current draft, which may have moved on
    /// while the save was in flight.
    pub applied: Settings,
    /// `Some` exactly while `screen == Settings`.
    pub settings_screen: Option<SettingsScreen>,
    /// The settings snapshot a `Save` sent to the frontend and whose
    /// `Persisted` report has not arrived yet. `Some` blocks a second `Save`
    /// (one save in flight at a time); `Persisted::Saved` applies this
    /// snapshot — whatever the draft says by then — and `SaveFailed` clears
    /// it, keeping the draft for a retry. Survives a `Cancel`: the write is
    /// already in flight, so its report must still be able to advance
    /// `applied` to what the file will actually hold.
    pub saving: Option<Settings>,
    /// The failed session's error, shown in the main menu until the next
    /// `MainMenuAction::NewGame` clears it.
    pub session_error: Option<SessionError>,
}
