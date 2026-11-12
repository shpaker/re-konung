//! Apply K1 creature [`Decision`]s through the movement [`World`].
//!
//! Import decisions from k1_game::rules and call this module to mutate the
//! world. Uses explicit `(CreatureId, Decision)` pairs so there is no index
//! mismatch between creatures and decisions.
//!
//! The module is also the engine side of what `k1-game`'s evaluation reads:
//! [`Ground`] over a [`World`] tells where a creature may stand right now, and
//! [`live`] hands the rules the live picture of the tick.
//!
//! See docs/rust.md, "Decision: separate evaluate from apply".

use super::movement::CreatureId;
use super::movement::grid::{Cell, Direction};
use super::movement::pace::Pace;
use super::movement::passage::Passage;
use super::movement::rng::Rng;
use super::movement::world::{World, WorldError};
use k1_game::rules::{Decision, Ground, Live, Random};

/// Where a creature may stand right now: the cell is inside the grid, the
/// passage supplier lets a creature stand on it, and nobody stands there.
///
/// A cell only reserved by a running step is not stood in and stays free:
/// the dispute over it is settled when the step lands, by the engine
/// (docs/rust.md, "Decision: who yields a cell"). Negative coordinates are
/// outside the fine-grid space and never free.
impl<P: Passage, K: Pace> Ground for World<P, K> {
    fn is_free(&self, cell: (i16, i16)) -> bool {
        let (Ok(column), Ok(row)) = (u32::try_from(cell.0), u32::try_from(cell.1)) else {
            return false;
        };
        let cell = Cell::new(column, row);
        self.grid().contains(cell)
            && self.passage().is_walkable(cell)
            && self.occupancy().occupant(cell).is_none()
    }
}

impl Random for Rng {
    fn below(&mut self, limit: u64) -> u64 {
        Rng::below(self, limit)
    }
}

/// The live picture at this tick, indexed like the given ids.
///
/// An id the world does not know is `None`, never an invented cell: the rules
/// skip a creature they cannot see, and a made-up position would decide squad
/// states from nowhere.
pub fn live<P: Passage, K: Pace>(
    world: &World<P, K>,
    creatures: &[CreatureId],
) -> Vec<Option<Live>> {
    creatures
        .iter()
        .map(|creature| {
            world.state(*creature).map(|state| Live {
                // A coordinate past `i16` folds to a negative cell no rule
                // can aim at; the K1 fine grid is far smaller than `i16`.
                cell: (
                    i16::try_from(state.cell.column).unwrap_or(-1),
                    i16::try_from(state.cell.row).unwrap_or(-1),
                ),
                direction: state.direction.map(|direction| direction.index() as u8),
                busy: state.moving || state.goal.is_some(),
            })
        })
        .collect()
}

/// A decision that the world accepted or rejected, per creature.
#[derive(Debug, Clone, Copy)]
pub enum Applied {
    /// The decision produced no world call: `Decision::None`, or a `Turn`
    /// whose byte lies outside the eight-way table. It does not stop a
    /// creature that is already walking toward an earlier goal — None means
    /// "no new order", not "cancel".
    Skipped,
    /// `move_to` accepted the order. `route_steps` is what the world reported:
    /// steps left on the route, or `0` when the creature is mid-step and the
    /// route will only be recomputed at the next cell border. Acceptance and a
    /// built route are distinct — `route_steps == 0` is accepted, not failed.
    Accepted { route_steps: usize },
    /// `move_to` rejected the order with this error.
    Failed(WorldError),
    /// `turn` accepted the new facing; it is readable from `World::state`.
    Turned,
    /// [`FailedOrderCooldown`] held the order back: the world was not called,
    /// so no route search was paid for.
    Held,
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
        .map(|(creature_id, decision)| run_one(world, *creature_id, decision))
        .collect()
}

