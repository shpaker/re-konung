//! Apply K1 creature [`Decision`]s through the movement [`World`].
//!
//! Import decisions from k1_game::rules and call this module to mutate the
//! world. Uses explicit `(CreatureId, Decision)` pairs so there is no index
//! mismatch between creatures and decisions.
//!
//! See docs/rust.md for "Evaluation vs application in a tick".

use super::movement::CreatureId;
use super::movement::grid::Cell;
use super::movement::pace::Pace;
use super::movement::passage::Passage;
use super::movement::world::{World, WorldError};
use k1_game::rules::Decision;

/// A decision that the world accepted or rejected, per creature.
#[derive(Debug, Clone, Copy)]
pub enum Applied {
    /// The decision produced no world call (Decision::None).
    Skipped,
    /// `move_to` succeeded with this many route steps.
    Routed { steps: usize },
    /// `move_to` rejected the order with this error.
    Failed(WorldError),
}

/// Apply evaluated decisions through the world.
///
/// Takes explicit `(CreatureId, Decision)` pairs: this avoids index-out-of-
/// bounds when the decision count does not match the roster count. The caller
/// builds these pairs from their own identity mapping (roster index →
/// World-assigned CreatureId). Returns one [`Applied`] per command in order.
pub fn apply<P: Passage, K: Pace>(
    world: &mut World<P, K>,
    commands: &[(CreatureId, Decision)],
) -> Vec<Applied> {
    commands
        .iter()
        .map(|(creature_id, decision)| match decision {
            Decision::None => Applied::Skipped,
            Decision::MoveTo(tx, ty) => match world.move_to(*creature_id, Cell::new(*tx, *ty)) {
                Ok(steps) => Applied::Routed { steps },
                Err(error) => Applied::Failed(error),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::movement::grid::{Direction, Grid};
    use super::super::movement::world::WorldRules;
    use super::*;
    use std::collections::HashSet;

    #[derive(Default)]
    struct Map {
        blocked: HashSet<Cell>,
    }

    impl Passage for Map {
        fn is_walkable(&self, cell: Cell) -> bool {
            !self.blocked.contains(&cell)
        }
    }

    struct Fixed(u32);

    impl Pace for Fixed {
        fn step_ticks(&self, _creature: CreatureId, _direction: Direction) -> u32 {
            self.0
        }
    }

    fn test_world() -> World<Map, Fixed> {
        World::new(
            Grid::new(8, 8),
            Map::default(),
            Fixed(2),
            WorldRules::default(),
            0,
        )
    }

    #[test]
    fn apply_skips_none_without_calling_the_world() {
        let mut world = test_world();
        let creature = world.add(Cell::new(1, 1)).unwrap();
        let result = apply(&mut world, &[(creature, Decision::None)]);
        assert!(matches!(result[0], Applied::Skipped));
    }

    #[test]
    fn apply_routes_a_move_to_decision_through_the_world() {
        let mut world = test_world();
        let creature = world.add(Cell::new(1, 1)).unwrap();
        let result = apply(&mut world, &[(creature, Decision::MoveTo(5, 1))]);
        match result[0] {
            Applied::Routed { steps } => assert!(steps > 0, "expected a real route"),
            other => panic!("expected Routed, got {other:?}"),
        }
        assert_eq!(world.state(creature).unwrap().goal, Some(Cell::new(5, 1)));
    }

    #[test]
    fn apply_reports_failed_for_unknown_creature_id() {
        let mut world = test_world();
        let ghost = CreatureId::new(99);
        let result = apply(&mut world, &[(ghost, Decision::MoveTo(1, 1))]);
        assert!(
            matches!(result[0], Applied::Failed(WorldError::UnknownCreature)),
            "{result:?}"
        );
    }

    #[test]
    fn apply_no_panic_on_empty_commands() {
        let mut world = test_world();
        let result = apply(&mut world, &[]);
        assert!(result.is_empty());
    }

    /// Guards the index-safety claim: two creatures where the second is unknown
    /// still produces two results in order, without touching a shared array.
    #[test]
    fn apply_returns_one_result_per_command_in_order() {
        let mut world = test_world();
        let a = world.add(Cell::new(1, 1)).unwrap();
        let commands = [
            (a, Decision::MoveTo(3, 1)),
            (CreatureId::new(999), Decision::MoveTo(3, 1)),
            (a, Decision::None),
        ];
        let result = apply(&mut world, &commands);
        assert!(matches!(result[0], Applied::Routed { .. }));
        assert!(matches!(
            result[1],
            Applied::Failed(WorldError::UnknownCreature)
        ));
        assert!(matches!(result[2], Applied::Skipped));
    }
}
