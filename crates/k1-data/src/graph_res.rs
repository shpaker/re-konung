//! GRAPH.RES — палитры, курсоры, тайлы земли и спрайты персонажей
//! (docs/k1/formats/graph-res.md, эталон — `konung/k1/graph_res.py`).

use thiserror::Error;

use crate::palette::rgb555;
use crate::reader::{ReadError, Reader};
use crate::rle::{self, IndexedImage, RleError};

pub type Result<T> = std::result::Result<T, GraphError>;

#[derive(Debug, Error)]
pub enum GraphError {
    #[error(transparent)]
    Rle(#[from] RleError),
    #[error(transparent)]
    Read(#[from] ReadError),
}

pub const PALETTE_BANK_COLORS: usize = 256 * 256;
pub const CURSOR_COUNT: usize = 11;
/// Число слотов тайлов: декорация с тайлом за их пределами — сырая
/// ([crate::map::Decoration::is_raw]).
pub const TILE_SLOTS: usize = 512;
// Спрайты персонажей: 3 базовые модели и 24 вида снаряжения
pub const CHARACTER_MODELS: usize = 3;
pub const EQUIPMENT_SETS: usize = 24;
pub const CHARACTER_BLOCKS: usize = CHARACTER_MODELS + EQUIPMENT_SETS;
// В блоке персонажа: 10 анимаций × 8 направлений × до 12 кадров, 768 картинок
pub const CHARACTER_ANIMATIONS: usize = 10;
pub const DIRECTIONS: usize = 8;
pub const CHARACTER_FRAMES: usize = 12;
pub const CHARACTER_IMAGES: usize = 768;

/// Блок: u32 метка (смысл не ясен), u32 размер, данные; end — первый байт после блока.
///
/// От чего считается размер, зависит от вида блока: у курсора — от конца
/// заголовка, у тайлов — от конца таблицы, у спрайтов персонажа — от начала блока.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    pub offset: usize,
    pub tag: u32,
    pub size: u32,
    pub end: usize,
}

/// Кадр анимации: номер картинки и смещения от точки персонажа.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpriteFrame {
    pub image_id: i16,
    pub image_offset: (i16, i16),
    pub shadow_offset: (i16, i16),
    /// Куда цепляется снаряжение (только у персонажей GRAPH.RES)
    pub weapon_offset: (i16, i16),
}

/// Анимированный спрайт: [анимация][направление] → кадры; картинки — по запросу.
///
/// Так устроены и персонажи GRAPH.RES, и динамические объекты OBJECTS.RES.
/// Картинок сотни, поэтому они декодируются при обращении.
#[derive(Debug, Clone)]
pub struct Sprites<'a> {
    /// Байты всего файла: картинки декодируются из него по смещениям таблицы
    pub data: &'a [u8],
    /// Начало картинок; смещения в таблице считаются от него
    pub images_start: usize,
    pub sequences: Vec<Vec<Vec<SpriteFrame>>>,
    /// Пары (тень, картинка) смещений от images_start; < 0 — нет
    pub table: Vec<(i32, i32)>,
}

impl Sprites<'_> {
    pub fn image(&self, image_id: i16) -> Result<Option<IndexedImage>> {
        if image_id < 0 {
            return Ok(None);
        }
        let Some((_, offset)) = self.table.get(image_id as usize) else {
            return Ok(None);
        };
        if *offset < 0 {
            return Ok(None);
        }
        let mut reader = Reader::new(self.data);
        reader.seek(self.images_start + *offset as usize);
        Ok(Some(rle::decode_indexed(&mut reader)?))
    }

    pub fn shadow(&self, image_id: i16) -> Result<Option<IndexedImage>> {
        if image_id < 0 {
            return Ok(None);
        }
        let Some((offset, _)) = self.table.get(image_id as usize) else {
            return Ok(None);
        };
        // Нулевое смещение тени игра тоже считает пустым (Slavik resources.cpp:91)
        if *offset <= 0 {
            return Ok(None);
        }
        let mut reader = Reader::new(self.data);
        reader.seek(self.images_start + *offset as usize);
        Ok(Some(rle::decode_shadow(&mut reader)?))
    }
}

/// Заголовок спрайта с позиции start (сразу после метки и размера блока).
///
/// Порядок: число кадров и номера картинок по анимациям, смещения оружия
/// (если weapons), таблица картинок, смещения тени и картинки по кадрам.
/// У персонажей GRAPH.RES таблица идёт после смещений оружия, у динамических
/// объектов оружия нет (Slavik resources.cpp:20-102, 104-142).
pub fn read_sprites(
    data: &[u8],
    start: usize,
    animations: usize,
    frames: usize,
    images: usize,
    weapons: bool,
) -> Result<Sprites<'_>> {
    let mut reader = Reader::new(data);
    reader.seek(start);
    let mut counts = Vec::with_capacity(animations * DIRECTIONS);
    let mut ids = Vec::with_capacity(animations * DIRECTIONS);
    for _ in 0..animations * DIRECTIONS {
        counts.push(reader.s16()?);
        let mut row = Vec::with_capacity(frames);
        for _ in 0..frames {
            row.push(reader.s16()?);
        }
        ids.push(row);
    }
    let mut weapon = vec![vec![(0i16, 0i16); frames]; animations * DIRECTIONS];
    if weapons {
        for row in &mut weapon {
            for cell in row {
                *cell = (reader.s16()?, reader.s16()?);
            }
        }
    }
    let table = (0..images)
        .map(|_| Ok((reader.s32()?, reader.s32()?)))
        .collect::<Result<Vec<_>>>()?;
    let mut sequences = Vec::with_capacity(animations);
    for animation in 0..animations {
        let mut row = Vec::with_capacity(DIRECTIONS);
        for direction in 0..DIRECTIONS {
            let index = animation * DIRECTIONS + direction;
            let mut sequence = Vec::new();
            for frame in 0..frames {
                let shadow_offset = (reader.s16()?, reader.s16()?);
                let image_offset = (reader.s16()?, reader.s16()?);
                if i16::try_from(frame).unwrap() < counts[index] {
                    sequence.push(SpriteFrame {
                        image_id: ids[index][frame],
                        image_offset,
                        shadow_offset,
                        weapon_offset: weapon[index][frame],
                    });
                }
            }
            row.push(sequence);
        }
        sequences.push(row);
    }
    Ok(Sprites {
        data,
        images_start: reader.pos(),
        sequences,
        table,
    })
}

