//! The side-effect side of the player's MVU loop: what the pure reducer asks
//! the impure world to do. Effects are executed by the frontend adapter and
//! answered with result actions; the reducer itself never executes them.

use kn_settings::Settings;

/// A request from the reducer to the frontend adapter. Each effect is emitted
/// by exactly one action, and result actions (`Loading`, `Persisted`) emit
/// none, so the frame pipeline has no feedback loops.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Effect {
    /// Build the game session (the frontend runs the current start-map flow)
    /// and report back with `Loading(Succeeded)` or `Loading(Failed(e))`.
    StartSession,
    /// Apply `Settings` to the running app and persist; on persist failure the
    /// frontend rolls the runtime back to the current `applied` and reports
    /// `Persisted(SaveFailed(e))`, on success `Persisted(Saved)`.
    ApplyAndPersist(Settings),
    /// Quit the application; the process-level decision (and any exit
    /// confirmation) belongs to the frontend.
    ExitApp,
}
