//! Per-tick behavior decisions for K1 creatures.
//!
//! Only confirmed rules from Slavik / original engine. Unknown rules are not
//! invented; what is not confirmed is recorded in `docs/` and not implemented.
//! Criteria 2--5, 9 of #36.
//!
//! The public API is a two-phase tick: [`evaluate`] then [`apply`] so that
//! decisions are not order-dependent (see docs/rust.md).

use std::collections::HashMap;

use super::movement::grid::Cell;
use super::movement::pace::Pace;
use super::movement::passage::Passage;
use super::movement::world::World;
use k1_game::creatures::{Creature, Workplace};

/// Class constants from Slavik (`game.h`). Byte 0x03 of `Character`.
pub mod class {
    /// Warrior (воин).
    pub const WARRIOR: u8 = 0x41;
    /// Fighter-fencer (фехтовальщик).
    pub const FENCER: u8 = 0x42;
    /// Archer (лучник).
    pub const ARCHER: u8 = 0x43;
    /// Knight-mercenary (наёмный рыцарь).
    pub const MERCENARY: u8 = 0x44;
    /// Cavalryman (конница).
    pub const CAVALRY: u8 = 0x45;
    /// Bowman (стрелок из лука).
    pub const BOWMAN: u8 = 0x46;
    /// Sellsword (ополченец).
    pub const SELLSWORD: u8 = 0x47;
    /// Mounted knight (конный наёмник).
    pub const MOUNTED: u8 = 0x48;
    /// Veteran cavalry veteran (ветеран конницы).
    pub const VETERAN_CAVALRY: u8 = 0x49;
    /// Master fencer (мастер фехтования).
    pub const MASTER_FENCER: u8 = 0x4a;
    /// Royal guard (королевская гвардия).
    pub const ROYAL_GUARD: u8 = 0x4b;
    /// Bowman-master (мастер лучников).
    pub const BOWMAN_MASTER: u8 = 0x4c;
    /// Mounted master (мастер конных наёмников).
    pub const MOUNTED_MASTER: u8 = 0x4d;
    /// Heavy infantry veteran (ветеран пехоты).
    pub const HEAVY_INFANTRY: u8 = 0x4e;
    /// Imperial guard (императорская гвардия).
    pub const IMPERIAL_GUARD: u8 = 0x4f;
    /// Village man (поселянин).
    pub const VILLAGER: u8 = 0x50;
    /// Woman (женщина).
    pub const WOMAN: u8 = 0x51;
    /// Child (ребёнок).
    pub const CHILD: u8 = 0x52;
    /// Lone NPC (одиночный NPC).
    pub const LONE_NPC: u8 = 0x53;
}

// ─── Evaluate phase ────────────────────────────────────────────────────────

/// Decision produced by one creature during evaluation.
#[derive(Debug, Clone, Copy)]
pub enum Decision {
    /// Stay where it is — do not issue any world command.
    None,
    /// Move to target cell (fine-grid lattice coordinates).
    MoveTo(u32, u32),
}

/// Evaluate every creature and return decisions indexed like the roster.
///
/// Uses only read access — no world mutations yet. This is the *Evaluate* half
/// of the tick loop (see docs/rust.md, "Evaluation vs application in a tick").
pub fn evaluate(creatures: &[Creature], _workplaces: &HashMap<usize, Workplace>) -> Vec<Decision> {
    let mut decisions = Vec::with_capacity(creatures.len());
    for creature in creatures {
        decisions.push(decision_for(creature));
    }
    decisions
}

fn decision_for(creature: &Creature) -> Decision {
    match creature_type(creature) {
        CreatureType::LoneNpc => Decision::None, // Criterion 5
        CreatureType::Villager => villager_decision(creature),
        CreatureType::RoamingMonster | CreatureType::WorkerMonster => Decision::None,
    }
}

/// What category of creature this record represents.
enum CreatureType {
    LoneNpc,
    Villager,
    RoamingMonster,
    WorkerMonster,
}

fn creature_type(creature: &Creature) -> CreatureType {
    if creature.class_id == class::LONE_NPC {
        return CreatureType::LoneNpc;
    }
    if is_villager(creature) {
        return CreatureType::Villager;
    }
    if creature.roaming {
        return CreatureType::RoamingMonster;
    }
    CreatureType::WorkerMonster
}

fn is_villager(creature: &Creature) -> bool {
    matches!(
        creature.class_id,
        class::VILLAGER | class::WOMAN | class::CHILD
    )
}