#[derive(Debug, Clone)]
pub struct Tile {
    pub index: usize,
    pub image: IndexedImage,
    /// Начало палитры тайла (номер цвета в банке)
    pub palette_offset: usize,
}

#[derive(Debug, Clone)]
pub struct GraphRes<'a> {
    /// Банк из 65536 цветов RGB555; палитра тайла — 256 цветов с palette_offset
    pub colors: Vec<u16>,
    /// Курсоры не декодируются: блоки пропущены по размеру
    pub cursors: Vec<Block>,
    /// Тайлы по слотам; None — слот пуст
    pub tiles: Vec<Option<Tile>>,
    pub tiles_block: Block,
    pub character_blocks: Vec<Block>,
    /// Модели людей 0–2 (Character.sprite_set) и наборы снаряжения 0–23
    pub models: Vec<Sprites<'a>>,
    pub equipment: Vec<Sprites<'a>>,
    pub size: usize,
}

impl GraphRes<'_> {
    pub fn palette(&self, offset: usize) -> Vec<(u8, u8, u8)> {
        self.colors[offset..offset + 256]
            .iter()
            .map(|&c| rgb555(c))
            .collect()
    }

    pub fn tile(&self, index: usize) -> Option<&Tile> {
        self.tiles.get(index).and_then(Option::as_ref)
    }
}

fn block(reader: &mut Reader<'_>) -> Result<Block> {
    let offset = reader.pos();
    let tag = reader.u32()?;
    let size = reader.u32()?;
    Ok(Block {
        offset,
        tag,
        size,
        end: 0, // считается вызывающим: от конца заголовка, таблицы или начала блока
    })
}

pub fn parse(data: &[u8]) -> Result<GraphRes<'_>> {
    let mut reader = Reader::new(data);
    let colors = (0..PALETTE_BANK_COLORS)
        .map(|_| reader.u16())
        .collect::<std::result::Result<Vec<_>, _>>()?;

    // Курсоры: размер — от конца заголовка; картинки не нужны, пропускаем блоки
    let mut cursors = Vec::with_capacity(CURSOR_COUNT);
    for _ in 0..CURSOR_COUNT {
        let mut cursor = block(&mut reader)?;
        cursor.end = reader.pos() + cursor.size as usize;
        cursors.push(cursor);
        reader.seek(cursor.end);
    }

    // Тайлы: размер — от конца таблицы слотов
    let mut tiles_block = block(&mut reader)?;
    let entries = (0..TILE_SLOTS)
        .map(|_| Ok((reader.s32()?, reader.s32()?)))
        .collect::<Result<Vec<_>>>()?;
    let data_start = reader.pos();
    tiles_block.end = data_start + tiles_block.size as usize;
    let mut tiles = vec![None; TILE_SLOTS];
    for (index, &(offset, palette)) in entries.iter().enumerate() {
        if offset < 0 || palette < 0 {
            continue;
        }
        reader.seek(data_start + offset as usize);
        let image = rle::decode_indexed(&mut reader)?;
        // Смещение палитры записано в байтах от начала банка
        tiles[index] = Some(Tile {
            index,
            image,
            palette_offset: palette as usize / 2,
        });
    }

    reader.seek(tiles_block.end);
    let mut character_blocks = Vec::with_capacity(CHARACTER_BLOCKS);
    let mut sprites = Vec::with_capacity(CHARACTER_BLOCKS);
    for _ in 0..CHARACTER_BLOCKS {
        // Размер спрайта персонажа — от начала блока
        let mut sprites_block = block(&mut reader)?;
        sprites_block.end = sprites_block.offset + sprites_block.size as usize;
        character_blocks.push(sprites_block);
        sprites.push(read_sprites(
            data,
            reader.pos(),
            CHARACTER_ANIMATIONS,
            CHARACTER_FRAMES,
            CHARACTER_IMAGES,
            true,
        )?);
        reader.seek(sprites_block.end);
    }
    let size = reader.pos();

    let (models, equipment) = sprites.split_at(CHARACTER_MODELS);
    Ok(GraphRes {
        colors,
        cursors,
        tiles,
        tiles_block,
        character_blocks,
        models: models.to_vec(),
        equipment: equipment.to_vec(),
        size,
    })
}
