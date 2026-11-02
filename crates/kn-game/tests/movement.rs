//! Синтетические тесты движка перемещения (issue #35): маршруты, пересчёт,
//! движение по маршруту, одновременные существа и детерминизм. Данных игры и
//! Bevy тут нет — всё проверяется headless.

use std::collections::{HashMap, HashSet};

use kn_game::movement::CreatureId;
use kn_game::movement::grid::{Cell, Direction, Grid};
use kn_game::movement::pace::Pace;
use kn_game::movement::passage::Passage;
use kn_game::movement::world::{Event, World, WorldError, WorldRules};

/// Проходимость как множество закрытых клеток: движку всё равно, откуда они.
#[derive(Default, Clone)]
struct Map {
    blocked: HashSet<Cell>,
}

impl Passage for Map {
    fn is_walkable(&self, cell: Cell) -> bool {
        !self.blocked.contains(&cell)
    }
}

/// Длительность шага по направлениям — как таблица кадров анимации у поставщика
/// данных: запад, северо-запад, север, северо-восток, восток, юго-восток, юг,
/// юго-запад.
#[derive(Clone, Copy)]
struct Table([u32; 8]);

impl Table {
    fn uniform(ticks: u32) -> Table {
        Table([ticks; 8])
    }
}

impl Pace for Table {
    fn step_ticks(&self, _creature: CreatureId, direction: Direction) -> u32 {
        self.0[direction.index()]
    }
}

fn rules() -> WorldRules {
    WorldRules {
        wait_before_repath_ticks: 4,
        repath_cooldown_ticks: 10,
        ..WorldRules::default()
    }
}

fn world(columns: u32, rows: u32, pace: Table) -> World<Map, Table> {
    World::new(Grid::new(columns, rows), Map::default(), pace, rules(), 1)
}

/// Кто где стоит: клетка → существа. Двух существ в одной клетке быть не должно.
fn occupants(world: &World<Map, Table>) -> HashMap<Cell, Vec<CreatureId>> {
    let mut map: HashMap<Cell, Vec<CreatureId>> = HashMap::new();
    for state in world.states() {
        map.entry(state.cell).or_default().push(state.creature);
    }
    map
}

/// Инварианты движения, которые проверяются на каждом тике.
fn assert_invariants(world: &World<Map, Table>) {
    for (cell, list) in occupants(world) {
        assert!(list.len() < 2, "в клетке {cell:?} стоят {list:?}");
        assert!(
            world.passage().is_walkable(cell) && world.grid().contains(cell),
            "существо стоит в непроходимой клетке {cell:?}"
        );
        assert_eq!(
            world.creature_at(cell),
            Some(*list.first().unwrap()),
            "клетка {cell:?} не помечена занятой"
        );
    }
    for state in world.states() {
        if let Some(goal) = state.goal {
            assert!(world.grid().contains(goal), "цель {goal:?} вне сетки");
        }
        if state.moving {
            let step_to = state.step_to.expect("шаг без клетки назначения");
            assert!(state.step_ticks > 0, "шаг нулевой длины: {state:?}");
            assert!(
                state.step_elapsed <= state.step_ticks,
                "шаг длиннее, чем обещали: {state:?}"
            );
            assert!(
                world.passage().is_walkable(step_to),
                "существо идёт в непроходимую клетку {step_to:?}"
            );
        } else {
            assert_eq!(
                state.step_to, None,
                "остановившееся существо идёт: {state:?}"
            );
        }
    }
}

fn run(world: &mut World<Map, Table>, ticks: usize) -> Vec<Event> {
    let mut all = Vec::new();
    for _ in 0..ticks {
        all.extend(world.tick_step());
        assert_invariants(world);
    }
    all
}

fn route_sets(events: &[Event]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, Event::RouteSet { .. }))
        .count()
}

fn arrived(events: &[Event], creature: CreatureId) -> bool {
    events
        .iter()
        .any(|event| matches!(event, Event::Arrived { creature: c, .. } if *c == creature))
}

