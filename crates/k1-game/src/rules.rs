//! K1 creature behavior classification and decision evaluation.
//!
//! Pure K1 logic: classifies creatures by [`CreatureType`] and produces
//! [`Decision`]s from roster data. This module owns neither [`kn_game`] nor
//! [`World`]; application lives in kn-game where those types exist.
//!
//! Criterion 5 of epic #36: lone NPCs (group kind 0x53) produce
//! [`Decision::None`]. Other types are placeholders pending equipment data
//! and monster group-state wiring.

use crate::creatures::Creature;

/// Group kind byte from Game.N (`kind` field at offset 0x1E).
pub mod group {
    /// Player group (отряд игрока).
    pub const PLAYER: u8 = 0x20;
    /// Roaming monster groups (бродячие отряды).
    pub const FIRST_ROAMING: u8 = 0x41;
    /// Last roaming kind.
    pub const LAST_ROAMING: u8 = 0x50;
    /// Lone NPC (одиночный NPC).
    pub const LONE_NPC: u8 = 0x53;
    /// Village villager groups (жители деревень), lowest 2 bits = side.
    pub const VILLAGE_FIRST: u8 = 0x80;
}

/// Class constants from Slavik (`game.h`). Byte 0x04 of `Character`.
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
}

/// Decision produced by one creature during evaluation.
#[derive(Debug, Clone, Copy)]
pub enum Decision {
    /// No action — creature stays where it is.
    None,
    /// Move to target cell (fine-grid lattice coordinates).
    MoveTo(u32, u32),
}

/// What category of creature this record represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatureType {
    /// Member of a wandering monster group (kind 0x41–0x50).
    RoamingMonster,
    /// Lone NPC (group kind 0x53). Never moves.
    LoneNpc,
    /// Village villager (kind 0x80–0x82).
    Villager,
    /// Player character or warrior in a non-roaming group.
    PlayerOrWarrior,
}

/// Evaluate every creature and return decisions indexed like the roster.
pub fn evaluate(creatures: &[Creature]) -> Vec<Decision> {
    let mut decisions = Vec::with_capacity(creatures.len());
    for creature in creatures {
        decisions.push(decision_for(creature));
    }
    decisions
}

fn decision_for(creature: &Creature) -> Decision {
    match classify(creature) {
        CreatureType::LoneNpc => Decision::None, // Criterion 5
        CreatureType::Villager => villager_decision(creature),
        CreatureType::RoamingMonster | CreatureType::PlayerOrWarrior => Decision::None,
    }
}

/// Classify a creature by its group kind and role.
///
/// Classification order matters: lone NPC check uses group_kind first because
/// `0x53` is a *group* value, not a character class byte.
pub fn classify(creature: &Creature) -> CreatureType {
    match creature.group_kind {
        group::LONE_NPC => CreatureType::LoneNpc,
        group::FIRST_ROAMING..=group::LAST_ROAMING => CreatureType::RoamingMonster,
        group::VILLAGE_FIRST.. => CreatureType::Villager,
        _ => {
            // Player group (0x20) or unrecognised kind — always player/warrior.
            CreatureType::PlayerOrWarrior
        }
    }
}

/// Villager behavior per tick (placeholder).
///
/// Women and children (class 0x51/0x52): 25 % walk chance.
/// Citizens with work slot filled: move to workplace cell.
/// Without work: 50% walk. Needs equipment data + RNG.
fn villager_decision(_creature: &Creature) -> Decision {
    Decision::None
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_creature(group_kind: u8, id: usize) -> Creature {
        Creature {
            id,
            cell: (10, 10),
            direction: 0,
            state: 0,
            class_id: 0,
            group: 0,
            group_kind,
            roaming: matches!(group_kind, group::FIRST_ROAMING..=group::LAST_ROAMING),
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

    // ── Classification tests (by independent group_kind values) ──

    #[test]
    fn group_0x53_is_lone_npc() {
        assert_eq!(
            classify(&make_creature(group::LONE_NPC, 1)),
            CreatureType::LoneNpc
        );
    }

    #[test]
    fn group_0x20_is_player_or_warrior() {
        assert_eq!(
            classify(&make_creature(group::PLAYER, 1)),
            CreatureType::PlayerOrWarrior
        );
    }

    #[test]
    fn group_0x41_to_0x50_are_roaming_monsters() {
        for kind in group::FIRST_ROAMING..=group::LAST_ROAMING {
            assert_eq!(
                classify(&make_creature(kind, 0)),
                CreatureType::RoamingMonster
            );
        }
    }

    #[test]
    fn group_0x80_to_0xff_are_villagers() {
        for kind in group::VILLAGE_FIRST..=0xff {
            assert_eq!(classify(&make_creature(kind, 0)), CreatureType::Villager);
        }
    }

    // Negative: unknown group kinds fall through to player_or_warrior.
    #[test]
    fn unknown_group_kinds_default_to_player_or_warrior() {
        assert_eq!(
            classify(&make_creature(0, 0)), // empty slot
            CreatureType::PlayerOrWarrior
        );
        assert_eq!(
            classify(&make_creature(0x10, 0)), // low value
            CreatureType::PlayerOrWarrior
        );
    }

    // ── Decision tests ──

    #[test]
    fn lone_npc_gets_no_movement_decision() {
        // Criterion 5: lone NPCs (group kind 0x53) take no movement decisions.
        let npc = make_creature(group::LONE_NPC, 42);
        let decisions = evaluate(&[npc]);
        assert_eq!(decisions.len(), 1);
        assert!(matches!(decisions[0], Decision::None));
    }

    #[test]
    fn roaming_monster_gets_none_placeholder() {
        let m = make_creature(group::FIRST_ROAMING, 100);
        let decisions = evaluate(&[m]);
        assert!(matches!(decisions[0], Decision::None));
    }

    #[test]
    fn empty_roster_returns_empty_decisions() {
        let decisions = evaluate(&[]);
        assert!(decisions.is_empty());
    }

    #[test]
    fn mixed_roster_indexed_correctly() {
        let creatures = vec![
            make_creature(group::LONE_NPC, 1),      // None
            make_creature(group::FIRST_ROAMING, 2), // None placeholder
            make_creature(group::VILLAGE_FIRST, 3), // None placeholder
        ];
        let decisions = evaluate(&creatures);
        assert_eq!(decisions.len(), 3);
        for d in &decisions {
            assert!(matches!(d, Decision::None));
        }
    }
}
