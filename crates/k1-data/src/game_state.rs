//! `Game.N` — начальное состояние новой игры: персонажи, отряды, клады, переходы
//! и деревни на всех картах (docs/k1/formats/game-state.md,
//! эталон — `konung/k1/game_state.py`).
//!
//! Места — клетки мелкой сетки 160×320. В файле у них сначала y, потом x,
//! в структурах — как принято у нас: x, y.

use thiserror::Error;

use crate::map;
use crate::reader::Reader;
use crate::writer::Writer;

pub const ITEM_SLOTS: usize = 8192;
pub const GROUP_SLOTS: usize = 200;
pub const LOOT_SLOTS: usize = 500;
pub const EXIT_SLOTS: usize = 250;
pub const CHARACTER_SLOTS: usize = 2000;
pub const VILLAGE_SLOTS: usize = 20;

/// Размеры записей — как struct-форматы Python (docs/k1/formats/game-state.md)
pub const ITEM_SIZE: usize = 16; // <bbbb4sfhBB
pub const GROUP_SIZE: usize = 32; // <hih8hBBBBBBBB
pub const LOOT_SIZE: usize = 64; // <8shhhhhh22h
pub const EXIT_SIZE: usize = 17; // <BBBbBhhhhhh
pub const CHARACTER_SIZE: usize = 256; // <BBBBBBhhhhhBBB32sBiii9h2s9h6h32H5H10hBBBBBBBBi36s
pub const BUILDING_SLOTS: usize = 119;
pub const BUILDING_SIZE: usize = 8; // <bbhhh
pub const JOBS: usize = 5;
pub const JOB_SIZE: usize = 8; // <BBhhh
pub const VILLAGE_HEAD_SIZE: usize = 24; // <BBBB5i
/// Хвост деревни: <5i3h25h32h23hBBb; поля (таймеры, товары знахаря, купца и
/// кузнеца, флаги) в записи не подписаны — храним как есть
pub const VILLAGE_TAIL_SIZE: usize = 189;
pub const VILLAGE_SIZE: usize =
    VILLAGE_HEAD_SIZE + BUILDING_SLOTS * BUILDING_SIZE + JOBS * JOB_SIZE + VILLAGE_TAIL_SIZE; // 1205
pub const GAME_STATE_SIZE: usize = ITEM_SLOTS * ITEM_SIZE
    + GROUP_SLOTS * GROUP_SIZE
    + LOOT_SLOTS * LOOT_SIZE
    + EXIT_SLOTS * EXIT_SIZE
    + CHARACTER_SLOTS * CHARACTER_SIZE
    + VILLAGE_SLOTS * VILLAGE_SIZE;

const _: () = assert!(
    GAME_STATE_SIZE == 709822,
    "размер Game.N не сходится с документированным"
);

/// Рабочие места деревни по порядку (Slavik game_locale.cpp, Locale::Jobs)
pub const JOB_ROLES: [&str; JOBS] = ["elder", "healer", "merchant", "smith", "voevoda"];

/// Навыки (Character::skills) и характеристики (Character::attributes) по порядку
/// в записи персонажа — по Slavik (docs/k1/formats/game-state.md); в Tiled
/// свойства персонажа называются так же, в camelCase (agility_base → agilityBase)
pub const SKILL_NAMES: [&str; 9] = [
    "fencing",
    "trading",
    "accuracy",
    "carpentry",
    "medicine",
    "identification",
    "smithing",
    "armor",
    "poison",
];
pub const ATTRIBUTE_NAMES: [&str; 9] = [
    "agility",
    "agility_base",
    "charisma",
    "charisma_base",
    "health",
    "strength",
    "strength_base",
    "stamina",
    "stamina_base",
];

/// Колчан со стрелами: концентрация у него — целое число стрел, у остальных — float
/// (Slavik game_item.cpp:185); название — запись 30–33 таблицы снаряжения
pub const QUIVER_ITEM_TYPE: i8 = 12;

/// Виды отрядов (байт 0x1E); бродячих игра ставит в случайную клетку
/// прямоугольника появления, остальных — на место из записи персонажа
pub const PLAYER_GROUP: u8 = 0x20;
pub const FIRST_FIXED_GROUP: u8 = 0x51;
pub const VILLAGE_GROUP_BIT: u8 = 0x80;

pub type Result<T> = std::result::Result<T, StateError>;

#[derive(Debug, Error)]
pub enum StateError {
    #[error("Game.N {actual} байт, ожидалось {GAME_STATE_SIZE}")]
    WrongSize { actual: usize },
    #[error("кладов {count}, максимум {LOOT_SLOTS}")]
    TooManyLoot { count: usize },
    #[error("переходов {count}, максимум {EXIT_SLOTS}")]
    TooManyExits { count: usize },
    #[error("клад {slot}: карта 0 обрывает список")]
    LootZeroMap { slot: usize },
    #[error("переход {slot}: целевая карта 0 обрывает список")]
    ExitZeroMap { slot: usize },
    #[error("получилось {actual} байт вместо {expected}: неверный размер блоков")]
    WrongSerializedSize { actual: usize, expected: usize },
    #[error(transparent)]
    Read(#[from] crate::reader::ReadError),
}

/// Прямоугольник в мелкой сетке; обе границы входят.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rect {
    pub left: i16,
    pub top: i16,
    pub right: i16,
    pub bottom: i16,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Item {
    pub slot: usize,
    pub type_id: i8,
    pub bonus_id: i8,
    pub special_id: i8,
    pub info_id: i8,
    /// 4 байта как есть: s32 у колчана, f32 у остальных ([Item::concentration_value])
    pub concentration: [u8; 4],
    pub weight: f32,
    pub poison: i16,
    pub flags: u8,
    pub unknown: u8,
}

/// Значение концентрации: целое число стрел у колчана, float у остальных.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Concentration {
    Arrows(i32),
    Weight(f32),
}

