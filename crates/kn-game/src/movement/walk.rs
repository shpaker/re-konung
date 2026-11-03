//! Walking a route: the creature goes step by step, each step lasting a given
//! number of ticks.
//!
//! This code searches nothing and knows nothing of A\*, walkability or
//! occupancy — it consumes a finished route ([`super::route::Route`]) and
//! answers one question: which cell the creature stands in now and which one it
//! is stepping to (issue #35: "route search and walking a route are different
//! parts of the code").

use super::CreatureId;
use super::grid::{Cell, Direction};
use super::route::Route;

/// A creature's step: the cell it is entering and how many ticks that takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub to: Cell,
    pub direction: Direction,
    /// Step duration in ticks — exactly as many as the data supplier said.
    pub ticks: u32,
    /// Ticks of the step already spent, including the one it started on.
    pub elapsed: u32,
}

/// One creature's state: where it stands, where it is going, and which
/// animation it is busy with.
///
/// This is exactly what the outer world needs to put a sprite in a cell and
/// pick an animation; how a sprite is drawn is not the engine's business.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalkState {
    pub creature: CreatureId,
    /// The cell the creature stands in.
    pub cell: Cell,
    /// The cell it came from into this one: the viewer sorts sprites by it and
    /// finishes the glide.
    pub entered_from: Option<Cell>,
    /// Where the creature was told to go; `None` — it arrived, or moves with no
    /// goal.
    pub goal: Option<Cell>,
    /// The pixel point of the cell it stands in.
    pub pixel: (i32, i32),
    /// The direction it faces: it changes at the cell border, and a standing
    /// creature can be turned in place (`Walker::turn`).
    pub direction: Option<Direction>,
    /// Whether a step is in progress right now.
    pub moving: bool,
    /// The cell the step is entering.
    pub step_to: Option<Cell>,
    pub step_ticks: u32,
    pub step_elapsed: u32,
}

impl WalkState {
    /// Ticks left before the current step ends.
    pub fn ticks_left_in_step(&self) -> u32 {
        self.step_ticks.saturating_sub(self.step_elapsed)
    }
}

/// A creature walking a route.
///
/// The route is consumed one step at a time: a step lasts `ticks` ticks, then
/// the creature moves into the next cell. While stepping it still occupies the
/// old cell and reserves the new one; direction changes only at the cell border
/// (issue #35, criterion 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Walker {
    pub creature: CreatureId,
    cell: Cell,
    entered_from: Option<Cell>,
    goal: Option<Cell>,
    /// Steps left; the first one is where the creature is going now, or is
    /// about to go.
    route: Vec<Direction>,
    step: Option<Step>,
    /// The direction a standing creature was turned to face: it holds while the
    /// creature stands, is masked by a route that has not started stepping, and
    /// is consumed by a step.
    face: Option<Direction>,
}

impl Walker {
    /// A creature with no route, standing in a cell.
    pub fn new(creature: CreatureId, cell: Cell) -> Walker {
        Walker {
            creature,
            cell,
            entered_from: None,
            goal: None,
            route: Vec::new(),
            step: None,
            face: None,
        }
    }

    pub fn cell(&self) -> Cell {
        self.cell
    }

    pub fn entered_from(&self) -> Option<Cell> {
        self.entered_from
    }

    /// The cell the creature was told to reach (after routing around a taken
    /// goal, the route's actual last cell).
    pub fn goal(&self) -> Option<Cell> {
        self.goal
    }

    pub fn route_len(&self) -> usize {
        self.route.len()
    }

    pub fn is_walking(&self) -> bool {
        self.step.is_some()
    }

    /// Whether the creature is heading for its goal — mid-step or between
    /// steps.
    pub fn is_moving(&self) -> bool {
        !self.route.is_empty() || self.step.is_some()
    }

    /// Whether it stands still with a route it cannot start: the cell ahead is
    /// taken or has become impassable.
    pub fn is_waiting(&self) -> bool {
        self.step.is_none() && !self.route.is_empty()
    }

