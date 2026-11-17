//! Shared staggered-grid geometry without game formats or a display engine.

pub mod grid;
pub mod projection;

pub use grid::{Cell, DIRECTION_COUNT, Direction, Grid};
pub use projection::{ODD_ROW_SHIFT, STEP_X, STEP_Y};

#[cfg(test)]
mod tests;
