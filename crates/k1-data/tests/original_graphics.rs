//! Тесты графики на реальных файлах игры из `original/` (в git их нет) —
//! зеркало `tests/k1/test_graphics.py`. Без распакованного издания пропускаются.

mod common;

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::game_dirs;

use k1_data::{
    characters, executable, flames_res, game_state, graph_res, lights_res, objects_res,
    repo::{GAME_FILES, GameDir},
    rle,
};

/// Файл игры по имени без учёта регистра: в дистрибутиве регистр разный
/// (в тестах Python это фикстура k1_file).
fn find_name(game: &GameDir, name: &str) -> PathBuf {
    for entry in fs::read_dir(game.root()).unwrap() {
        let path = entry.unwrap().path();
        if path
            .file_name()
            .and_then(|file| file.to_str())
            .is_some_and(|file| file.eq_ignore_ascii_case(name))
        {
            return path;
        }
    }
    panic!("{}: нет {name}", game.root().display());
}

/// Конец последней картинки или тени спрайта; заодно все они декодируются.
fn last_byte(sprites: &graph_res::Sprites<'_>) -> usize {
    let mut end = 0;
    for &(shadow, image) in &sprites.table {
        if image >= 0 {
            let mut reader = k1_data::reader::Reader::new(sprites.data);
            reader.seek(sprites.images_start + image as usize);
            rle::decode_indexed(&mut reader).unwrap();
            end = end.max(reader.pos());
        }
        if shadow > 0 {
            let mut reader = k1_data::reader::Reader::new(sprites.data);
            reader.seek(sprites.images_start + shadow as usize);
            rle::decode_shadow(&mut reader).unwrap();
            end = end.max(reader.pos());
        }
    }
    end
}

fn frames_have_images(sprites: &graph_res::Sprites<'_>) -> bool {
    sprites.sequences.iter().flatten().flatten().all(|frame| {
        sprites
            .table
            .get(frame.image_id as usize)
            .is_some_and(|&(_, image)| image >= 0)
    })
}

#[test]
fn graph_res_is_parsed_to_the_last_byte() {
    for (edition, game) in game_dirs() {
        let data = fs::read(find_name(&game, "graph.res")).unwrap();
        let graph = graph_res::parse(&data).unwrap();
        assert_eq!(graph.size, data.len(), "{edition}: размер");
        assert_eq!(graph.cursors.len(), 11, "{edition}: курсоры");
        let tiles = graph.tiles.iter().flatten();
        let ground = tiles
            .clone()
            .filter(|tile| (tile.image.width, tile.image.height) == (114, 64))
            .count();
        assert_eq!(tiles.count(), 305, "{edition}: тайлы");
        assert_eq!(ground, 221, "{edition}: тайлы земли 114×64");
    }
}

#[test]
fn character_sprites_end_with_their_blocks() {
    for (edition, game) in game_dirs() {
        let data = fs::read(find_name(&game, "graph.res")).unwrap();
        let graph = graph_res::parse(&data).unwrap();
        assert_eq!(graph.models.len(), 3, "{edition}: модели");
        assert_eq!(graph.equipment.len(), 24, "{edition}: снаряжение");
        let sprites = graph.models.iter().chain(&graph.equipment);
        for (block, sprites) in graph.character_blocks.iter().zip(sprites) {
            assert_eq!(last_byte(sprites), block.end, "{edition}: блок");
            assert!(frames_have_images(sprites), "{edition}: кадры без картинок");
        }
    }
}

/// Анимации 6–9 — те же 0–3 без оружия в руках (бит 0x04 байта 0x03): у каждого
/// блока спрайтов кадры пар состояний совпадают числом, а блоков ровно десять.
/// Подтверждение гипотезы — docs/k1/formats/graph-res.md, docs/k1/creatures.md.
#[test]
fn animations_six_to_nine_mirror_zero_to_three() {
    for (edition, game) in game_dirs() {
        let data = fs::read(find_name(&game, "graph.res")).unwrap();
        let graph = graph_res::parse(&data).unwrap();
        for sprites in graph.models.iter().chain(&graph.equipment) {
            assert_eq!(sprites.sequences.len(), 10, "{edition}: анимаций в блоке");
            for state in 0..4 {
                for direction in 0..8 {
                    assert_eq!(
                        sprites.sequences[state][direction].len(),
                        sprites.sequences[state + 6][direction].len(),
                        "{edition}: состояния {state} и {}",
                        state + 6
                    );
                }
            }
        }
    }
}

