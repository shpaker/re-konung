//! Изометрическая сетка мелкой решётки: клетки, 8 направлений, перевод в пиксели.
//!
//! Движок знает только геометрию. Проходимость и занятость клеток приходят
//! извне ([`super::passage::Passage`] и [`super::occupancy::Occupancy`]);
//! данные карты поставщику известны, а не движку: здесь нет ни форматов, ни
//! Bevy, ни специфики части серии.
//!
//! Формулы пиксельной позиции согласованы с `k1_data::game_state`
//! (`character_position` и `character_cell`): общей зависимости от K1 движку не
//! нужно, поэтому константы живут здесь, а сверку держит тест
//! `fine_grid_matches_k1_data` (см. `tests/movement_grid.rs`).

/// Шаг клетки мелкой решётки по x, пикселей: половина шага тайловой сетки.
pub const STEP_X: i32 = 58;
/// Шаг строки мелкой решётки по y, пикселей: половина шага строки тайлов.
pub const STEP_Y: i32 = 16;
/// Нечётные строки сдвинуты вправо на половину шага по x.
pub const ODD_ROW_SHIFT: i32 = STEP_X / 2;

/// Ширина мелкой сетки карты «Князя 1» (`k1_data::map::FINE_WIDTH`).
pub const FINE_WIDTH: u32 = 160;
/// Высота мелкой сетки карты «Князя 1» (`k1_data::map::FINE_HEIGHT`).
pub const FINE_HEIGHT: u32 = 320;

/// Единицы стоимости шага ([`Direction::cost`]): 1/8 пикселя.
pub const COST_UNITS_PER_PIXEL: u32 = 8;

/// Самое длинное смещение в пикселях за один шаг — восток или запад. Нужно для
/// оценки расстояния до окрестности занятой цели ([`super::route`]).
pub const MAX_STEP_PX: i32 = STEP_X;

/// Клетка мелкой решётки: столбец и строка, отсчёт от левого верхнего угла
/// карты; строки идут вниз по пикселям.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cell {
    pub column: u32,
    pub row: u32,
}

/// Направление шага. Порядок значений — тот же, что в таблице смещений
/// оригинала (Slavik `game.cpp:2918-2947`), поэтому индекс направления служит
/// ключом к кадрам анимации ходьбы. Имена описывают экран: y в пикселях карты
/// растёт вниз.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Direction {
    West,
    NorthWest,
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
}

impl Direction {
    /// Все направления в порядке таблицы оригинала.
    pub const ALL: [Direction; 8] = [
        Direction::West,
        Direction::NorthWest,
        Direction::North,
        Direction::NorthEast,
        Direction::East,
        Direction::SouthEast,
        Direction::South,
        Direction::SouthWest,
    ];

    /// Индекс направления в таблице оригинала: 0 — запад, далее по часовой.
    pub const fn index(self) -> usize {
        match self {
            Direction::West => 0,
            Direction::NorthWest => 1,
            Direction::North => 2,
            Direction::NorthEast => 3,
            Direction::East => 4,
            Direction::SouthEast => 5,
            Direction::South => 6,
            Direction::SouthWest => 7,
        }
    }

    /// Смещение клетки-соседа `(столбец, строка)`. Оно зависит от чётности
    /// строки, потому что нечётные строки сдвинуты вправо.
    pub const fn offset(self, odd_row: bool) -> (i32, i32) {
        match (self, odd_row) {
            (Direction::West, _) => (-1, 0),
            (Direction::NorthWest, true) => (-1, -1),
            (Direction::NorthWest, false) => (0, -1),
            (Direction::North, _) => (0, -2),
            (Direction::NorthEast, true) => (0, -1),
            (Direction::NorthEast, false) => (1, -1),
            (Direction::East, _) => (1, 0),
            (Direction::SouthEast, true) => (0, 1),
            (Direction::SouthEast, false) => (1, 1),
            (Direction::South, _) => (0, 2),
            (Direction::SouthWest, true) => (-1, 1),
            (Direction::SouthWest, false) => (0, 1),
        }
    }

