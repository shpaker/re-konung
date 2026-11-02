//! Замер стоимости движка перемещения (issue #35, критерий 7).
//!
//! Тест под `#[ignore]`: он считает в release и печатает таблицу, поэтому не
//! мешает обычному `cargo test`. Числа — для `docs/rust.md`.
//!
//! ```bash
//! cargo test --release --manifest-path crates/Cargo.toml -p kn-game \
//!     --test movement_cost -- --ignored --nocapture
//! ```

use std::collections::HashSet;
use std::hint::black_box;
use std::time::{Duration, Instant};

use kn_game::movement::CreatureId;
use kn_game::movement::grid::{Cell, Direction, Grid};
use kn_game::movement::occupancy::Occupancy;
use kn_game::movement::pace::Pace;
use kn_game::movement::passage::Passage;
use kn_game::movement::rng::Rng;
use kn_game::movement::route::{Route, RouteRequest, RouteRules, find_route};
use kn_game::movement::world::{World, WorldRules};

/// Существ в тестовом мире: «десятки» из критерия 7.
const CROWD: usize = 50;
/// Тиков в замере тика мира.
const TICKS: usize = 300;
/// Существо, для которого ищут маршрут в замерах поиска.
const MOVER: CreatureId = CreatureId::new(1000);

/// Проходимость как множество закрытых клеток.
#[derive(Default, Clone)]
struct Map {
    blocked: HashSet<Cell>,
}

impl Passage for Map {
    fn is_walkable(&self, cell: Cell) -> bool {
        !self.blocked.contains(&cell)
    }
}

/// Длительность шага — как число кадров анимации: три тика в любую сторону.
#[derive(Clone, Copy)]
struct Walk3;

impl Pace for Walk3 {
    fn step_ticks(&self, _creature: CreatureId, _direction: Direction) -> u32 {
        3
    }
}

fn report(label: &str, iterations: u32, total: Duration, unit: &str) {
    let per = total.as_secs_f64() * 1e6 / f64::from(iterations);
    println!("{label:<38} {per:>10.3} {unit}  ({} итераций)", iterations);
}

fn rules() -> RouteRules {
    RouteRules::default()
}

/// Маршрут по мелкой сетке K1 — то, что делает существо при приказе или пересчёте.
fn route_between<P: Passage>(
    passage: &P,
    occupancy: &Occupancy,
    from: Cell,
    goal: Cell,
    rng: &mut Rng,
) -> Option<Route> {
    let grid = Grid::fine();
    let request = RouteRequest {
        grid: &grid,
        passage,
        occupancy,
        rules: &rules(),
        mover: MOVER,
        from,
        goal,
    };
    find_route(&request, rng)
}

/// Мир из `count` существ, уже идущих по маршрутам: тик замеряется на них.
fn crowded_world(count: usize) -> World<Map, Walk3> {
    let mut world = World::new(
        Grid::fine(),
        Map::default(),
        Walk3,
        WorldRules {
            wait_before_repath_ticks: 4,
            repath_cooldown_ticks: 10,
            ..WorldRules::default()
        },
        7,
    );
    // Колонна существ по левой стороне карты, цели — на правой: маршруты
    // пересекаются, и часть шагов блокируется чужими клетками.
    for index in 0..count {
        let row = 4 + 2 * (index % 150) as u32;
        let cell = Cell::new(2 + (index / 150) as u32, row);
        let creature = world.add(cell).unwrap();
        world
            .move_to(creature, Cell::new(140 + (index % 7) as u32, row))
            .unwrap();
    }
    // Разводим по тикам, чтобы существа были в пути, а не стояли.
    for _ in 0..60 {
        world.tick_step();
    }
    world
}