#[test]
fn objects_res_has_all_objects() {
    for (edition, game) in game_dirs() {
        let data = fs::read(find_name(&game, "objects.res")).unwrap();
        let objects = objects_res::parse(&data).unwrap();
        let simple = objects.simple.iter().flatten();
        assert_eq!(simple.clone().count(), 463, "{edition}: простые объекты");
        assert!(
            simple
                .clone()
                .all(|object| object.frames[0].image.is_some()),
            "{edition}: у каждого объекта есть первый кадр"
        );
        assert_eq!(
            objects.dynamic.iter().flatten().count(),
            30,
            "{edition}: динамические объекты"
        );
        for (slot, sprites) in objects.dynamic.iter().enumerate() {
            let Some(sprites) = sprites else { continue };
            // Размер блока — в каталоге, блок — от метки и размера перед заголовком
            let at = |pos: usize| i32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
            let offset = at(8 + slot * 8);
            let size = at(12 + slot * 8);
            assert_eq!(
                last_byte(sprites) as i64,
                8 + 512 * 8 + offset as i64 + size as i64,
                "{edition}: блок {slot}"
            );
            assert!(frames_have_images(sprites), "{edition}: кадры {slot}");
        }
    }
}

#[test]
fn animated_objects_have_every_frame() {
    for (edition, game) in game_dirs() {
        let data = fs::read(find_name(&game, "objects.res")).unwrap();
        let objects = objects_res::parse(&data).unwrap();
        let mut animated = objects
            .simple
            .iter()
            .flatten()
            .filter(|object| objects_res::frame_duration_ms(object).is_some())
            .collect::<Vec<_>>();
        animated.sort_by_key(|object| object.object_id);
        let ids = animated
            .iter()
            .map(|object| object.object_id)
            .collect::<Vec<_>>();
        assert_eq!(ids, [341, 342, 343, 355, 451, 452], "{edition}: состав");
        for object in animated {
            assert!(
                object.frames[..object.frame_count as usize]
                    .iter()
                    .all(|frame| frame.image.is_some()),
                "{edition}: объект {}",
                object.object_id
            );
        }
    }
}

#[test]
fn executable_tables_are_found() {
    for (edition, game) in game_dirs() {
        let data = fs::read(executable::find(game.root()).unwrap()).unwrap();
        let tables = executable::parse(&data).unwrap();
        assert_eq!(
            tables.equipment.len(),
            executable::EQUIPMENT_COUNT,
            "{edition}: снаряжение"
        );
        assert!(
            tables.equipment.iter().all(|info| -1 <= info.sprite_set
                && i32::from(info.sprite_set) < executable::EQUIPMENT_SETS as i32),
            "{edition}: наборы спрайтов"
        );
        assert_eq!(tables.draw_order.len(), 8, "{edition}: направления");
        for row in &tables.draw_order {
            let mut sorted = *row;
            sorted.sort_unstable();
            assert_eq!(sorted, [0, 1, 2, 3, 4, 5], "{edition}: перестановка слотов");
        }
        assert_eq!(
            tables.flame_animations.len(),
            executable::FLAME_KINDS,
            "{edition}: огни"
        );
    }
}