/// One command through the world: the dispatch both [`apply`] and
/// [`FailedOrderCooldown::apply`] run.
fn run_one<P: Passage, K: Pace>(
    world: &mut World<P, K>,
    creature: CreatureId,
    decision: &Decision,
) -> Applied {
    match decision {
        Decision::None => Applied::Skipped,
        Decision::MoveTo(tx, ty) => match world.move_to(creature, Cell::new(*tx, *ty)) {
            Ok(route_steps) => Applied::Accepted { route_steps },
            Err(error) => Applied::Failed(error),
        },
        Decision::Turn(direction) => match Direction::ALL.get(usize::from(*direction)) {
            Some(direction) => match world.turn(creature, *direction) {
                Ok(()) => Applied::Turned,
                Err(error) => Applied::Failed(error),
            },
            // A byte outside the eight-way table is a malformed
            // decision; it reaches no world call.
            None => Applied::Skipped,
        },
    }
}

/// How soon a creature may be sent somewhere again after an order of its found
/// no route.
///
/// A rules loop re-decides every tick and picks a fresh cell each time, so a
/// creature with no reachable target would pay a route search per tick. The
/// world's own throttle covers a repeat of the *same* failed goal (#54), not a
/// different goal every tick; the window lives here rather than in the engine
/// so that a player's order is never delayed by it.
///
/// The memory belongs to one world: a caller that rebuilds the world builds a
/// cooldown with it. Reusing one across worlds neither panics nor holds
/// anything — the new world's counter is behind the recorded tick, and a window
/// from a world that no longer exists has nothing to protect.
#[derive(Debug, Clone)]
pub struct FailedOrderCooldown {
    window_ticks: u32,
    /// Tick of the last order that found no route, by creature number.
    failed_at: Vec<Option<u64>>,
}

impl FailedOrderCooldown {
    /// A window of `window_ticks`;
    /// [`WorldRules::repath_cooldown_ticks`](super::movement::world::WorldRules::repath_cooldown_ticks)
    /// is the number to start from, so one creature costs one search per window.
    pub fn new(window_ticks: u32) -> FailedOrderCooldown {
        FailedOrderCooldown {
            window_ticks,
            failed_at: Vec::new(),
        }
    }

    /// [`apply`], holding back the movement orders of creatures inside their
    /// window: those come back as [`Applied::Held`] and reach no search. A
    /// turn is cheap and is never held.
    pub fn apply<P: Passage, K: Pace>(
        &mut self,
        world: &mut World<P, K>,
        commands: &[(CreatureId, Decision)],
    ) -> Vec<Applied> {
        let tick = world.tick_number();
        commands
            .iter()
            .map(|(creature, decision)| {
                if matches!(decision, Decision::MoveTo(..)) && self.holding(*creature, tick) {
                    return Applied::Held;
                }
                let applied = run_one(world, *creature, decision);
                self.record(*creature, tick, &applied);
                applied
            })
            .collect()
    }

    fn holding(&self, creature: CreatureId, tick: u64) -> bool {
        let failed_at = self
            .failed_at
            .get(creature.raw() as usize)
            .copied()
            .flatten();
        // `checked_sub` is the reused-across-worlds case: the recorded tick is
        // then ahead of the counter and holds nothing.
        failed_at.is_some_and(|failed_at| {
            tick.checked_sub(failed_at)
                .is_some_and(|age| age < u64::from(self.window_ticks))
        })
    }

    /// Only a search that found no route starts the window; an accepted order
    /// ends it, so a creature that got through is free to be sent again.
    ///
    /// Nothing else touches the memory: an id the world does not know, or a
    /// decision that reached no world call, must not grow it — a large id would
    /// otherwise allocate a slot per number below it.
    fn record(&mut self, creature: CreatureId, tick: u64, applied: &Applied) {
        let recorded = match applied {
            Applied::Failed(WorldError::NoRoute) => Some(tick),
            Applied::Accepted { .. } => None,
            _ => return,
        };
        let index = creature.raw() as usize;
        if self.failed_at.len() <= index {
            self.failed_at.resize(index + 1, None);
        }
        self.failed_at[index] = recorded;
    }
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

    // ── the engine side of what the rules read ──

    #[test]
    fn ground_refuses_taken_impassable_and_outside_cells() {
        let mut world = test_world();
        world.add(Cell::new(3, 3)).unwrap();
        assert!(world.is_free((3, 2)), "an empty walkable cell is free");
        assert!(!world.is_free((3, 3)), "a cell someone stands in is taken");
        world.passage_mut().blocked.insert(Cell::new(5, 5));
        assert!(!world.is_free((5, 5)), "impassable is never free");
        assert!(!world.is_free((8, 0)), "outside the grid is never free");
        assert!(
            !world.is_free((-1, 4)),
            "negative coordinates are never free"
        );
        assert!(!world.is_free((4, -1)));
    }