#[test]
#[ignore = "замер стоимости: только release и по явному запуску"]
fn measures_the_cost_of_the_movement_engine() {
    let grid = Grid::fine();
    println!(
        "сетка {}x{} клеток, шаг {}x{} px",
        grid.columns,
        grid.rows,
        kn_game::movement::grid::STEP_X,
        kn_game::movement::grid::STEP_Y
    );
    let open = Map::default();
    let empty = Occupancy::new();
    let mut rng = Rng::new(1);
    let iterations = 200;

    // 1. Маршрут через всю карту по открытому полю.
    let start = Instant::now();
    for _ in 0..iterations {
        let route = route_between(
            &open,
            &empty,
            Cell::new(1, 2),
            Cell::new(158, 318),
            &mut rng,
        );
        black_box(route.expect("маршрут через всю карту"));
    }
    report(
        "маршрут через всю карту",
        iterations,
        start.elapsed(),
        "мкс",
    );

    // 2. Тот же маршрут среди стен: A* разворачивается шире.
    let mut walls = Map::default();
    for column in 0..grid.columns {
        for row in (0..grid.rows).step_by(7) {
            if column % 3 == 0 {
                walls.blocked.insert(Cell::new(column, row));
            }
        }
    }
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(route_between(
            &walls,
            &empty,
            Cell::new(1, 2),
            Cell::new(158, 318),
            &mut rng,
        ));
    }
    report("маршрут среди стен", iterations, start.elapsed(), "мкс");

    // 3. Худший случай поиска: цели нет, A* обходит всю достижимую область.
    let mut cage = Map::default();
    for column in 20..25 {
        for row in 0..grid.rows {
            cage.blocked.insert(Cell::new(column, row));
        }
    }
    let start = Instant::now();
    for _ in 0..50 {
        black_box(route_between(
            &cage,
            &empty,
            Cell::new(1, 2),
            Cell::new(158, 318),
            &mut rng,
        ));
    }
    report("маршрут до недостижимой цели", 50, start.elapsed(), "мкс");

    // 4. Пересчёт при блокировке: цель в плотной толпе, fallback по кольцам.
    let mut crowd = Occupancy::new();
    for index in 0..400u32 {
        crowd.stand(
            Cell::new(120 + index % 20, 100 + index / 20),
            CreatureId::new(index),
        );
    }
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(route_between(
            &open,
            &crowd,
            Cell::new(2, 4),
            Cell::new(128, 104),
            &mut rng,
        ));
    }
    report(
        "поиск рядом с 400 занятыми клетками",
        iterations,
        start.elapsed(),
        "мкс",
    );

    // 5. Тик мира на существах в пути: десятки и сотни.
    for count in [CROWD, 500] {
        let mut world = crowded_world(count);
        let moving = world.states().iter().filter(|state| state.moving).count();
        let start = Instant::now();
        for _ in 0..TICKS {
            black_box(world.tick_step());
        }
        let total = start.elapsed();
        report(
            &format!("тик {} существ (в пути {moving})", count),
            TICKS as u32,
            total,
            "мкс",
        );
        println!(
            "{:<38} {:>10.3} мкс  (на существо за тик)",
            "",
            total.as_secs_f64() * 1e6 / (TICKS * count) as f64
        );
    }

    // 6. Тик с блокировкой: каждые 20 тиков перекрываем клетку перед существом,
    // и тик включает пересчёт маршрута.
    let mut world = crowded_world(CROWD);
    let start = Instant::now();
    for tick in 0..TICKS {
        if tick % 20 == 0 {
            let cell = Cell::new(20 + (tick / 20) as u32, 10);
            world.passage_mut().blocked.insert(cell);
        }
        black_box(world.tick_step());
    }
    report(
        &format!("тик {} существ + блокировка", CROWD),
        TICKS as u32,
        start.elapsed(),
        "мкс",
    );

    // 7. Соседние операции для сравнения.
    let iterations = 20_000;
    let lookups = |label: &'static str, probe: &dyn Fn(u32)| {
        let start = Instant::now();
        for index in 0..iterations {
            probe(index);
        }
        report(label, iterations, start.elapsed(), "мкс");
    };
    lookups("занятость клетки (поиск)", &|index| {
        black_box(crowd.blocks(Cell::new(index % 160, (index / 160) % 320), MOVER));
    });
    lookups("эвристика A* (isqrt)", &|index| {
        black_box(Cell::new(index % 160, (index / 160) % 320).cost_distance(Cell::new(150, 200)));
    });
    lookups("соседняя клетка", &|index| {
        black_box(Cell::new(index % 160, (index / 160) % 320).neighbour(Direction::NorthEast));
    });
    lookups("клетка → пиксели", &|index| {
        black_box(Cell::new(index % 160, (index / 160) % 320).centre());
    });
}
