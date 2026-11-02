//! Step duration: the data supplier sets it, not the engine.

use super::grid::Direction;

/// How many ticks a creature's step lasts in this direction.
pub trait Pace {
    /// Step duration in game ticks.
    ///
    /// For K1 this is the number of walk-animation frames in that direction
    /// (Slavik `game_play.cpp:3727-3740`): the engine knows neither the frame
    /// count nor that it is an animation at all. A duration of 0 is counted as
    /// one tick — a zero-length step does not exist, and data must not be able
    /// to panic the engine.
    fn step_ticks(&self, creature: super::CreatureId, direction: Direction) -> u32;
}