/// Текстовые таблицы ищутся по строению и однозначно. Строки игры здесь не
/// сравниваются: проверяются только число записей и то, какая запись пуста.
#[test]
fn text_tables_are_found_and_shaped() {
    for (edition, game) in game_dirs() {
        let data = fs::read(executable::find(game.root()).unwrap()).unwrap();
        let texts = executable::parse_texts(&data).unwrap();
        assert_eq!(
            texts.monsters.len(),
            executable::MONSTER_NAMES,
            "{edition}: монстры"
        );
        assert_eq!(
            texts.names.len(),
            executable::PEOPLE_NAMES,
            "{edition}: имена"
        );
        assert_eq!(
            texts.nicknames.len(),
            executable::NICKNAMES,
            "{edition}: прозвища"
        );
        assert_eq!(texts.maps.len(), executable::MAP_NAMES, "{edition}: карты");
        // У монстров заполнены первые 20 записей, хвост пуст (docs/k1/formats/executable.md)
        assert!(
            texts.monsters[..20].iter().all(|name| !name.is_empty()),
            "{edition}: названия монстров"
        );
        assert!(
            texts.monsters[20..].iter().all(String::is_empty),
            "{edition}: хвост монстров"
        );
        // Имя 0 и прозвище 0 — пустые записи, остальные заполнены
        assert!(texts.names[0].is_empty(), "{edition}: имя 0");
        assert!(
            texts.names[1..].iter().all(|name| !name.is_empty()),
            "{edition}: имена людей"
        );
        assert!(texts.nicknames[0].is_empty(), "{edition}: прозвище 0");
        assert!(
            texts.nicknames[1..].iter().all(|nick| !nick.is_empty()),
            "{edition}: прозвища"
        );
        assert!(
            texts.maps.iter().all(|map| !map.is_empty()),
            "{edition}: карты"
        );
        // Без таблиц разбор exe не падает: их отсутствие — None, а не ошибка
        assert!(
            executable::parse(&data).unwrap().texts.is_some(),
            "{edition}: parse"
        );
    }
}

/// Имя по таблицам собирается для каждого расставленного на картах персонажа.
///
/// В каждом начальном состоянии один житель без имени и прозвища (номер 0 в
/// обоих байтах) — так в данных, и это единственное исключение.
#[test]
fn every_placed_character_is_named() {
    for (edition, game) in game_dirs() {
        let data = fs::read(executable::find(game.root()).unwrap()).unwrap();
        let texts = executable::parse_texts(&data).unwrap();
        for hero in game.state_numbers(GAME_FILES).unwrap() {
            let state = game_state::parse(&game.state_bytes(hero, GAME_FILES).unwrap()).unwrap();
            let mut nameless = Vec::new();
            let mut monsters = 0;
            let mut with_nick = 0;
            for number in game.map_numbers(GAME_FILES).unwrap() {
                let entities = game_state::on_map(&state, number as usize, Some(hero as usize));
                for (character, _, _) in characters::placed(&entities, &state) {
                    let name = executable::character_name(
                        Some(&texts),
                        character.class_id,
                        character.name_id,
                        character.nick_id,
                    );
                    let kind = i32::from(character.class_id & 0x7F);
                    let monster = character.class_id & executable::MONSTER_CLASS != 0
                        && kind < i32::from(executable::FIRST_VILLAGER_CLASS);
                    if monster {
                        // Название вида берётся по `класс & 0x3F − 1`, а не по номеру записи
                        monsters += 1;
                        let view = kind - i32::from(executable::MONSTER_CLASS) - 1;
                        let Ok(view) = usize::try_from(view) else {
                            panic!(
                                "{edition} game.{hero} konung.{number}: класс {:#x} вне таблицы видов",
                                character.class_id
                            );
                        };
                        assert!(
                            name == texts.monsters[view] && !name.is_empty(),
                            "{edition} game.{hero} konung.{number}: вид {view} без названия"
                        );
                    } else if character.nick_id != 0 {
                        // Прозвище идёт через пробел вторым словом, и оно непустое
                        with_nick += 1;
                        assert!(
                            name.split_once(' ')
                                .is_some_and(|(given, nick)| !given.is_empty() && !nick.is_empty()),
                            "{edition} game.{hero} konung.{number}: прозвище не в имени"
                        );
                    }
                    if name.is_empty() {
                        nameless.push((character.class_id, character.name_id, character.nick_id));
                    }
                }
            }
            assert_eq!(
                nameless,
                vec![(0x51, 0, 0)],
                "{edition} game.{hero}: жители без имени"
            );
            assert!(
                monsters > 0 && with_nick > 0,
                "{edition} game.{hero}: на карте не расставлено ни монстра, ни жителя с прозвищем"
            );
        }
    }
}

