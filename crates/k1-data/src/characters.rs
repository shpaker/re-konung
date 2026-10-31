//! Кадр персонажа на карте: модель или монстр, снаряжение, тень
//! (docs/k1/formats/graph-res.md, эталон — `konung/k1/characters.py`).
//!
//! Воин — модель GRAPH.RES по Character.sprite_set с его палитрой, поверх неё —
//! наборы снаряжения надетых предметов в порядке слотов для его направления.
//! Монстры, женщины и дети — динамический объект OBJECTS.RES с тем же номером,
//! без снаряжения.

use crate::executable::{EQUIPMENT_PALETTES, Executable};
use crate::game_state::{Character, GameState, MapEntities};
use crate::graph_res::{GraphRes, Result, SpriteFrame, Sprites};
use crate::objects_res::ObjectsRes;
use crate::rle::IndexedImage;

pub const COLORS_PER_PALETTE: usize = 256;
/// У динамических объектов 6 анимаций; состояния 6–9 игра сводит к 0–3
/// (Slavik game_character.cpp:147)
pub const DYNAMIC_ANIMATIONS: u8 = 6;

#[derive(Debug, Clone)]
pub struct Layer {
    pub image: IndexedImage,
    /// Начало палитры в банке цветов GRAPH.RES (номер цвета)
    pub palette_offset: usize,
    /// Левый верхний угол на карте
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone)]
pub struct CharacterSprite {
    pub slot: usize,
    /// Сначала модель, потом снаряжение
    pub layers: Vec<Layer>,
    pub shadow: Option<Layer>,
}

impl CharacterSprite {
    /// Нижний край модели: по нему игра сортирует, что рисовать раньше.
    pub fn bottom(&self) -> i32 {
        let body = &self.layers[0];
        body.y + body.image.height as i32
    }
}

fn sequence_frame<'s>(
    sprites: &'s Sprites<'_>,
    animation: usize,
    direction: usize,
    frame: usize,
) -> Option<&'s SpriteFrame> {
    let sequence = sprites.sequences.get(animation)?.get(direction)?;
    if sequence.is_empty() {
        return None;
    }
    sequence.get(frame).or_else(|| sequence.first())
}

/// Спрайт персонажа и номер анимации его состояния.
fn sprites_for<'a>(
    character: &Character,
    graph: &'a GraphRes<'a>,
    objects: &'a ObjectsRes<'a>,
) -> (Option<&'a Sprites<'a>>, usize) {
    if character.has_dynamic_sprite() {
        (
            objects
                .dynamic
                .get(character.sprite_set as usize)
                .and_then(Option::as_ref),
            (character.state % DYNAMIC_ANIMATIONS) as usize,
        )
    } else {
        (
            graph.models.get(character.sprite_set as usize),
            character.state as usize,
        )
    }
}

/// Сколько кадров в анимации персонажа: его состояние и направление.
pub fn frame_count(character: &Character, graph: &GraphRes<'_>, objects: &ObjectsRes<'_>) -> usize {
    let (sprites, animation) = sprites_for(character, graph, objects);
    let Some(sprites) = sprites else { return 0 };
    sprites
        .sequences
        .get(animation)
        .and_then(|row| row.get(character.direction as usize))
        .map_or(0, Vec::len)
}

