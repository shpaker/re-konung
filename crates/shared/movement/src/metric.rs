//! Integer pixel-length metric used by route search.

use kn_isometric::{Cell, Direction, STEP_X, STEP_Y};

/// Units per pixel in route costs.
pub const COST_UNITS_PER_PIXEL: u32 = 8;
const COST_DIAGONAL: u32 = 265;

/// Step cost for route search, in [`COST_UNITS_PER_PIXEL`] (1/8 pixel)
/// units: 8·58 east and west, 8·32 north and south, and 8·√(29² + 16²)
/// rounded up along the diagonals.
///
/// Rounding up is deliberate: a step must never cost less than its real
/// length, which is what keeps the search heuristic admissible
/// (docs/development/rust/movement.md, "Decision: step cost and the A* heuristic").
pub const fn step_cost(direction: Direction) -> u32 {
    match direction {
        Direction::East | Direction::West => COST_UNITS_PER_PIXEL * STEP_X as u32,
        Direction::North | Direction::South => COST_UNITS_PER_PIXEL * 2 * STEP_Y as u32,
        _ => COST_DIAGONAL,
    }
}

/// Distance between two cells' pixel points in [`step_cost`] units:
/// the straight line in pixels times 8, rounded down.
///
/// Rounding down is what the A* heuristic requires: it must not
/// overestimate.
pub fn cost_distance(from: Cell, other: Cell) -> u32 {
    let (ax, ay) = from.centre();
    let (bx, by) = other.centre();
    let dx = i64::from(ax - bx);
    let dy = i64::from(ay - by);
    let scaled = u64::try_from(64 * (dx * dx + dy * dy)).unwrap_or(u64::MAX);
    usize::try_from(scaled.isqrt())
        .map(|root| u32::try_from(root).unwrap_or(u32::MAX))
        .unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagonal_cost_matches_pixel_length() {
        for direction in Direction::ALL {
            let (dx, dy) = direction.pixel_offset();
            let scaled = 8.0 * f64::from(dx * dx + dy * dy).sqrt();
            let cost = f64::from(step_cost(direction));
            assert!(
                (cost - scaled).abs() < 8.0,
                "{direction:?}: cost {cost} is far from {scaled}"
            );
            assert!(
                cost >= scaled,
                "{direction:?}: cost {cost} is cheaper than the step's {scaled}"
            );
        }
        assert_eq!(step_cost(Direction::SouthEast), COST_DIAGONAL);
        assert_eq!(step_cost(Direction::East), 464);
        assert_eq!(step_cost(Direction::South), 256);
    }
}
