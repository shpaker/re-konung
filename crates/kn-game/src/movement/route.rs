//! Поиск маршрута: от клетки до клетки по проходимым и свободным клеткам.
//!
//! Здесь только поиск. Движение по найденному маршруту — [`super::walk`], и
//! смешивать их нельзя (issue #35): у поиска нет ни понятия тика, ни длительности
//! шага, а у движения нет знания о том, как маршрут искался.
//!
//! Маршрут ищется готовым A\* (`pathfinding::astar`); оригинальный алгоритм не
//! портируется, и лимита в 32 шага у нас нет — это осознанное отступление от
//! оригинала (docs/rust.md, «Решение: маршрут без лимита в 32 шага»).

use std::collections::HashSet;

use pathfinding::prelude::astar;

use super::CreatureId;
use super::grid::{Cell, Direction, Grid};
use super::occupancy::Occupancy;
use super::passage::Passage;
use super::rng::Rng;

/// Правила поиска маршрута — числа, которые движок не выдумывает о правилах
/// игры, но которые обязан выбрать сам.
#[derive(Debug, Clone, Copy)]
pub struct RouteRules {
    /// На каком расстоянии от занятой цели искать свободную клетку, в клетках
    /// решётки ([`Cell::lattice_distance`]).
    ///
    /// Цель занята — существо идёт в **самую близкую** свободную клетку рядом,
    /// а не в ту, до которой дешевле дойти: поиск перебирает кольца окрестности
    /// от центра наружу и останавливается на первом, до которого есть маршрут.
    /// Ограничение нужно, чтобы занятая цель в глухом углу не заставила обойти
    /// всю карту: колец перебирается не больше `radius`, но каждое кольцо —
    /// отдельный поиск (при свободной цели поиск один). `0` означает «только сама
    /// цель»: стоящее на ней существо не обойдёшь.
    pub goal_fallback_radius: u32,
}

impl Default for RouteRules {
    fn default() -> RouteRules {
        RouteRules {
            goal_fallback_radius: 4,
        }
    }
}

/// Один шаг маршрута: клетка, в которую входят, и направление шага.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Leg {
    pub cell: Cell,
    pub direction: Direction,
}

/// Найденный маршрут: шаги от текущей клетки до цели.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    legs: Vec<Leg>,
    goal: Cell,
}

impl Route {
    /// Шаги маршрута по порядку.
    pub fn legs(&self) -> &[Leg] {
        &self.legs
    }

    /// Клетки маршрута без направлений.
    pub fn cells(&self) -> Vec<Cell> {
        self.legs.iter().map(|leg| leg.cell).collect()
    }

    /// Клетка, которой маршрут кончается: запрошенная цель или — если она была
    /// занята или непроходима — ближайшая свободная рядом с ней.
    pub fn goal(&self) -> Cell {
        self.goal
    }

    pub fn len(&self) -> usize {
        self.legs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.legs.is_empty()
    }

    /// Стоимость маршрута в единицах [`Direction::cost`].
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

/// В клетку можно войти: она в сетке, проходима по данным поставщика и не занята
/// чужим существом.
///
/// Своя клетка существа не мешает: на время поиска она временно освобождается,
/// как в оригинале (Slavik `game.cpp:2889-2895`).
pub(super) fn is_enterable<P: Passage>(
    grid: &Grid,
    passage: &P,
    occupancy: &Occupancy,
    mover: CreatureId,
    cell: Cell,
) -> bool {
    grid.contains(cell) && passage.is_walkable(cell) && !occupancy.blocks(cell, mover)
}

/// Шаг разрешён: обе клетки вдоль его угла проходимы.
///
/// Занятость боковых клеток здесь ни при чём: срезание угла — правило
/// проходимости, а чужое существо в боковой клетке мешает войти в клетку
/// назначения, и это проверяет [`is_enterable`].
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

/// Всё, что нужно поиску: мир, по которому идут, и кто куда идёт.
pub struct RouteRequest<'a, P: Passage> {
    pub grid: &'a Grid,
    pub passage: &'a P,
    pub occupancy: &'a Occupancy,
    pub rules: &'a RouteRules,
    /// Существо, для которого ищут маршрут: его собственные клетки ему не
    /// мешают.
    pub mover: CreatureId,
    pub from: Cell,
    pub goal: Cell,
}

