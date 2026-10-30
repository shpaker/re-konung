//! Тесты на реальных файлах игры из `original/` (в git их нет).
//!
//! Без распакованного издания тест пропускается: ранний return с сообщением,
//! аналог pytest.skip в tests/conftest.py. Как получить original/ —
//! docs/k1/editions.md.

use std::path::PathBuf;

use k1_data::{
    map,
    repo::{GAME_FILES, GameDir, TUTORIAL_FILES},
};

fn game_dirs() -> Vec<(&'static str, GameDir)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../original/k1");
    ["rus", "eng"]
        .into_iter()
        .filter_map(|edition| {
            let game = GameDir::new(root.join(edition).join("game"));
            if game.root().is_dir() {
                Some((edition, game))
            } else {
                eprintln!("пропуск: нет original/k1/{edition}/game (docs/k1/editions.md)");
                None
            }
        })
        .collect()
}

#[test]
fn serialize_returns_every_map_byte_for_byte() {
    for (edition, game) in game_dirs() {
        let numbers = game.map_numbers(GAME_FILES).unwrap();
        assert_eq!(numbers.len(), 43, "{edition}: число карт");
        for number in numbers {
            let data = game.map_bytes(number, GAME_FILES).unwrap();
            let konung_map = map::parse(&data).unwrap();
            assert_eq!(
                map::serialize(&konung_map).unwrap(),
                data,
                "{edition} konung.{number}"
            );
        }
    }
}

#[test]
fn foot_has_only_four_values() {
    // Правило Slavik «младшие 4 бита = 0xF» и его версия с ошибкой приоритета
    // («младший бит», docs/k1/formats/map.md) на этих значениях совпадают
    for (edition, game) in game_dirs() {
        for number in game.map_numbers(GAME_FILES).unwrap() {
            let konung_map = map::parse(&game.map_bytes(number, GAME_FILES).unwrap()).unwrap();
            for row in &konung_map.foot {
                for &value in row {
                    assert!(
                        matches!(value, 0x00 | 0x0F | 0x40 | 0x4F),
                        "{edition} konung.{number}: {value:#04x}"
                    );
                    assert_eq!(map::is_blocked(value), value & 1 != 0);
                }
            }
        }
    }
}

#[test]
fn lights_files_match_their_size_and_fine_follows_tiles() {
    let dirs = game_dirs();
    let mut found = 0;
    for (edition, game) in &dirs {
        let numbers = game.map_numbers(GAME_FILES).unwrap();
        for &number in &numbers {
            let Some(data) = game.lights_bytes(number, GAME_FILES).unwrap() else {
                continue;
            };
            found += 1;
            let lights = map::parse_lights(&data).unwrap();
            assert_eq!(lights.tiles.len(), map::HEIGHT, "{edition} lights.{number}");
            assert_eq!(
                lights.fine.len(),
                map::FINE_HEIGHT,
                "{edition} lights.{number}"
            );
            // Мелкий блок — номер маски тайла на 4 его клетки: запись его пересчитывает
            assert_eq!(
                lights.fine,
                map::lights_fine(&lights.tiles),
                "{edition} lights.{number}"
            );
            assert_eq!(
                map::serialize_lights(&lights).unwrap(),
                data,
                "{edition} lights.{number}"
            );
        }
        // У карт 6, 8 и 46 света нет
        let without_lights: Vec<u32> = numbers
            .iter()
            .copied()
            .filter(|&number| game.lights_path(number, GAME_FILES).unwrap().is_none())
            .collect();
        assert_eq!(without_lights, [6, 8, 46], "{edition}: карты без света");
    }
    if !dirs.is_empty() {
        assert_eq!(found, 40 * dirs.len(), "число файлов света");
    }
}

#[test]
fn tutorial_maps_return_byte_for_byte() {
    // Обучение есть только в ENG: Tutorial.4 и Tutorial.5.
    // Проверка «единственная сырая декорация в слоте 994» — в Python-тесте:
    // она требует GRAPH.RES, который в Rust ещё не перенесён.
    for (edition, game) in game_dirs() {
        for number in game.map_numbers(TUTORIAL_FILES).unwrap() {
            let data = game.map_bytes(number, TUTORIAL_FILES).unwrap();
            assert_eq!(
                map::serialize(&map::parse(&data).unwrap()).unwrap(),
                data,
                "{edition} tutorial.{number}"
            );
            let lights = game
                .lights_bytes(number, TUTORIAL_FILES)
                .unwrap()
                .unwrap_or_else(|| panic!("{edition} tutorial.{number}: нет света"));
            assert_eq!(
                map::serialize_lights(&map::parse_lights(&lights).unwrap()).unwrap(),
                lights,
                "{edition} tlights.{number}"
            );
        }
    }
}

#[test]
fn maps_reference_known_file_sets() {
    for (edition, game) in game_dirs() {
        for number in game.map_numbers(GAME_FILES).unwrap() {
            let path = game.find("konung", number).unwrap();
            assert_eq!(
                k1_data::repo::file_set(path.file_name().unwrap().to_str().unwrap()),
                GAME_FILES,
                "{edition} konung.{number}"
            );
        }
        for number in game.map_numbers(TUTORIAL_FILES).unwrap() {
            let path = game.find("tutorial", number).unwrap();
            assert_eq!(
                k1_data::repo::file_set(path.file_name().unwrap().to_str().unwrap()),
                TUTORIAL_FILES,
                "{edition} tutorial.{number}"
            );
        }
    }
}
