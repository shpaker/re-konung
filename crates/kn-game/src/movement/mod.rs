//! Shared movement engine: the isometric fine grid, cell walkability and
//! occupancy, route search, walking a route and the game tick.
//!
//! The layer knows neither Bevy nor anything specific to K1: movement mechanics
//! in K1 and K2 are presumably nearly identical (AGENTS.md, "Priority: modern
//! experience"), so the mechanics live here and the part's knowledge arrives
//! from outside:
//!
//! - cell walkability — [`passage::Passage`] (map data, objects);
//! - step duration in ticks — [`pace::Pace`] (number of animation frames);
//! - cell occupancy — engine state ([`occupancy::Occupancy`]): only the engine
//!   knows who is mid-step and where.
//!
//! The engine keeps no clock: it counts ticks, and how many milliseconds a tick
//! is belongs to the outer world — 60 ms for K1, [`crate::TICK_MS`] (Slavik
//! `game.h:30`).
//!
//! Game rules — who goes where and why — are not here: epic #33 part 3 adds
//! them, and the engine contains no invented ones. Anything not confirmed
//! against the original is either handed to a data supplier or written up as a
//! project decision with its reason in `docs/rust.md`.
//!
//! Searching ([`route`]) and walking ([`walk`]) are deliberately split: search
//! has no notion of a tick, walking has no idea how its route was found.
//! [`world`] is the only place they meet, on one tick.

pub mod grid;
pub mod occupancy;
pub mod pace;
pub mod passage;
pub mod rng;
pub mod route;
pub mod walk;
pub mod world;

/// A creature of the engine: the number issued by [`world::World::add`].
///
/// The number is also the order creatures are visited in on a tick, so a
/// dispute over a cell is settled by the lower number, deterministically
/// (docs/rust.md, "Decision: who yields a cell").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CreatureId(u32);

impl CreatureId {
    pub const fn new(raw: u32) -> CreatureId {
        CreatureId(raw)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

pub use grid::{
    Cell, DIRECTION_COUNT, Direction, FINE_HEIGHT, FINE_WIDTH, Grid, ODD_ROW_SHIFT, STEP_X, STEP_Y,
};
pub use occupancy::{Holder, Occupancy};
pub use pace::Pace;
pub use passage::Passage;
pub use rng::Rng;
pub use route::{Leg, Route, RouteRules, find_route};
pub use walk::{Step, WalkState, Walker};
pub use world::{Event, World, WorldError, WorldRules};
