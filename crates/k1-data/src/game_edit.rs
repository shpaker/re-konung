//! Добавление и удаление персонажей в начальном состоянии `Game.N`
//! (docs/k1/formats/game-state.md, эталон — `konung/k1/game_edit.py`).
//!
//! Отряды уложены плотно: за отрядом зарезервировано growth_limit слотов
//! персонажей, его члены — первые size из них, следующий отряд начинается сразу
//! за резервом. Ссылки на номер персонажа — первый член отряда, враг
//! (номер + 1, 0 — нет) и персонаж рабочего места деревни (0 — место свободно);
//! на номер отряда — отряд персонажа, староста деревни и отряд-цель. Правки
//! ниже сохраняют эту укладку и переписывают все ссылки, поэтому остальной
//! файл не меняется.
//!
//! В отличие от Python-эталона, `add_character`/`remove_character` принимают
//! номера записей, а сами объекты читают из состояния: в Rust нельзя занять
//! `&Character` из `state` и одновременно править `state`.

use std::collections::{HashSet, VecDeque};

use thiserror::Error;

use crate::game_state::{
    CHARACTER_SLOTS, Character, GROUP_SLOTS, GameState, Group, ITEM_SLOTS, Item, PLAYER_GROUP,
};

/// NPC на своих местах: обычно отряд из одного персонажа без резерва, но в каждом
/// Game.N есть такие отряды из 2, 14 и 21 персонажа
pub const NPC_GROUP: u8 = 0x53;
/// Байт диалога 0xD0 у персонажа без своего диалога — так у большинства в файлах
pub const NO_DIALOG: u8 = 0xFF;
/// Номер записи колчана со стрелами — восьмое из «разного» (0xCC, Slavik game.h:498,
/// game_character.cpp:694); во всех файлах 0, но это тоже ссылка на предмет
pub const ARROWS: usize = 8;
/// Резерв, размер и наибольший размер отряда — по байту
pub const GROUP_RESERVE_MAX: u8 = 0xFF;

/// Правку нельзя сделать, не сломав укладку или ссылки.
#[derive(Debug, Error)]
pub enum EditError {
    #[error("персонажей больше {}: не хватает слотов", CHARACTER_SLOTS)]
    NotEnoughCharacterSlots,
    #[error("отрядов больше {}: не хватает слотов", GROUP_SLOTS)]
    NotEnoughGroupSlots,
    #[error("предметов больше {}: не хватает слотов", ITEM_SLOTS)]
    NotEnoughItemSlots,
    #[error("персонажа {slot} нет")]
    NoSuchCharacter { slot: usize },
    #[error("отряда {slot} нет")]
    NoSuchGroup { slot: usize },
    #[error("персонажей отряда игрока удалять нельзя")]
    PlayerRemoval,
    #[error("добавлять персонажей в отряд игрока нельзя")]
    PlayerGroupAdd,
    #[error("персонаж {slot} занимает рабочее место деревни — его удалять нельзя")]
    WorkingCharacter { slot: usize },
    #[error(
        "в отряде {slot} уже {} персонажей — больше не помещается",
        GROUP_RESERVE_MAX
    )]
    ReserveFull { slot: usize },
}

pub fn add_character(
    state: &mut GameState,
    template_slot: usize,
    group_slot: usize,
    x: i16,
    y: i16,
) -> Result<usize, EditError> {
    let group = find_group(state, group_slot)?.clone();
    if group.kind == PLAYER_GROUP {
        return Err(EditError::PlayerGroupAdd);
    }
    let own_group = group.kind == NPC_GROUP && group.growth_limit == 1;
    let grows = !own_group && group.size == group.growth_limit;
    if grows && group.growth_limit == GROUP_RESERVE_MAX {
        return Err(EditError::ReserveFull { slot: group_slot });
    }
    let template = find_character(state, template_slot)?;
    check_room(
        state,
        template,
        if own_group || grows { 1 } else { 0 },
        usize::from(own_group),
    )?;
    let mut character = template.clone();
    character.x = x;
    character.y = y;
    character.enemy = 0;
    character.unknown_d0 = NO_DIALOG;
    clone_items(state, &mut character);
    let mut target_group = group_slot;
    if own_group {
        // Копия одиночного NPC получает свой отряд — копию отряда template
        // в первом свободном слоте, за резервом последнего отряда
        let last = state
            .groups
            .iter()
            .flatten()
            .next_back()
            .expect("в редактируемом состоянии есть хотя бы один отряд");
        let mut new_group = find_group(state, group_slot)?.clone();
        new_group.slot = last.slot + 1;
        new_group.first_character = reserves_end(state) as i16;
        new_group.size = 0;
        new_group.max_size = 1;
        new_group.growth_limit = 1;
        target_group = new_group.slot;
        state.groups[target_group] = Some(new_group);
    } else if grows {
        grow(state, group_slot)?;
    }
    let slot = {
        let group = find_group_mut(state, target_group)?;
        group.first_character as usize + group.size as usize
    };
    character.slot = slot;
    character.group = target_group as u8;
    state.characters[slot] = Some(character);
    let group = find_group_mut(state, target_group)?;
    group.size += 1;
    group.max_size = group.max_size.max(group.size);
    Ok(slot)
}