    #[test]
    fn ground_treats_a_step_reservation_as_free() {
        let mut world = test_world();
        let creature = world.add(Cell::new(2, 4)).unwrap();
        world.move_to(creature, Cell::new(3, 4)).unwrap();
        world.tick_step();
        assert!(
            world.state(creature).unwrap().moving,
            "the step must have started"
        );
        assert!(
            !world.is_free((2, 4)),
            "the cell under a walking creature is stood in"
        );
        assert!(
            world.is_free((3, 4)),
            "a step reserves its destination but does not stand in it"
        );
    }

    #[test]
    fn live_maps_cell_direction_and_busy() {
        let mut world = test_world();
        let creature = world.add(Cell::new(1, 1)).unwrap();
        let picture = live(&world, &[creature]);
        assert_eq!(picture.len(), 1);
        let first = picture[0].expect("the world knows a creature it added");
        assert_eq!(first.cell, (1, 1));
        assert_eq!(first.direction, None, "nothing has turned or walked");
        assert!(!first.busy);
        world.turn(creature, Direction::NorthEast).unwrap();
        let picture = live(&world, &[creature]);
        assert_eq!(
            picture[0].unwrap().direction,
            Some(3),
            "the facing is reported as the original's direction byte"
        );
        world.move_to(creature, Cell::new(4, 1)).unwrap();
        world.tick_step();
        let picture = live(&world, &[creature]);
        assert!(
            picture[0].unwrap().busy,
            "a walking creature waits for arrival"
        );
    }

    /// An id the world does not know is absent, not a creature standing at an
    /// invented cell: a made-up `(0, 0)` would put it inside every spawn area
    /// that covers the origin and decide squad states from nowhere.
    #[test]
    fn live_reports_an_unknown_creature_as_absent() {
        let world = test_world();
        let picture = live(&world, &[CreatureId::new(99)]);
        assert_eq!(picture.len(), 1);
        assert!(picture[0].is_none(), "{picture:?}");
    }

    /// A rules loop re-rolls a target every tick, so a creature with no
    /// reachable cell would pay a route search per tick — the world's own
    /// throttle covers a repeat of the *same* failed goal only (#54). With the
    /// window, one search per window is the bound, and the creature is held
    /// the rest of the time without ever reaching the engine.
    #[test]
    fn a_cooldown_bounds_the_searches_of_an_isolated_creature() {
        const WINDOW: u32 = 10;
        const TICKS: u32 = 40;
        let mut world = test_world();
        // Wall the creature in: every neighbour of its cell is impassable.
        let start = Cell::new(4, 4);
        for direction in Direction::ALL {
            if let Some(neighbour) = start.neighbour(direction) {
                world.passage_mut().blocked.insert(neighbour);
            }
        }
        let creature = world.add(start).unwrap();
        let mut cooldown = FailedOrderCooldown::new(WINDOW);
        let mut searches = 0;
        let mut held = 0;
        for tick in 0..TICKS {
            // A different unreachable goal every tick, as a re-rolled walk is.
            let goal = Cell::new(1 + tick % 3, 1);
            let result = cooldown.apply(
                &mut world,
                &[(creature, Decision::MoveTo(goal.column, goal.row))],
            );
            match result[0] {
                Applied::Failed(WorldError::NoRoute) => searches += 1,
                Applied::Held => held += 1,
                other => panic!("an isolated creature must go nowhere: {other:?}"),
            }
            world.tick_step();
        }
        assert_eq!(
            searches,
            (TICKS / WINDOW) as usize,
            "one search per window, not one per tick"
        );
        assert_eq!(held, (TICKS - TICKS / WINDOW) as usize);
        assert_eq!(
            world.cell_of(creature),
            Some(start),
            "the creature never moved"
        );
    }

