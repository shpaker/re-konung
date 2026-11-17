//! K1 map dimensions supplied to shared geometry.

use kn_isometric::Grid;

pub const FINE_WIDTH: u32 = k1_data::map::FINE_WIDTH as u32;
pub const FINE_HEIGHT: u32 = k1_data::map::FINE_HEIGHT as u32;

pub const fn fine_grid() -> Grid {
    Grid::new(FINE_WIDTH, FINE_HEIGHT)
}