#[test]
fn flames_res_is_parsed_to_the_last_byte() {
    for (edition, game) in game_dirs() {
        let data = fs::read(find_name(&game, "flames.res")).unwrap();
        let flames = flames_res::parse(&data).unwrap();
        assert_eq!(
            flames.frames.len(),
            executable::FLAME_FRAMES,
            "{edition}: кадры"
        );
        assert!(
            flames.tags.iter().all(|&tag| tag == 0x05000000),
            "{edition}: метки"
        );
        let objects_data = fs::read(find_name(&game, "objects.res")).unwrap();
        let objects = objects_res::parse(&objects_data).unwrap();
        for object in objects.simple.iter().flatten() {
            for flame in &object.flames {
                assert!(
                    (flame.flame_id as usize) < executable::FLAME_KINDS,
                    "{edition}: вид огня {}",
                    flame.flame_id
                );
            }
        }
        let tables =
            executable::parse(&fs::read(executable::find(game.root()).unwrap()).unwrap()).unwrap();
        assert_eq!(
            tables.flame_animations[executable::FLAME_KINDS - 1].1 as usize,
            flames.frames.len() - 1,
            "{edition}: последняя анимация"
        );
    }
}

#[test]
fn lights_res_masks_fill_the_file() {
    for (edition, game) in game_dirs() {
        let data = fs::read(find_name(&game, "lights.res")).unwrap();
        let masks = lights_res::parse(&data).unwrap();
        assert_eq!(
            masks.tiles.len(),
            lights_res::TILE_MASKS,
            "{edition}: маски"
        );
    }
}

#[test]
fn every_character_on_every_map_is_drawn() {
    for (edition, game) in game_dirs() {
        let graph_data = fs::read(find_name(&game, "graph.res")).unwrap();
        let graph = graph_res::parse(&graph_data).unwrap();
        let objects_data = fs::read(find_name(&game, "objects.res")).unwrap();
        let objects = objects_res::parse(&objects_data).unwrap();
        let tables =
            executable::parse(&fs::read(executable::find(game.root()).unwrap()).unwrap()).unwrap();
        for hero in game.state_numbers(GAME_FILES).unwrap() {
            let state = game_state::parse(&game.state_bytes(hero, GAME_FILES).unwrap()).unwrap();
            for number in game.map_numbers(GAME_FILES).unwrap() {
                let entities = game_state::on_map(&state, number as usize, Some(hero as usize));
                for (character, x, y) in characters::placed(&entities, &state) {
                    let slot = character.slot;
                    let sprite = characters::compose(
                        character, x, y, &state, &graph, &objects, &tables, None,
                    )
                    .unwrap();
                    assert!(
                        sprite.is_some(),
                        "{edition}: game.{hero} konung.{number} слот {slot}: нет кадра"
                    );
                    let sprite = sprite.unwrap();
                    let count = characters::frame_count(character, &graph, &objects);
                    let frames =
                        characters::animation(character, x, y, &state, &graph, &objects, &tables)
                            .unwrap();
                    assert!(count > 0 && frames.len() == count, "{edition}: слот {slot}");
                    if !character.has_dynamic_sprite() {
                        let drawn = character
                            .equipment
                            .iter()
                            .filter(|&&index| index > 0)
                            .filter(|&&index| {
                                let item = state.items[index as usize].as_ref().unwrap();
                                tables.equipment[item.info_id as usize].sprite_set >= 0
                            })
                            .count();
                        assert_eq!(
                            sprite.layers.len(),
                            1 + drawn,
                            "{edition}: game.{hero} konung.{number} слот {slot}"
                        );
                    }
                }
            }
        }
    }
}

/// Дайджест списка строк — FNV-1a 64 по UTF-8, где строки разделены переводом
/// строки. В сравнение и в вывод при расхождении попадает только он: строки игры
/// не попадают ни в git, ни в лог теста.
fn digest(strings: &[String]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for string in strings {
        for &byte in string.as_bytes().iter().chain(std::iter::once(&b'\n')) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x1000_0000_0000_01b3);
        }
    }
    hash
}

/// Python-эталон печатает число записей и дайджест каждой таблицы, а также
/// дайджест имён всех персонажей из Game.N — того же, что считает Rust.
const PYTHON_DIGEST: &str = r#"
import sys
from pathlib import Path
from konung.k1 import executable, game_state

def digest(strings):
    hash = 0xCBF29CE484222325
    for s in strings:
        for b in (s + "\n").encode("utf-8"):
            hash = ((hash ^ b) * 0x10000000000001B3) & 0xFFFFFFFFFFFFFFFF
    return hash

game = Path(sys.argv[1])
texts = executable.load(game).texts
if texts is None:
    raise SystemExit("нет текстовых таблиц")