    /// A huge id the world does not know must not make the memory grow: it is
    /// refused before any search, so it records nothing — allocating a slot per
    /// number below it is what the guard is for.
    #[test]
    fn an_unknown_creature_does_not_grow_the_cooldown_memory() {
        let mut world = test_world();
        let mut cooldown = FailedOrderCooldown::new(10);
        let ghost = CreatureId::new(u32::MAX / 2);
        let result = cooldown.apply(&mut world, &[(ghost, Decision::MoveTo(1, 1))]);
        assert!(
            matches!(result[0], Applied::Failed(WorldError::UnknownCreature)),
            "{result:?}"
        );
        assert!(
            cooldown.failed_at.is_empty(),
            "a refusal is not a failed search: {:?}",
            cooldown.failed_at.len()
        );
        // Neither is a decision that reaches no world call.
        let creature = world.add(Cell::new(1, 1)).unwrap();
        cooldown.apply(&mut world, &[(creature, Decision::None)]);
        assert!(cooldown.failed_at.is_empty());
    }

    /// The window ends with the order that got through: the creature is free to
    /// be sent again at once, and the memory says so.
    #[test]
    fn an_accepted_order_ends_the_window() {
        let mut world = test_world();
        let mut cooldown = FailedOrderCooldown::new(3);
        let start = Cell::new(4, 4);
        for direction in Direction::ALL {
            if let Some(neighbour) = start.neighbour(direction) {
                world.passage_mut().blocked.insert(neighbour);
            }
        }
        let creature = world.add(start).unwrap();
        let index = creature.raw() as usize;
        let refused = cooldown.apply(&mut world, &[(creature, Decision::MoveTo(1, 1))]);
        assert!(
            matches!(refused[0], Applied::Failed(WorldError::NoRoute)),
            "{refused:?}"
        );
        assert_eq!(cooldown.failed_at[index], Some(0));

        // Inside the window another target is held and reaches no search.
        world.tick_step();
        let held = cooldown.apply(&mut world, &[(creature, Decision::MoveTo(2, 2))]);
        assert!(matches!(held[0], Applied::Held), "{held:?}");

        // Past the window the way is open: the order is accepted and the
        // creature is no longer held.
        world.tick_step();
        world.tick_step();
        world.passage_mut().blocked.clear();
        let accepted = cooldown.apply(&mut world, &[(creature, Decision::MoveTo(2, 2))]);
        assert!(
            matches!(accepted[0], Applied::Accepted { .. }),
            "{accepted:?}"
        );
        assert_eq!(cooldown.failed_at[index], None);
        assert!(
            !cooldown.holding(creature, world.tick_number()),
            "an accepted order must clear the window"
        );
    }

    /// A cooldown reused with a rebuilt world meets a counter that went back to
    /// zero: the recorded tick is ahead of it, so the subtraction must not panic
    /// and the stale window must hold nothing.
    #[test]
    fn a_rebuilt_world_holds_nothing_and_does_not_panic() {
        let mut cooldown = FailedOrderCooldown::new(10);
        let start = Cell::new(4, 4);
        let mut first = test_world();
        for direction in Direction::ALL {
            if let Some(neighbour) = start.neighbour(direction) {
                first.passage_mut().blocked.insert(neighbour);
            }
        }
        let creature = first.add(start).unwrap();
        for _ in 0..5 {
            first.tick_step();
        }
        let refused = cooldown.apply(&mut first, &[(creature, Decision::MoveTo(1, 1))]);
        assert!(
            matches!(refused[0], Applied::Failed(WorldError::NoRoute)),
            "{refused:?}"
        );
        assert_eq!(cooldown.failed_at[creature.raw() as usize], Some(5));

        // The same ids, a fresh counter — and an open map, so the order that
        // reaches the engine is accepted rather than held.
        let mut second = test_world();
        let again = second.add(start).unwrap();
        assert_eq!(
            again, creature,
            "the ids must coincide for the case to be real"
        );
        assert_eq!(second.tick_number(), 0);
        let accepted = cooldown.apply(&mut second, &[(again, Decision::MoveTo(6, 6))]);
        assert!(
            matches!(accepted[0], Applied::Accepted { .. }),
            "a window from a world that is gone must hold nothing: {accepted:?}"
        );
    }