// --- критерий 2: маршруты ---------------------------------------------------

#[test]
fn routes_straight_and_around_an_obstacle() {
    {
        let mut world = world(24, 24, Table::uniform(1));
        let creature = world.add(Cell::new(4, 10)).unwrap();
        let steps = world.move_to(creature, Cell::new(12, 10)).unwrap();
        assert_eq!(steps, 8);
        let events = run(&mut world, steps + 2);
        assert_eq!(world.cell_of(creature), Some(Cell::new(12, 10)));
        assert!(arrived(&events, creature), "{events:?}");
    }

    // Препятствие на пути — существо идёт в обход и доходит.
    let mut world = world(24, 24, Table::uniform(1));
    let creature = world.add(Cell::new(4, 10)).unwrap();
    for column in 8..=10 {
        for row in 8..=12 {
            world.passage_mut().blocked.insert(Cell::new(column, row));
        }
    }
    let steps = world.move_to(creature, Cell::new(14, 10)).unwrap();
    assert!(
        steps > 10,
        "обход должен быть длиннее прямых 10 шагов: {steps}"
    );
    let events = run(&mut world, steps + 4);
    assert_eq!(world.cell_of(creature), Some(Cell::new(14, 10)));
    assert!(arrived(&events, creature), "{events:?}");
}

#[test]
fn route_is_longer_than_the_original_32_step_limit() {
    let mut world = world(160, 320, Table::uniform(1));
    let creature = world.add(Cell::new(1, 10)).unwrap();
    let steps = world.move_to(creature, Cell::new(121, 10)).unwrap();
    assert_eq!(steps, 120, "лимит оригинала вернулся");
    run(&mut world, steps + 2);
    assert_eq!(world.cell_of(creature), Some(Cell::new(121, 10)));
}

#[test]
fn unreachable_goal_is_refused_not_a_panic() {
    let mut world = world(20, 20, Table::uniform(1));
    let creature = world.add(Cell::new(2, 10)).unwrap();
    for row in 0..20 {
        world.passage_mut().blocked.insert(Cell::new(10, row));
    }
    world.rules_mut().route.goal_fallback_radius = 0;
    assert_eq!(
        world.move_to(creature, Cell::new(15, 10)),
        Err(WorldError::NoRoute)
    );
    assert_eq!(world.cell_of(creature), Some(Cell::new(2, 10)));
    assert_eq!(run(&mut world, 50), Vec::new(), "отказ породил движение");
}

#[test]
fn occupied_goal_routes_to_a_free_cell_nearby() {
    let mut world = world(20, 20, Table::uniform(1));
    let walker = world.add(Cell::new(2, 10)).unwrap();
    let stander = world.add(Cell::new(8, 10)).unwrap();
    let steps = world.move_to(walker, Cell::new(8, 10)).unwrap();
    assert!(steps > 0);
    let goal = world.state(walker).unwrap().goal.unwrap();
    assert_ne!(goal, Cell::new(8, 10), "маршрут ведёт в занятую клетку");
    assert_eq!(goal.lattice_distance(Cell::new(8, 10)), 1);
    let events = run(&mut world, steps + 4);
    assert_eq!(world.cell_of(walker), Some(goal));
    assert_eq!(world.cell_of(stander), Some(Cell::new(8, 10)));
    assert!(arrived(&events, walker), "{events:?}");
}

// --- критерий 3: пересчёт ---------------------------------------------------