/// Villager behavior per tick.
///
/// Women and children: 25 % walk chance.
/// Citizens with work slot filled: move to workplace cell.
/// Without work: 50% walk.
fn villager_decision(_creature: &Creature) -> Decision {
    // TODO: equip data needed for slot checking; RNG needed for probability.
    Decision::None
}

// ─── Apply phase ──────────────────────────────────────────────────────────

/// Apply evaluated decisions through the world.
///
/// Creatures are processed in their roster order (lower ID gets priority when
/// multiple want the same cell — see docs/rust.md, "Decision: who yields a cell").
pub fn apply<P: Passage, K: Pace>(
    world: &mut World<P, K>,
    creatures: &[Creature],
    decisions: &[Decision],
) {
    for (index, creature) in creatures.iter().enumerate() {
        let Some(world_id) = world_creature_id(index, creatures) else {
            continue;
        };
        let decision = decisions[index];
        match decision {
            Decision::None => {}
            Decision::MoveTo(tx, ty) => {
                let target = Cell::new(tx, ty);
                if let Err(e) = world.move_to(world_id, target) {
                    eprintln!("{creature:?}: move_to error {e:?}");
                }
            }
        }
    }
}

/// Map roster index to World-assigned CreatureId.
///
/// TODO: wire up when creatures are actually added to the world during init.
/// For now returns None so the apply phase is a no-op without errors.
fn world_creature_id(
    _index: usize,
    _creatures: &[Creature],
) -> Option<super::movement::CreatureId> {
    None
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn make_creature(class_id: u8, roaming: bool, id: usize) -> Creature {
        Creature {
            id,
            cell: (10, 10),
            direction: 0,
            state: 0,
            class_id,
            group: 0,
            roaming,
            spawn: k1_data::game_state::Rect {
                left: 0,
                top: 0,
                right: 50,
                bottom: 50,
            },
            workplace: None,
            sprite_set: 0,
        }
    }

    #[test]
    fn lone_npc_class_53_gets_no_movement_decision() {
        // Criterion 5: lone NPCs (class 0x53) take no movement decisions.
        let npc = make_creature(class::LONE_NPC, false, 42);
        let decisions = evaluate(&[npc], &HashMap::new());
        assert_eq!(decisions.len(), 1);
        assert!(matches!(decisions[0], Decision::None));
    }

    #[test]
    fn villager_class_50_produces_decision() {
        // Villagers produce decisions (currently None until equip data available).
        let v = make_creature(class::VILLAGER, false, 10);
        let decisions = evaluate(&[v], &HashMap::new());
        assert_eq!(decisions.len(), 1);
        assert!(matches!(decisions[0], Decision::None));
    }

    #[test]
    fn woman_class_51_produces_decision() {
        let w = make_creature(class::WOMAN, false, 20);
        let decisions = evaluate(&[w], &HashMap::new());
        assert!(matches!(decisions[0], Decision::None));
    }

    #[test]
    fn child_class_52_produces_decision() {
        let c = make_creature(class::CHILD, false, 30);
        let decisions = evaluate(&[c], &HashMap::new());
        assert!(matches!(decisions[0], Decision::None));
    }

    #[test]
    fn warrior_class_41_not_villager() {
        let warrior = make_creature(class::WARRIOR, false, 50);
        let decisions = evaluate(&[warrior], &HashMap::new());
        assert!(matches!(decisions[0], Decision::None));
    }

    #[test]
    fn roaming_monster_produces_none_for_now() {
        // Roaming monsters get no decision until group-state wiring.
        let m = make_creature(0x41, true, 100);
        let decisions = evaluate(&[m], &HashMap::new());
        assert!(matches!(decisions[0], Decision::None));
    }

    #[test]
    fn empty_roster_returns_empty_decisions() {
        let decisions = evaluate(&[], &HashMap::new());
        assert!(decisions.is_empty());
    }

    #[test]
    fn mixed_roster_indexed_correctly() {
        let creatures = vec![
            make_creature(class::LONE_NPC, false, 1), // None
            make_creature(class::WOMAN, false, 2),    // None (TODO)
            make_creature(class::WARRIOR, false, 3),  // None (TODO)
        ];
        let decisions = evaluate(&creatures, &HashMap::new());
        assert_eq!(decisions.len(), 3);
        assert!(matches!(decisions[0], Decision::None));
        assert!(matches!(decisions[1], Decision::None));
        assert!(matches!(decisions[2], Decision::None));
    }
}
