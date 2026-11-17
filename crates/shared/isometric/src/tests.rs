use crate::projection::round_half_even;
use crate::*;

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
fn projection_steps() {
    assert_eq!((STEP_X, STEP_Y, ODD_ROW_SHIFT), (58, 16, 29));
}