    /// The direction it faces: the step's direction while walking, otherwise
    /// the next step of the route, otherwise the in-place turn of `turn`, and
    /// after arriving with no turn on record the direction it came from.
    pub fn facing(&self) -> Option<Direction> {
        if let Some(step) = self.step {
            return Some(step.direction);
        }
        if let Some(&next) = self.route.first() {
            return Some(next);
        }
        if let Some(face) = self.face {
            return Some(face);
        }
        self.entered_from
            .and_then(|from| Direction::between(from, self.cell))
    }

    /// Turn in place: face `direction` while standing.
    ///
    /// A step that starts later consumes the turn, so after a walk the
    /// creature faces where it came from; while a route has not begun
    /// stepping, its first direction masks the face.
    pub fn turn(&mut self, direction: Direction) {
        self.face = Some(direction);
    }

    /// The cell the next step points at.
    pub fn next_cell(&self) -> Option<Cell> {
        let direction = *self.route.first()?;
        self.cell.neighbour(direction)
    }

    pub fn step(&self) -> Option<Step> {
        self.step
    }

    /// Ticks left before the current step ends.
    pub fn ticks_left_in_step(&self) -> u32 {
        self.step
            .map(|step| step.ticks.saturating_sub(step.elapsed))
            .unwrap_or(0)
    }

    /// Take a route: the creature walks it from the cell it is in now.
    ///
    /// The route replaces the old one whole, and only while the creature is not
    /// mid-step: a cell already given up cannot be taken back during a step, so
    /// a repath is waited for at the cell border (docs/rust.md, "Decision:
    /// repathing without thrash"). `false` — a step is running and the route
    /// stayed as it was.
    pub fn set_route(&mut self, route: &Route) -> bool {
        if self.step.is_some() {
            return false;
        }
        self.route = route.legs().iter().map(|leg| leg.direction).collect();
        self.goal = Some(route.goal());
        true
    }

    /// Drop the route; the creature stays where it stands.
    pub fn clear_route(&mut self) {
        self.route.clear();
        self.goal = None;
    }

    /// Start the next step, `ticks` ticks long.
    ///
    /// The creature has already reserved the destination cell — that is
    /// [`super::world`]'s job, here only time is accounted. `None` — nowhere to
    /// go, or a step is already running. A duration of 0 counts as one tick: a
    /// zero-length step does not exist, and data must not panic the engine.
    pub fn start_step(&mut self, ticks: u32) -> Option<Direction> {
        if self.step.is_some() {
            return None;
        }
        let direction = *self.route.first()?;
        let to = self.cell.neighbour(direction)?;
        self.step = Some(Step {
            to,
            direction,
            ticks: ticks.max(1),
            elapsed: 1,
        });
        // Walking consumes an in-place turn: after the arrival the creature
        // faces where it came from, and a face remembered from before the walk
        // must not resurface over the direction of travel.
        self.face = None;
        Some(direction)
    }

    /// Advance the creature by one tick. `true` — the step ended on this tick
    /// and the creature is in the new cell.
    ///
    /// A step `d` ticks long finishes on the `d`-th tick after it started:
    /// exactly `d` ticks are spent walking, and there is no pause between steps.
    pub fn tick(&mut self) -> bool {
        let Some(step) = self.step else {
            return false;
        };
        if step.elapsed < step.ticks {
            self.step = Some(Step {
                elapsed: step.elapsed + 1,
                ..step
            });
            return false;
        }
        self.step = None;
        // The route may have been dropped mid-step (`clear_route`): the step's
        // destination lives in the step, so consuming the route does not depend
        // on it.
        if !self.route.is_empty() {
            self.route.remove(0);
        }
        self.entered_from = Some(self.cell);
        self.cell = step.to;
        true
    }

    /// Interrupt a started step: the creature stays in the old cell and the
    /// route does not advance. This is how the world rolls back a step whose
    /// destination became impassable while the creature was walking.
    pub fn cancel_step(&mut self) -> Option<Step> {
        self.step.take()
    }

    /// Drop the goal once the route is spent entirely. `true` — the creature
    /// arrived.
    pub fn finish(&mut self) -> bool {
        if self.step.is_none() && self.route.is_empty() && self.goal.is_some() {
            self.goal = None;
            return true;
        }
        false
    }

