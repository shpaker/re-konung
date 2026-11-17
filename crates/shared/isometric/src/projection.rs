//! Pixel projection of the staggered grid.

use crate::grid::{Cell, Direction, Grid};

/// Fine-grid horizontal step in pixels: half of the tile grid's step.
pub const STEP_X: i32 = 58;
/// Fine-grid vertical step in pixels: half of a tile row step.
pub const STEP_Y: i32 = 16;
/// Odd rows lie this far left of the same column on even rows.
pub const ODD_ROW_SHIFT: i32 = STEP_X / 2;

impl Direction {
    /// Offset of the cell's pixel point: independent of row parity — the row
    /// shift exactly cancels the difference between the two offset tables.
    pub const fn pixel_offset(self) -> (i32, i32) {
        match self {
            Direction::West => (-STEP_X, 0),
            Direction::NorthWest => (-ODD_ROW_SHIFT, -STEP_Y),
            Direction::North => (0, -2 * STEP_Y),
            Direction::NorthEast => (ODD_ROW_SHIFT, -STEP_Y),
            Direction::East => (STEP_X, 0),
            Direction::SouthEast => (ODD_ROW_SHIFT, STEP_Y),
            Direction::South => (0, 2 * STEP_Y),
            Direction::SouthWest => (-ODD_ROW_SHIFT, STEP_Y),
        }
    }
}

impl Cell {
    /// The cell's pixel point — the centre of its rhombus, where the sprite
    /// is placed.
    ///
    /// The cross-component tests compare this projection with K1 record positions.
    pub fn centre(self) -> (i32, i32) {
        let column = self.column as i64;
        let row = self.row as i64;
        let shift = i64::from(2 - (row & 1) as i32) * i64::from(ODD_ROW_SHIFT);
        (
            (column * i64::from(STEP_X) + shift) as i32,
            (row * i64::from(STEP_Y) + i64::from(STEP_Y)) as i32,
        )
    }
}

impl Grid {
    /// The cell closest to a map pixel point.
    ///
    /// Halves round to even, as `round()` does in the Python reference and in
    /// `k1_data::game_state::character_cell`: otherwise the reverse conversion
    /// would disagree with the forward one on cell borders.
    pub fn cell_at(self, x: i32, y: i32) -> Option<Cell> {
        let row = round_half_even(i64::from(y) - i64::from(STEP_Y), i64::from(STEP_Y));
        let shift = i64::from(2 - (row & 1) as i32) * i64::from(ODD_ROW_SHIFT);
        let column = round_half_even(i64::from(x) - shift, i64::from(STEP_X));
        if column < 0 || row < 0 {
            return None;
        }
        let cell = Cell::new(column as u32, row as u32);
        self.contains(cell).then_some(cell)
    }
}

/// Division rounding halves to even, the way Python's `round()` does.
pub(crate) fn round_half_even(numerator: i64, denominator: i64) -> i64 {
    let floor = numerator.div_euclid(denominator);
    let twice = 2 * numerator.rem_euclid(denominator);
    match twice.cmp(&denominator) {
        std::cmp::Ordering::Greater => floor + 1,
        std::cmp::Ordering::Less => floor,
        std::cmp::Ordering::Equal => {
            if floor & 1 == 0 {
                floor
            } else {
                floor + 1
            }
        }
    }
}
