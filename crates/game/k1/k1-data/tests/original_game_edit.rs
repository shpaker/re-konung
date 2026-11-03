//! Тесты редактирования состояния на реальных файлах из `original/` (в git их нет).
//!
//! Без распакованного издания тесты пропускаются (общий хелпер — common).
//! Зеркало `tests/k1/test_game_edit.py`.

mod common;

use std::collections::{HashMap, HashSet};

use common::game_dirs;
use k1_data::{
    game_edit, game_state,
    game_state::{CHARACTER_SLOTS, Character, GameState, Group},
    repo::{GAME_FILES, GameDir, TUTORIAL_FILES},
};

fn load_state(game: &GameDir) -> GameState {
    game_state::parse(&game.state_bytes(0, GAME_FILES).unwrap()).unwrap()
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

/// Укладка, как во всех файлах дистрибутива: отряды подряд, резервы плотно,
/// персонажи только внутри отрядов, ссылки сходятся, у предмета один владелец.
fn check_layout(state: &GameState) {
    let groups: Vec<&Group> = state.groups.iter().flatten().collect();
    let slots: Vec<usize> = groups.iter().map(|group| group.slot).collect();
    assert_eq!(
        slots,
        (0..groups.len()).collect::<Vec<_>>(),
        "отряды не идут подряд"
    );
    let mut first = 0usize;
    let mut members: HashSet<usize> = HashSet::new();
    for group in &groups {
        assert_eq!(
            group.first_character as usize, first,
            "отряд {}",
            group.slot
        );
        assert!(
            group.size as usize <= group.max_size as usize,
            "отряд {}",
            group.slot
        );
        assert!(
            group.max_size as usize <= group.growth_limit as usize,
            "отряд {}",
            group.slot
        );
        for slot in group.members() {
            let character = state.characters[slot]
                .as_ref()
                .unwrap_or_else(|| panic!("персонаж {slot} из отряда {}", group.slot));
            assert_eq!(character.group as usize, group.slot, "персонаж {slot}");
            members.insert(slot);
        }
        first += group.growth_limit as usize;
    }
    // Резерв последнего отряда не выходит за блок: игра растит отряды в first + size
    assert!(first <= CHARACTER_SLOTS, "резерв вышел за блок персонажей");
    let character_slots: HashSet<usize> =
        state.characters.iter().flatten().map(|c| c.slot).collect();
    assert_eq!(character_slots, members, "персонажи вне отрядов");
    for village in state.villages.iter().flatten() {
        let elder = state.groups[village.elder_group as usize]
            .as_ref()
            .expect("староста деревни — существующий отряд");
        let elder_members: HashSet<usize> = elder.members().collect();
        for job in &village.jobs {
            assert!(
                job.character == 0 || elder_members.contains(&(job.character as usize)),
                "рабочее место {} в деревне {}",
                job.role,
                village.slot
            );
        }
    }
    let mut owners: HashMap<usize, String> = HashMap::new();
    for character in state.characters.iter().flatten() {
        for slot in character
            .equipment
            .iter()
            .map(|&slot| slot as i64)
            .chain(character.inventory.iter().map(|&slot| slot as i64))
            .chain(character.accessories.iter().map(|&slot| slot as i64))
            .filter(|&slot| slot > 0)
        {
            let slot = slot as usize;
            assert!(
                state.items.get(slot).is_some_and(Option::is_some),
                "персонаж {} ссылается на несуществующий предмет {slot}",
                character.slot
            );
            assert!(
                owners
                    .insert(slot, format!("персонаж {}", character.slot))
                    .is_none(),
                "предмет {slot} в двух местах"
            );
        }
    }
    for loot in &state.loot {
        for slot in loot.items.iter().filter(|&&slot| slot > 0) {
            let slot = *slot as usize;
            assert!(
                owners.insert(slot, format!("клад {}", loot.slot)).is_none(),
                "предмет {slot} уже у {}",
                owners[&slot]
            );
        }
    }
    let data = game_state::serialize(state).unwrap();
    assert_eq!(
        game_state::serialize(&game_state::parse(&data).unwrap()).unwrap(),
        data
    );
}

/// Идентичность персонажа для сравнения «те же объекты» вперемешку со сдвигом слотов.
fn identity(character: &Character) -> (u8, u8, i32, i32, i16, i16) {
    (
        character.name_id,
        character.nick_id,
        character.gold,
        character.experience,
        character.x,
        character.y,
    )
}

#[test]
fn every_state_file_has_the_dense_layout() {
    for (edition, game) in game_dirs() {
        let mut seen = 0;
        for files in [GAME_FILES, TUTORIAL_FILES] {
            for number in game.state_numbers(files).unwrap() {
                let state = game_state::parse(&game.state_bytes(number, files).unwrap()).unwrap();
                check_layout(&state);
                seen += 1;
            }
        }
        assert!(seen >= 3, "{edition}: файлов состояния меньше трёх");
    }
}

#[test]
fn remove_npc_drops_its_group() {
    for (_edition, game) in game_dirs() {
        let mut state = load_state(&game);
        let npc_group = state
            .groups
            .iter()
            .flatten()
            .find(|group| group.kind == game_edit::NPC_GROUP)
            .cloned()
            .expect("в файле есть отряд NPC");
        let groups_before = state.groups.iter().flatten().count();
        let people_before = state.characters.iter().flatten().count();
        game_edit::remove_character(&mut state, npc_group.first_character as usize).unwrap();
        assert_eq!(state.groups.iter().flatten().count(), groups_before - 1);
        assert_eq!(state.characters.iter().flatten().count(), people_before - 1);
        check_layout(&state);
    }
}

#[test]
fn remove_villager_shifts_the_rest_of_the_group() {
    for (_edition, game) in game_dirs() {
        let mut state = load_state(&game);
        let workers = workers(&state);
        let village = state
            .groups
            .iter()
            .flatten()
            .find(|group| {
                group.kind & game_state::VILLAGE_GROUP_BIT != 0
                    && group.members().any(|slot| !workers.contains(&slot))
            })
            .cloned()
            .expect("в файле есть деревня со свободными жителями");
        let slot = village.members().find(|s| !workers.contains(s)).unwrap();
        let size = village.size;
        let after: Vec<_> = village
            .members()
            .filter(|s| *s > slot)
            .map(|s| identity(state.characters[s].as_ref().unwrap()))
            .collect();
        game_edit::remove_character(&mut state, slot).unwrap();
        let village_now = state.groups[village.slot].as_ref().unwrap().clone();
        assert_eq!(village_now.size, size - 1);
        let now: Vec<_> = village_now
            .members()
            .filter(|s| *s >= slot)
            .map(|s| identity(state.characters[s].as_ref().unwrap()))
            .collect();
        assert_eq!(now, after, "остальные члены отряда сдвинулись как те же");
        check_layout(&state);
    }
}

#[test]
fn add_copy_to_full_group_grows_its_reserve() {
    // Деревни в данных никогда не заполнены до резерва, бродячие отряды — бывают;
    // рост резерва один и тот же
    for (_edition, game) in game_dirs() {
        let mut state = load_state(&game);
        let full = state
            .groups
            .iter()
            .flatten()
            .find(|group| group.is_roaming() && group.size == group.growth_limit)
            .cloned()
            .expect("в файле есть полный бродячий отряд");
        let template_slot = full.first_character as usize;
        let later: Vec<Group> = state
            .groups
            .iter()
            .flatten()
            .filter(|group| group.slot > full.slot)
            .cloned()
            .collect();
        let firsts: Vec<i16> = later.iter().map(|group| group.first_character).collect();
        let slot = game_edit::add_character(&mut state, template_slot, full.slot, 10, 20).unwrap();
        assert_eq!(slot, full.first_character as usize + full.size as usize);
        let character = state.characters[slot].as_ref().unwrap();
        assert_eq!((character.x, character.y), (10, 20));
        let now_firsts: Vec<i16> = state
            .groups
            .iter()
            .flatten()
            .filter(|group| later.iter().any(|l| l.slot == group.slot))
            .map(|group| group.first_character)
            .collect();
        assert_eq!(now_firsts, firsts.iter().map(|f| f + 1).collect::<Vec<_>>());
        check_layout(&state);
    }
}

#[test]
fn add_copy_of_npc_gets_its_own_group() {
    for (_edition, game) in game_dirs() {
        let mut state = load_state(&game);
        let npc_group = state
            .groups
            .iter()
            .flatten()
            .find(|group| group.kind == game_edit::NPC_GROUP && group.growth_limit == 1)
            .cloned()
            .expect("в файле есть одиночный NPC");
        let template_slot = npc_group.first_character as usize;
        let groups_before = state.groups.iter().flatten().count();
        let slot =
            game_edit::add_character(&mut state, template_slot, npc_group.slot, 5, 6).unwrap();
        assert_eq!(state.groups.iter().flatten().count(), groups_before + 1);
        let character = state.characters[slot].as_ref().unwrap();
        let new_group = state.groups[character.group as usize].as_ref().unwrap();
        assert_eq!(new_group.kind, game_edit::NPC_GROUP);
        assert_eq!(new_group.map_id, npc_group.map_id);
        assert_eq!(character.unknown_d0, game_edit::NO_DIALOG);
        check_layout(&state);
    }
}

#[test]
fn copy_then_remove_original_keeps_layout() {
    for (_edition, game) in game_dirs() {
        let mut state = load_state(&game);
        let npc_group = state
            .groups
            .iter()
            .flatten()
            .find(|group| group.kind == game_edit::NPC_GROUP && group.growth_limit == 1)
            .cloned()
            .expect("в файле есть одиночный NPC");
        let original_slot = npc_group.first_character as usize;
        game_edit::add_character(&mut state, original_slot, npc_group.slot, 1, 2).unwrap();
        game_edit::remove_character(&mut state, original_slot).unwrap();
        check_layout(&state);
    }
}

#[test]
fn copy_of_npc_crowd_member_joins_its_group() {
    // В каждом Game.N есть отряды 0x53 из нескольких персонажей
    for (_edition, game) in game_dirs() {
        let mut state = load_state(&game);
        let crowd = state
            .groups
            .iter()
            .flatten()
            .find(|group| group.kind == game_edit::NPC_GROUP && group.size > 1)
            .cloned()
            .expect("в файле есть многолюдный отряд NPC");
        let groups_before = state.groups.iter().flatten().count();
        let slot =
            game_edit::add_character(&mut state, crowd.first_character as usize, crowd.slot, 3, 4)
                .unwrap();
        assert_eq!(
            state.characters[slot].as_ref().unwrap().group as usize,
            crowd.slot
        );
        assert_eq!(state.groups.iter().flatten().count(), groups_before);
        check_layout(&state);
    }
}

#[test]
fn no_room_keeps_state_unchanged() {
    for (_edition, game) in game_dirs() {
        let mut state = load_state(&game);
        let village = state
            .groups
            .iter()
            .flatten()
            .find(|group| group.kind & game_state::VILLAGE_GROUP_BIT != 0)
            .cloned()
            .expect("в файле есть деревня");
        let template_slot = village.first_character as usize;
        let mut added = 0;
        let mut before = game_state::serialize(&state).unwrap();
        // цикл до первой ошибки: правка, не влезающая в состояние, не должна менять его
        while game_edit::add_character(&mut state, template_slot, village.slot, 1, 1).is_ok() {
            added += 1;
            before = game_state::serialize(&state).unwrap();
        }
        assert!(added > 0);
        assert_eq!(game_state::serialize(&state).unwrap(), before);
        check_layout(&state);
    }
}

#[test]
fn player_and_workers_cannot_be_removed() {
    for (_edition, game) in game_dirs() {
        let mut state = load_state(&game);
        let player_first = state.groups[0].as_ref().unwrap().first_character as usize;
        assert!(matches!(
            game_edit::remove_character(&mut state, player_first),
            Err(game_edit::EditError::PlayerRemoval)
        ));
        let worker = *workers(&state)
            .iter()
            .min()
            .expect("в файле есть рабочие места");
        assert!(matches!(
            game_edit::remove_character(&mut state, worker),
            Err(game_edit::EditError::WorkingCharacter { .. })
        ));
    }
}
