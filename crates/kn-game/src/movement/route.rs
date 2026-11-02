//! Route search: cell to cell over walkable and free cells.
//!
//! Search only. Walking a finished route is [`super::walk`], and the two must
//! not be mixed (issue #35): search has neither a notion of a tick nor a step
//! duration, and walking has no idea how its route was found.
//!
//! The route comes from an off-the-shelf A\* (`pathfinding::astar`); the
//! original's algorithm is not ported and we have no 32-step limit — a
//! deliberate departure from the original (docs/rust.md, "Decision: routes
//! without the 32-step limit").

use std::collections::HashSet;

use pathfinding::prelude::astar;

use super::CreatureId;
use super::grid::{Cell, Direction, Grid};
use super::occupancy::Occupancy;
use super::passage::Passage;
use super::rng::Rng;

/// Route-search rules: numbers the engine must not invent about game rules,
/// yet has to pick itself.
#[derive(Debug, Clone, Copy)]
pub struct RouteRules {
    /// How far from a taken goal to look for a free cell, in lattice cells
    /// ([`Cell::lattice_distance`]).
    ///
    /// With the goal taken, the creature goes to the **nearest** free cell
    /// beside it, not to the one cheapest to reach: search walks the rings of
    /// the neighbourhood outward from the centre and stops at the first ring it
    /// can route to. The cap exists so a taken goal in a dead end does not
    /// force a sweep of the whole map: at most `radius` rings, and each ring is
    /// a separate search (a free goal is one search). `0` means "the goal cell
    /// only": a goal another creature stands on then has no route at all.
    pub goal_fallback_radius: u32,
}

impl Default for RouteRules {
    fn default() -> RouteRules {
        RouteRules {
            goal_fallback_radius: 4,
        }
    }
}

/// One step of a route: the cell entered and the direction of the step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Leg {
    pub cell: Cell,
    pub direction: Direction,
}

/// A found route: the steps from the current cell to the goal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    legs: Vec<Leg>,
    goal: Cell,
}

impl Route {
    /// The route's steps, in order.
    pub fn legs(&self) -> &[Leg] {
        &self.legs
    }

    /// The route's cells, without directions.
    pub fn cells(&self) -> Vec<Cell> {
        self.legs.iter().map(|leg| leg.cell).collect()
    }

    /// The cell the route ends at: the goal that was asked for or — if it was
    /// taken or impassable — the nearest free cell beside it.
    pub fn goal(&self) -> Cell {
        self.goal
    }

    pub fn len(&self) -> usize {
        self.legs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.legs.is_empty()
    }

    /// Route cost in [`Direction::cost`] units.
    pub fn cost(&self) -> u32 {
        self.legs.iter().map(|leg| leg.direction.cost()).sum()
    }

    pub(crate) fn from_cells(cells: &[Cell], goal: Cell) -> Option<Route> {
        let mut legs = Vec::with_capacity(cells.len().saturating_sub(1));
        for window in cells.windows(2) {
            let (from, to) = (window[0], window[1]);
            let direction = Direction::between(from, to)?;
            legs.push(Leg {
                cell: to,
                direction,
            });
        }
        Some(Route { legs, goal })
    }
}

/// A cell may be entered: inside the grid, walkable per the supplier's data,
/// and not held by another creature.
///
/// The mover's own cells do not block it: they are given up for the duration of
/// the search, as in the original (Slavik `game.cpp:2889-2895`).
pub(super) fn is_enterable<P: Passage>(
    grid: &Grid,
    passage: &P,
    occupancy: &Occupancy,
    mover: CreatureId,
    cell: Cell,
) -> bool {
    grid.contains(cell) && passage.is_walkable(cell) && !occupancy.blocks(cell, mover)
}

