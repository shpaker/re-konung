//! The side-effect side of the player's MVU loop: what the pure reducer asks
//! the impure world to do. Effects are executed by the frontend adapter and
//! answered with result actions; the reducer itself never executes them.

use kn_settings::Settings;

/// A request from the reducer to the frontend adapter. Each effect is emitted
/// by exactly one action, and result actions (`Loading`, `Persisted`) emit
/// none, so the frame pipeline has no feedback loops.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Effect {
    /// Build the game session for the chosen hero: the frontend opens the
    /// hero's start map at the hero's on-map position and reports back with
    /// `Loading(Succeeded)` or `Loading(Failed(e))`. `hero` is the `Game.N`
    /// starting-state file `HeroSelectAction::Choose` carried; `class_row`
    /// is the constructor's occupation row (0–3), which the session's
    /// character record carries (the original writes it in `Update8`,
    /// `game.cpp:2453-2472`).
    StartSession {
        /// The chosen hero's starting-state number.
        hero: u32,
        /// The constructor's occupation table row.
        class_row: usize,
    },
    /// Apply `Settings` to the running app and persist; on persist failure the
    /// frontend rolls the runtime back to the current `applied` and reports
    /// `Persisted(SaveFailed(e))`, on success `Persisted(Saved)`.
    ApplyAndPersist(Settings),
    /// Quit the application; the process-level decision (and any exit
    /// confirmation) belongs to the frontend.
    ExitApp,
}
