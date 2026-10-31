//! Тесты графики на реальных файлах игры из `original/` (в git их нет) —
//! зеркало `tests/k1/test_graphics.py`. Без распакованного издания пропускаются.

mod common;

use std::fs;
use std::path::PathBuf;

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