/// A step is allowed when both cells alongside its corner are walkable.
///
/// Flank occupancy is not this rule's business: corner cutting is a walkability
/// rule, while another creature in a flank cell only keeps the destination cell
/// from being entered, which [`is_enterable`] checks.
pub(super) fn is_open_corner<P: Passage>(
    grid: &Grid,
    passage: &P,
    from: Cell,
    direction: Direction,
) -> bool {
    let Some(flanks) = direction.corner_flanks() else {
        return true;
    };
    flanks.into_iter().all(|flank| {
        from.neighbour(flank)
            .is_some_and(|cell| grid.contains(cell) && passage.is_walkable(cell))
    })
}

/// Everything search needs: the world being walked, and who is going where.
pub struct RouteRequest<'a, P: Passage> {
    pub grid: &'a Grid,
    pub passage: &'a P,
    pub occupancy: &'a Occupancy,
    pub rules: &'a RouteRules,
    /// The creature the route is searched for: its own cells do not block it.
    pub mover: CreatureId,
    pub from: Cell,
    pub goal: Cell,
}

impl<P: Passage> RouteRequest<'_, P> {
    fn enterable(&self, cell: Cell) -> bool {
        is_enterable(self.grid, self.passage, self.occupancy, self.mover, cell)
    }

    /// The next cell of a step and its cost, or `None` when the step is
    /// forbidden.
    fn step(&self, from: Cell, direction: Direction) -> Option<(Cell, u32)> {
        let to = self.grid.neighbour(from, direction)?;
        if !self.enterable(to) || !is_open_corner(self.grid, self.passage, from, direction) {
            return None;
        }
        Some((to, direction.cost()))
    }
}

/// Finds the shortest route from `from` to `goal`.
///
/// `None` means there is no route: the goal is unreachable, walled in, or
/// outside the grid. That is a value, not a panic — a creature without a route
/// stays where it is.
///
/// A taken or impassable goal is approached through the rings of its
/// neighbourhood, centre outward, so the creature walks to the nearest free
/// cell beside it rather than the cheapest one to reach
/// ([`RouteRules::goal_fallback_radius`]).
///
/// The order directions are tried in is rotated by a value taken from `rng`, so
/// a choice between routes of equal cost depends on the seed and replays the
/// same way under the same seed (issue #35, criterion 6).
pub fn find_route<P: Passage>(request: &RouteRequest<'_, P>, rng: &mut Rng) -> Option<Route> {
    let grid = *request.grid;
    if !grid.contains(request.from) || !grid.contains(request.goal) {
        return None;
    }
    let turn = rng.below(8) as usize;
    let (u, v) = request.goal.lattice();
    // Rings apply only to a goal that cannot be entered: taken or impassable. A
    // free but unreachable goal stays unreachable — otherwise the creature
    // would stand beside it and report `Arrived` instead of `NoRoute`
    // (docs/rust.md, "Decision: an occupied goal takes the nearest ring").
    let rings = if request.enterable(request.goal) {
        0
    } else {
        request.rules.goal_fallback_radius
    };
    for radius in 0..=i64::from(rings) {
        let candidates: Vec<Cell> = ring(u, v, radius)
            .into_iter()
            .filter_map(|(u, v)| Cell::from_lattice(u, v))
            .filter(|cell| request.enterable(*cell))
            .collect();
        if candidates.is_empty() {
            continue;
        }
        if let Some(route) = astar_to(request, &candidates, turn) {
            return Some(route);
        }
    }
    None
}

/// A* to one ring of cells around the goal: the ring is both the success test
/// and the anchor of the estimate.
fn astar_to<P: Passage>(
    request: &RouteRequest<'_, P>,
    candidates: &[Cell],
    turn: usize,
) -> Option<Route> {
    let chosen: HashSet<Cell> = candidates.iter().copied().collect();
    let from = request.from;
    // One goal cell: the estimate is exactly to it. A ring of several cells:
    // the estimate is to the ring's centre minus the ring's longest offset,
    // or it would overestimate the way to the farthest cell (docs/rust.md,
    // "Decision: step cost and the A* heuristic").
    let anchor = if candidates.len() == 1 {
        candidates[0]
    } else {
        request.goal
    };
    let slack = if anchor == request.goal {
        candidates
            .iter()
            .map(|cell| request.goal.cost_distance(*cell))
            .max()
            .unwrap_or(0)
    } else {
        0
    };

    let successors = |node: &Cell| {
        let from = *node;
        let mut steps: [Option<(Cell, u32)>; 8] = [None; 8];
        for (index, slot) in steps.iter_mut().enumerate() {
            let direction = Direction::ALL[(turn + index) % 8];
            *slot = request.step(from, direction);
        }
        steps.into_iter().flatten()
    };
    let heuristic = |node: &Cell| node.cost_distance(anchor).saturating_sub(slack);

    let (cells, _cost) = astar(&from, successors, heuristic, |node| chosen.contains(node))?;
    let last = *cells.last()?;
    Route::from_cells(&cells, last)
}