pub fn remove_character(state: &mut GameState, slot: usize) -> Result<(), EditError> {
    let character = find_character(state, slot)?.clone();
    let group_slot = character.group as usize;
    let group = find_group(state, group_slot)?.clone();
    if group.kind == PLAYER_GROUP {
        return Err(EditError::PlayerRemoval);
    }
    if workers(state).contains(&slot) {
        return Err(EditError::WorkingCharacter { slot });
    }
    free_items(state, &character);
    // Следующие члены того же отряда сдвигаются на слот вниз
    let last = group.first_character as usize + group.size as usize - 1;
    renumber(state, &|other| {
        if other == slot {
            None
        } else if slot < other && other <= last {
            Some(other - 1)
        } else {
            Some(other)
        }
    })?;
    let group = find_group_mut(state, group_slot)?;
    group.size -= 1;
    // Оставшийся пустым отряд одиночного NPC убирается целиком
    if group.size == 0 && group.kind == NPC_GROUP {
        remove_group(state, group_slot)?;
    }
    Ok(())
}

fn grow(state: &mut GameState, slot: usize) -> Result<(), EditError> {
    // Увеличить резерв отряда на слот: всё за ним сдвигается на слот вверх
    let group = find_group(state, slot)?.clone();
    let end = group.first_character as usize + group.growth_limit as usize;
    renumber(state, &|other| {
        Some(if other >= end { other + 1 } else { other })
    })?;
    for other in state.groups.iter_mut().flatten() {
        if other.slot > slot {
            other.first_character += 1;
        }
    }
    find_group_mut(state, slot)?.growth_limit += 1;
    Ok(())
}

fn remove_group(state: &mut GameState, slot: usize) -> Result<(), EditError> {
    // Убрать пустой отряд: следующие сдвигаются на слот вниз, их персонажи —
    // на его резерв, ссылки на номера отрядов переписываются
    let group = find_group(state, slot)?.clone();
    let reserve = group.growth_limit as usize;
    let end = group.first_character as usize + reserve;
    renumber(state, &|other| {
        Some(if other >= end { other - reserve } else { other })
    })?;
    let old = std::mem::take(&mut state.groups);
    let mut groups: Vec<Option<Group>> = vec![None; GROUP_SLOTS];
    for mut other in old.into_iter().flatten().filter(|g| g.slot != slot) {
        if other.slot > slot {
            other.slot -= 1;
            other.first_character -= reserve as i16;
        }
        if other.target_group as usize > slot {
            other.target_group -= 1;
        }
        let target = other.slot;
        groups[target] = Some(other);
    }
    state.groups = groups;
    for character in state.characters.iter_mut().flatten() {
        if character.group as usize > slot {
            character.group -= 1;
        }
    }
    for village in state.villages.iter_mut().flatten() {
        if village.elder_group as usize > slot {
            village.elder_group -= 1;
        }
    }
    Ok(())
}

/// Переложить персонажей: moved(номер) — новый номер, None — персонаж удалён.
///
/// Ссылки «враг» и «рабочее место» следуют за персонажем; first_character
/// отрядов правит вызывающий, он знает новую укладку.
fn renumber(
    state: &mut GameState,
    moved: &dyn Fn(usize) -> Option<usize>,
) -> Result<(), EditError> {
    // Первая проверка — до любой правки состояния: если персонажи не
    // помещаются, state остаётся нетронутым
    for character in state.characters.iter().flatten() {
        if let Some(slot) = moved(character.slot)
            && slot >= CHARACTER_SLOTS
        {
            return Err(EditError::NotEnoughCharacterSlots);
        }
    }
    let old = std::mem::take(&mut state.characters);
    let mut placed: Vec<Option<Character>> = vec![None; CHARACTER_SLOTS];
    for character in old.into_iter().flatten() {
        let Some(slot) = moved(character.slot) else {
            continue;
        };
        let mut character = character;
        character.slot = slot;
        placed[slot] = Some(character);
    }
    for character in placed.iter_mut().flatten() {
        if character.enemy > 0 {
            character.enemy = moved(character.enemy as usize - 1).map_or(0, |enemy| {
                i16::try_from(enemy + 1).expect("ссылка на врага вне слотов персонажей")
            });
        }
    }
    for village in state.villages.iter_mut().flatten() {
        for job in &mut village.jobs {
            if job.character != 0 {
                job.character = moved(job.character as usize).unwrap_or_default() as i16;
            }
        }
    }
    state.characters = placed;
    Ok(())
}

