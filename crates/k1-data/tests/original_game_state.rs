//! Тесты `Game.N` на реальных файлах из `original/` (в git их нет).
//!
//! Без распакованного издания тесты пропускаются (общий хелпер — common).
//! Зеркало `tests/k1/test_game_state.py`.

mod common;

use std::collections::{HashMap, HashSet};

use common::game_dirs;
use k1_data::{
    game_state, map,
    repo::{GAME_FILES, TUTORIAL_FILES},
};

/// Отряды, клады, переходы и деревни ссылаются на реальные карты и слоты.
#[test]
fn serialize_returns_every_state_byte_for_byte() {
    for (edition, game) in game_dirs() {
        for (files, label) in [(GAME_FILES, "game"), (TUTORIAL_FILES, "tgame")] {
            for number in game.state_numbers(files).unwrap() {
                let data = game.state_bytes(number, files).unwrap();
                let state = game_state::parse(&data).unwrap();
                assert_eq!(
                    game_state::serialize(&state).unwrap(),
                    data,
                    "{edition} {label}.{number}"
                );
            }
        }
    }
}

#[test]
fn references_between_blocks_match() {
    for (edition, game) in game_dirs() {
        for files in [GAME_FILES, TUTORIAL_FILES] {
            let maps: HashSet<u32> = game.map_numbers(files).unwrap().into_iter().collect();
            assert!(!maps.is_empty(), "{edition}: нет карт набора {files:?}");
            for number in game.state_numbers(files).unwrap() {
                let state = game_state::parse(&game.state_bytes(number, files).unwrap()).unwrap();
                let mut owners: HashMap<usize, usize> = HashMap::new();
                for group in state.groups.iter().flatten() {
                    assert!(
                        maps.contains(&(group.map_id as u32)),
                        "{edition} {number}: отряд {group:?} на несуществующей карте"
                    );
                    for index in group.members() {
                        assert!(
                            owners.insert(index, group.slot).is_none(),
                            "{edition} {number}: персонаж {index} в двух отрядах"
                        );
                        let character = state.characters[index].as_ref().unwrap_or_else(|| {
                            panic!(
                                "{edition} {number}: персонаж {index} из отряда {}",
                                group.slot
                            )
                        });
                        assert_eq!(character.group as usize, group.slot);
                        // Всех, кроме бродячих, игра ставит на место из файла
                        if !group.is_roaming() {
                            assert_ne!((character.x, character.y), (0, 0));
                        }
                    }
                }
                for loot in &state.loot {
                    assert!(maps.contains(&(loot.map_id as u32)), "клад {}", loot.slot);
                    assert!(
                        loot.items.iter().all(|&item| item == 0
                            || state.items.get(item as usize).is_some_and(Option::is_some)),
                        "клад {}: ссылка на несуществующий предмет",
                        loot.slot
                    );
                }
                for exit in &state.exits {
                    let source = u32::from(exit.map_id as u8);
                    assert!(
                        maps.contains(&source) && maps.contains(&(exit.target_map as u32)),
                        "переход {}",
                        exit.slot
                    );
                }
                for village in state.villages.iter().flatten() {
                    assert!(
                        maps.contains(&(village.map_id as u32)),
                        "деревня {}",
                        village.slot
                    );
                }
            }
        }
    }
}

#[test]
fn player_starts_on_walkable_cell_of_hero_map() {
    for (edition, game) in game_dirs() {
        for files in [GAME_FILES, TUTORIAL_FILES] {
            for number in game.state_numbers(files).unwrap() {
                let state = game_state::parse(&game.state_bytes(number, files).unwrap()).unwrap();
                let player = state.groups[0].as_ref().unwrap();
                assert_eq!(player.kind, game_state::PLAYER_GROUP);
                // Карту в отряде игрока игра не читает: она считается по номеру
                // героя. В Game.N там 1, в TGame.1 — сама стартовая карта 4
                let expected_map = if files == GAME_FILES { 1 } else { 4 };
                assert_eq!(player.map_id, expected_map);
                let start = game_state::start_map(number as usize);
                let on_map = game_state::on_map(&state, start, Some(number as usize));
                assert_eq!(on_map.start.map(|group| group.slot), Some(player.slot));
                let location = map::parse(&game.map_bytes(start as u32, files).unwrap()).unwrap();
                let top = usize::try_from(player.spawn.top).unwrap();
                let left = usize::try_from(player.spawn.left).unwrap();
                assert_eq!(
                    location.foot[top][left], 0,
                    "{edition} {files:?} герой {number}: старт на непроходимой клетке"
                );
            }
        }
    }
}

