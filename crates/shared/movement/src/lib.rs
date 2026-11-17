//! Deterministic movement over an isometric grid.
//!
//! Callers supply walkability through Passage and step durations through Pace.
//! The engine counts externally driven ticks and emits movement events.
//! Route search and step execution meet only in World.

pub mod metric;
pub mod occupancy;
pub mod pace;
pub mod passage;
pub mod rng;
pub mod route;
pub mod step_durations;
pub mod walk;
pub mod world;

/// A creature of the engine: the number issued by [`world::World::add`].
///
/// The number is also the order creatures are visited in on a tick, so a
/// dispute over a cell is settled by the lower number, deterministically
/// (docs/development/rust/movement.md, "Decision: who yields a cell").
///
/// A number names one creature for the world's whole life: [`world::World::remove`]
/// does not hand it to a later one, so an id kept outside the engine cannot end
/// up ordering a stranger (docs/development/rust/movement.md, "Decision: taking a creature out keeps
/// the numbers").
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

pub use occupancy::{Holder, Occupancy};
pub use pace::Pace;
pub use passage::Passage;
pub use rng::Rng;
pub use route::{Leg, Route, RouteRules, find_route};
pub use step_durations::StepDurations;
pub use walk::{Step, WalkState, Walker};
pub use world::{Event, GoalPolicy, MovementMode, World, WorldError, WorldRules};