impl<P: Passage> RouteRequest<'_, P> {
    fn enterable(&self, cell: Cell) -> bool {
        is_enterable(self.grid, self.passage, self.occupancy, self.mover, cell)
    }

    /// Следующая клетка шага и его стоимость, или `None`, если шаг запрещён.
    fn step(&self, from: Cell, direction: Direction) -> Option<(Cell, u32)> {
        let to = self.grid.neighbour(from, direction)?;
        if !self.enterable(to) || !is_open_corner(self.grid, self.passage, from, direction) {
            return None;
        }
        Some((to, direction.cost()))
    }
}

/// Ищет кратчайший маршрут из `from` в `goal`.
///
/// `None` — маршрута нет: цель недоступна, заперта со всех сторон или лежит вне
/// сетки. Это значение, а не паника: существо без маршрута стоит на месте.
///
/// Занятая или непроходимая цель обходится по кольцам её окрестности от центра
/// наружу, поэтому существо идёт в ближайшую свободную клетку рядом, а не в ту,
/// до которой дешевле дойти ([`RouteRules::goal_fallback_radius`]).
///
/// Порядок перебора направлений сдвинут на величину из `rng`, поэтому выбор
/// между маршрутами равной стоимости зависит от seed и повторяется при том же
/// seed (issue #35, критерий 6).
pub fn find_route<P: Passage>(request: &RouteRequest<'_, P>, rng: &mut Rng) -> Option<Route> {
    let grid = *request.grid;
    if !grid.contains(request.from) || !grid.contains(request.goal) {
        return None;
    }
    let turn = rng.below(8) as usize;
    let (u, v) = request.goal.lattice();
    // Кольца окрестности — только для цели, в которую нельзя войти: занятой или
    // непроходимой. Свободная, но недостижимая цель остаётся недостижимой —
    // иначе существо вставало бы рядом и сообщало `Arrived` вместо `NoRoute`
    // (docs/rust.md, «Решение: занятая цель — ближайшее кольцо»).
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

/// A* до одного кольца клеток вокруг цели: кольцо задаёт и критерий успеха, и
/// оценку.
fn astar_to<P: Passage>(
    request: &RouteRequest<'_, P>,
    candidates: &[Cell],
    turn: usize,
) -> Option<Route> {
    let chosen: HashSet<Cell> = candidates.iter().copied().collect();
    let from = request.from;
    // Одна клетка-цель — оценка ровно до неё. Кольцо из нескольких клеток:
    // оценка до центра кольца минус самое длинное смещение кольца, иначе она
    // переоценивала бы путь до дальней клетки (docs/rust.md, «Решение: стоимость
    // шага и эвристика A*»).
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

/// Клетки решётки на расстоянии `radius` от `(u, v)` — все пары со сдвигом
/// `(a, b)`, у которых `max(|a|, |b|) == radius`. Нулевое кольцо — сама клетка.
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
                    "шаг {from:?} → {:?} срезает угол",
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
        .expect("прямой маршрут");
        assert_eq!(route.len(), 10);
        assert!(
            route
                .legs()
                .iter()
                .all(|leg| leg.direction == Direction::East),
            "восток дешевле зигзага по диагоналям: {:?}",
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
        let route = search(&grid, &map, &Occupancy::new(), from, goal).expect("обходной маршрут");
        let cells = route.cells();
        assert!(
            cells.iter().all(|cell| !map.blocked.contains(cell)),
            "маршрут идёт по непроходимым: {cells:?}"
        );
        assert!(
            cells.iter().any(|cell| cell.row < 8 || cell.row > 14),
            "не обошёл препятствие: {cells:?}"
        );
        assert!(
            route.len() > 4,
            "обход должен быть длиннее прямой: {cells:?}"
        );
        every_step_respects_corners(&grid, &map, &route);
    }

    #[test]
    fn corner_cutting_is_forbidden() {
        let grid = Grid::new(24, 24);
        let mut map = Map::default();
        let from = Cell::new(10, 10);
        let goal = Cell::new(11, 10);
        // Единственный короткий шаг — на восток через вершину ромба; одна из
        // боковых клеток закрыта, и пройти можно только в обход.
        map.blocked
            .insert(from.neighbour(Direction::NorthEast).unwrap());
        let route = search(&grid, &map, &Occupancy::new(), from, goal).expect("маршрут в обход");
        assert!(route.len() > 1, "срезал угол: {:?}", route.cells());
        every_step_respects_corners(&grid, &map, &route);
    }

    #[test]
    fn corner_blocked_on_both_sides_leaves_no_way_in() {
        let grid = Grid::new(8, 8);
        let from = Cell::new(3, 4);
        let goal = Cell::new(4, 4);
        let mut map = Map::default();
        // Вокруг цели закрыты все соседние клетки, кроме той, где стоим: войти
        // в неё можно только шагом на восток, а он идёт через угол.
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
        assert_eq!(find_in(&map), None, "угол прорезали");
        // Откроем обе боковые клетки — и тот же самый шаг становится возможен.
        let mut open = Map::default();
        for direction in [Direction::NorthEast, Direction::SouthEast] {
            let cell = from.neighbour(direction).unwrap();
            open.blocked.remove(&cell);
        }
        let route = find_in(&open).expect("маршрут через открытую вершину");
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
        .expect("маршрут через полкарты");
        assert_eq!(route.len(), 120);
        assert!(
            route.len() > 32,
            "лимит оригинала вернулся: {:?}",
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
        // Цель вне сетки — тоже отсутствие маршрута.
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
            .expect("маршрут к занятой цели");
        assert_ne!(route.goal(), goal);
        assert_eq!(route.cells().last(), Some(&route.goal()));
        assert_eq!(
            goal.lattice_distance(route.goal()),
            1,
            "ближайшая свободная клетка не на первом кольце: {:?}",
            route.goal()
        );
        assert!(!occupancy.blocks(route.goal(), MOVER));
        every_step_respects_corners(&grid, &map, &route);
    }

    #[test]
    fn a_free_but_unreachable_goal_does_not_take_a_ring_cell() {
        // Цель проходима и свободна, но все восемь соседей в стене: в неё нельзя
        // войти, и соседняя клетка вместо неё — это не ответ «дойди до цели»,
        // а подмена цели. Кольца окрестности работают только для занятой или
        // непроходимой цели (docs/rust.md, «Решение: занятая цель — ближайшее
        // кольцо»), здесь же ответа нет.
        let grid = Grid::new(24, 24);
        let mut map = Map::default();
        let goal = Cell::new(12, 12);
        for direction in Direction::ALL {
            map.blocked.insert(goal.neighbour(direction).unwrap());
        }
        assert!(map.is_walkable(goal), "сама цель свободна");
        for (index, from) in [Cell::new(4, 12), Cell::new(20, 2), Cell::new(12, 3)]
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                search(&grid, &map, &Occupancy::new(), from, goal),
                None,
                "случай {index}: вместо недостижимой цели подставлена соседняя клетка"
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
            "собственная клетка помешала поиску"
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
            "стоимость {:?} дешевле прямой линии {:?}",
            route.cost(),
            straight
        );
        // Сетка не даёт идти точно по прямой, но и в обход по кругу не гонит.
        assert!(
            route.cost() <= straight + 2 * Direction::East.cost(),
            "маршрут слишком далёк от прямого: {:?} против {:?}",
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

    /// Кратчайшая стоимость пути до клетки без эвристики — Чистый Дейкстра по
    /// тем же шагам. Нужна как независимая проверка того, что оценка не
    /// переоценивает.
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

    /// Эвристика обязана смотреть на фактическую клетку маршрута, а не только на
    /// запрошенную цель: иначе она переоценивает, и A\* отдаёт маршрут длиннее
    /// кратчайшего. Проверка — стоимость найденного маршрута против стоимости
    /// того же пути, посчитанной без эвристики.
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
        // Стена из шести столбцов с широким проходом: маршрут обязан его найти.
        for column in 8..14 {
            for row in 0..24 {
                if !(4..=8).contains(&row) {
                    field.blocked.insert(Cell::new(column, row));
                }
            }
        }
        // Карман: вокруг цели свободно ровно одно место — кольцо кандидатов из
        // одной клетки, и оценка обязана смотреть на неё, а не на занятый центр.
        let mut pocket = Map::default();
        let goal = Cell::new(12, 12);
        let keep = goal.neighbour(Direction::East).unwrap();
        for direction in Direction::ALL {
            let cell = goal.neighbour(direction).unwrap();
            if cell != keep {
                pocket.blocked.insert(cell);
            }
        }
        // Карман без свободной клетки в первом кольце: кандидаты только во
        // втором, и оценка тоже обязана смотреть на кольцо, а не на центр.
        let mut wide = Map::default();
        let wide_goal = Cell::new(12, 12);
        for direction in Direction::ALL {
            wide.blocked.insert(wide_goal.neighbour(direction).unwrap());
        }
        // Цель, закрытая самой картой (не занята существом): fallback тоже обязан
        // работать, и кольцо кандидатов у неё — первое кольцо вокруг клетки карты.
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
            // Цель непроходима по данным карты — маршрут до свободной рядом.
            (&wide, Occupancy::new(), Cell::new(2, 12), impassable_goal),
            (&pocket, occupancy.clone(), Cell::new(2, 12), goal),
            (&pocket, occupancy.clone(), Cell::new(12, 2), goal),
            (&pocket, occupancy, Cell::new(20, 20), goal),
            (&wide, wide_occupancy, Cell::new(20, 2), wide_goal),
        ];
        for (index, (map, places, from, target)) in cases.iter().enumerate() {
            let route = search(&grid, map, places, *from, *target)
                .unwrap_or_else(|| panic!("случай {index}: маршрута нет"));
            let optimum = optimal_cost(&grid, map, places, *from, route.goal())
                .unwrap_or_else(|| panic!("случай {index}: без эвристики пути нет"));
            assert_eq!(
                route.cost(),
                optimum,
                "случай {index}: маршрут к {:?} дороже кратчайшего: {:?} против {:?}",
                route.goal(),
                route.cost(),
                optimum
            );
            every_step_respects_corners(&grid, map, &route);
        }
    }

    /// Тот же контроль на случайных полях: если бы оценка где-то переоценивала,
    /// на одном из полей A\* выдал бы маршрут дороже кратчайшего. Поля заданы
    /// детерминированно, чтобы прогон повторялся.
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
            // Половина случаев — занятая цель: кольцо кандидатов вместо центра.
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
                "оценка переоценивает: {from:?} → {:?} на поле со {target:?}",
                route.goal()
            );
            every_step_respects_corners(&grid, &map, &route);
            checked += 1;
        }
        assert!(checked > 10, "случаев с маршрутом слишком мало: {checked}");
    }

    #[test]
    fn seeds_break_ties_deterministically() {
        let grid = Grid::new(24, 24);
        let mut map = Map::default();
        // Стена поперёк с двумя симметричными брешами: маршрутов ровно два, и
        // они равной стоимости.
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
            let route = route_with(seed).expect("маршрут через брешь");
            assert_eq!(route, route_with(seed).unwrap(), "seed {seed} непостоянен");
            distinct.insert(route);
        }
        assert!(
            distinct.len() > 1,
            "seed ни на что не влияет: все маршруты одинаковы"
        );
    }
}