#[test]
fn tutorial_has_its_own_state() {
    // Обучение (только ENG) — TGame.1 за героя 1; Game.N его не видит, и наоборот
    for (edition, game) in game_dirs() {
        let tutorial = game.state_numbers(TUTORIAL_FILES).unwrap();
        if edition == "eng" {
            assert_eq!(tutorial, [1]);
        } else {
            assert!(tutorial.is_empty(), "{edition}: обучение не ожидалось");
        }
        assert_eq!(game.state_numbers(GAME_FILES).unwrap(), [0, 1, 2]);
        for number in tutorial {
            let state =
                game_state::parse(&game.state_bytes(number, TUTORIAL_FILES).unwrap()).unwrap();
            let game_state_main =
                game_state::parse(&game.state_bytes(number, GAME_FILES).unwrap()).unwrap();
            assert_ne!(state, game_state_main);
            let tutorial_maps = game.map_numbers(TUTORIAL_FILES).unwrap();
            assert_eq!(tutorial_maps, [4, 5], "{edition}: карты обучения");
            assert!(tutorial_maps.contains(&(game_state::start_map(number as usize) as u32)));
        }
    }
}

/// Смещение блока персонажей: предметы + отряды + клады + переходы (0x2A69A в Python-тесте).
const CHARACTERS_BLOCK: usize = game_state::ITEM_SLOTS * game_state::ITEM_SIZE
    + game_state::GROUP_SLOTS * game_state::GROUP_SIZE
    + game_state::LOOT_SLOTS * game_state::LOOT_SIZE
    + game_state::EXIT_SLOTS * game_state::EXIT_SIZE;

#[test]
fn character_field_edits_touch_only_their_bytes() {
    // Поля, которые правятся в Tiled: класс, уровень, имя, прозвище, набор спрайтов,
    // золото, очки, опыт, навыки, характеристики (смещения — docs/k1/formats/game-state.md).
    // Значения читаются по смещениям из документа, а не той же структурой, что пишет.
    let Some((_, game)) = game_dirs().into_iter().next() else {
        return;
    };
    assert_eq!(
        CHARACTERS_BLOCK, 0x2A69A,
        "смещение блока персонажей из документа"
    );
    let data = game.state_bytes(0, GAME_FILES).unwrap();
    let mut state = game_state::parse(&data).unwrap();
    let static_index = state
        .characters
        .iter()
        .position(|c| c.as_ref().is_some_and(|c| !c.has_dynamic_sprite()))
        .unwrap();
    {
        let character = state.characters[static_index].as_mut().unwrap();
        character.class_id ^= 1;
        character.level = character.level.wrapping_add(1);
        character.name_id = 7;
        character.nick_id = 3;
        character.sprite_set = (character.sprite_set + 1) % 3;
        character.gold = 12345;
        character.free_points = 6;
        character.experience = -7;
        character.skills = std::array::from_fn(|index| (100 + index) as i16);
        character.attributes = std::array::from_fn(|index| index as i16 - 4);
    }
    let class_id = state.characters[static_index].as_ref().unwrap().class_id;
    let level = state.characters[static_index].as_ref().unwrap().level;
    let sprite_set = state.characters[static_index].as_ref().unwrap().sprite_set;
    let slot = state.characters[static_index].as_ref().unwrap().slot;
    let edited = game_state::serialize(&state).unwrap();

    let at = CHARACTERS_BLOCK + slot * game_state::CHARACTER_SIZE;
    let diffs: Vec<usize> = (0..data.len())
        .filter(|&index| data[index] != edited[index])
        .collect();
    assert!(!diffs.is_empty());
    assert!(
        diffs
            .iter()
            .all(|&index| (at..at + game_state::CHARACTER_SIZE).contains(&index)),
        "изменены байты вне записи персонажа: {diffs:?}"
    );
    let changed: HashSet<usize> = diffs.into_iter().map(|index| index - at).collect();
    let allowed: HashSet<usize> = [0x04, 0x33, 0xD4, 0xD5, 0xD6]
        .into_iter()
        .chain(0x34..0x40)
        .chain(0x40..0x40 + 18)
        .chain(0x54..0x54 + 18)
        .collect();
    assert!(
        changed.is_subset(&allowed),
        "изменены чужие байты записи: {:?} ⪆ {:?}",
        &changed - &allowed,
        allowed
    );
    assert_eq!(edited[at + 0x04], class_id);
    assert_eq!(edited[at + 0x33], level);
    assert_eq!(&edited[at + 0xD4..at + 0xD7], &[7, 3, sprite_set]);
    let read_i32s = |offset: usize| -> [i32; 3] {
        std::array::from_fn(|index| {
            i32::from_le_bytes(edited[at + offset + index * 4..][..4].try_into().unwrap())
        })
    };
    let read_i16s = |offset: usize| -> [i16; 9] {
        std::array::from_fn(|index| {
            i16::from_le_bytes(edited[at + offset + index * 2..][..2].try_into().unwrap())
        })
    };
    assert_eq!(read_i32s(0x34), [12345, 6, -7]);
    assert_eq!(
        read_i16s(0x40),
        std::array::from_fn(|index| (100 + index) as i16)
    );
    assert_eq!(
        read_i16s(0x54),
        std::array::from_fn(|index| index as i16 - 4)
    );
}
