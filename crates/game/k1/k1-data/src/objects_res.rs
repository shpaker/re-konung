//! OBJECTS.RES — объекты карты: 30 динамических и 482 простых
//! (docs/k1/formats/objects-res.md, эталон — `konung/k1/objects_res.py`).
//!
//! Простой объект — до 8 кадров (картинка + тень) со смещениями и точками огней.
//! Динамический объект — монстр: анимации по 8 направлениям, как спрайт персонажа.

use thiserror::Error;

use crate::graph_res::{Sprites, read_sprites};
use crate::reader::{ReadError, Reader};
use crate::rle::{self, IndexedImage, RleError};

pub type Result<T> = std::result::Result<T, ObjectsError>;

#[derive(Debug, Error)]
pub enum ObjectsError {
    #[error("объект {object_id}: огней {count}, слотов {FLAME_SLOTS}")]
    FlameCount { object_id: usize, count: i32 },
    #[error(transparent)]
    Graph(#[from] crate::graph_res::GraphError),
    #[error(transparent)]
    Rle(#[from] RleError),
    #[error(transparent)]
    Read(#[from] ReadError),
}

pub const SLOTS: usize = 512;
pub const DYNAMIC_COUNT: usize = 30;
pub const SIMPLE_COUNT: usize = SLOTS - DYNAMIC_COUNT;
pub const FRAMES: usize = 8;
pub const FLAME_SLOTS: usize = 20;
/// Тик игры, мс: на нём меняются кадры объектов, персонажей и огней (Slavik game.h:30)
pub const TICK_MS: u32 = 60;
// Динамический объект: 6 анимаций × 8 направлений × до 18 кадров, 512 картинок
pub const DYNAMIC_ANIMATIONS: usize = 6;
pub const DYNAMIC_FRAMES: usize = 18;
pub const DYNAMIC_IMAGES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flame {
    pub flame_id: i8,
    pub x: i16,
    pub y: i16,
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub image: Option<IndexedImage>,
    pub shadow: Option<IndexedImage>,
    /// Смещение левого верхнего угла картинки и тени от позиции объекта
    pub offset: (i16, i16),
    pub shadow_offset: (i16, i16),
}

#[derive(Debug, Clone)]
pub struct SimpleObject {
    pub object_id: usize,
    pub flags: u32,
    pub frame_count: u32,
    /// Начало палитры объекта (номер цвета в банке GRAPH.RES)
    pub palette_offset: usize,
    pub frame_time: i32,
    pub flames: Vec<Flame>,
    pub frames: Vec<Frame>,
    pub unknown: u32,
}

/// Сколько длится кадр анимированного объекта; None — объект не анимирован.
///
/// Кадр меняется раз в frame_time тиков (Slavik game_play.cpp:1003-1017).
pub fn frame_duration_ms(simple: &SimpleObject) -> Option<u32> {
    (simple.frame_count >= 2 && simple.frame_time >= 1).then(|| simple.frame_time as u32 * TICK_MS)
}

#[derive(Debug, Clone)]
pub struct ObjectsRes<'a> {
    /// Простые объекты по номерам (слот OBJECTS.RES − [DYNAMIC_COUNT]); None — слот пуст
    pub simple: Vec<Option<SimpleObject>>,
    /// Монстры по слотам: номер — Character.sprite_set у персонажей с битом 0x40 в классе
    pub dynamic: Vec<Option<Sprites<'a>>>,
}

fn simple_object(reader: &mut Reader<'_>, object_id: usize) -> Result<SimpleObject> {
    let flags = reader.u32()?;
    reader.skip(4); // размер блока
    let flame_count = reader.s32()?;
    if !(0..=FLAME_SLOTS as i32).contains(&flame_count) {
        // Число огней задаёт и длину хвоста (FLAME_SLOTS − count)·5: вне слотов
        // она стала бы отрицательной, а reserve — гигантским
        return Err(ObjectsError::FlameCount {
            object_id,
            count: flame_count,
        });
    }
    let flame_count = flame_count as usize;
    let frame_count = reader.u32()?;
    let unknown = reader.u32()?;
    let palette_offset = reader.u32()? as usize / 2;
    let frame_time = reader.s32()?;

    let mut flames = Vec::with_capacity(flame_count);
    for _ in 0..flame_count {
        flames.push(Flame {
            flame_id: reader.s8()?,
            x: reader.s16()?,
            y: reader.s16()?,
        });
    }
    reader.skip((FLAME_SLOTS - flame_count) * 5);

    let data_offsets = (0..FRAMES)
        .map(|_| Ok((reader.s32()?, reader.s32()?)))
        .collect::<Result<Vec<_>>>()?;
    let mut offsets = Vec::with_capacity(FRAMES);
    for _ in 0..FRAMES {
        let shadow_offset = (reader.s16()?, reader.s16()?);
        let offset = (reader.s16()?, reader.s16()?);
        offsets.push((shadow_offset, offset));
    }
    let data_start = reader.pos();

    let mut frames = Vec::with_capacity(FRAMES);
    for ((shadow_at, image_at), (shadow_offset, offset)) in data_offsets.into_iter().zip(offsets) {
        let mut image = None;
        if image_at >= 0 {
            reader.seek(data_start + image_at as usize);
            image = Some(rle::decode_indexed(reader)?);
        }
        let mut shadow = None;
        if shadow_at >= 0 {
            reader.seek(data_start + shadow_at as usize);
            shadow = Some(rle::decode_shadow(reader)?);
        }
        frames.push(Frame {
            image,
            shadow,
            offset,
            shadow_offset,
        });
    }

    Ok(SimpleObject {
        object_id,
        flags,
        frame_count,
        palette_offset,
        frame_time,
        flames,
        frames,
        unknown,
    })
}

pub fn parse(data: &[u8]) -> Result<ObjectsRes<'_>> {
    let mut reader = Reader::new(data);
    reader.skip(8); // метка и размер каталога
    let entries = (0..SLOTS)
        .map(|_| Ok((reader.s32()?, reader.s32()?)))
        .collect::<Result<Vec<_>>>()?;
    let data_start = reader.pos();

    let mut simple = vec![None; SIMPLE_COUNT];
    let mut dynamic = Vec::with_capacity(DYNAMIC_COUNT);
    for (slot, &(offset, _size)) in entries.iter().enumerate() {
        if offset < 0 {
            if slot < DYNAMIC_COUNT {
                dynamic.push(None);
            }
            continue;
        }
        if slot < DYNAMIC_COUNT {
            // Пропускаем метку и размер блока
            dynamic.push(Some(read_sprites(
                data,
                data_start + offset as usize + 8,
                DYNAMIC_ANIMATIONS,
                DYNAMIC_FRAMES,
                DYNAMIC_IMAGES,
                false,
            )?));
            continue;
        }
        reader.seek(data_start + offset as usize);
        let object_id = slot - DYNAMIC_COUNT;
        simple[object_id] = Some(simple_object(&mut reader, object_id)?);
    }
    Ok(ObjectsRes { simple, dynamic })
}