    /// What the outside needs to place the sprite in a cell and pick an
    /// animation.
    pub fn state(&self) -> WalkState {
        WalkState {
            creature: self.creature,
            cell: self.cell,
            entered_from: self.entered_from,
            goal: self.goal,
            pixel: self.cell.centre(),
            direction: self.facing(),
            moving: self.step.is_some(),
            step_to: self.step.map(|step| step.to),
            step_ticks: self.step.map_or(0, |step| step.ticks),
            step_elapsed: self.step.map_or(0, |step| step.elapsed),
        }
    }

    /// The cells the creature holds: the one under it and the one its step
    /// reserves.
    pub fn held_cells(&self) -> Vec<Cell> {
        let mut cells = vec![self.cell];
        if let Some(step) = self.step {
            cells.push(step.to);
        }
        cells
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creature(id: u32) -> CreatureId {
        CreatureId::new(id)
    }

    /// A route from directions — the same shape search builds, minus the
    /// search.
    fn route_from(from: Cell, directions: &[Direction]) -> Route {
        let mut cells = vec![from];
        let mut current = from;
        for &direction in directions {
            current = current.neighbour(direction).unwrap();
            cells.push(current);
        }
        Route::from_cells(&cells, current).unwrap()
    }

    #[test]
    fn step_last_exactly_the_given_number_of_ticks() {
        let from = Cell::new(4, 4);
        for ticks in [1u32, 2, 5, 7] {
            let mut walker = Walker::new(creature(1), from);
            walker.set_route(&route_from(from, &[Direction::East]));
            walker.start_step(ticks).unwrap();
            let mut finished_at = None;
            for tick in 0..ticks + 3 {
                if walker.tick() {
                    finished_at = Some(tick);
                    break;
                }
            }
            assert_eq!(finished_at, Some(ticks - 1), "a step of {ticks} ticks");
            assert_eq!(walker.cell(), from.neighbour(Direction::East).unwrap());
        }
    }

    #[test]
    fn route_is_consumed_one_step_at_a_time() {
        let from = Cell::new(4, 4);
        let mut walker = Walker::new(creature(1), from);
        walker.set_route(&route_from(from, &[Direction::East, Direction::SouthEast]));
        let first = walker.next_cell().unwrap();
        walker.start_step(2).unwrap();
        assert_eq!(
            walker.next_cell(),
            Some(first),
            "the route advanced too early"
        );
        assert_eq!(walker.ticks_left_in_step(), 1);
        assert!(!walker.tick());
        assert!(walker.tick());
        assert_eq!(walker.cell(), first);
        assert_eq!(walker.route_len(), 1);
        assert_ne!(walker.next_cell(), Some(first));
        walker.start_step(1).unwrap();
        assert!(walker.tick());
        assert_eq!(walker.route_len(), 0);
        assert!(walker.finish());
        assert!(!walker.is_moving());
        assert_eq!(walker.goal(), None);
    }

    /// Direction changes at the cell border: the middle of a step leaves it
    /// alone.
    #[test]
    fn direction_changes_only_at_the_cell_border() {
        let from = Cell::new(6, 6);
        let mut walker = Walker::new(creature(1), from);
        walker.set_route(&route_from(from, &[Direction::East, Direction::SouthEast]));
        walker.start_step(4).unwrap();
        for _ in 0..3 {
            assert!(!walker.tick());
            assert_eq!(walker.facing(), Some(Direction::East));
            assert_eq!(
                walker.cell(),
                from,
                "the cell changed before the step ended"
            );
        }
        assert!(walker.tick());
        assert_eq!(walker.cell(), from.neighbour(Direction::East).unwrap());
        assert_eq!(
            walker.facing(),
            Some(Direction::SouthEast),
            "not turned to the next step"
        );
        assert_eq!(walker.ticks_left_in_step(), 0);
    }

    #[test]
    fn walker_stops_at_the_last_cell() {
        let from = Cell::new(2, 2);
        let mut walker = Walker::new(creature(1), from);
        walker.set_route(&route_from(from, &[Direction::East]));
        walker.start_step(1).unwrap();
        assert!(walker.tick());
        assert_eq!(
            walker.facing(),
            Some(Direction::East),
            "the turn persisted after arriving"
        );
        assert_eq!(walker.entered_from(), Some(from));
        assert_eq!(
            walker.held_cells(),
            vec![walker.cell()],
            "the reservation was not dropped"
        );
        assert!(!walker.is_moving());
        assert!(walker.next_cell().is_none());
        assert!(walker.start_step(3).is_none());
        assert!(!walker.tick(), "an idle tick after stopping");
        assert!(walker.finish());
        assert!(!walker.finish());
    }

    #[test]
    fn route_is_not_replaced_mid_step() {
        let from = Cell::new(2, 2);
        let mut walker = Walker::new(creature(1), from);
        walker.set_route(&route_from(from, &[Direction::East; 2]));
        walker.start_step(3).unwrap();
        let other = route_from(
            from.neighbour(Direction::East).unwrap(),
            &[Direction::North],
        );
        assert!(!walker.set_route(&other), "the route was replaced mid-step");
        assert!(!walker.is_waiting());
        assert_eq!(walker.facing(), Some(Direction::East));
    }

    #[test]
    fn state_reports_enough_for_a_sprite() {
        let from = Cell::new(3, 4);
        let mut walker = Walker::new(creature(1), from);
        walker.set_route(&route_from(from, &[Direction::SouthWest]));
        walker.start_step(5).unwrap();
        walker.tick();
        let state = walker.state();
        assert_eq!(state.cell, from);
        assert_eq!(state.pixel, from.centre());
        assert_eq!(state.direction, Some(Direction::SouthWest));
        assert!(state.moving);
        assert_eq!(state.step_to, from.neighbour(Direction::SouthWest));
        assert_eq!((state.step_ticks, state.step_elapsed), (5, 2));
        assert_eq!(state.ticks_left_in_step(), 3);
        assert_eq!(state.goal, from.neighbour(Direction::SouthWest));
        assert_eq!(state.entered_from, None);
    }

    #[test]
    fn zero_tick_step_is_one_tick() {
        let from = Cell::new(1, 1);
        let mut walker = Walker::new(creature(1), from);
        walker.set_route(&route_from(from, &[Direction::West]));
        walker.start_step(0).unwrap();
        assert!(walker.tick());
        assert_eq!(walker.cell(), from.neighbour(Direction::West).unwrap());
    }

    #[test]
    fn waiting_needs_a_route_and_no_step() {
        let from = Cell::new(5, 5);
        let mut walker = Walker::new(creature(1), from);
        assert!(!walker.is_waiting());
        walker.set_route(&route_from(from, &[Direction::North; 2]));
        assert!(walker.is_waiting(), "a route but no step means waiting");
        walker.start_step(2).unwrap();
        assert!(!walker.is_waiting(), "a creature mid-step is not waiting");
    }

    /// A turn in place survives standing still: nothing clears the face while
    /// the creature neither steps nor holds a route.
    #[test]
    fn turn_survives_standing_still() {
        let mut walker = Walker::new(creature(1), Cell::new(4, 4));
        assert_eq!(walker.facing(), None);
        walker.turn(Direction::North);
        assert_eq!(walker.facing(), Some(Direction::North));
        for _ in 0..4 {
            assert!(!walker.tick());
            assert_eq!(walker.facing(), Some(Direction::North));
        }
    }

    /// A route masks the face while it has not started stepping, and the step
    /// consumes it: after the arrival the creature faces where it came from,
    /// not the direction it was turned to before the walk.
    #[test]
    fn a_step_consumes_the_in_place_turn() {
        let from = Cell::new(4, 4);
        let mut walker = Walker::new(creature(1), from);
        walker.turn(Direction::North);
        walker.set_route(&route_from(from, &[Direction::East]));
        assert_eq!(
            walker.facing(),
            Some(Direction::East),
            "the route's first direction must win over the face"
        );
        walker.start_step(1).unwrap();
        assert_eq!(
            walker.facing(),
            Some(Direction::East),
            "the step's direction must win over the face"
        );
        assert!(walker.tick());
        assert_eq!(
            walker.facing(),
            Some(Direction::East),
            "the walk must consume the turn, so the arrival faces its own direction"
        );
    }
}
