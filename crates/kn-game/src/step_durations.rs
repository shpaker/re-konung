//! A step-duration table for the movement engine: [`movement::pace::Pace`] filled
//! from data the caller brought.
//!
//! The engine asks for a creature and a direction and gets the length of one step
//! in ticks. Answers live in a table so that a supplier reads them once per
//! creature rather than once per tick — in K1 that reading is
//! [`k1_game::creatures::step_durations`], which touches `GRAPH.RES` and
//! `Objects.res`.

use crate::movement::grid::DIRECTION_COUNT;
use crate::movement::{CreatureId, Direction, Pace};

/// Step durations of the creatures of one world.
///
/// Row `n` belongs to the creature added as the `n`-th, so a [`CreatureId`] is the
/// index into the table and the engine never looks a creature up in game data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepDurations {
    rows: Vec<[u32; DIRECTION_COUNT]>,
}

impl StepDurations {
    pub fn new(rows: Vec<[u32; DIRECTION_COUNT]>) -> StepDurations {
        StepDurations { rows }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

impl Pace for StepDurations {
    /// One tick is the floor: neither a creature the table does not know nor an
    /// animation without frames may ask the engine for a step of no length
    /// (docs/k1/movement.md, "Step duration"; the rule is ours, not the game's).
    fn step_ticks(&self, creature: CreatureId, direction: Direction) -> u32 {
        self.rows
            .get(creature.raw() as usize)
            .and_then(|row| row.get(direction.index()))
            .copied()
            .unwrap_or(0)
            .max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(east: u32) -> [u32; DIRECTION_COUNT] {
        let mut row = [12; DIRECTION_COUNT];
        row[Direction::East.index()] = east;
        row
    }

    #[test]
    fn a_creature_reads_its_own_row_by_its_number() {
        let durations = StepDurations::new(vec![row(12), row(1)]);
        assert_eq!(durations.step_ticks(CreatureId::new(1), Direction::East), 1);
        assert_eq!(
            durations.step_ticks(CreatureId::new(0), Direction::East),
            12
        );
    }

    #[test]
    fn a_zero_duration_and_an_unknown_creature_both_last_one_tick() {
        let durations = StepDurations::new(vec![row(0)]);
        assert_eq!(
            durations.step_ticks(CreatureId::new(0), Direction::East),
            1,
            "an animation without walk frames must not stop the engine"
        );
        assert_eq!(
            durations.step_ticks(CreatureId::new(7), Direction::North),
            1,
            "a creature outside the table must not panic the engine either"
        );
    }
}
