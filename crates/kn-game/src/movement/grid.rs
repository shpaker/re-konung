//! The isometric fine grid: cells, 8 directions, conversion to pixels.
//!
//! Geometry only. Walkability and occupancy arrive from outside
//! ([`super::passage::Passage`] and [`super::occupancy::Occupancy`]): map data
//! belongs to the supplier, not to the engine, so there are no formats, no
//! Bevy and no specifics of any part of the series here.
//!
//! The pixel formulas are agreed with `k1_data::game_state`
//! (`character_position` and `character_cell`). The engine needs no dependency
//! on K1, so the constants live here and the cross-check lives in the
//! `fine_grid_matches_k1_data` test (`tests/movement_grid.rs`).

/// Fine-grid horizontal step in pixels: half of the tile grid's step.
pub const STEP_X: i32 = 58;
/// Fine-grid vertical step in pixels: half of a tile row step.
pub const STEP_Y: i32 = 16;
/// Odd rows are shifted right by half the horizontal step.
pub const ODD_ROW_SHIFT: i32 = STEP_X / 2;

/// Columns of the K1 map's fine grid (`k1_data::map::FINE_WIDTH`).
pub const FINE_WIDTH: u32 = 160;
/// Rows of the K1 map's fine grid (`k1_data::map::FINE_HEIGHT`).
pub const FINE_HEIGHT: u32 = 320;

/// Unit of [`Direction::cost`]: 1/8 of a pixel.
pub const COST_UNITS_PER_PIXEL: u32 = 8;

/// A fine-grid cell: column and row counted from the map's top-left corner;
/// rows grow downward in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cell {
    pub column: u32,
    pub row: u32,
}

/// Step direction. The variant order is the order of the original's offset
/// table (Slavik `game.cpp:2918-2947`), so a direction index is also the key
/// into walk-animation frames. Names describe the screen: map pixel y grows
/// downward.
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

/// How many directions a step has: the length of the original's offset table, and
/// of one row of a caller's step-duration table.
pub const DIRECTION_COUNT: usize = 8;

impl Direction {
    /// Every direction, in the order of the original's table.
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

    /// Index in the original's table: 0 is west, then clockwise.
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

    /// Neighbour offset `(column, row)`. It depends on row parity because odd
    /// rows are shifted right.
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

    /// Offset of the cell's pixel point: independent of row parity — the row
    /// shift exactly cancels the difference between the two offset tables.
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

    /// Direction of the step from a cell to its neighbour, or `None` if the
    /// two cells are not neighbours.
    pub fn between(from: Cell, to: Cell) -> Option<Direction> {
        Direction::ALL
            .into_iter()
            .find(|&dir| from.neighbour(dir) == Some(to))
    }

    /// East, west, north and south: the neighbour touches the current cell at
    /// one corner only (the rhombi share a vertex). Such a step crosses the
    /// vertex where four cells meet, so it is allowed only when both cells
    /// alongside the path are walkable — otherwise the creature would cut a
    /// corner.
    ///
    /// Screen-diagonal steps (north-east and onward) share a whole edge with
    /// the current cell: there is no corner left for them to cut.
    pub const fn corner_flanks(self) -> Option<[Direction; 2]> {
        match self {
            Direction::East => Some([Direction::NorthEast, Direction::SouthEast]),
            Direction::West => Some([Direction::NorthWest, Direction::SouthWest]),
            Direction::North => Some([Direction::NorthWest, Direction::NorthEast]),
            Direction::South => Some([Direction::SouthWest, Direction::SouthEast]),
            _ => None,
        }
    }

    /// Step cost for route search, in [`COST_UNITS_PER_PIXEL`] (1/8 pixel)
    /// units: 8·58 east and west, 8·32 north and south, and 8·√(29² + 16²)
    /// rounded up along the diagonals.
    ///
    /// Rounding up is deliberate: a step must never cost less than its real
    /// length, which is what keeps the search heuristic admissible
    /// (docs/rust.md, "Decision: step cost and the A* heuristic").
    pub const fn cost(self) -> u32 {
        match self {
            Direction::East | Direction::West => COST_UNITS_PER_PIXEL * STEP_X as u32,
            Direction::North | Direction::South => COST_UNITS_PER_PIXEL * 2 * STEP_Y as u32,
            _ => COST_DIAGONAL,
        }
    }
}

/// `ceil(8 · √(29² + 16²))` — the diagonal step's cost; the
/// `diagonal_cost_matches_pixel_length` test checks the number.
const COST_DIAGONAL: u32 = 265;

impl Cell {
    pub const fn new(column: u32, row: u32) -> Cell {
        Cell { column, row }
    }