/// Lattice cells at distance `radius` from `(u, v)`: every offset `(a, b)` with
/// `max(|a|, |b|) == radius`. A zero ring is the cell itself.
fn ring(u: i64, v: i64, radius: i64) -> Vec<(i64, i64)> {
    if radius == 0 {
        return vec![(u, v)];
    }
    let mut cells = Vec::with_capacity((8 * radius) as usize);
    for a in -radius..=radius {
        for b in -radius..=radius {
            if a.abs().max(b.abs()) == radius {
                cells.push((u + a, v + b));
            }
        }
    }
    cells
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    const MOVER: CreatureId = CreatureId::new(1);
    const OTHER: CreatureId = CreatureId::new(2);

    #[derive(Default)]
    struct Map {
        blocked: HashSet<Cell>,
    }

    impl Passage for Map {
        fn is_walkable(&self, cell: Cell) -> bool {
            !self.blocked.contains(&cell)
        }
    }

    fn request<'a>(
        grid: &'a Grid,
        map: &'a Map,
        occupancy: &'a Occupancy,
        rules: &'a RouteRules,
        from: Cell,
        goal: Cell,
    ) -> RouteRequest<'a, Map> {
        RouteRequest {
            grid,
            passage: map,
            occupancy,
            rules,
            mover: MOVER,
            from,
            goal,
        }
    }

    fn search(
        grid: &Grid,
        map: &Map,
        occupancy: &Occupancy,
        from: Cell,
        goal: Cell,
    ) -> Option<Route> {
        let rules = RouteRules::default();
        search_with(grid, map, occupancy, from, goal, &rules)
    }

    fn search_with(
        grid: &Grid,
        map: &Map,
        occupancy: &Occupancy,
        from: Cell,
        goal: Cell,
        rules: &RouteRules,
    ) -> Option<Route> {
        let request = request(grid, map, occupancy, rules, from, goal);
        find_route(&request, &mut Rng::new(0))
    }

    fn search_seed(
        grid: &Grid,
        map: &Map,
        occupancy: &Occupancy,
        from: Cell,
        goal: Cell,
        seed: u64,
    ) -> Option<Route> {
        let rules = RouteRules::default();
        let request = request(grid, map, occupancy, &rules, from, goal);
        find_route(&request, &mut Rng::new(seed))
    }

    fn every_step_respects_corners(grid: &Grid, map: &Map, route: &Route) {
        let mut previous = None;
        for leg in route.legs() {
            if let Some(from) = previous {
                assert!(
                    is_open_corner(grid, map, from, leg.direction),
                    "step {from:?} → {:?} cuts a corner",
                    leg.cell
                );
            }
            previous = Some(leg.cell);
        }
    }

    #[test]
    fn straight_route_goes_straight() {
        let grid = Grid::new(24, 24);
        let map = Map::default();
        let route = search(
            &grid,
            &map,
            &Occupancy::new(),
            Cell::new(4, 10),
            Cell::new(14, 10),
        )
        .expect("a straight route");
        assert_eq!(route.len(), 10);
        assert!(
            route
                .legs()
                .iter()
                .all(|leg| leg.direction == Direction::East),
            "east is cheaper than a zigzag of diagonals: {:?}",
            route.cells()
        );
        assert_eq!(route.cost(), 10 * Direction::East.cost());
        every_step_respects_corners(&grid, &map, &route);
    }

    #[test]
    fn route_goes_around_an_obstacle() {
        let grid = Grid::new(24, 24);
        let mut map = Map::default();
        for column in 11..=13 {
            for row in 8..=14 {
                map.blocked.insert(Cell::new(column, row));
            }
        }
        let from = Cell::new(10, 10);
        let goal = Cell::new(14, 10);
        let route = search(&grid, &map, &Occupancy::new(), from, goal).expect("a detour route");
        let cells = route.cells();
        assert!(
            cells.iter().all(|cell| !map.blocked.contains(cell)),
            "the route runs over impassable cells: {cells:?}"
        );
        assert!(
            cells.iter().any(|cell| cell.row < 8 || cell.row > 14),
            "did not go around the obstacle: {cells:?}"
        );
        assert!(
            route.len() > 4,
            "the detour must be longer than the straight line: {cells:?}"
        );
        every_step_respects_corners(&grid, &map, &route);
    }

    #[test]
    fn corner_cutting_is_forbidden() {
        let grid = Grid::new(24, 24);
        let mut map = Map::default();
        let from = Cell::new(10, 10);
        let goal = Cell::new(11, 10);
        // The only short step is east, through the rhombus vertex; one flank
        // cell is closed, so the way in goes around.
        map.blocked
            .insert(from.neighbour(Direction::NorthEast).unwrap());
        let route = search(&grid, &map, &Occupancy::new(), from, goal).expect("a detour route");
        assert!(route.len() > 1, "cut a corner: {:?}", route.cells());
        every_step_respects_corners(&grid, &map, &route);
    }

    #[test]
    fn corner_blocked_on_both_sides_leaves_no_way_in() {
        let grid = Grid::new(8, 8);
        let from = Cell::new(3, 4);
        let goal = Cell::new(4, 4);
        let mut map = Map::default();
        // Every neighbour of the goal is closed except the one we stand on: the
        // only way in is a step east, and that step goes through a corner.
        for direction in Direction::ALL {
            if let Some(cell) = goal.neighbour(direction)
                && cell != from
            {
                map.blocked.insert(cell);
            }
        }
        let rules = RouteRules {
            goal_fallback_radius: 0,
        };
        let find_in = |map: &Map| search_with(&grid, map, &Occupancy::new(), from, goal, &rules);
        assert_eq!(find_in(&map), None, "cut through the corner");
        // Open both flanks and that very same step becomes possible.
        let mut open = Map::default();
        for direction in [Direction::NorthEast, Direction::SouthEast] {
            let cell = from.neighbour(direction).unwrap();
            open.blocked.remove(&cell);
        }
        let route = find_in(&open).expect("a route through the open vertex");
        assert_eq!(route.len(), 1);
        assert_eq!(route.legs()[0].direction, Direction::East);
    }

    #[test]
    fn route_is_not_limited_to_32_steps() {
        let grid = Grid::fine();
        let map = Map::default();
        let route = search(
            &grid,
            &map,
            &Occupancy::new(),
            Cell::new(1, 10),
            Cell::new(121, 10),
        )
        .expect("a route across half the map");
        assert_eq!(route.len(), 120);
        assert!(
            route.len() > 32,
            "the original's limit came back: {:?}",
            route.len()
        );
    }

    #[test]
    fn unreachable_goal_is_no_route_not_a_panic() {
        let grid = Grid::new(24, 24);
        let mut map = Map::default();
        for row in 0..24 {
            map.blocked.insert(Cell::new(12, row));
        }
        let rules = RouteRules {
            goal_fallback_radius: 1,
        };
        assert_eq!(
            search_with(
                &grid,
                &map,
                &Occupancy::new(),
                Cell::new(4, 12),
                Cell::new(20, 12),
                &rules,
            ),
            None
        );
        // A goal outside the grid is no route either.
        assert_eq!(
            search(
                &grid,
                &map,
                &Occupancy::new(),
                Cell::new(4, 4),
                Cell::new(40, 4)
            ),
            None
        );
    }

    #[test]
    fn blocked_goal_falls_back_to_a_free_cell_nearby() {
        let grid = Grid::new(24, 24);
        let map = Map::default();
        let goal = Cell::new(12, 12);
        let mut occupancy = Occupancy::new();
        occupancy.stand(goal, OTHER);
        let route = search(&grid, &map, &occupancy, Cell::new(4, 12), goal)
            .expect("a route to a taken goal");
        assert_ne!(route.goal(), goal);
        assert_eq!(route.cells().last(), Some(&route.goal()));
        assert_eq!(
            goal.lattice_distance(route.goal()),
            1,
            "the nearest free cell is not on the first ring: {:?}",
            route.goal()
        );
        assert!(!occupancy.blocks(route.goal(), MOVER));
        every_step_respects_corners(&grid, &map, &route);
    }

    #[test]
    fn a_free_but_unreachable_goal_does_not_take_a_ring_cell() {
        // The goal is walkable and free, but all eight neighbours are wall: it
        // cannot be entered, and a neighbouring cell in its place is not the
        // answer to "go to the goal" — it is a substituted goal. The rings
        // apply only to a taken or impassable goal (docs/rust.md, "Decision: an
        // occupied goal takes the nearest ring"); here there is no answer.
        let grid = Grid::new(24, 24);
        let mut map = Map::default();
        let goal = Cell::new(12, 12);
        for direction in Direction::ALL {
            map.blocked.insert(goal.neighbour(direction).unwrap());
        }
        assert!(map.is_walkable(goal), "the goal itself stays free");
        for (index, from) in [Cell::new(4, 12), Cell::new(20, 2), Cell::new(12, 3)]
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                search(&grid, &map, &Occupancy::new(), from, goal),
                None,
                "case {index}: a neighbour was substituted for the unreachable goal"
            );
        }
    }

    #[test]
    fn occupied_goal_with_no_free_cell_nearby_has_no_route() {
        let grid = Grid::new(24, 24);
        let map = Map::default();
        let goal = Cell::new(12, 12);
        let mut occupancy = Occupancy::new();
        for row in 6..19 {
            for column in 6..19 {
                occupancy.stand(Cell::new(column, row), OTHER);
            }
        }
        let rules = RouteRules {
            goal_fallback_radius: 2,
        };
        assert_eq!(
            search_with(&grid, &map, &occupancy, Cell::new(2, 2), goal, &rules,),
            None
        );
    }

    #[test]
    fn standing_on_the_goal_is_an_empty_route() {
        let grid = Grid::new(8, 8);
        let map = Map::default();
        let cell = Cell::new(3, 3);
        let route = search(&grid, &map, &Occupancy::new(), cell, cell).unwrap();
        assert!(route.is_empty());
        assert_eq!(route.goal(), cell);
    }

    #[test]
    fn own_cell_does_not_block_the_search() {
        let grid = Grid::new(16, 16);
        let map = Map::default();
        let from = Cell::new(8, 8);
        let mut occupancy = Occupancy::new();
        occupancy.stand(from, MOVER);
        assert!(
            search(&grid, &map, &occupancy, from, Cell::new(12, 4)).is_some(),
            "the creature's own cell got in the way of the search"
        );
    }

    #[test]
    fn route_cost_stays_close_to_a_straight_line() {
        let grid = Grid::new(40, 40);
        let map = Map::default();
        let from = Cell::new(2, 30);
        let goal = Cell::new(30, 4);
        let route = search(&grid, &map, &Occupancy::new(), from, goal).unwrap();
        let straight = from.cost_distance(goal);
        assert!(
            route.cost() >= straight,
            "cost {:?} is cheaper than a straight line {:?}",
            route.cost(),
            straight
        );
        // The grid does not allow an exact straight line, but this is no detour
        // around the field either.
        assert!(
            route.cost() <= straight + 2 * Direction::East.cost(),
            "route strays from the straight line: {:?} against {:?}",
            route.cost(),
            straight
        );
        every_step_respects_corners(&grid, &map, &route);
    }

    #[test]
    fn ring_enumerates_cells_by_distance() {
        assert_eq!(ring(0, 0, 0), vec![(0, 0)]);
        assert_eq!(ring(0, 0, 1).len(), 8);
        assert_eq!(ring(0, 0, 2).len(), 16);
        assert!(ring(0, 0, 1).contains(&(1, 1)));
        assert!(!ring(0, 0, 2).contains(&(0, 0)));
        assert_eq!(
            Cell::from_lattice(1, 1)
                .unwrap()
                .lattice_distance(Cell::new(0, 0)),
            1
        );
    }

    /// Cheapest cost to a cell without a heuristic — plain Dijkstra over the
    /// same steps. It is the independent check that the estimate does not
    /// overestimate.
    fn optimal_cost(
        grid: &Grid,
        map: &Map,
        occupancy: &Occupancy,
        from: Cell,
        to: Cell,
    ) -> Option<u32> {
        let rules = RouteRules::default();
        let request = request(grid, map, occupancy, &rules, from, to);
        pathfinding::prelude::dijkstra(
            &from,
            |node: &Cell| {
                Direction::ALL
                    .into_iter()
                    .filter_map(|direction| request.step(*node, direction))
                    .collect::<Vec<_>>()
            },
            |node: &Cell| *node == to,
        )
        .map(|(_path, cost)| cost)
    }

    /// The estimate must look at the cell the route actually ends at, not only
    /// at the goal that was asked for: otherwise it overestimates and A\*
    /// returns a route longer than the shortest. The check is the found route's
    /// cost against the same path costed without a heuristic.
    #[test]
    fn route_never_costs_more_than_the_shortest_path() {
        let grid = Grid::new(24, 24);
        let open = Map::default();
        let mut walls = Map::default();
        for column in 0..24 {
            for row in 0..24 {
                if column > 2 && (column + row) % 9 == 0 {
                    walls.blocked.insert(Cell::new(column, row));
                }
            }
        }
        let mut field = Map::default();
        // A wall of six columns with one wide gap: the route has to find it.
        for column in 8..14 {
            for row in 0..24 {
                if !(4..=8).contains(&row) {
                    field.blocked.insert(Cell::new(column, row));
                }
            }
        }
        // Pocket: exactly one free spot around the goal, so the candidate ring
        // is a single cell and the estimate must look at it, not at the taken
        // centre.
        let mut pocket = Map::default();
        let goal = Cell::new(12, 12);
        let keep = goal.neighbour(Direction::East).unwrap();
        for direction in Direction::ALL {
            let cell = goal.neighbour(direction).unwrap();
            if cell != keep {
                pocket.blocked.insert(cell);
            }
        }
        // A pocket with no free cell on the first ring: candidates only on the
        // second, and the estimate must again look at the ring, not at the
        // centre.
        let mut wide = Map::default();
        let wide_goal = Cell::new(12, 12);
        for direction in Direction::ALL {
            wide.blocked.insert(wide_goal.neighbour(direction).unwrap());
        }
        // A goal blocked by the map itself rather than by a creature: the
        // fallback has to work here too, and its candidate ring is the first
        // ring around a map cell.
        let impassable_goal = Cell::new(3, 3);
        wide.blocked.insert(impassable_goal);
        let mut occupancy = Occupancy::new();
        occupancy.stand(goal, OTHER);
        let mut wide_occupancy = Occupancy::new();
        wide_occupancy.stand(wide_goal, OTHER);

        let cases = [
            (&open, Occupancy::new(), Cell::new(2, 2), Cell::new(20, 20)),
            (&open, Occupancy::new(), Cell::new(20, 2), Cell::new(2, 20)),
            (&walls, Occupancy::new(), Cell::new(1, 1), Cell::new(22, 21)),
            (
                &walls,
                Occupancy::new(),
                Cell::new(11, 1),
                Cell::new(11, 22),
            ),
            (
                &field,
                Occupancy::new(),
                Cell::new(1, 12),
                Cell::new(22, 12),
            ),
            (&field, Occupancy::new(), Cell::new(22, 1), Cell::new(1, 21)),
            // The goal is impassable per the map data — route to a free cell
            // beside it.
            (&wide, Occupancy::new(), Cell::new(2, 12), impassable_goal),
            (&pocket, occupancy.clone(), Cell::new(2, 12), goal),
            (&pocket, occupancy.clone(), Cell::new(12, 2), goal),
            (&pocket, occupancy, Cell::new(20, 20), goal),
            (&wide, wide_occupancy, Cell::new(20, 2), wide_goal),
        ];
        for (index, (map, places, from, target)) in cases.iter().enumerate() {
            let route = search(&grid, map, places, *from, *target)
                .unwrap_or_else(|| panic!("case {index}: no route"));
            let optimum = optimal_cost(&grid, map, places, *from, route.goal())
                .unwrap_or_else(|| panic!("case {index}: no path without a heuristic"));
            assert_eq!(
                route.cost(),
                optimum,
                "case {index}: the route to {:?} costs more than the shortest: {:?} against {:?}",
                route.goal(),
                route.cost(),
                optimum
            );
            every_step_respects_corners(&grid, map, &route);
        }
    }

    /// The same control over random fields: if the estimate overestimated
    /// anywhere, one of the fields would make A\* return a route costing more
    /// than the shortest. The fields are generated deterministically so the run
    /// repeats.
    #[test]
    fn heuristic_is_admissible_on_random_fields() {
        let grid = Grid::new(20, 20);
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut checked = 0;
        for _case in 0..60 {
            let mut map = Map::default();
            for row in 0..grid.rows {
                for column in 0..grid.columns {
                    if next() % 100 < 25 {
                        map.blocked.insert(Cell::new(column, row));
                    }
                }
            }
            let free: Vec<Cell> = (0..grid.rows)
                .flat_map(|row| (0..grid.columns).map(move |column| Cell::new(column, row)))
                .filter(|cell| !map.blocked.contains(cell))
                .collect();
            if free.len() < 4 {
                continue;
            }
            let from = free[(next() % free.len() as u64) as usize];
            let target = free[(next() % free.len() as u64) as usize];
            if target == from {
                continue;
            }
            let mut occupancy = Occupancy::new();
            // Half the cases have a taken goal: a candidate ring instead of the
            // centre.
            if next() % 2 == 0 {
                occupancy.stand(target, OTHER);
            }
            let Some(route) = search(&grid, &map, &occupancy, from, target) else {
                continue;
            };
            let optimum = optimal_cost(&grid, &map, &occupancy, from, route.goal());
            assert_eq!(
                Some(route.cost()),
                optimum,
                "the estimate overestimates: {from:?} → {:?} on the field with {target:?}",
                route.goal()
            );
            every_step_respects_corners(&grid, &map, &route);
            checked += 1;
        }
        assert!(checked > 10, "too few cases produced a route: {checked}");
    }

    #[test]
    fn seeds_break_ties_deterministically() {
        let grid = Grid::new(24, 24);
        let mut map = Map::default();
        // A wall across the field with two symmetric gaps: exactly two routes,
        // and they cost the same.
        for column in 0..24 {
            if column != 6 && column != 18 {
                map.blocked.insert(Cell::new(column, 12));
            }
        }
        let from = Cell::new(12, 8);
        let goal = Cell::new(12, 16);
        let route_with = |seed: u64| {
            search_seed(&grid, &map, &Occupancy::new(), from, goal, seed).map(|route| route.cells())
        };
        let mut distinct = HashSet::new();
        for seed in 0..12 {
            let route = route_with(seed).expect("a route through a gap");
            assert_eq!(
                route,
                route_with(seed).unwrap(),
                "seed {seed} is not stable"
            );
            distinct.insert(route);
        }
        assert!(
            distinct.len() > 1,
            "the seed changes nothing: every route is the same"
        );
    }
}