    /// Смещение пиксельной точки клетки: от чётности строки не зависит — сдвиг
    /// строк как раз компенсирует разницу двух таблиц смещений.
    pub const fn pixel_offset(self) -> (i32, i32) {
        match self {
            Direction::West => (-STEP_X, 0),
            Direction::NorthWest => (-ODD_ROW_SHIFT, -STEP_Y),
            Direction::North => (0, -2 * STEP_Y),
            Direction::NorthEast => (ODD_ROW_SHIFT, -STEP_Y),
            Direction::East => (STEP_X, 0),
            Direction::SouthEast => (ODD_ROW_SHIFT, STEP_Y),
            Direction::South => (0, 2 * STEP_Y),
            Direction::SouthWest => (-ODD_ROW_SHIFT, STEP_Y),
        }
    }

    /// Направление шага из клетки в соседнюю, или `None`, если клетки не
    /// соседние.
    pub fn between(from: Cell, to: Cell) -> Option<Direction> {
        Direction::ALL
            .into_iter()
            .find(|&dir| from.neighbour(dir) == Some(to))
    }

    /// Восток, запад, север и юг: соседняя клетка касается текущей только
    /// углом (общая вершина ромбов). Такой шаг проходит сквозь вершину, где
    /// сходятся четыре клетки, поэтому разрешён, лишь когда обе клетки вдоль
    /// пути проходимы и свободны, — иначе существо срезало бы угол.
    ///
    /// Шаги «по диагонали экрана» (северо-восток и дальше) делят с текущей
    /// клеткой целое ребро: срезать угол им негде.
    pub const fn corner_flanks(self) -> Option<[Direction; 2]> {
        match self {
            Direction::East => Some([Direction::NorthEast, Direction::SouthEast]),
            Direction::West => Some([Direction::NorthWest, Direction::SouthWest]),
            Direction::North => Some([Direction::NorthWest, Direction::NorthEast]),
            Direction::South => Some([Direction::SouthWest, Direction::SouthEast]),
            _ => None,
        }
    }

    /// Стоимость шага для поиска маршрута, в единицах
    /// [`COST_UNITS_PER_PIXEL`] (1/8 пикселя): 8·58 на восток и запад, 8·32 на
    /// север и юг и 8·√(29² + 16²) по диагонали, округлённая вверх.
    ///
    /// Округление вверх принципиально: стоимость шага никогда не меньше его
    /// настоящей длины, поэтому эвристика поиска остаётся допустимой
    /// (docs/rust.md, «Решение: стоимость шага и эвристика A*»).
    pub const fn cost(self) -> u32 {
        match self {
            Direction::East | Direction::West => COST_UNITS_PER_PIXEL * STEP_X as u32,
            Direction::North | Direction::South => COST_UNITS_PER_PIXEL * 2 * STEP_Y as u32,
            _ => COST_DIAGONAL,
        }
    }
}

/// `ceil(8 · √(29² + 16²))` — стоимость диагонального шага; число сверяет тест
/// `diagonal_cost_matches_pixel_length`.
const COST_DIAGONAL: u32 = 265;

impl Cell {
    pub const fn new(column: u32, row: u32) -> Cell {
        Cell { column, row }
    }

    /// Пиксельная точка клетки — центр её ромба, туда встаёт спрайт.
    ///
    /// Формула совпадает с `k1_data::game_state::character_position`.
    pub fn centre(self) -> (i32, i32) {
        let column = self.column as i64;
        let row = self.row as i64;
        let shift = i64::from(2 - (row & 1) as i32) * i64::from(ODD_ROW_SHIFT);
        (
            (column * i64::from(STEP_X) + shift) as i32,
            (row * i64::from(STEP_Y) + i64::from(STEP_Y)) as i32,
        )
    }

    /// Клетка-сосед в этом направлении, или `None` за левым верхним краем
    /// координат. Границы сетки знает [`Grid::neighbour`].
    pub fn neighbour(self, direction: Direction) -> Option<Cell> {
        let (dc, dr) = direction.offset(self.row & 1 == 1);
        let column = self.column as i64 + i64::from(dc);
        let row = self.row as i64 + i64::from(dr);
        if column < 0 || row < 0 {
            return None;
        }
        let column = u32::try_from(column).ok()?;
        let row = u32::try_from(row).ok()?;
        Some(Cell::new(column, row))
    }