fn workers(state: &GameState) -> HashSet<usize> {
    state
        .villages
        .iter()
        .flatten()
        .flat_map(|village| village.jobs.iter())
        .map(|job| job.character as usize)
        .filter(|&slot| slot != 0)
        .collect()
}

fn item_refs(character: &Character) -> Vec<usize> {
    let mut refs: Vec<i64> = Vec::new();
    refs.extend(character.equipment.iter().map(|&slot| slot as i64));
    refs.extend(character.inventory.iter().map(|&slot| slot as i64));
    refs.extend(character.accessories.iter().map(|&slot| slot as i64));
    refs.push(character.misc[ARROWS] as i64);
    refs.into_iter()
        .filter(|&slot| slot > 0)
        .map(|slot| slot as usize)
        .collect()
}

fn free_items(state: &mut GameState, character: &Character) {
    for slot in item_refs(character) {
        state.items[slot] = None;
    }
}

/// У копии свои предметы: у каждой записи предмета ровно один владелец.
///
/// Места вызывающий проверяет заранее ([`check_room`]).
fn clone_items(state: &mut GameState, character: &mut Character) {
    let mut free: VecDeque<usize> = (1..ITEM_SLOTS)
        .filter(|&slot| state.items[slot].is_none())
        .collect();
    let mut clone_ref = |slot: &mut i16| {
        if *slot <= 0 {
            return;
        }
        let new = free.pop_front().expect("свободный слот предмета");
        let mut item: Item = state.items[*slot as usize]
            .clone()
            .expect("ссылка на существующий предмет");
        item.slot = new;
        state.items[new] = Some(item);
        *slot = i16::try_from(new).expect("слот предмета вне диапазона записи");
    };
    for slot in &mut character.equipment {
        clone_ref(slot);
    }
    // Снаряжение, инвентарь и перстни хранят номер предмета в свой ширине
    for slot in &mut character.inventory {
        let mut reference = *slot as i16;
        clone_ref(&mut reference);
        *slot = reference as u16;
    }
    for slot in &mut character.accessories {
        let mut reference = *slot as i16;
        clone_ref(&mut reference);
        *slot = reference as u16;
    }
    clone_ref(&mut character.misc[ARROWS]);
}

/// Слот сразу за резервом последнего отряда.
fn reserves_end(state: &GameState) -> usize {
    let last = state
        .groups
        .iter()
        .flatten()
        .next_back()
        .expect("в редактируемом состоянии есть хотя бы один отряд");
    last.first_character as usize + last.growth_limit as usize
}

/// Хватит ли места на копию: `characters` слотов персонажа за резервами,
/// `groups` слотов отрядов и записи предметов — до любой правки состояния.
fn check_room(
    state: &GameState,
    template: &Character,
    characters: usize,
    groups: usize,
) -> Result<(), EditError> {
    if reserves_end(state) + characters > CHARACTER_SLOTS {
        return Err(EditError::NotEnoughCharacterSlots);
    }
    if state.groups.iter().flatten().count() + groups > GROUP_SLOTS {
        return Err(EditError::NotEnoughGroupSlots);
    }
    let free = state.items[1..]
        .iter()
        .filter(|item| item.is_none())
        .count();
    if item_refs(template).len() > free {
        return Err(EditError::NotEnoughItemSlots);
    }
    Ok(())
}

fn find_character(state: &GameState, slot: usize) -> Result<&Character, EditError> {
    state
        .characters
        .get(slot)
        .and_then(Option::as_ref)
        .ok_or(EditError::NoSuchCharacter { slot })
}

fn find_group(state: &GameState, slot: usize) -> Result<&Group, EditError> {
    state
        .groups
        .get(slot)
        .and_then(Option::as_ref)
        .ok_or(EditError::NoSuchGroup { slot })
}

fn find_group_mut(state: &mut GameState, slot: usize) -> Result<&mut Group, EditError> {
    state
        .groups
        .get_mut(slot)
        .and_then(Option::as_mut)
        .ok_or(EditError::NoSuchGroup { slot })
}

#[cfg(test)]
mod tests {
    use crate::game_state::{self, Item, JOB_ROLES, Job, Village};

    use super::*;

    fn empty_state() -> GameState {
        GameState {
            items: (0..game_state::ITEM_SLOTS).map(|_| None).collect(),
            groups: (0..GROUP_SLOTS).map(|_| None).collect(),
            loot: Vec::new(),
            exits: Vec::new(),
            characters: (0..CHARACTER_SLOTS).map(|_| None).collect(),
            villages: (0..game_state::VILLAGE_SLOTS).map(|_| None).collect(),
        }
    }

