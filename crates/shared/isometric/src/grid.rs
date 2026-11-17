//! Cells, directions and finite bounds of a staggered grid.

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
    /// rows are shifted left relative to the same column on even rows.
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

    /// Direction of the step from a cell to its neighbour, or `None` if the
    /// two cells are not neighbours.
    pub fn between(from: Cell, to: Cell) -> Option<Direction> {
        Direction::ALL
            .into_iter()
            .find(|&dir| from.neighbour(dir) == Some(to))
    }

    /// Directions alongside a step crossing a rhombus vertex. The movement
    /// consumer decides whether these cells permit the step.
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
}

impl Cell {
    pub const fn new(column: u32, row: u32) -> Cell {
        Cell { column, row }
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

    pub const fn contains(self, cell: Cell) -> bool {
        cell.column < self.columns && cell.row < self.rows
    }

    /// The neighbour inside the grid, otherwise `None`.
    pub fn neighbour(self, cell: Cell, direction: Direction) -> Option<Cell> {
        let next = cell.neighbour(direction)?;
        self.contains(next).then_some(next)
    }
}
