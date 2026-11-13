//! Bevy-free application layer of the player: screens, typed actions, a pure
//! reducer and effects.
//!
//! This crate is the State→Action→Controller→Effects core of the player's
//! MVU (Model-View-Update) loop. A Bevy frontend wraps it: the view submits
//! user intents as [`Action`]s, [`AppController::flush`] reduces them into the
//! next [`AppState`] plus [`Effect`]s, the adapter executes the effects in the
//! real world and reports the outcomes back as actions:
//!
//! ```text
//! View → Action → AppController::submit → flush → AppState + Vec<Effect>
//!   ↑                                                  │
//!   └──────────── Bevy adapter executes effects ───────┘
//! ```
//!
//! Invariants the frontend can rely on:
//!
//! * [`AppController::flush`] is pure: no clock, no io, no randomness. All
//!   impurity (window mode, file writes, process exit) lives behind
//!   [`Effect`]s in the adapter.
//! * Result actions ([`Action::Loading`], [`Action::Persisted`]) never produce
//!   effects, so executing an effect cannot feed back into the queue within a
//!   frame.
//! * Every other effect is emitted by exactly one action; `flush` drains the
//!   queue fully and actions are applied in submission order.
//! * An action whose screen is not current is a no-op (a second
//!   [`MainMenuAction::NewGame`] while loading, a settings action from the
//!   menu), so stale or duplicated input cannot corrupt the state.
//! * `settings_screen` is `Some` exactly while the screen is
//!   [`Screen::Settings`], and its `draft` is the only copy of unsaved
//!   settings. A `Save` freezes the snapshot into
//!   [`AppState::saving`](crate::AppState::saving); [`PersistOutcome::Saved`]
//!   advances `applied` to that snapshot — never to the current draft — and
//!   one save is in flight at a time. The snapshot survives a `Cancel`, so
//!   the file and the state never disagree.

mod action;
mod controller;
mod effect;
mod state;

pub use action::{
    Action, HeroSelectAction, LoadingAction, MainMenuAction, PersistOutcome, SettingsAction,
    SettingsField,
};
pub use controller::AppController;
pub use effect::Effect;
pub use state::{AppState, SaveError, SaveStatus, Screen, SessionError, SettingsScreen};
