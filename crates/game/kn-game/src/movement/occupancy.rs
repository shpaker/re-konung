//! Cell occupancy: one creature per cell.

use std::collections::HashMap;

use super::CreatureId;
use super::grid::Cell;

/// Who holds a cell.
///
/// A creature owns the cell it stands on and reserves the one it is stepping
/// into for the duration of the step (docs/rust.md, "Decision: who yields a
/// cell"). The reservation is the answer to a head-on exchange: a cell someone
/// is already stepping into is taken, and nobody else enters it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Holder {
    pub creature: CreatureId,
    /// `true` — the creature stands here, `false` — a step only reserves it.
    pub standing: bool,
}

/// Cells held by the engine's creatures.
///
/// Engine state, not supplier state: only the engine knows who is mid-step and
/// where, so only the engine releases cells on arrival and on a cancelled
/// route.
#[derive(Debug, Clone, Default)]
pub struct Occupancy {
    holders: HashMap<Cell, Holder>,
}

impl Occupancy {
    pub fn new() -> Occupancy {
        Occupancy::default()
    }

    /// Who holds the cell, standing or by reservation.
    pub fn holder(&self, cell: Cell) -> Option<Holder> {
        self.holders.get(&cell).copied()
    }

    /// Who stands in the cell.
    pub fn occupant(&self, cell: Cell) -> Option<CreatureId> {
        self.holder(cell)
            .filter(|holder| holder.standing)
            .map(|holder| holder.creature)
    }

    /// Whether the cell gets in a creature's way: another creature's does, its
    /// own (the cell under it or its own step reservation) does not.
    ///
    /// During a route search a creature temporarily gives up its own cells, as
    /// the original does (Slavik `game.cpp:2889-2895`).
    pub fn blocks(&self, cell: Cell, creature: CreatureId) -> bool {
        self.holder(cell)
            .is_some_and(|holder| holder.creature != creature)
    }

    /// Take a cell to stand in. `false` — someone else holds it.
    pub fn stand(&mut self, cell: Cell, creature: CreatureId) -> bool {
        self.hold(cell, creature, true)
    }

    /// Reserve a cell for a step. `false` — someone else holds it.
    pub fn reserve(&mut self, cell: Cell, creature: CreatureId) -> bool {
        self.hold(cell, creature, false)
    }

    fn hold(&mut self, cell: Cell, creature: CreatureId, standing: bool) -> bool {
        if self
            .holder(cell)
            .is_some_and(|holder| holder.creature != creature)
        {
            return false;
        }
        self.holders.insert(cell, Holder { creature, standing });
        true
    }

    /// Stop holding a cell (only one's own record).
    pub fn release(&mut self, cell: Cell, creature: CreatureId) {
        if self
            .holder(cell)
            .is_some_and(|holder| holder.creature == creature)
        {
            self.holders.remove(&cell);
        }
    }

    /// Stop holding every cell of a creature: the one under it plus any
    /// reservation. Cells not held by it are left alone.
    pub fn release_all(&mut self, creature: CreatureId) {
        for cell in self.cells_of(creature) {
            self.holders.remove(&cell);
        }
    }

    /// Every cell a creature holds: the one under it plus any reservation.
    pub fn cells_of(&self, creature: CreatureId) -> Vec<Cell> {
        let mut cells: Vec<Cell> = self
            .holders
            .iter()
            .filter(|(_, holder)| holder.creature == creature)
            .map(|(cell, _)| *cell)
            .collect();
        cells.sort_unstable();
        cells
    }

    pub fn is_empty(&self) -> bool {
        self.holders.is_empty()
    }

    pub fn len(&self) -> usize {
        self.holders.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: CreatureId = CreatureId::new(1);
    const B: CreatureId = CreatureId::new(2);

    #[test]
    fn one_holder_per_cell() {
        let cell = Cell::new(3, 4);
        let mut occupancy = Occupancy::new();
        assert!(occupancy.stand(cell, A));
        assert!(!occupancy.stand(cell, B));
        assert!(!occupancy.reserve(cell, B));
        assert_eq!(occupancy.occupant(cell), Some(A));
        assert_eq!(
            occupancy.holder(cell),
            Some(Holder {
                creature: A,
                standing: true
            })
        );
    }

    #[test]
    fn reservation_blocks_but_is_not_an_occupant() {
        let cell = Cell::new(1, 1);
        let mut occupancy = Occupancy::new();
        assert!(occupancy.reserve(cell, A));
        assert_eq!(occupancy.occupant(cell), None);
        assert!(occupancy.blocks(cell, B));
        assert!(
            !occupancy.blocks(cell, A),
            "one's own reservation does not block oneself"
        );
        assert!(!occupancy.stand(cell, B));
        occupancy.release(cell, A);
        assert!(!occupancy.blocks(cell, B));
    }

    #[test]
    fn own_cell_does_not_block_own_search() {
        let cell = Cell::new(5, 5);
        let mut occupancy = Occupancy::new();
        occupancy.stand(cell, A);
        assert!(!occupancy.blocks(cell, A));
        assert!(occupancy.blocks(cell, B));
        assert_eq!(occupancy.cells_of(A), vec![cell]);
    }

    #[test]
    fn stand_replaces_reservation_of_the_same_creature() {
        let cell = Cell::new(2, 2);
        let mut occupancy = Occupancy::new();
        occupancy.reserve(cell, A);
        assert!(occupancy.stand(cell, A));
        assert_eq!(occupancy.occupant(cell), Some(A));
    }

    #[test]
    fn release_all_frees_standing_and_reservation_alike() {
        let mut occupancy = Occupancy::new();
        let stood = Cell::new(2, 2);
        let reserved = Cell::new(3, 2);
        let foreign = Cell::new(4, 2);
        occupancy.stand(stood, A);
        occupancy.reserve(reserved, A);
        occupancy.stand(foreign, B);
        occupancy.release_all(A);
        assert_eq!(occupancy.holder(stood), None);
        assert_eq!(occupancy.holder(reserved), None);
        assert_eq!(
            occupancy.holder(foreign),
            Some(Holder {
                creature: B,
                standing: true
            }),
            "a foreign cell must survive the release"
        );
    }
}