/// Клетка впереди стала непроходима: существо пересчитывает маршрут, доходит и
/// не пересчитывает его каждый тик.
#[test]
fn blocked_route_is_repathed_once_and_the_creature_arrives() {
    let mut world = world(24, 24, Table::uniform(1));
    let creature = world.add(Cell::new(2, 10)).unwrap();
    world.move_to(creature, Cell::new(18, 10)).unwrap();
    // Один шаг сделан, следующий перекрыт.
    world.tick_step();
    world.passage_mut().blocked.insert(Cell::new(3, 10));

    let mut events = Vec::new();
    for _ in 0..200 {
        let tick_events = world.tick_step();
        assert_invariants(&world);
        events.extend(tick_events);
        if arrived(&events, creature) {
            break;
        }
    }
    assert!(
        events.iter().any(|event| *event
            == Event::TerrainBlocked {
                creature,
                cell: Cell::new(3, 10)
            }),
        "{events:?}"
    );
    assert!(arrived(&events, creature), "существо застряло: {events:?}");
    assert_eq!(world.cell_of(creature), Some(Cell::new(18, 10)));
    // Дрожи нет: за 200 тиков при cooldown в 10 тиков — единицы пересчётов.
    assert!(
        route_sets(&events) <= 3,
        "пересчётов слишком много ({}): {events:?}",
        route_sets(&events)
    );
}

/// Клетку впереди держит другое существо: первый по номеру проходит, второй
/// подождать, пересчитать и дойти следом.
#[test]
fn creature_ahead_frees_the_cell_and_the_second_follows() {
    let mut world = world(24, 24, Table::uniform(2));
    let first = world.add(Cell::new(2, 10)).unwrap();
    let second = world.add(Cell::new(1, 10)).unwrap();
    let goal = Cell::new(9, 10);
    assert_eq!(world.move_to(first, goal), Ok(7));
    // Впереди стоит первое существо, поэтому второму нужно его обойти.
    assert!(world.move_to(second, goal).unwrap() >= 8);

    let mut events = Vec::new();
    for _ in 0..200 {
        let tick_events = world.tick_step();
        assert_invariants(&world);
        events.extend(tick_events);
        if arrived(&events, first) && arrived(&events, second) {
            break;
        }
    }
    assert!(arrived(&events, first), "{events:?}");
    assert!(arrived(&events, second), "{events:?}");
    assert_eq!(world.cell_of(first), Some(goal));
    let end = world.cell_of(second).unwrap();
    assert!(
        end.lattice_distance(goal) <= 1,
        "второй встал слишком далеко от цели: {end:?}"
    );
    assert_eq!(
        world.occupancy().holder(Cell::new(2, 10)),
        None,
        "клетка не освобождена, когда существо её покинуло"
    );
    assert!(
        events.iter().any(
            |event| matches!(event, Event::Waiting { creature: c, blocked_by, .. }
                if *c == second && *blocked_by == first)
        ),
        "второй не ждал первого: {events:?}"
    );
}

/// Встречные приказы: ни одно существо не входит в клетку, куда уже идёт
/// другое, и оба в конце концов останавливаются.
#[test]
fn head_on_exchange_never_shares_a_cell() {
    let mut world = world(24, 24, Table::uniform(1));
    let a = world.add(Cell::new(10, 10)).unwrap();
    let b = world.add(Cell::new(16, 10)).unwrap();
    world.move_to(a, Cell::new(16, 10)).unwrap();
    world.move_to(b, Cell::new(10, 10)).unwrap();

    let mut events = Vec::new();
    for _ in 0..200 {
        let tick_events = world.tick_step();
        assert_invariants(&world);
        events.extend(tick_events);
        if arrived(&events, a) && arrived(&events, b) {
            break;
        }
    }
    assert!(arrived(&events, a), "не разошлись: {events:?}");
    assert!(arrived(&events, b), "не разошлись: {events:?}");
    assert_ne!(world.cell_of(a), world.cell_of(b));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Waiting { .. })),
        "встречный приказ не породил ожидания: {events:?}"
    );
}

// --- критерий 4: движение ---------------------------------------------------

