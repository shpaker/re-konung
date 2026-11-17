//! Recovered K1 target policies; evidence and intentional limits: creatures/combat.md.

use crate::rules::{Ground, Random, group, neighbour};

/// Only placed creatures enter this snapshot, in character-record order.
#[derive(Debug, Clone, Copy)]
pub struct Combatant<Id = usize> {
    pub id: Id,
    pub group: usize,
    pub group_kind: u8,
    pub class_id: u8,
    pub state: u8,
    pub cell: (i16, i16),
    pub target: Option<Id>,
}

impl<Id: Copy> Combatant<Id> {
    pub fn alive(self) -> bool {
        self.class_id & 0x80 == 0 && !matches!(self.state, 3 | 9)
    }

    pub fn roaming(self) -> bool {
        (group::FIRST_ROAMING..=group::LAST_ROAMING).contains(&self.group_kind)
    }
}

/// The original's integer ranking, not movement cost on the isometric lattice.
pub fn distance(from: (i16, i16), to: (i16, i16)) -> u32 {
    let horizontal = i32::from(from.0).abs_diff(i32::from(to.0));
    let vertical = i32::from(from.1).abs_diff(i32::from(to.1));
    horizontal.max(vertical) + u32::from(horizontal.min(vertical) > 1)
}

pub fn ambush(class_id: u8) -> bool {
    matches!(class_id, 0x44 | 0x57)
}

pub fn opponents<Id: Copy>(attacker: Combatant<Id>, target: Combatant<Id>) -> bool {
    attacker.alive()
        && target.alive()
        && attacker.group != target.group
        && ((attacker.group_kind == group::PLAYER && target.roaming())
            || (attacker.roaming() && target.group_kind == group::PLAYER))
}

/// Caller gates acquisition on the group's live combat decision and animation readiness.
pub fn monster_target<Id: Copy + Ord>(
    attacker: Combatant<Id>,
    creatures: &[Combatant<Id>],
) -> Option<Id> {
    if !attacker.roaming() || !attacker.alive() || matches!(attacker.class_id, 0x51..=0x53) {
        return None;
    }
    let candidates: Vec<_> = creatures
        .iter()
        .copied()
        .filter(|target| opponents(attacker, *target))
        .collect();
    if ambush(attacker.class_id) {
        return (0..8).find_map(|direction| {
            let cell = neighbour(attacker.cell, direction)?;
            candidates
                .iter()
                .find(|target| target.cell == cell)
                .map(|target| target.id)
        });
    }
    let shared_target = || {
        creatures
            .iter()
            .filter(|ally| ally.group == attacker.group && ally.alive())
            .filter_map(|ally| ally.target)
            .find(|id| candidates.iter().any(|target| target.id == *id))
    };
    let nearest = |unassigned: bool| {
        candidates
            .iter()
            .filter(|target| {
                !unassigned
                    || !creatures.iter().any(|ally| {
                        ally.group == attacker.group
                            && ally.alive()
                            && ally.target == Some(target.id)
                    })
            })
            .min_by_key(|target| (distance(attacker.cell, target.cell), target.id))
            .map(|target| target.id)
    };
    match attacker.class_id {
        0x41 | 0x4d | 0x4f => shared_target().or_else(|| nearest(false)),
        0x43 | 0x45 | 0x46 | 0x4c | 0x55 => nearest(true).or_else(|| nearest(false)),
        _ => nearest(false),
    }
}

/// An explicit move/attack order suppresses this response in the caller.
pub fn retaliation<Id: Copy + Eq>(
    attacker: Combatant<Id>,
    creatures: &[Combatant<Id>],
) -> Option<Id> {
    (0..8).find_map(|direction| {
        let cell = neighbour(attacker.cell, direction)?;
        creatures
            .iter()
            .find(|other| {
                other.cell == cell
                    && opponents(attacker, **other)
                    && other.target == Some(attacker.id)
            })
            .map(|other| other.id)
    })
}

