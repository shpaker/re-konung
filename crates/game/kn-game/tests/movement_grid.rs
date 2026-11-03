//! The engine's grid is checked against the Python reference and `k1-data`: the
//! pixels-to-cell conversion agrees with the forward one (issue #35, criterion 1).
//!
//! The only engine test that knows about `k1-data`: a comparison of formulas,
//! not a layer dependency — the engine never uses K1.

use kn_game::movement::grid::{Cell, Direction, FINE_HEIGHT, FINE_WIDTH, Grid};

#[test]
fn fine_grid_matches_k1_data() {
    let grid = Grid::fine();
    assert_eq!((grid.columns as usize, grid.rows as usize), (160, 320));
    for row in 0..FINE_HEIGHT {
        for column in 0..FINE_WIDTH {
            let cell = Cell::new(column, row);
            let ours = cell.centre();
            let theirs = k1_data::game_state::character_position(column as i32, row as i32);
            assert_eq!(ours, theirs, "cell {cell:?}");
            let (x, y) = ours;
            assert_eq!(
                grid.cell_at(x, y),
                Some(cell),
                "the reverse conversion disagrees for {cell:?}"
            );
            assert_eq!(
                k1_data::game_state::character_cell(f64::from(x), f64::from(y)),
                (column as i32, row as i32),
                "reference and engine read the point {x},{y} differently"
            );
        }
    }
}

#[test]
fn neighbours_on_the_fine_grid_match_the_original_table() {
    let grid = Grid::fine();
    // The original's offset table (Slavik game.cpp:2918-2947): rows of different
    // parity are shifted, so a neighbour depends on the row's parity.
    let expected_even = [
        (-1, 0),
        (0, -1),
        (0, -2),
        (1, -1),
        (1, 0),
        (1, 1),
        (0, 2),
        (0, 1),
    ];
    let expected_odd = [
        (-1, 0),
        (-1, -1),
        (0, -2),
        (0, -1),
        (1, 0),
        (0, 1),
        (0, 2),
        (-1, 1),
    ];
    for (odd_row, expected) in [(false, expected_even), (true, expected_odd)] {
        let from = Cell::new(80, if odd_row { 101 } else { 100 });
        for (direction, (dc, dr)) in Direction::ALL.into_iter().zip(expected) {
            assert_eq!(
                direction.offset(odd_row),
                (dc, dr),
                "{direction:?} odd_row={odd_row}"
            );
            let to = grid.neighbour(from, direction).unwrap();
            assert_eq!(
                to,
                Cell::new(
                    (from.column as i32 + dc) as u32,
                    (from.row as i32 + dr) as u32
                )
            );
            let (fx, fy) = from.centre();
            let (tx, ty) = to.centre();
            let (dx, dy) = direction.pixel_offset();
            assert_eq!(
                (tx - fx, ty - fy),
                (dx, dy),
                "{direction:?} odd_row={odd_row}"
            );
            // Pixel distance to a neighbour: 58 px east and west, 32 px north
            // and south, √(29² + 16²) ≈ 33.1 px along the screen diagonals.
            let expected = match direction {
                Direction::East | Direction::West => 58 * 58,
                Direction::North | Direction::South => 32 * 32,
                _ => 29 * 29 + 16 * 16,
            };
            assert_eq!(dx * dx + dy * dy, expected, "{direction:?}");
        }
    }
}

/// The K1 rules re-declare the offset table because `k1-game` must not depend
/// on `kn-game`; this holds the two copies together, so a change to the
/// engine's table cannot silently reorder the rules' "first free neighbour of
/// the workplace" (issue #36, criterion 3).
#[test]
fn the_rules_copy_of_the_offset_table_matches_the_engine() {
    for (index, direction) in Direction::ALL.into_iter().enumerate() {
        let (even, odd) = k1_game::rules::NEIGHBOUR_OFFSETS[index];
        assert_eq!(
            direction.offset(false),
            even,
            "{direction:?} (index {index}) on an even row"
        );
        assert_eq!(
            direction.offset(true),
            odd,
            "{direction:?} (index {index}) on an odd row"
        );
    }
}