#[test]
fn step_takes_exactly_the_ticks_the_supplier_gave() {
    // Восток — 5 тиков, юг — 3, остальное — 1: длительность задаёт поставщик.
    let mut world = world(24, 24, Table([5, 5, 1, 1, 5, 3, 3, 3]));
    let creature = world.add(Cell::new(4, 10)).unwrap();
    world.move_to(creature, Cell::new(5, 10)).unwrap();

    let mut cells = Vec::new();
    let mut directions = Vec::new();
    let mut events = Vec::new();
    for _ in 0..8 {
        events.extend(world.tick_step());
        assert_invariants(&world);
        let state = world.state(creature).unwrap();
        cells.push(state.cell);
        directions.push(state.direction);
    }
    // Шаг начат на тике 1, существо в новой клетке на тике 6 — ровно 5 тиков.
    assert_eq!(cells[0], Cell::new(4, 10));
    assert_eq!(cells[4], Cell::new(4, 10));
    assert_eq!(cells[5], Cell::new(5, 10));
    assert_eq!(directions[..5], [Some(Direction::East); 5]);
    assert!(arrived(&events, creature), "{events:?}");
}

#[test]
fn direction_changes_only_on_the_cell_border() {
    let mut world = world(24, 24, Table::uniform(3));
    let creature = world.add(Cell::new(4, 10)).unwrap();
    world.move_to(creature, Cell::new(8, 14)).unwrap();
    let mut previous_cell = world.cell_of(creature).unwrap();
    let mut previous_direction = world.state(creature).unwrap().direction;
    for _ in 0..60 {
        world.tick_step();
        let state = world.state(creature).unwrap();
        if state.cell == previous_cell {
            assert_eq!(
                state.direction, previous_direction,
                "направление изменилось в середине клетки: {state:?}"
            );
        }
        previous_cell = state.cell;
        previous_direction = state.direction;
        assert_invariants(&world);
    }
}

#[test]
fn route_is_consumed_step_by_step_and_the_creature_stops() {
    let mut world = world(24, 24, Table::uniform(1));
    let creature = world.add(Cell::new(3, 6)).unwrap();
    world.move_to(creature, Cell::new(6, 6)).unwrap();
    let mut visited = vec![world.cell_of(creature).unwrap()];
    for _ in 0..20 {
        let events = world.tick_step();
        assert_invariants(&world);
        for event in events {
            if let Event::StepFinished { cell, .. } = event {
                visited.push(cell);
            }
        }
    }
    assert_eq!(
        visited,
        vec![
            Cell::new(3, 6),
            Cell::new(4, 6),
            Cell::new(5, 6),
            Cell::new(6, 6)
        ],
        "маршрут съедается не по шагу"
    );
    assert_eq!(world.cell_of(creature), Some(Cell::new(6, 6)));
    assert_eq!(world.state(creature).unwrap().goal, None);
    assert!(!world.state(creature).unwrap().moving);
    assert_eq!(
        world.tick_step(),
        Vec::new(),
        "остановившееся существо шумит"
    );
}

// --- критерий 5: несколько существ ------------------------------------------

#[test]
fn dozens_of_creatures_move_in_the_same_ticks() {
    let mut world = world(24, 60, Table::uniform(2));
    let mut crowd = Vec::new();
    for index in 0..40 {
        let cell = Cell::new(2 + (index % 4) as u32, 4 + index as u32);
        crowd.push(world.add(cell).unwrap());
    }
    // У каждого своя цель в восточном квартале: маршруты пересекаются, и
    // существа мешают друг другу без необходимости.
    for (index, &creature) in crowd.iter().enumerate() {
        let goal = Cell::new(14 + (index % 5) as u32, 8 + (index / 5) as u32);
        world.move_to(creature, goal).unwrap();
    }

    let mut events = Vec::new();
    for _ in 0..600 {
        let tick_events = world.tick_step();
        assert_invariants(&world);
        events.extend(tick_events);
        if crowd.iter().all(|&creature| arrived(&events, creature)) {
            break;
        }
    }
    for &creature in &crowd {
        assert!(arrived(&events, creature), "не все дошли: {creature:?}");
    }
    let cells: Vec<Cell> = world.states().iter().map(|state| state.cell).collect();
    assert_eq!(cells.iter().collect::<HashSet<_>>().len(), cells.len());
    // Каждый держит ровно свою клетку: никаких висящих резервов.
    assert_eq!(world.occupancy().len(), crowd.len());
}

