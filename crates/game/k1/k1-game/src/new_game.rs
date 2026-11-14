//! Creation-time attribute allocation. Source and edition checks: docs/k1/new-game.md.

/// Attribute order: agility, charisma, strength, endurance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Allocation {
    pub values: [i16; 4],
    pub free_points: i32,
}

pub const DEFAULT_ATTRIBUTES: [[i16; 4]; 4] = [
    [3, 3, 15, 10],
    [15, 3, 3, 10],
    [3, 3, 10, 12],
    [3, 25, 7, 7],
];
const PRIMARY: [usize; 4] = [2, 0, 3, 1];
// The original menu evaluates upgrades at level 10, before starting at level 1.
pub const LIMITS: [[i16; 4]; 4] = [
    [37, 37, 95, 37],
    [95, 37, 37, 37],
    [37, 37, 37, 95],
    [37, 95, 37, 37],
];
pub const COST_BANDS: [[i32; 3]; 13] = [
    [13, 9, 2],
    [19, 14, 5],
    [26, 20, 11],
    [34, 28, 14],
    [43, 36, 18],
    [53, 44, 23],
    [63, 52, 29],
    [74, 60, 35],
    [80, 65, 72],
    [90, 70, 100],
    [95, 75, 150],
    [100, 80, 200],
    [999999, 999999, 999],
];

fn cost(class: usize, attribute: usize, value: i16) -> i32 {
    let column = usize::from(PRIMARY[class] != attribute);
    COST_BANDS
        .iter()
        .find(|band| i32::from(value) < band[column])
        .unwrap()[2]
}

fn invested(class: usize, values: [i16; 4]) -> i32 {
    values
        .into_iter()
        .enumerate()
        .map(|(attribute, value)| {
            (0..value)
                .map(|step| cost(class, attribute, step))
                .sum::<i32>()
        })
        .sum()
}

/// Recomputes the conserved budget instead of trusting a frontend point counter.
pub fn allocation(class: usize, values: Option<[i16; 4]>) -> Option<Allocation> {
    let initial = *DEFAULT_ATTRIBUTES.get(class)?;
    let values = values.unwrap_or(initial);
    if values
        .iter()
        .zip(LIMITS[class])
        .any(|(&value, limit)| value < 0 || value > limit)
    {
        return None;
    }
    let free_points = invested(class, initial) - invested(class, values);
    (free_points >= 0).then_some(Allocation {
        values,
        free_points,
    })
}

impl Allocation {
    /// One original plus/minus operation, including refunds across price bands.
    pub fn adjusted(self, class: usize, attribute: usize, delta: i16) -> Option<Self> {
        if !matches!(delta, -1 | 1) {
            return None;
        }
        let mut values = self.values;
        let value = values.get_mut(attribute)?;
        *value = value.checked_add(delta)?;
        allocation(class, Some(values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refunds_are_reversible_without_creating_points() {
        for class in 0..4 {
            let initial = allocation(class, None).unwrap();
            assert_eq!(initial.free_points, 0);
            for attribute in 0..4 {
                assert!(initial.adjusted(class, attribute, 1).is_none());
                let mut current = initial;
                while let Some(next) = current.adjusted(class, attribute, -1) {
                    assert_eq!(next.adjusted(class, attribute, 1), Some(current));
                    current = next;
                }
                assert_eq!(current.values[attribute], 0);
            }
        }
    }

    #[test]
    fn price_band_refund_uses_the_lower_value() {
        let warrior = allocation(0, None).unwrap();
        let lower = warrior.adjusted(0, 2, -1).unwrap();
        assert_eq!(lower.free_points, 5);
        let third = lower
            .adjusted(0, 2, -1)
            .unwrap()
            .adjusted(0, 2, -1)
            .unwrap();
        assert_eq!(third.free_points, 12);
        assert_eq!(third.adjusted(0, 0, 1).unwrap().free_points, 10);
    }

    #[test]
    fn forged_and_out_of_range_allocations_are_rejected() {
        assert!(allocation(4, None).is_none());
        for (class, &limits) in LIMITS.iter().enumerate() {
            assert!(allocation(class, Some([-1, 0, 0, 0])).is_none());
            assert!(allocation(class, Some([i16::MAX; 4])).is_none());
            assert!(allocation(class, Some(limits)).is_none());
            assert!(
                allocation(class, None)
                    .unwrap()
                    .adjusted(class, 4, 1)
                    .is_none()
            );
        }
    }
}
