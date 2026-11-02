//! Apply K1 creature [`Decision`]s through the movement [`World`].
//!
//! Import decisions from k1_game::rules and call this module to mutate the
//! world. Uses explicit `(CreatureId, Decision)` pairs so there is no index
//! mismatch between creatures and decisions.
//!
//! See docs/rust.md, "Decision: separate evaluate from apply".

use super::movement::CreatureId;
use super::movement::grid::Cell;
use super::movement::pace::Pace;
use super::movement::passage::Passage;
use super::movement::world::{World, WorldError};
use k1_game::rules::Decision;

/// A decision that the world accepted or rejected, per creature.
#[derive(Debug, Clone, Copy)]
pub enum Applied {
    /// The decision produced no world call (Decision::None). It does not stop
    /// a creature that is already walking toward an earlier goal — None means
    /// "no new order", not "cancel".
    Skipped,
    /// `move_to` accepted the order. `route_steps` is what the world reported:
    /// steps left on the route, or `0` when the creature is mid-step and the
    /// route will only be recomputed at the next cell border. Acceptance and a
    /// built route are distinct — `route_steps == 0` is accepted, not failed.
    Accepted { route_steps: usize },
    /// `move_to` rejected the order with this error.
    Failed(WorldError),
}

/// Apply evaluated decisions through the world.
///
/// Takes explicit `(CreatureId, Decision)` pairs: this avoids index-out-of-
/// bounds when the decision count does not match the roster count. The caller
/// builds these pairs from their own identity mapping (roster index →
/// World-assigned CreatureId). Returns one [`Applied`] per command in order.
///
/// Repeating the same goal for a creature still pursuing it is idempotent — the
/// world keeps its route and waiting timer, so a tick loop may re-send orders.
pub fn apply<P: Passage, K: Pace>(
    world: &mut World<P, K>,
    commands: &[(CreatureId, Decision)],
) -> Vec<Applied> {
    commands
        .iter()
        .map(|(creature_id, decision)| match decision {
            Decision::None => Applied::Skipped,
            Decision::MoveTo(tx, ty) => match world.move_to(*creature_id, Cell::new(*tx, *ty)) {
                Ok(route_steps) => Applied::Accepted { route_steps },
                Err(error) => Applied::Failed(error),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::movement::grid::{Direction, Grid};
    use super::super::movement::world::{Event, WorldRules};
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
    fn apply_sends_a_move_to_decision_through_the_world() {
        let mut world = test_world();
        let creature = world.add(Cell::new(1, 1)).unwrap();
        let result = apply(&mut world, &[(creature, Decision::MoveTo(5, 1))]);
        match result[0] {
            Applied::Accepted { route_steps } => assert!(route_steps > 0, "expected a real route"),
            other => panic!("expected Accepted, got {other:?}"),
        }
        assert_eq!(world.state(creature).unwrap().goal, Some(Cell::new(5, 1)));
    }

    /// Decision::None does not cancel an in-progress route: the creature keeps
    /// walking to the goal set by an earlier command.
    #[test]
    fn none_does_not_stop_a_running_route() {
        let mut world = test_world();
        let creature = world.add(Cell::new(1, 1)).unwrap();
        apply(&mut world, &[(creature, Decision::MoveTo(5, 1))]);
        world.tick_step();
        apply(&mut world, &[(creature, Decision::None)]);
        assert_eq!(
            world.state(creature).unwrap().goal,
            Some(Cell::new(5, 1)),
            "None must not clear the goal"
        );
    }

    /// Re-sending the same goal keeps the route and the waiting timer: only the
    /// first order searches, so no second `RouteSet` event is produced.
    #[test]
    fn repeated_order_emits_no_second_route_set() {
        let mut world = test_world();
        let creature = world.add(Cell::new(1, 1)).unwrap();
        let first = apply(&mut world, &[(creature, Decision::MoveTo(5, 1))]);
        assert!(
            matches!(first[0], Applied::Accepted { route_steps } if route_steps > 0),
            "{first:?}"
        );
        // The first order's search surfaces as a RouteSet on the next tick.
        let started = world.tick_step();
        assert!(
            started.iter().any(|e| matches!(e, Event::RouteSet { .. })),
            "the first order must set a route: {started:?}"
        );
        // The same order again while the creature still holds the route: accepted,
        // but it must not re-search, so the following tick carries no RouteSet.
        let again = apply(&mut world, &[(creature, Decision::MoveTo(5, 1))]);
        assert!(matches!(again[0], Applied::Accepted { .. }), "{again:?}");
        let next = world.tick_step();
        assert!(
            !next.iter().any(|e| matches!(e, Event::RouteSet { .. })),
            "a repeated order must not re-search: {next:?}"
        );
    }

    /// A tick loop that re-sends the same goal every tick must not defeat the
    /// waiting mechanism: a creature held up by another stands and waits, the
    /// repeats add no new `RouteSet` while it waits, and it resumes the moment
    /// the cell frees. The route's first step is forced through a single free
    /// neighbour so the test is deterministic about which cell is blocked.
    #[test]
    fn repeated_order_while_blocked_waits_then_resumes() {
        let mut world = test_world();
        let start = Cell::new(2, 4);
        // A screen-diagonal step has no corner-flank requirement, so the only
        // free neighbour is a forced first step.
        let first_step = start.neighbour(Direction::NorthEast).unwrap();
        let goal = first_step.neighbour(Direction::NorthEast).unwrap();

        // Wall off every other neighbour of `start`.
        for direction in Direction::ALL {
            if direction == Direction::NorthEast {
                continue;
            }
            if let Some(neighbour) = start.neighbour(direction) {
                world.passage_mut().blocked.insert(neighbour);
            }
        }

        let creature = world.add(start).unwrap();
        // Build the route to `goal`; its only way out is `first_step`.
        let opened = apply(
            &mut world,
            &[(creature, Decision::MoveTo(goal.column, goal.row))],
        );
        assert!(
            matches!(opened[0], Applied::Accepted { route_steps } if route_steps > 0),
            "{opened:?}"
        );
        // The held cell: the route's forced first step, occupied by a second
        // creature that will not move on its own.
        let blocker = world.add(first_step).unwrap();

        // The first tick drains the initial search's RouteSet and finds the
        // creature held.
        let mut waiting = false;
        let mut extra_route_sets = 0;
        for tick in 0..30 {
            // Re-send the same order every tick, exactly as a rules loop does.
            apply(
                &mut world,
                &[(creature, Decision::MoveTo(goal.column, goal.row))],
            );
            let events = world.tick_step();
            if events.iter().any(|e| matches!(e, Event::Waiting { .. })) {
                waiting = true;
            }
            // Skip the initial RouteSet carried by the very first tick.
            if tick > 0 {
                extra_route_sets += events
                    .iter()
                    .filter(|e| matches!(e, Event::RouteSet { .. }))
                    .count();
            }
        }
        assert!(waiting, "the creature must wait behind the held cell");
        assert_eq!(
            extra_route_sets, 0,
            "repeated orders must not re-search while the creature waits"
        );

        // Free the path: send the blocker to a far, reachable cell so it steps
        // off `first_step` and holds nothing on the creature's route.
        world.move_to(blocker, Cell::new(6, 6)).unwrap();
        let mut resumed = false;
        for _ in 0..40 {
            apply(
                &mut world,
                &[(creature, Decision::MoveTo(goal.column, goal.row))],
            );
            let events = world.tick_step();
            if events
                .iter()
                .any(|e| matches!(e, Event::StepFinished { creature: c, .. } if *c == creature))
            {
                resumed = true;
            }
        }
        assert!(
            resumed,
            "the creature must resume once the blocker steps off"
        );
        assert_eq!(
            world.cell_of(creature),
            Some(goal),
            "the creature should reach its goal after the hold clears"
        );
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
        assert!(matches!(result[0], Applied::Accepted { .. }));
        assert!(matches!(
            result[1],
            Applied::Failed(WorldError::UnknownCreature)
        ));
        assert!(matches!(result[2], Applied::Skipped));
    }
}