#[test]
fn freed_cell_is_taken_by_another_creature() {
    let mut world = world(24, 24, Table::uniform(1));
    let first = world.add(Cell::new(4, 8)).unwrap();
    let second = world.add(Cell::new(4, 10)).unwrap();
    world.move_to(first, Cell::new(10, 8)).unwrap();
    let vacated = Cell::new(4, 8);
    // Первый сходит с клетки, а её уже занимает цель второго.
    run(&mut world, 2);
    assert_eq!(world.cell_of(first), Some(Cell::new(5, 8)));
    assert_eq!(world.creature_at(vacated), None, "клетка не освобождена");
    world.move_to(second, vacated).unwrap();
    run(&mut world, 8);
    assert_eq!(world.cell_of(second), Some(vacated));
    assert_eq!(world.creature_at(vacated), Some(second));
    assert_eq!(world.cell_of(first), Some(Cell::new(10, 8)));
}

#[test]
fn creatures_do_not_occupy_the_same_cell_in_the_same_ticks() {
    // Узкий дворик: шесть существ идут в одну клетку и не налезают друг на
    // друга ни на одном тике.
    let mut world = world(16, 16, Table::uniform(1));
    let mut crowd = Vec::new();
    for index in 0..6 {
        let cell = Cell::new(2, 2 + 2 * index as u32);
        crowd.push(world.add(cell).unwrap());
    }
    for &creature in &crowd {
        world.move_to(creature, Cell::new(13, 8)).unwrap();
    }
    run(&mut world, 200);
    let cells: Vec<Cell> = world.states().iter().map(|state| state.cell).collect();
    assert_eq!(
        cells.iter().collect::<HashSet<_>>().len(),
        cells.len(),
        "двое встали в одну клетку: {cells:?}"
    );
    assert!(
        world.states().iter().all(|state| !state.moving),
        "толпа не устоялась"
    );
}

// --- критерий 6: детерминизм ------------------------------------------------

/// Трасса: тик, номер существа, его клетка и направление.
type Trace = Vec<(u64, u32, Cell, Option<Direction>)>;

/// Один и тот же скрипт прогона: трасса зависит от seed и повторяется при нём.
fn trace(seed: u64) -> Trace {
    let mut world = World::new(
        Grid::new(24, 24),
        Map::default(),
        Table::uniform(1),
        rules(),
        seed,
    );
    for column in 0..24 {
        if column != 6 && column != 18 {
            world.passage_mut().blocked.insert(Cell::new(column, 12));
        }
    }
    let a = world.add(Cell::new(12, 8)).unwrap();
    let b = world.add(Cell::new(12, 16)).unwrap();
    world.move_to(a, Cell::new(12, 20)).unwrap();
    world.move_to(b, Cell::new(12, 4)).unwrap();
    let mut trace = Vec::new();
    for _ in 0..80 {
        world.tick_step();
        for state in world.states() {
            trace.push((
                world.tick_number(),
                state.creature.raw(),
                state.cell,
                state.direction,
            ));
        }
    }
    trace
}

#[test]
fn same_seed_replays_the_same_trace() {
    assert_eq!(trace(7), trace(7));
    assert_eq!(trace(u64::MAX), trace(u64::MAX));
    // Один и тот же seed из разных прогонов — одна и та же трасса.
    for seed in 0..6 {
        assert_eq!(trace(seed), trace(seed), "seed {seed} воспроизводится");
    }
}

/// Seed развязывает равные маршруты: на симметричной карте разные seed дают
/// разные трассы. Там, где выбор не равный, маршрут один и тот же — иначе
/// детерминизм был бы недостижим.
#[test]
fn other_seed_changes_the_trace() {
    let traces: HashSet<Trace> = (0..6).map(trace).collect();
    assert!(
        traces.len() > 1,
        "seed ни на что не влияет: все трассы совпали"
    );
}