    /// Координаты в решётке без сдвига строк.
    ///
    /// Клетки мелкой решётки — обычная квадратная сетка в базисе шагов
    /// «юго-восток» и «северо-восток»: четыре шага, которые на экране идут по
    /// диагонали, в этих координатах ортогональные, а восток, запад, север и
    /// юг — наоборот, диагональные. Соседство, расстояние в клетках и запрет
    /// срезания углов описываются тогда без ветвления по чётности строки.
    pub fn lattice(self) -> (i64, i64) {
        let column = self.column as i64;
        let row = self.row as i64;
        (column + row / 2, column - (row + 1) / 2)
    }

    /// Обратный перевод из решётки без сдвига строк, или `None`, если паре
    /// координат клетка не соответствует — она лежит за левым верхним краем.
    pub fn from_lattice(u: i64, v: i64) -> Option<Cell> {
        let row = u - v;
        let column = (u + v + (row & 1)) / 2;
        if row < 0 || column < 0 {
            return None;
        }
        Some(Cell::new(column as u32, row as u32))
    }

    /// Минимальное число шагов до другой клетки без учёта проходимости —
    /// расстояние Чебышёва в координатах решётки.
    pub fn lattice_distance(self, other: Cell) -> u32 {
        let (a, b) = self.lattice();
        let (c, d) = other.lattice();
        i64::abs_diff(a, c).max(i64::abs_diff(b, d)) as u32
    }

    /// Расстояние между точками клеток в единицах [`Direction::cost`] —
    /// прямая линия в пикселях, умноженная на 8, округлённая вниз.
    ///
    /// Округление вниз нужно эвристике A*: она обязана не переоценивать.
    pub fn cost_distance(self, other: Cell) -> u32 {
        let (ax, ay) = self.centre();
        let (bx, by) = other.centre();
        let dx = i64::from(ax - bx);
        let dy = i64::from(ay - by);
        let scaled = u64::try_from(64 * (dx * dx + dy * dy)).unwrap_or(u64::MAX);
        usize::try_from(scaled.isqrt())
            .map(|root| u32::try_from(root).unwrap_or(u32::MAX))
            .unwrap_or(u32::MAX)
    }
}

/// Размер сетки, в которой ищутся соседи.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    pub columns: u32,
    pub rows: u32,
}

impl Grid {
    pub const fn new(columns: u32, rows: u32) -> Grid {
        Grid { columns, rows }
    }

    /// Мелкая сетка карты «Князя 1»: 160×320.
    pub const fn fine() -> Grid {
        Grid::new(FINE_WIDTH, FINE_HEIGHT)
    }

    pub const fn contains(self, cell: Cell) -> bool {
        cell.column < self.columns && cell.row < self.rows
    }

    /// Соседняя клетка внутри сетки, иначе `None`.
    pub fn neighbour(self, cell: Cell, direction: Direction) -> Option<Cell> {
        let next = cell.neighbour(direction)?;
        self.contains(next).then_some(next)
    }

    /// Ближайшая клетка к пиксельной точке карты.
    ///
    /// Округление половинок — к чётному, как `round()` в Python-этлоне и в
    /// `k1_data::game_state::character_cell`: иначе обратный перевод расходился
    /// бы с прямым на границах клеток.
    pub fn cell_at(self, x: i32, y: i32) -> Option<Cell> {
        let row = round_half_even(i64::from(y) - i64::from(STEP_Y), i64::from(STEP_Y));
        let shift = i64::from(2 - (row & 1) as i32) * i64::from(ODD_ROW_SHIFT);
        let column = round_half_even(i64::from(x) - shift, i64::from(STEP_X));
        if column < 0 || row < 0 {
            return None;
        }
        let cell = Cell::new(column as u32, row as u32);
        self.contains(cell).then_some(cell)
    }
}