/// Fixed original 800x600 view extent: modern window size must not change the escape rule.
pub fn retreat_target<G: Ground, R: Random>(
    cell: (i16, i16),
    ground: &G,
    random: &mut R,
) -> Option<(i16, i16)> {
    const HORIZONTAL: i16 = 9;
    const VERTICAL: i16 = 20;
    let side = random.below(4);
    let horizontal_edge = side < 2;
    let extent = if horizontal_edge {
        HORIZONTAL
    } else {
        VERTICAL
    };
    let fixed = match side {
        0 => -VERTICAL * 2,
        1 => VERTICAL * 2,
        2 => -HORIZONTAL * 2,
        _ => HORIZONTAL * 2,
    };
    (-extent..extent).find_map(|offset| {
        let delta = if horizontal_edge {
            (offset, fixed)
        } else {
            (fixed, offset)
        };
        let target = (cell.0.checked_add(delta.0)?, cell.1.checked_add(delta.1)?);
        ground.is_free(target).then_some(target)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retreat_checks_each_edge_and_skips_blocked_candidates() {
        struct Field(Vec<(i16, i16)>);
        impl Ground for Field {
            fn is_free(&self, cell: (i16, i16)) -> bool {
                self.0.contains(&cell)
            }
        }
        struct Choice(u64);
        impl Random for Choice {
            fn below(&mut self, limit: u64) -> u64 {
                assert_eq!(limit, 4);
                self.0
            }
        }
        for (side, first) in [(71, 120), (71, 200), (62, 140), (98, 140)]
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                retreat_target((80, 160), &Field(vec![first]), &mut Choice(side as u64)),
                Some(first)
            );
            let next = if side < 2 {
                (first.0 + 1, first.1)
            } else {
                (first.0, first.1 + 1)
            };
            assert_eq!(
                retreat_target((80, 160), &Field(vec![next]), &mut Choice(side as u64)),
                Some(next)
            );
            assert_eq!(
                retreat_target((80, 160), &Field(vec![]), &mut Choice(side as u64)),
                None
            );
        }
    }

    fn actor(id: usize, cell: (i16, i16), monster: bool) -> Combatant {
        Combatant {
            id,
            group: usize::from(monster),
            group_kind: if monster { 0x41 } else { group::PLAYER },
            class_id: if monster { 0x42 } else { 5 },
            state: 0,
            cell,
            target: None,
        }
    }

    #[test]
    fn original_distance_and_record_order_resolve_ties() {
        assert_eq!(distance((10, 10), (12, 12)), 3);
        assert_eq!(distance((10, 10), (12, 11)), 2);
        let attacker = actor(10, (10, 10), true);
        let mut creatures = vec![
            actor(0, (12, 10), false),
            actor(1, (11, 10), false),
            actor(2, (9, 10), false),
            attacker,
        ];
        assert_eq!(monster_target(attacker, &creatures), Some(1));
        creatures[1].class_id |= 0x80;
        assert_eq!(monster_target(attacker, &creatures), Some(2));
        creatures[2].state = 3;
        assert_eq!(monster_target(attacker, &creatures), Some(0));
        creatures[0].state = 9;
        assert_eq!(monster_target(attacker, &creatures), None);
    }

    #[test]
    fn class_policies_share_or_distribute_targets() {
        let mut attacker = actor(10, (10, 10), true);
        let mut ally = actor(11, (20, 10), true);
        ally.target = Some(1);
        let creatures = vec![actor(0, (11, 10), false), actor(1, (20, 10), false), ally];
        for class_id in [0x41, 0x4d, 0x4f] {
            attacker.class_id = class_id;
            assert_eq!(monster_target(attacker, &creatures), Some(1));
        }
        for class_id in [0x43, 0x45, 0x46, 0x4c, 0x55] {
            attacker.class_id = class_id;
            assert_eq!(monster_target(attacker, &creatures), Some(0));
        }
    }

    #[test]
    fn ambush_and_response_scan_neighbours_in_direction_order() {
        let mut attacker = actor(10, (10, 10), true);
        attacker.class_id = 0x44;
        let mut west = actor(1, (9, 10), false);
        let east = actor(0, (11, 10), false);
        assert_eq!(monster_target(attacker, &[east, west]), Some(1));
        west.target = Some(10);
        assert_eq!(retaliation(attacker, &[east, west]), Some(1));
        assert_eq!(monster_target(attacker, &[actor(0, (15, 10), false)]), None);
    }
}