    fn group(slot: usize, kind: u8, first: i16, size: u8, growth_limit: u8) -> Group {
        Group {
            slot,
            first_character: first,
            size,
            max_size: size,
            growth_limit,
            kind,
            ..Group::default()
        }
    }

    fn character(slot: usize, group_slot: u8) -> Character {
        Character {
            slot,
            group: group_slot,
            ..Character::default()
        }
    }

    #[test]
    fn remove_missing_character_is_an_error() {
        let mut state = empty_state();
        assert!(matches!(
            remove_character(&mut state, 0),
            Err(EditError::NoSuchCharacter { slot: 0 })
        ));
    }

    #[test]
    fn player_group_rejects_adding() {
        let mut state = empty_state();
        state.groups[0] = Some(group(0, PLAYER_GROUP, 0, 1, 1));
        state.characters[0] = Some(character(0, 0));
        assert!(matches!(
            add_character(&mut state, 0, 0, 1, 1),
            Err(EditError::PlayerGroupAdd)
        ));
    }

    #[test]
    fn full_reserve_rejects_growing() {
        let mut state = empty_state();
        state.groups[0] = Some(group(0, NPC_GROUP, 0, GROUP_RESERVE_MAX, GROUP_RESERVE_MAX));
        assert!(matches!(
            add_character(&mut state, 0, 0, 1, 1),
            Err(EditError::ReserveFull { slot: 0 })
        ));
    }

    #[test]
    fn working_character_cannot_be_removed() {
        let mut state = empty_state();
        state.groups[0] = Some(group(0, 0x51 | 0x80, 0, 2, 4));
        state.characters[0] = Some(character(0, 0));
        state.characters[1] = Some(character(1, 0));
        state.villages[0] = Some(Village {
            slot: 0,
            building_counts: (0, 0),
            elder_group: 0,
            map_id: 0,
            timers: [0; 5],
            buildings: Vec::new(),
            jobs: JOB_ROLES
                .iter()
                .map(|&role| Job {
                    role,
                    direction: 0,
                    unknown: 0,
                    character: if role == "elder" { 1 } else { 0 },
                    x: 0,
                    y: 0,
                })
                .collect(),
            tail: [0; game_state::VILLAGE_TAIL_SIZE],
        });
        assert!(matches!(
            remove_character(&mut state, 1),
            Err(EditError::WorkingCharacter { slot: 1 })
        ));
        // состояние не тронуто: ошибка raised до любых правок
        assert!(state.characters[0].is_some() && state.characters[1].is_some());
    }

    #[test]
    fn remove_shifts_following_members_and_enemy_reference() {
        let mut state = empty_state();
        state.groups[0] = Some(group(0, 0x51, 0, 2, 4));
        state.characters[0] = Some(character(0, 0));
        let mut follower = character(1, 0);
        follower.enemy = 1; // ссылка на удалённый первый член: номер + 1
        state.characters[1] = Some(follower);
        remove_character(&mut state, 0).unwrap();
        let shifted = state.characters[0]
            .as_ref()
            .expect("следующий член сдвинулся на слот вниз");
        assert_eq!(shifted.slot, 0);
        assert_eq!(shifted.enemy, 0, "ссылка на удалённого врага обнуляется");
        assert!(state.characters[1].is_none());
        assert_eq!(state.groups[0].as_ref().unwrap().size, 1);
    }

    #[test]
    fn copy_gets_its_own_items() {
        let mut state = empty_state();
        state.groups[0] = Some(group(0, 0x51, 0, 1, 4));
        state.characters[0] = Some(character(0, 0));
        for slot in [5usize, 7, 9, 11] {
            state.items[slot] = Some(Item {
                slot,
                ..Item::default()
            });
        }
        let mut template = state.characters[0].as_mut().unwrap().clone();
        template.equipment = [5, 0, 0, 0, 0, 0];
        template.inventory[0] = 7;
        template.accessories[0] = 9;
        template.misc[ARROWS] = 11;
        state.characters[0] = Some(template);
        let slot = add_character(&mut state, 0, 0, 3, 4).unwrap();
        assert_eq!(slot, 1);
        let copy = state.characters[1].as_ref().unwrap();
        // Копия получает свободные слоты по порядку ссылок, оригинал остаётся при своих
        assert_eq!(copy.equipment[0], 1);
        assert_eq!(copy.inventory[0], 2);
        assert_eq!(copy.accessories[0], 3);
        assert_eq!(copy.misc[ARROWS], 4);
        assert_eq!(state.characters[0].as_ref().unwrap().equipment[0], 5);
        for slot in 1..5 {
            assert_eq!(state.items[slot].as_ref().unwrap().slot, slot);
        }
        let group = state.groups[0].as_ref().unwrap();
        assert_eq!((group.size, group.max_size), (2, 2));
    }
}