print("monsters", len(texts.monsters), format(digest(texts.monsters), "016x"))
print("names", len(texts.names), format(digest(texts.names), "016x"))
print("nicknames", len(texts.nicknames), format(digest(texts.nicknames), "016x"))
print("maps", len(texts.maps), format(digest(texts.maps), "016x"))
names = [
    executable.character_name(texts, c.class_id, c.name_id, c.nick_id)
    for number in game_state.numbers(game)
    for c in game_state.load(game, number).characters
    if c
]
print("characters", len(names), format(digest(names), "016x"))
"#;

/// Интерпретатор с установленным пакетом: переменная KONUNG_PYTHON, затем
/// `.venv` проекта, затем `uv run python` (создаст окружение сам), затем python3.
fn interpreters(repo: &Path) -> Vec<(OsString, Vec<OsString>)> {
    let mut out = Vec::new();
    if let Some(python) = std::env::var_os("KONUNG_PYTHON") {
        out.push((python, Vec::new()));
    }
    let venv = if cfg!(windows) {
        repo.join(".venv").join("Scripts").join("python.exe")
    } else {
        repo.join(".venv").join("bin").join("python")
    };
    if venv.is_file() {
        out.push((venv.into_os_string(), Vec::new()));
    }
    out.push((
        OsString::from("uv"),
        vec![OsString::from("run"), OsString::from("python")],
    ));
    out.push((OsString::from("python3"), Vec::new()));
    out
}

/// (имя таблицы, число записей, шестнадцатеричный дайджест) из Python-эталона.
fn parse_digest_line(line: &str) -> (String, usize, String) {
    let mut fields = line.split(' ');
    let (Some(name), Some(count), Some(hash)) = (fields.next(), fields.next(), fields.next())
    else {
        panic!("не разобралась строка дайджеста: {line}");
    };
    let count = count
        .parse()
        .unwrap_or_else(|error| panic!("{line}: {error}"));
    (name.to_owned(), count, hash.to_owned())
}

fn python_digests(game_root: &Path) -> Vec<(String, usize, String)> {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut failures = Vec::new();
    for (program, prefix) in interpreters(&repo) {
        let mut command = Command::new(&program);
        command
            .args(&prefix)
            .args(["-c", PYTHON_DIGEST])
            .arg(game_root)
            .current_dir(&repo);
        let outcome = match command.output() {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                return stdout
                    .lines()
                    .filter(|line| !line.is_empty())
                    .map(parse_digest_line)
                    .collect();
            }
            Ok(output) => format!(
                "{}: {}",
                program.to_string_lossy(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            Err(error) => format!("{}: {error}", program.to_string_lossy()),
        };
        failures.push(outcome);
    }
    panic!(
        "не удалось получить дайджесты у Python-эталона (нужен uv sync или KONUNG_PYTHON):\n{}",
        failures.join("\n")
    );
}

#[test]
fn text_tables_match_the_python_reference() {
    for (edition, game) in game_dirs() {
        let data = fs::read(executable::find(game.root()).unwrap()).unwrap();
        let texts = executable::parse_texts(&data).unwrap();
        let mut names = Vec::new();
        for hero in game.state_numbers(GAME_FILES).unwrap() {
            let state = game_state::parse(&game.state_bytes(hero, GAME_FILES).unwrap()).unwrap();
            names.extend(state.characters.iter().flatten().map(|character| {
                executable::character_name(
                    Some(&texts),
                    character.class_id,
                    character.name_id,
                    character.nick_id,
                )
            }));
        }
        for (name, count, hash) in python_digests(game.root()) {
            let (rust_count, rust_hash) = match name.as_str() {
                "monsters" => (texts.monsters.len(), digest(&texts.monsters)),
                "names" => (texts.names.len(), digest(&texts.names)),
                "nicknames" => (texts.nicknames.len(), digest(&texts.nicknames)),
                "maps" => (texts.maps.len(), digest(&texts.maps)),
                "characters" => (names.len(), digest(&names)),
                other => panic!("неизвестная таблица {other}"),
            };
            assert_eq!(rust_count, count, "{edition}: число записей {name}");
            assert_eq!(
                format!("{rust_hash:016x}"),
                hash,
                "{edition}: дайджест {name} — состав или порядок строк разошёлся с Python"
            );
        }
    }
}