impl Item {
    pub fn concentration_value(&self) -> Concentration {
        if self.type_id == QUIVER_ITEM_TYPE {
            Concentration::Arrows(i32::from_le_bytes(self.concentration))
        } else {
            Concentration::Weight(f32::from_le_bytes(self.concentration))
        }
    }
}

/// Отряд: персонажи first_character … first_character + size − 1 на карте map_id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Group {
    pub slot: usize,
    pub first_character: i16,
    pub respawn_timer: i32,
    pub growth_timer: i16,
    pub spawn: Rect,
    pub leash: Rect,
    pub max_size: u8,
    pub growth_limit: u8,
    pub target_group: u8,
    pub map_id: u8,
    pub size: u8,
    pub ai_state: u8,
    pub kind: u8,
    pub unknown: u8,
}

impl Group {
    /// Бродячий отряд: игра расставляет его членов случайно в spawn.
    pub fn is_roaming(&self) -> bool {
        self.kind < FIRST_FIXED_GROUP && self.kind & VILLAGE_GROUP_BIT == 0
    }

    pub fn members(&self) -> impl Iterator<Item = usize> + Clone {
        let start = self.first_character as usize;
        start..start + self.size as usize
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loot {
    pub slot: usize,
    pub x: i16,
    pub y: i16,
    pub gold: i16,
    pub map_id: i16,
    pub items: [i16; 22],
    /// Поля, которые игра считает при загрузке: пиксели, картинка, номер на карте
    pub pixels: [u8; 8],
    pub image: i16,
    pub loot_id: i16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exit {
    pub slot: usize,
    pub map_id: i8,
    pub target_map: u8,
    pub zone: Rect,
    pub arrival_x: i16,
    pub arrival_y: i16,
    pub direction: u8,
    pub unknown: [u8; 2],
}

// Имена полей unknown_* — как в Python-этлоне и docs/k1/formats/game-state.md
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, PartialEq)]
pub struct Character {
    pub slot: usize,
    pub state: u8,
    pub direction: u8,
    pub animation_flags: [u8; 2],
    pub class_id: u8,
    pub group: u8,
    pub enemy: i16,
    pub x: i16,
    pub y: i16,
    pub move_x: i16,
    pub move_y: i16,
    pub frame: u8,
    pub unknown_11: u8,
    pub weapon_slot: u8,
    pub path: [u8; 32],
    pub level: u8,
    pub gold: i32,
    pub free_points: i32,
    pub experience: i32,
    pub skills: [i16; 9],
    pub unknown_52: [u8; 2],
    pub attributes: [i16; 9],
    pub equipment: [i16; 6],
    pub inventory: [u16; 32],
    pub accessories: [u16; 5],
    pub misc: [i16; 10],
    pub unknown_d0: u8,
    pub flags: u8,
    pub unknown_d2: [u8; 2],
    pub name_id: u8,
    pub nick_id: u8,
    pub sprite_set: u8,
    pub frame_count: u8,
    pub palette_offset: i32,
    /// Пиксельные позиции и размер картинки: их считает игра, в файлах нули
    pub runtime: [u8; 36],
}

impl Character {
    /// Бит 0x40 класса: спрайт — динамический объект OBJECTS.RES, а не модель GRAPH.RES.
    /// Так рисуются монстры, а ещё женщины и дети в деревнях.
    pub fn has_dynamic_sprite(&self) -> bool {
        self.class_id & 0x40 != 0
    }
}

// Default — все поля нулевые; массивы длиннее 32 не выводят Default из derive
impl Default for Character {
    fn default() -> Self {
        Self {
            slot: 0,
            state: 0,
            direction: 0,
            animation_flags: [0; 2],
            class_id: 0,
            group: 0,
            enemy: 0,
            x: 0,
            y: 0,
            move_x: 0,
            move_y: 0,
            frame: 0,
            unknown_11: 0,
            weapon_slot: 0,
            path: [0; 32],
            level: 0,
            gold: 0,
            free_points: 0,
            experience: 0,
            skills: [0; 9],
            unknown_52: [0; 2],
            attributes: [0; 9],
            equipment: [0; 6],
            inventory: [0; 32],
            accessories: [0; 5],
            misc: [0; 10],
            unknown_d0: 0,
            flags: 0,
            unknown_d2: [0; 2],
            name_id: 0,
            nick_id: 0,
            sprite_set: 0,
            frame_count: 0,
            palette_offset: 0,
            runtime: [0; 36],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Building {
    pub info_id: i8,
    pub state: i8,
    /// Номер объекта Konung.N той же карты
    pub object_index: i16,
    pub unknown: i16,
    pub countdown: i16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub role: &'static str,
    pub direction: u8,
    pub unknown: u8,
    pub character: i16,
    pub x: i16,
    pub y: i16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Village {
    pub slot: usize,
    pub building_counts: (u8, u8),
    pub elder_group: u8,
    pub map_id: u8,
    pub timers: [i32; 5],
    pub buildings: Vec<Building>,
    pub jobs: Vec<Job>,
    /// Всё после рабочих мест: таймеры, товары знахаря, купца и кузнеца, флаги.
    /// Поля в записи не подписаны, храним как есть ([VILLAGE_TAIL_SIZE])
    pub tail: [u8; VILLAGE_TAIL_SIZE],
}

/// Пустые слоты — None: у предметов это 16 × 0xFF, у остальных — нули.
#[derive(Debug, Clone, PartialEq)]
pub struct GameState {
    pub items: Vec<Option<Item>>,
    pub groups: Vec<Option<Group>>,
    pub loot: Vec<Loot>,
    pub exits: Vec<Exit>,
    pub characters: Vec<Option<Character>>,
    pub villages: Vec<Option<Village>>,
}

fn all_bytes(raw: &[u8], byte: u8) -> bool {
    raw.iter().all(|&b| b == byte)
}

fn read_arr<const N: usize>(reader: &mut Reader<'_>) -> [u8; N] {
    reader.bytes(N).unwrap().try_into().unwrap()
}

fn read_i16s<const N: usize>(reader: &mut Reader<'_>) -> [i16; N] {
    let mut values = [0i16; N];
    for value in &mut values {
        *value = reader.s16().unwrap();
    }
    values
}

fn read_u16s<const N: usize>(reader: &mut Reader<'_>) -> [u16; N] {
    let mut values = [0u16; N];
    for value in &mut values {
        *value = reader.u16().unwrap();
    }
    values
}

fn read_i32s<const N: usize>(reader: &mut Reader<'_>) -> [i32; N] {
    let mut values = [0i32; N];
    for value in &mut values {
        *value = reader.s32().unwrap();
    }
    values
}

fn parse_item(slot: usize, raw: &[u8]) -> Item {
    let mut reader = Reader::new(raw);
    Item {
        slot,
        type_id: reader.s8().unwrap(),
        bonus_id: reader.s8().unwrap(),
        special_id: reader.s8().unwrap(),
        info_id: reader.s8().unwrap(),
        concentration: read_arr(&mut reader),
        weight: reader.f32().unwrap(),
        poison: reader.s16().unwrap(),
        flags: reader.u8().unwrap(),
        unknown: reader.u8().unwrap(),
    }
}

fn parse_group(slot: usize, raw: &[u8]) -> Group {
    let mut reader = Reader::new(raw);
    let first_character = reader.s16().unwrap();
    let respawn_timer = reader.s32().unwrap();
    let growth_timer = reader.s16().unwrap();
    // Прямоугольники в файле вперемешку: top, top, bottom, bottom, left, left, right, right
    let rects = read_i16s::<8>(&mut reader);
    let spawn = Rect {
        left: rects[4],
        top: rects[0],
        right: rects[6],
        bottom: rects[2],
    };
    let leash = Rect {
        left: rects[5],
        top: rects[1],
        right: rects[7],
        bottom: rects[3],
    };
    Group {
        slot,
        first_character,
        respawn_timer,
        growth_timer,
        spawn,
        leash,
        max_size: reader.u8().unwrap(),
        growth_limit: reader.u8().unwrap(),
        target_group: reader.u8().unwrap(),
        map_id: reader.u8().unwrap(),
        size: reader.u8().unwrap(),
        ai_state: reader.u8().unwrap(),
        kind: reader.u8().unwrap(),
        unknown: reader.u8().unwrap(),
    }
}

fn parse_loot(slot: usize, raw: &[u8]) -> Loot {
    let mut reader = Reader::new(raw);
    let pixels = read_arr::<8>(&mut reader);
    let y = reader.s16().unwrap();
    let x = reader.s16().unwrap();
    let image = reader.s16().unwrap();
    let loot_id = reader.s16().unwrap();
    let gold = reader.s16().unwrap();
    let map_id = reader.s16().unwrap();
    let items = read_i16s::<22>(&mut reader);
    Loot {
        slot,
        x,
        y,
        gold,
        map_id,
        items,
        pixels,
        image,
        loot_id,
    }
}

fn parse_exit(slot: usize, raw: &[u8]) -> Exit {
    let mut reader = Reader::new(raw);
    let unknown = read_arr::<2>(&mut reader);
    let direction = reader.u8().unwrap();
    let map_id = reader.s8().unwrap();
    let target_map = reader.u8().unwrap();
    let arrival_y = reader.s16().unwrap();
    let arrival_x = reader.s16().unwrap();
    let zone = Rect {
        top: reader.s16().unwrap(),
        left: reader.s16().unwrap(),
        bottom: reader.s16().unwrap(),
        right: reader.s16().unwrap(),
    };
    Exit {
        slot,
        map_id,
        target_map,
        zone,
        arrival_x,
        arrival_y,
        direction,
        unknown,
    }
}

fn parse_character(slot: usize, raw: &[u8]) -> Character {
    let mut reader = Reader::new(raw);
    Character {
        slot,
        state: reader.u8().unwrap(),
        direction: reader.u8().unwrap(),
        animation_flags: read_arr(&mut reader),
        class_id: reader.u8().unwrap(),
        group: reader.u8().unwrap(),
        enemy: reader.s16().unwrap(),
        y: reader.s16().unwrap(),
        x: reader.s16().unwrap(),
        move_y: reader.s16().unwrap(),
        move_x: reader.s16().unwrap(),
        frame: reader.u8().unwrap(),
        unknown_11: reader.u8().unwrap(),
        weapon_slot: reader.u8().unwrap(),
        path: read_arr(&mut reader),
        level: reader.u8().unwrap(),
        gold: reader.s32().unwrap(),
        free_points: reader.s32().unwrap(),
        experience: reader.s32().unwrap(),
        skills: read_i16s(&mut reader),
        unknown_52: read_arr(&mut reader),
        attributes: read_i16s(&mut reader),
        equipment: read_i16s(&mut reader),
        inventory: read_u16s(&mut reader),
        accessories: read_u16s(&mut reader),
        misc: read_i16s(&mut reader),
        unknown_d0: reader.u8().unwrap(),
        flags: reader.u8().unwrap(),
        unknown_d2: read_arr(&mut reader),
        name_id: reader.u8().unwrap(),
        nick_id: reader.u8().unwrap(),
        sprite_set: reader.u8().unwrap(),
        frame_count: reader.u8().unwrap(),
        palette_offset: reader.s32().unwrap(),
        runtime: read_arr(&mut reader),
    }
}

fn parse_village(slot: usize, raw: &[u8]) -> Village {
    let mut reader = Reader::new(raw);
    let first = reader.u8().unwrap();
    let second = reader.u8().unwrap();
    let elder_group = reader.u8().unwrap();
    let map_id = reader.u8().unwrap();
    let timers = read_i32s(&mut reader);
    let mut buildings = Vec::with_capacity(BUILDING_SLOTS);
    for _ in 0..BUILDING_SLOTS {
        buildings.push(Building {
            info_id: reader.s8().unwrap(),
            state: reader.s8().unwrap(),
            object_index: reader.s16().unwrap(),
            unknown: reader.s16().unwrap(),
            countdown: reader.s16().unwrap(),
        });
    }
    let mut jobs = Vec::with_capacity(JOBS);
    for role in JOB_ROLES {
        let direction = reader.u8().unwrap();
        let unknown = reader.u8().unwrap();
        let character = reader.s16().unwrap();
        let y = reader.s16().unwrap();
        let x = reader.s16().unwrap();
        jobs.push(Job {
            role,
            direction,
            unknown,
            character,
            x,
            y,
        });
    }
    let tail = read_arr(&mut reader);
    Village {
        slot,
        building_counts: (first, second),
        elder_group,
        map_id,
        timers,
        buildings,
        jobs,
        tail,
    }
}

pub fn parse(data: &[u8]) -> Result<GameState> {
    if data.len() != GAME_STATE_SIZE {
        return Err(StateError::WrongSize { actual: data.len() });
    }

    let chunks = |offset: usize, slots: usize, size: usize| {
        (0..slots).map(move |slot| {
            (
                slot,
                &data[offset + slot * size..offset + (slot + 1) * size],
            )
        })
    };
    let mut offset = 0;
    let take = |offset: &mut usize, slots: usize, size: usize| {
        let start = *offset;
        *offset += slots * size;
        start
    };

    let items_start = take(&mut offset, ITEM_SLOTS, ITEM_SIZE);
    let items = chunks(items_start, ITEM_SLOTS, ITEM_SIZE)
        .map(|(slot, raw)| (!all_bytes(raw, 0xFF)).then(|| parse_item(slot, raw)))
        .collect();

    let groups_start = take(&mut offset, GROUP_SLOTS, GROUP_SIZE);
    let groups = chunks(groups_start, GROUP_SLOTS, GROUP_SIZE)
        .map(|(slot, raw)| (!all_bytes(raw, 0)).then(|| parse_group(slot, raw)))
        .collect();

    // Клады и переходы идут подряд с нулевого слота; список обрывает первая
    // запись с картой 0, как в игре (хвост в файлах дистрибутива — нули)
    let loot_start = take(&mut offset, LOOT_SLOTS, LOOT_SIZE);
    let mut loot = Vec::new();
    for (slot, raw) in chunks(loot_start, LOOT_SLOTS, LOOT_SIZE) {
        let entry = parse_loot(slot, raw);
        if entry.map_id == 0 {
            break;
        }
        loot.push(entry);
    }

    let exits_start = take(&mut offset, EXIT_SLOTS, EXIT_SIZE);
    let mut exits = Vec::new();
    for (slot, raw) in chunks(exits_start, EXIT_SLOTS, EXIT_SIZE) {
        let entry = parse_exit(slot, raw);
        if entry.target_map == 0 {
            break;
        }
        exits.push(entry);
    }

    let characters_start = take(&mut offset, CHARACTER_SLOTS, CHARACTER_SIZE);
    let characters = chunks(characters_start, CHARACTER_SLOTS, CHARACTER_SIZE)
        .map(|(slot, raw)| (!all_bytes(raw, 0)).then(|| parse_character(slot, raw)))
        .collect();

    let villages_start = take(&mut offset, VILLAGE_SLOTS, VILLAGE_SIZE);
    let villages = chunks(villages_start, VILLAGE_SLOTS, VILLAGE_SIZE)
        .map(|(slot, raw)| (!all_bytes(raw, 0)).then(|| parse_village(slot, raw)))
        .collect();

    Ok(GameState {
        items,
        groups,
        loot,
        exits,
        characters,
        villages,
    })
}

fn write_item(writer: &mut Writer, item: &Option<Item>) {
    let Some(item) = item else {
        writer.bytes(&[0xFF; ITEM_SIZE]);
        return;
    };
    writer.s8(item.type_id);
    writer.s8(item.bonus_id);
    writer.s8(item.special_id);
    writer.s8(item.info_id);
    writer.bytes(&item.concentration);
    writer.f32(item.weight);
    writer.s16(item.poison);
    writer.u8(item.flags);
    writer.u8(item.unknown);
}

fn write_group(writer: &mut Writer, group: &Option<Group>) {
    let Some(group) = group else {
        writer.zeros(GROUP_SIZE);
        return;
    };
    writer.s16(group.first_character);
    writer.s32(group.respawn_timer);
    writer.s16(group.growth_timer);
    for rect in [&group.spawn, &group.leash] {
        writer.s16(rect.top);
    }
    for rect in [&group.spawn, &group.leash] {
        writer.s16(rect.bottom);
    }
    for rect in [&group.spawn, &group.leash] {
        writer.s16(rect.left);
    }
    for rect in [&group.spawn, &group.leash] {
        writer.s16(rect.right);
    }
    writer.u8(group.max_size);
    writer.u8(group.growth_limit);
    writer.u8(group.target_group);
    writer.u8(group.map_id);
    writer.u8(group.size);
    writer.u8(group.ai_state);
    writer.u8(group.kind);
    writer.u8(group.unknown);
}

fn write_loot(writer: &mut Writer, loot: &Loot) {
    writer.bytes(&loot.pixels);
    writer.s16(loot.y);
    writer.s16(loot.x);
    writer.s16(loot.image);
    writer.s16(loot.loot_id);
    writer.s16(loot.gold);
    writer.s16(loot.map_id);
    for item in loot.items {
        writer.s16(item);
    }
}

fn write_exit(writer: &mut Writer, exit: &Exit) {
    writer.bytes(&exit.unknown);
    writer.u8(exit.direction);
    writer.s8(exit.map_id);
    writer.u8(exit.target_map);
    writer.s16(exit.arrival_y);
    writer.s16(exit.arrival_x);
    writer.s16(exit.zone.top);
    writer.s16(exit.zone.left);
    writer.s16(exit.zone.bottom);
    writer.s16(exit.zone.right);
}

fn write_character(writer: &mut Writer, character: &Option<Character>) {
    let Some(c) = character else {
        writer.zeros(CHARACTER_SIZE);
        return;
    };
    writer.u8(c.state);
    writer.u8(c.direction);
    writer.bytes(&c.animation_flags);
    writer.u8(c.class_id);
    writer.u8(c.group);
    writer.s16(c.enemy);
    writer.s16(c.y);
    writer.s16(c.x);
    writer.s16(c.move_y);
    writer.s16(c.move_x);
    writer.u8(c.frame);
    writer.u8(c.unknown_11);
    writer.u8(c.weapon_slot);
    writer.bytes(&c.path);
    writer.u8(c.level);
    writer.s32(c.gold);
    writer.s32(c.free_points);
    writer.s32(c.experience);
    for value in c.skills {
        writer.s16(value);
    }
    writer.bytes(&c.unknown_52);
    for value in c.attributes {
        writer.s16(value);
    }
    for value in c.equipment {
        writer.s16(value);
    }
    for value in c.inventory {
        writer.u16(value);
    }
    for value in c.accessories {
        writer.u16(value);
    }
    for value in c.misc {
        writer.s16(value);
    }
    writer.u8(c.unknown_d0);
    writer.u8(c.flags);
    writer.bytes(&c.unknown_d2);
    writer.u8(c.name_id);
    writer.u8(c.nick_id);
    writer.u8(c.sprite_set);
    writer.u8(c.frame_count);
    writer.s32(c.palette_offset);
    writer.bytes(&c.runtime);
}

fn write_village(writer: &mut Writer, village: &Option<Village>) {
    let Some(village) = village else {
        writer.zeros(VILLAGE_SIZE);
        return;
    };
    writer.u8(village.building_counts.0);
    writer.u8(village.building_counts.1);
    writer.u8(village.elder_group);
    writer.u8(village.map_id);
    for timer in village.timers {
        writer.s32(timer);
    }
    for building in &village.buildings {
        writer.s8(building.info_id);
        writer.s8(building.state);
        writer.s16(building.object_index);
        writer.s16(building.unknown);
        writer.s16(building.countdown);
    }
    for job in &village.jobs {
        writer.u8(job.direction);
        writer.u8(job.unknown);
        writer.s16(job.character);
        writer.s16(job.y);
        writer.s16(job.x);
    }
    writer.bytes(&village.tail);
}

/// Состояние обратно в формат `Game.N`.
///
/// Клады и переходы пишутся подряд с нулевого слота в порядке списков,
/// остальные слоты — нули: так устроены все файлы дистрибутива.
pub fn serialize(state: &GameState) -> Result<Vec<u8>> {
    if state.loot.len() > LOOT_SLOTS {
        return Err(StateError::TooManyLoot {
            count: state.loot.len(),
        });
    }
    if state.exits.len() > EXIT_SLOTS {
        return Err(StateError::TooManyExits {
            count: state.exits.len(),
        });
    }
    for loot in &state.loot {
        if loot.map_id == 0 {
            return Err(StateError::LootZeroMap { slot: loot.slot });
        }
    }
    for exit in &state.exits {
        if exit.target_map == 0 {
            return Err(StateError::ExitZeroMap { slot: exit.slot });
        }
    }

    let mut writer = Writer::with_capacity(GAME_STATE_SIZE);
    for item in &state.items {
        write_item(&mut writer, item);
    }
    for group in &state.groups {
        write_group(&mut writer, group);
    }
    for loot in &state.loot {
        write_loot(&mut writer, loot);
    }
    writer.zeros(LOOT_SIZE * (LOOT_SLOTS - state.loot.len()));
    for exit in &state.exits {
        write_exit(&mut writer, exit);
    }
    writer.zeros(EXIT_SIZE * (EXIT_SLOTS - state.exits.len()));
    for character in &state.characters {
        write_character(&mut writer, character);
    }
    for village in &state.villages {
        write_village(&mut writer, village);
    }

    let out = writer.into_vec();
    if out.len() != GAME_STATE_SIZE {
        return Err(StateError::WrongSerializedSize {
            actual: out.len(),
            expected: GAME_STATE_SIZE,
        });
    }
    Ok(out)
}

/// Всё из `Game.N`, что стоит на одной карте.
#[derive(Debug, Clone, PartialEq)]
pub struct MapEntities<'a> {
    /// Отряды этой карты, кроме отряда игрока; у бродячих важен прямоугольник
    /// появления, у остальных — места персонажей
    pub groups: Vec<&'a Group>,
    /// Персонажи с местом из файла: NPC и жители
    pub characters: Vec<&'a Character>,
    /// Отряд игрока, если эта карта — стартовая
    pub start: Option<&'a Group>,
    pub loot: Vec<&'a Loot>,
    pub exits: Vec<&'a Exit>,
    /// Переходы с других карт, которые ведут сюда: важно их место прибытия
    pub arrivals: Vec<&'a Exit>,
    pub jobs: Vec<(&'a Village, &'a Job)>,
}

/// Карта, где начинается новая игра из `Game.<hero>`.
///
/// Карту в отряде игрока игра не читает (во всех файлах там 1), а считает
/// по номеру героя (Slavik game.cpp:2458).
pub fn start_map(hero: usize) -> usize {
    3 * hero + 1
}

/// Что стоит на карте `number`; `hero` — номер `Game.N`, чтобы найти точку старта.
pub fn on_map(state: &GameState, number: usize, hero: Option<usize>) -> MapEntities<'_> {
    let player = state.groups.first().and_then(|group| group.as_ref());
    let start = player.filter(|_| hero.is_some_and(|hero| start_map(hero) == number));
    let groups: Vec<&Group> = state
        .groups
        .iter()
        .filter_map(Option::as_ref)
        .filter(|group| group.kind != PLAYER_GROUP && group.map_id as usize == number)
        .collect();
    let mut characters = Vec::new();
    for group in &groups {
        if group.is_roaming() {
            continue;
        }
        for index in group.members() {
            if let Some(character) = state.characters.get(index).and_then(Option::as_ref) {
                characters.push(character);
            }
        }
    }
    let jobs = state
        .villages
        .iter()
        .filter_map(|village| {
            let village = village.as_ref()?;
            (village.map_id as usize == number).then_some(village)
        })
        .flat_map(|village: &Village| village.jobs.iter().map(move |job| (village, job)))
        .collect();
    MapEntities {
        groups,
        characters,
        start,
        loot: state
            .loot
            .iter()
            .filter(|loot| loot.map_id as usize == number)
            .collect(),
        exits: state
            .exits
            .iter()
            .filter(|exit| exit.map_id as usize == number)
            .collect(),
        arrivals: state
            .exits
            .iter()
            .filter(|exit| exit.target_map as usize == number)
            .collect(),
        jobs,
    }
}

// Клетка мелкой сетки: шаг 58 по x, 16 по строке; нечётные строки сдвинуты
pub const FINE_STEP_X: i32 = map::TILE_STEP_X / 2;
pub const FINE_STEP_Y: i32 = map::TILE_STEP_Y / 2;

/// Точка персонажа или клада (клетка мелкой сетки) в пикселях карты.
///
/// По Slavik — середина клетки: на (58, 16) правее и ниже позиции объекта карты.
pub fn character_position(x: i32, y: i32) -> (i32, i32) {
    (
        x * FINE_STEP_X + (2 - (y & 1)) * (FINE_STEP_X / 2),
        y * FINE_STEP_Y + FINE_STEP_Y,
    )
}

/// Обратно: ближайшая клетка мелкой сетки к точке на карте.
///
/// Округвание половинок — к чётному, как `round()` в Python-этлоне.
pub fn character_cell(x: f64, y: f64) -> (i32, i32) {
    let row = ((y - f64::from(FINE_STEP_Y)) / f64::from(FINE_STEP_Y)).round_ties_even() as i32;
    let column = ((x - f64::from((2 - (row & 1)) * (FINE_STEP_X / 2))) / f64::from(FINE_STEP_X))
        .round_ties_even() as i32;
    (column, row)
}

// Клетка вокруг своей точки: ±29 по x, ±8 по y. Из-за сдвига строк
// левый край прямоугольника — у нечётных строк, правый — у чётных
const CELL_HALF_X: i32 = FINE_STEP_X / 2;
const CELL_HALF_Y: i32 = FINE_STEP_Y / 2;
const RECT_RIGHT_EDGE: i32 = FINE_STEP_X + CELL_HALF_X;

/// Прямоугольник клеток в пикселях карты: left, top, right, bottom по краям клеток.
pub fn rect_pixels(rect: &Rect) -> (i32, i32, i32, i32) {
    (
        i32::from(rect.left) * FINE_STEP_X,
        i32::from(rect.top) * FINE_STEP_Y + FINE_STEP_Y - CELL_HALF_Y,
        i32::from(rect.right) * FINE_STEP_X + RECT_RIGHT_EDGE,
        i32::from(rect.bottom) * FINE_STEP_Y + FINE_STEP_Y + CELL_HALF_Y,
    )
}

/// Обратно: прямоугольник в пикселях — клетки, которые он накрывает.
pub fn rect_cells(left: f64, top: f64, right: f64, bottom: f64) -> Rect {
    let round = |value: f64| i16::try_from(value.round_ties_even() as i32).unwrap_or(i16::MIN);
    Rect {
        left: round(left / f64::from(FINE_STEP_X)),
        top: round((top - f64::from(FINE_STEP_Y - CELL_HALF_Y)) / f64::from(FINE_STEP_Y)),
        right: round((right - f64::from(RECT_RIGHT_EDGE)) / f64::from(FINE_STEP_X)),
        bottom: round((bottom - f64::from(FINE_STEP_Y + CELL_HALF_Y)) / f64::from(FINE_STEP_Y)),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn empty_state_bytes() -> Vec<u8> {
        let mut data = vec![0xFF; ITEM_SLOTS * ITEM_SIZE];
        data.resize(GAME_STATE_SIZE, 0);
        data
    }

    #[test]
    fn empty_state_parses_and_round_trips() {
        let data = empty_state_bytes();
        let state = parse(&data).unwrap();
        assert!(state.loot.is_empty());
        assert!(state.exits.is_empty());
        assert!(state.groups[0].is_none());
        assert!(state.items.iter().all(Option::is_none));
        assert!(state.characters.iter().all(Option::is_none));
        assert!(state.villages.iter().all(Option::is_none));
        assert_eq!(serialize(&state).unwrap(), data);
    }

    #[test]
    fn wrong_size_is_an_error() {
        assert!(matches!(
            parse(&[0u8; GAME_STATE_SIZE - 1]),
            Err(StateError::WrongSize { .. })
        ));
    }

    #[test]
    fn serialize_keeps_edits() {
        let mut state = parse(&empty_state_bytes()).unwrap();
        state.loot.push(Loot {
            slot: 0,
            x: 17,
            y: 40,
            gold: 25,
            map_id: 3,
            items: [0; 22],
            pixels: [0; 8],
            image: 0,
            loot_id: 0,
        });
        let zone = Rect {
            left: 4,
            top: 158,
            right: 5,
            bottom: 159,
        };
        state.exits.push(Exit {
            slot: 0,
            map_id: 3,
            target_map: 11,
            zone,
            arrival_x: 8,
            arrival_y: 164,
            direction: 7,
            unknown: [0; 2],
        });
        let edited = parse(&serialize(&state).unwrap()).unwrap();
        let loot = &edited.loot[..];
        let found: Vec<_> = loot
            .iter()
            .map(|loot| (loot.x, loot.y, loot.gold, loot.map_id))
            .collect();
        assert_eq!(found, [(17, 40, 25, 3)]);
        assert_eq!(edited.exits[0].zone, zone);
        assert_eq!(
            (edited.exits[0].arrival_x, edited.exits[0].arrival_y),
            (8, 164)
        );
    }

    #[test]
    fn serialize_rejects_lists_broken_by_zero_map() {
        let mut state = parse(&empty_state_bytes()).unwrap();
        state.loot.push(Loot {
            slot: 0,
            x: 0,
            y: 0,
            gold: 0,
            map_id: 0,
            items: [0; 22],
            pixels: [0; 8],
            image: 0,
            loot_id: 0,
        });
        assert!(matches!(
            serialize(&state),
            Err(StateError::LootZeroMap { slot: 0 })
        ));
        state.loot.clear();
        state.exits.push(Exit {
            slot: 0,
            map_id: 3,
            target_map: 0,
            zone: Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            arrival_x: 0,
            arrival_y: 0,
            direction: 0,
            unknown: [0; 2],
        });
        assert!(matches!(
            serialize(&state),
            Err(StateError::ExitZeroMap { slot: 0 })
        ));
    }

    #[test]
    fn group_rects_split_back_and_forth() {
        // Перемешанный порядок прямоугольников в записи: один прогон
        // serialize→parse обязан вернуть те же spawn/leash
        let mut state = parse(&empty_state_bytes()).unwrap();
        state.groups[5] = Some(Group {
            slot: 5,
            first_character: 12,
            respawn_timer: 300,
            growth_timer: 7,
            spawn: Rect {
                left: 40,
                top: 80,
                right: 44,
                bottom: 84,
            },
            leash: Rect {
                left: 30,
                top: 70,
                right: 54,
                bottom: 94,
            },
            max_size: 8,
            growth_limit: 12,
            target_group: 3,
            map_id: 9,
            size: 4,
            ai_state: 1,
            kind: FIRST_FIXED_GROUP,
            unknown: 0,
        });
        let parsed = parse(&serialize(&state).unwrap()).unwrap();
        assert_eq!(parsed.groups[5], state.groups[5]);
        assert!(parsed.groups[5].as_ref().unwrap().members().eq(12..16));
    }

    #[test]
    fn character_and_rect_positions_round_trip() {
        for (x, y) in [(0, 0), (33, 86), (17, 219), (159, 319)] {
            let (px, py) = character_position(x, y);
            assert_eq!(character_cell(f64::from(px), f64::from(py)), (x, y));
        }
        let rect = Rect {
            left: 4,
            top: 150,
            right: 9,
            bottom: 161,
        };
        let (left, top, right, bottom) = rect_pixels(&rect);
        assert_eq!(
            rect_cells(left as f64, top as f64, right as f64, bottom as f64),
            rect
        );
    }

    #[test]
    fn item_concentration_follows_type() {
        let mut item = parse_item(0, &[0; ITEM_SIZE]);
        item.type_id = QUIVER_ITEM_TYPE;
        item.concentration = 17i32.to_le_bytes();
        assert_eq!(item.concentration_value(), Concentration::Arrows(17));
        item.type_id = 1;
        item.concentration = 1.5f32.to_le_bytes();
        assert_eq!(item.concentration_value(), Concentration::Weight(1.5));
    }

    #[test]
    fn village_round_trips_through_slots() {
        let mut state = parse(&empty_state_bytes()).unwrap();
        state.villages[2] = Some(Village {
            slot: 2,
            building_counts: (3, 4),
            elder_group: 5,
            map_id: 7,
            timers: [1, -2, 3, -4, 5],
            buildings: (0..BUILDING_SLOTS)
                .map(|index| Building {
                    info_id: index as i8,
                    state: -1,
                    object_index: index as i16,
                    unknown: 0,
                    countdown: 3,
                })
                .collect(),
            jobs: JOB_ROLES
                .iter()
                .enumerate()
                .map(|(index, &role)| Job {
                    role,
                    direction: index as u8,
                    unknown: 0,
                    character: 100 + index as i16,
                    x: 10 + index as i16,
                    y: 20 + index as i16,
                })
                .collect(),
            tail: [7; VILLAGE_TAIL_SIZE],
        });
        let parsed = parse(&serialize(&state).unwrap()).unwrap();
        assert_eq!(parsed.villages[2], state.villages[2]);
        let entities = on_map(&parsed, 7, None);
        assert_eq!(entities.jobs.len(), JOBS);
        assert_eq!(entities.jobs[0].1.role, "elder");
        assert_eq!(entities.jobs[4].1.role, "voevoda");
    }

    #[test]
    fn start_map_follows_hero() {
        assert_eq!(start_map(0), 1);
        assert_eq!(start_map(1), 4);
        assert_eq!(start_map(2), 7);
    }

    #[test]
    fn on_map_collects_only_this_map() {
        let mut state = parse(&empty_state_bytes()).unwrap();
        state.groups[1] = Some(Group {
            slot: 1,
            first_character: 0,
            respawn_timer: 0,
            growth_timer: 0,
            spawn: Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            leash: Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            max_size: 1,
            growth_limit: 1,
            target_group: 0,
            map_id: 3,
            size: 1,
            ai_state: 0,
            kind: FIRST_FIXED_GROUP,
            unknown: 0,
        });
        state.characters[0] = Some(parse_character(0, &[1; CHARACTER_SIZE]));
        state.characters[0].as_mut().unwrap().group = 1;
        state.exits.push(Exit {
            slot: 0,
            map_id: 3,
            target_map: 5,
            zone: Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            arrival_x: 1,
            arrival_y: 2,
            direction: 0,
            unknown: [0; 2],
        });
        let entities = on_map(&state, 3, None);
        assert_eq!(entities.groups.len(), 1);
        assert_eq!(entities.characters.len(), 1);
        assert_eq!(entities.exits.len(), 1);
        assert!(entities.arrivals.is_empty());
        assert!(entities.loot.is_empty());
        let arrivals = on_map(&state, 5, None);
        assert_eq!(arrivals.arrivals.len(), 1);
        // персонаж вне членства чужих отрядов не собирается: владельцев — только из groups
        let mut owners = HashMap::new();
        for group in entities.groups {
            for index in group.members() {
                owners.insert(index, group.slot);
            }
        }
        assert_eq!(owners[&0], 1);
    }
}