/// Деление с округлением половинок к чётному — так же, как `round()` в Python.
fn round_half_even(numerator: i64, denominator: i64) -> i64 {
    let floor = numerator.div_euclid(denominator);
    let twice = 2 * numerator.rem_euclid(denominator);
    match twice.cmp(&denominator) {
        std::cmp::Ordering::Greater => floor + 1,
        std::cmp::Ordering::Less => floor,
        std::cmp::Ordering::Equal => {
            if floor & 1 == 0 {
                floor
            } else {
                floor + 1
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_indices_follow_the_original_table() {
        for (index, direction) in Direction::ALL.iter().enumerate() {
            assert_eq!(direction.index(), index);
        }
    }

    /// Соседи: таблица смещений и пиксельные шаги согласованы и на чётной, и
    /// на нечётной строке — все 8 направлений.
    #[test]
    fn neighbour_offsets_match_pixel_offsets() {
        for odd_row in [false, true] {
            let from = Cell::new(10, if odd_row { 7 } else { 6 });
            let shifts = [
                ((-1, 0), (-58, 0)),
                ((if odd_row { -1 } else { 0 }, -1), (-29, -16)),
                ((0, -2), (0, -32)),
                ((if odd_row { 0 } else { 1 }, -1), (29, -16)),
                ((1, 0), (58, 0)),
                ((if odd_row { 0 } else { 1 }, 1), (29, 16)),
                ((0, 2), (0, 32)),
                ((if odd_row { -1 } else { 0 }, 1), (-29, 16)),
            ];
            for (direction, (offset, pixel)) in Direction::ALL.into_iter().zip(shifts) {
                assert_eq!(direction.pixel_offset(), pixel, "{direction:?}");
                assert_eq!(direction.offset(odd_row), offset, "{direction:?}");
                let to = from.neighbour(direction).unwrap();
                assert_eq!(Direction::between(from, to), Some(direction));
                let (fx, fy) = from.centre();
                let (tx, ty) = to.centre();
                assert_eq!((tx - fx, ty - fy), pixel, "{direction:?} odd={odd_row}");
            }
        }
    }

    /// Клетки, стоящие поперёк шага «через угол», — его ортогональные соседи:
    /// шаг идёт через вершину ромба, где сходятся ровно четыре клетки, и каждая
    /// боковая клетка лежит от этой вершины на полшага по оси картинки.
    #[test]
    fn corner_flanks_stand_across_the_step() {
        let from = Cell::new(10, 10);
        let half = [
            (ODD_ROW_SHIFT, 0),
            (-ODD_ROW_SHIFT, 0),
            (0, STEP_Y),
            (0, -STEP_Y),
        ];
        for direction in Direction::ALL {
            let Some(flanks) = direction.corner_flanks() else {
                continue;
            };
            let to = from.neighbour(direction).unwrap();
            let (ax, ay) = from.centre();
            let (bx, by) = to.centre();
            let (mid_x, mid_y) = ((ax + bx) / 2, (ay + by) / 2);
            assert_eq!((bx - ax, by - ay), direction.pixel_offset());
            let mut seen = Vec::new();
            for flank in [
                from.neighbour(flanks[0]).unwrap(),
                from.neighbour(flanks[1]).unwrap(),
            ] {
                // Боковая клетка соседна и с началом, и с концом шага.
                assert!(Direction::between(from, flank).is_some());
                assert!(Direction::between(to, flank).is_some());
                let (fx, fy) = flank.centre();
                let offset = (fx - mid_x, fy - mid_y);
                assert!(
                    half.contains(&offset),
                    "{direction:?}: {flank:?} стоит от вершины шага как {offset:?}"
                );
                seen.push(offset);
            }
            assert_ne!(seen[0], seen[1], "{direction:?}: боковые клетки совпали");
        }
    }

    #[test]
    fn round_half_even_matches_ties_to_even() {
        for (numerator, denominator, expected) in [
            (5, 2, 2),
            (7, 2, 4),
            (-5, 2, -2),
            (-7, 2, -4),
            (16, 16, 1),
            (-16, 16, -1),
            (24, 16, 2),
            (40, 16, 2),
            (8, 16, 0),
            (-8, 16, 0),
        ] {
            assert_eq!(
                round_half_even(numerator, denominator),
                expected,
                "{numerator}/{denominator}"
            );
        }
    }

    #[test]
    fn diagonal_cost_matches_pixel_length() {
        for direction in Direction::ALL {
            let (dx, dy) = direction.pixel_offset();
            let scaled = 8.0 * f64::from(dx * dx + dy * dy).sqrt();
            let cost = f64::from(direction.cost());
            assert!(
                (cost - scaled).abs() < 8.0,
                "{direction:?}: стоимость {cost} далека от {scaled}"
            );
            assert!(
                cost >= scaled,
                "{direction:?}: стоимость {cost} дешевле шага {scaled}"
            );
        }
        assert_eq!(Direction::SouthEast.cost(), COST_DIAGONAL);
        assert_eq!(Direction::East.cost(), 464);
        assert_eq!(Direction::South.cost(), 256);
    }

    #[test]
    fn lattice_round_trip_covers_the_grid() {
        for row in 0..9 {
            for column in 0..9 {
                let cell = Cell::new(column, row);
                let (u, v) = cell.lattice();
                assert_eq!(Cell::from_lattice(u, v), Some(cell));
            }
        }
    }

    /// Точка решётки за левым верхним краем не должна превращаться в клетку:
    /// иначе окрестность цели у края карты ловила бы клетки из другого угла.
    #[test]
    fn lattice_point_outside_the_corner_is_not_a_cell() {
        assert_eq!(Cell::from_lattice(-1, 0), None);
        assert_eq!(Cell::from_lattice(0, 1), None);
        assert_eq!(Cell::new(0, 0).lattice(), (0, 0));
        assert_eq!(Cell::from_lattice(0, 0), Some(Cell::new(0, 0)));
    }

    #[test]
    fn lattice_distance_is_the_shortest_number_of_steps() {
        let from = Cell::new(4, 4);
        for direction in Direction::ALL {
            let to = from.neighbour(direction).unwrap();
            assert_eq!(from.lattice_distance(to), 1, "{direction:?}");
        }
        assert_eq!(from.lattice_distance(Cell::new(6, 4)), 2);
        assert_eq!(from.lattice_distance(Cell::new(4, 8)), 2);
        assert_eq!(from.lattice_distance(from), 0);
    }

    #[test]
    fn cell_at_picks_the_same_cell_as_centre() {
        let grid = Grid::new(12, 12);
        for row in 0..12 {
            for column in 0..12 {
                let cell = Cell::new(column, row);
                let (x, y) = cell.centre();
                assert_eq!(grid.cell_at(x, y), Some(cell));
                // Точки ближе к этой клетке, чем к соседним: ±(28, 7) с
                // запасом до половины ромба (29, 8).
                assert_eq!(grid.cell_at(x + 28, y + 7), Some(cell));
                assert_eq!(grid.cell_at(x - 28, y - 7), Some(cell));
            }
        }
        assert_eq!(grid.cell_at(-500, 0), None);
        assert_eq!(grid.cell_at(0, -500), None);
    }

    #[test]
    fn neighbour_of_edge_cell_leaves_the_grid() {
        let grid = Grid::new(2, 2);
        assert_eq!(grid.neighbour(Cell::new(0, 0), Direction::West), None);
        assert_eq!(grid.neighbour(Cell::new(0, 0), Direction::North), None);
        assert_eq!(grid.neighbour(Cell::new(1, 1), Direction::South), None);
        assert_eq!(grid.neighbour(Cell::new(1, 1), Direction::SouthEast), None);
        assert_eq!(
            grid.neighbour(Cell::new(0, 0), Direction::East),
            Some(Cell::new(1, 0))
        );
    }

    #[test]
    fn fine_grid_constants() {
        let grid = Grid::fine();
        assert_eq!((grid.columns, grid.rows), (160, 320));
        assert_eq!((STEP_X, STEP_Y, ODD_ROW_SHIFT), (58, 16, 29));
    }
}
