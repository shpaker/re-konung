//! Cell walkability: where a creature may stand is knowledge from outside.

use super::grid::Cell;

/// Where a creature may stand.
///
/// The engine knows neither map formats nor objects nor anything specific to
/// one part of the series: the supplier (for K1 `k1-data`, later `k2-data`)
/// answers one question about a cell. Other creatures occupying cells is not
/// this trait's business — that is [`super::occupancy::Occupancy`].
pub trait Passage {
    /// Whether a supplier's data lets a creature stand in this cell.
    fn is_walkable(&self, cell: Cell) -> bool;
}