    /// The cell's pixel point — the centre of its rhombus, where the sprite
    /// is placed.
    ///
    /// The formula matches `k1_data::game_state::character_position`.
    pub fn centre(self) -> (i32, i32) {
        let column = self.column as i64;
        let row = self.row as i64;
        let shift = i64::from(2 - (row & 1) as i32) * i64::from(ODD_ROW_SHIFT);
        (
            (column * i64::from(STEP_X) + shift) as i32,
            (row * i64::from(STEP_Y) + i64::from(STEP_Y)) as i32,
        )
    }

    /// The neighbour in this direction, or `None` past the top-left edge of
    /// the coordinate space. The grid's bounds are [`Grid::neighbour`]'s job.
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

    /// Coordinates in the lattice without the row shift.
    ///
    /// Fine-grid cells form an ordinary square lattice in the basis of the
    /// south-east and north-east steps: the four steps that look diagonal on
    /// screen are orthogonal here, and east, west, north and south become the
    /// diagonals. Adjacency, cell distance and the corner-cutting rule then
    /// need no branch on row parity.
    pub fn lattice(self) -> (i64, i64) {
        let column = self.column as i64;
        let row = self.row as i64;
        (column + row / 2, column - (row + 1) / 2)
    }

    /// Back from the unshifted lattice, or `None` when the pair matches no
    /// cell — it lies past the top-left edge.
    pub fn from_lattice(u: i64, v: i64) -> Option<Cell> {
        let row = u - v;
        let column = (u + v + (row & 1)) / 2;
        if row < 0 || column < 0 {
            return None;
        }
        Some(Cell::new(column as u32, row as u32))
    }

    /// Fewest steps to another cell, ignoring walkability — Chebyshev distance
    /// in lattice coordinates.
    pub fn lattice_distance(self, other: Cell) -> u32 {
        let (a, b) = self.lattice();
        let (c, d) = other.lattice();
        i64::abs_diff(a, c).max(i64::abs_diff(b, d)) as u32
    }

    /// Distance between two cells' pixel points in [`Direction::cost`] units:
    /// the straight line in pixels times 8, rounded down.
    ///
    /// Rounding down is what the A* heuristic requires: it must not
    /// overestimate.
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

/// Size of the grid that neighbour lookups run in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    pub columns: u32,
    pub rows: u32,
}

impl Grid {
    pub const fn new(columns: u32, rows: u32) -> Grid {
        Grid { columns, rows }
    }

    /// The K1 map's fine grid: 160×320.
    pub const fn fine() -> Grid {
        Grid::new(FINE_WIDTH, FINE_HEIGHT)
    }

    pub const fn contains(self, cell: Cell) -> bool {
        cell.column < self.columns && cell.row < self.rows
    }

    /// The neighbour inside the grid, otherwise `None`.
    pub fn neighbour(self, cell: Cell, direction: Direction) -> Option<Cell> {
        let next = cell.neighbour(direction)?;
        self.contains(next).then_some(next)
    }

    /// The cell closest to a map pixel point.
    ///
    /// Halves round to even, as `round()` does in the Python reference and in
    /// `k1_data::game_state::character_cell`: otherwise the reverse conversion
    /// would disagree with the forward one on cell borders.
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

/// Division rounding halves to even, the way Python's `round()` does.
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

    /// Neighbours: the offset table and the pixel steps agree on even and on
    /// odd rows alike — all 8 directions.
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

    /// The cells alongside a step "through a corner" are its orthogonal
    /// neighbours: the step crosses a rhombus vertex where exactly four cells
    /// meet, and every flank sits half a step from that vertex along a screen
    /// axis.
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
                // A flank neighbours both the start and the end of the step.
                assert!(Direction::between(from, flank).is_some());
                assert!(Direction::between(to, flank).is_some());
                let (fx, fy) = flank.centre();
                let offset = (fx - mid_x, fy - mid_y);
                assert!(
                    half.contains(&offset),
                    "{direction:?}: {flank:?} sits off the step's vertex as {offset:?}"
                );
                seen.push(offset);
            }
            assert_ne!(seen[0], seen[1], "{direction:?}: flanks coincide");
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
                "{direction:?}: cost {cost} is far from {scaled}"
            );
            assert!(
                cost >= scaled,
                "{direction:?}: cost {cost} is cheaper than the step's {scaled}"
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

    /// A lattice point past the top-left corner must not become a cell:
    /// otherwise a goal neighbourhood near the map edge would pick up cells
    /// from the opposite corner.
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
                // Points nearer this cell than its neighbours: ±(28, 7), with
                // margin inside half a rhombus (29, 8).
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
