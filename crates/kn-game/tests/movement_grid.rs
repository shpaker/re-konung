//! Сетка мелкого движка сверяется с Python-эталоном и `k1-data`: обратный
//! перевод пикселей в клетку согласован с прямым (issue #35, критерий 1).
//!
//! Это единственный тест движка, который знает о `k1-data`: сверка формул, а не
//! зависимость — движок K1 не использует.

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
            assert_eq!(ours, theirs, "клетка {cell:?}");
            let (x, y) = ours;
            assert_eq!(
                grid.cell_at(x, y),
                Some(cell),
                "обратный перевод не совпал для {cell:?}"
            );
            assert_eq!(
                k1_data::game_state::character_cell(f64::from(x), f64::from(y)),
                (column as i32, row as i32),
                "эталон и движок по-разному думают о точке {x},{y}"
            );
        }
    }
}

#[test]
fn neighbours_on_the_fine_grid_match_the_original_table() {
    let grid = Grid::fine();
    // Таблица смещений оригинала (Slavik game.cpp:2918-2947): строки разной
    // чётности сдвинуты, поэтому сосед зависит от чётности строки.
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
            // Пиксельное расстояние до соседа: 58 px на восток и запад, 32 px
            // на север и юг, √(29² + 16²) ≈ 33,1 px по диагоналям экрана.
            let expected = match direction {
                Direction::East | Direction::West => 58 * 58,
                Direction::North | Direction::South => 32 * 32,
                _ => 29 * 29 + 16 * 16,
            };
            assert_eq!(dx * dx + dy * dy, expected, "{direction:?}");
        }
    }
}
