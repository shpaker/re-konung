//! Занятость клеток: в одной клетке стоит одно существо.

use std::collections::HashMap;

use super::CreatureId;
use super::grid::Cell;

/// Кто держит клетку.
///
/// Существо владеет клеткой, на которой стоит, и на время шага резервирует ту,
/// куда идёт (docs/rust.md, «Решение: кто уступает клетку»). Резерв и есть
/// ответ на встречный обмен: клетка, в которую кто-то уже идёт, занята, и в
/// неё не войдут.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Holder {
    pub creature: CreatureId,
    /// `true` — существо стоит в клетке, `false` — только резервирует её шагом.
    pub standing: bool,
}

/// Клетки, занятые существами движка.
///
/// Это состояние движка, а не поставщика: только движок знает, кто куда идёт
/// шагом, и только ему освобождать клетки при приходе и отмене маршрута.
#[derive(Debug, Clone, Default)]
pub struct Occupancy {
    holders: HashMap<Cell, Holder>,
}

impl Occupancy {
    pub fn new() -> Occupancy {
        Occupancy::default()
    }

    /// Кто держит клетку — стоя или резервом.
    pub fn holder(&self, cell: Cell) -> Option<Holder> {
        self.holders.get(&cell).copied()
    }

    /// Кто стоит в клетке.
    pub fn occupant(&self, cell: Cell) -> Option<CreatureId> {
        self.holder(cell)
            .filter(|holder| holder.standing)
            .map(|holder| holder.creature)
    }

    /// Мешает ли клетка существу: чужая — да, своя (клетка под ногами или
    /// резерв своего шага) — нет.
    ///
    /// На время поиска пути свои клетки существо временно освобождает — так же
    /// делает оригинал (Slavik `game.cpp:2889-2895`).
    pub fn blocks(&self, cell: Cell, creature: CreatureId) -> bool {
        self.holder(cell)
            .is_some_and(|holder| holder.creature != creature)
    }

    /// Занять клетку стоя. `false` — клетку держит кто-то другой.
    pub fn stand(&mut self, cell: Cell, creature: CreatureId) -> bool {
        self.hold(cell, creature, true)
    }

    /// Зарезервировать клетку за шагом. `false` — клетку держит кто-то другой.
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

    /// Перестать держать клетку (своей записи).
    pub fn release(&mut self, cell: Cell, creature: CreatureId) {
        if self
            .holder(cell)
            .is_some_and(|holder| holder.creature == creature)
        {
            self.holders.remove(&cell);
        }
    }

    /// Все клетки, которыми существо что-то держит: клетку под ногами и резерв.
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
        assert!(!occupancy.blocks(cell, A), "свой резерв не мешает себе");
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
}