/// Кадр персонажа, стоящего в точке (x, y) карты; None — если кадра нет.
///
/// frame_index — номер кадра анимации; без него — кадр из записи персонажа.
// Аргументов столько же, сколько у эталона compose() в `konung/k1/characters.py`
#[allow(clippy::too_many_arguments)]
pub fn compose(
    character: &Character,
    x: i32,
    y: i32,
    state: &GameState,
    graph: &GraphRes<'_>,
    objects: &ObjectsRes<'_>,
    tables: &Executable,
    frame_index: Option<usize>,
) -> Result<Option<CharacterSprite>> {
    let (sprites, animation) = sprites_for(character, graph, objects);
    let Some(sprites) = sprites else {
        return Ok(None);
    };
    let direction = character.direction as usize;
    let frame_index = frame_index.unwrap_or(character.frame as usize);
    let Some(sprite_frame) = sequence_frame(sprites, animation, direction, frame_index) else {
        return Ok(None);
    };
    let Some(image) = sprites.image(sprite_frame.image_id)? else {
        return Ok(None);
    };

    let palette = usize::try_from(character.palette_offset.div_euclid(2)).unwrap_or(0);
    let mut layers = vec![Layer {
        image,
        palette_offset: palette,
        x: x + sprite_frame.image_offset.0 as i32,
        y: y + sprite_frame.image_offset.1 as i32,
    }];
    let shadow = sprites.shadow(sprite_frame.image_id)?.map(|image| Layer {
        image,
        palette_offset: 0,
        x: x + sprite_frame.shadow_offset.0 as i32,
        y: y + sprite_frame.shadow_offset.1 as i32,
    });

    if !character.has_dynamic_sprite() {
        for &slot in tables.draw_order.get(direction).unwrap_or(&[0; 6]).iter() {
            let item_index = character.equipment[slot];
            if item_index <= 0 {
                continue;
            }
            let Some(Some(item)) = state.items.get(item_index as usize) else {
                continue;
            };
            if item.info_id < 0 {
                continue;
            }
            let Some(info) = tables.equipment.get(item.info_id as usize) else {
                continue;
            };
            if info.sprite_set < 0 {
                continue;
            }
            let Some(gear) = graph.equipment.get(info.sprite_set as usize) else {
                continue;
            };
            let Some(gear_frame) = sequence_frame(gear, animation, direction, frame_index) else {
                continue;
            };
            let Some(gear_image) = gear.image(gear_frame.image_id)? else {
                continue;
            };
            layers.push(Layer {
                image: gear_image,
                palette_offset: EQUIPMENT_PALETTES[info.sprite_set as usize] * COLORS_PER_PALETTE,
                x: x + sprite_frame.weapon_offset.0 as i32 + gear_frame.image_offset.0 as i32,
                y: y + sprite_frame.weapon_offset.1 as i32 + gear_frame.image_offset.1 as i32,
            });
        }
    }
    Ok(Some(CharacterSprite {
        slot: character.slot,
        layers,
        shadow,
    }))
}

/// Все кадры анимации персонажа; кадр меняется каждый тик (Slavik game_play.cpp:3729).
pub fn animation(
    character: &Character,
    x: i32,
    y: i32,
    state: &GameState,
    graph: &GraphRes<'_>,
    objects: &ObjectsRes<'_>,
    tables: &Executable,
) -> Result<Vec<CharacterSprite>> {
    let mut sprites = Vec::new();
    for index in 0..frame_count(character, graph, objects) {
        if let Some(sprite) = compose(character, x, y, state, graph, objects, tables, Some(index))?
        {
            sprites.push(sprite);
        }
    }
    Ok(sprites)
}

/// Персонажи карты и клетки, где их рисовать.
///
/// NPC и жители — на своих местах. Бродячий отряд — его первый член в середине
/// прямоугольника появления: туда игра целится, расставляя отряд (Slavik PlaceMob).
pub fn placed<'a>(
    entities: &MapEntities<'a>,
    state: &'a GameState,
) -> Vec<(&'a Character, i32, i32)> {
    let mut result = entities
        .characters
        .iter()
        .map(|&character| (character, character.x as i32, character.y as i32))
        .collect::<Vec<_>>();
    for group in &entities.groups {
        if !group.is_roaming() || group.size == 0 {
            continue;
        }
        let Some(first) = state
            .characters
            .get(group.first_character as usize)
            .and_then(Option::as_ref)
        else {
            continue;
        };
        let spawn = group.spawn;
        let x = (i32::from(spawn.left) + i32::from(spawn.right) + 1) / 2;
        let y = (i32::from(spawn.top) + i32::from(spawn.bottom) + 1) / 2;
        result.push((first, x, y));
    }
    result
}