    #[test]
    fn apply_turns_a_standing_creature() {
        let mut world = test_world();
        let creature = world.add(Cell::new(1, 1)).unwrap();
        let result = apply(&mut world, &[(creature, Decision::Turn(6))]);
        assert!(matches!(result[0], Applied::Turned), "{result:?}");
        assert_eq!(
            world.state(creature).unwrap().direction,
            Some(Direction::South)
        );
    }

    #[test]
    fn apply_reports_failed_when_turning_an_unknown_creature() {
        let mut world = test_world();
        let result = apply(&mut world, &[(CreatureId::new(99), Decision::Turn(0))]);
        assert!(
            matches!(result[0], Applied::Failed(WorldError::UnknownCreature)),
            "{result:?}"
        );
    }

    #[test]
    fn apply_skips_a_turn_byte_outside_the_direction_table() {
        let mut world = test_world();
        let creature = world.add(Cell::new(1, 1)).unwrap();
        let result = apply(&mut world, &[(creature, Decision::Turn(8))]);
        assert!(matches!(result[0], Applied::Skipped), "{result:?}");
        assert_eq!(world.state(creature).unwrap().direction, None);
    }

    /// The criterion loop wired together: every tick the rules evaluate over
    /// a live picture and the decisions go back through the world. A jobless
    /// villager walks on its half roll; the roaming squad member stands in
    /// its calm squad. Through dozens of decisions nobody may end up in an
    /// unwalkable cell or share a cell — the invariants the world keeps for
    /// itself, checked here from outside (issue #36).
    #[test]
    fn rules_and_the_world_run_together_for_dozens_of_ticks() {
        use k1_data::game_state::Rect;
        use k1_game::creatures::Creature;
        use k1_game::rules::{Context, class, evaluate, group, group_states};

        fn roster_creature(id: usize, group_kind: u8, class_id: u8, cell: (i16, i16)) -> Creature {
            Creature {
                id,
                cell,
                direction: 0,
                state: 6,
                class_id,
                group: id,
                group_kind,
                roaming: matches!(group_kind, group::FIRST_ROAMING..=group::LAST_ROAMING),
                spawn: Rect {
                    left: 0,
                    top: 0,
                    right: 7,
                    bottom: 7,
                },
                workplace: None,
                name_id: 0,
                nick_id: 0,
                sprite_set: 0,
            }
        }

        let mut world = test_world();
        let villager = world.add(Cell::new(1, 1)).unwrap();
        let monster = world.add(Cell::new(5, 5)).unwrap();
        let ids = [villager, monster];
        let roster = [
            roster_creature(1, group::VILLAGE_FIRST, 5, (1, 1)),
            roster_creature(2, group::FIRST_ROAMING, class::WARRIOR, (5, 5)),
        ];
        let mut random = Rng::new(0x5eed);
        let mut the_villager_walked = false;
        for _tick in 0..60 {
            let live = live(&world, &ids);
            let squad_states = group_states(&roster, &live);
            let decisions = {
                let mut context = Context {
                    ground: &world,
                    random: &mut random,
                    jobs: &[],
                    states: &squad_states,
                };
                evaluate(&roster, &live, &mut context)
            };
            let commands: Vec<(CreatureId, Decision)> =
                ids.iter().copied().zip(decisions).collect();
            let applied = apply(&mut world, &commands);
            // The calm squad member gets no order, and none is refused:
            // the wiring must not lose the villager's walk either.
            assert!(
                !matches!(applied[1], Applied::Accepted { .. }),
                "{applied:?}"
            );
            let events = world.tick_step();
            if events
                .iter()
                .any(|event| matches!(event, Event::StepStarted { creature, .. } if *creature == villager))
            {
                the_villager_walked = true;
            }
            let mut seen = HashSet::new();
            for id in ids {
                let state = world.state(id).unwrap();
                assert!(
                    world.grid().contains(state.cell),
                    "a creature left the grid: {state:?}"
                );
                assert!(
                    world.passage().is_walkable(state.cell),
                    "a creature stands in an unwalkable cell: {state:?}"
                );
                assert!(
                    seen.insert(state.cell),
                    "two creatures stand in the same cell {:?}",
                    state.cell
                );
            }
        }
        assert!(
            the_villager_walked,
            "the fixed seed never sent the villager anywhere — the test would check nothing"
        );
    }
}
