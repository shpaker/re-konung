//! Таблицы из исполняемого файла игры (docs/k1/formats/executable.md,
//! эталон — `konung/k1/executable.py`).
//!
//! Снаряжение персонажа рисуется наборами спрайтов GRAPH.RES, а какой набор
//! у какого предмета, игра берёт из таблицы в exe. В RUS это `konung.res`
//! (переименованный установщиком PE), в ENG — `KONUNG.EXE`. Таблицы ищутся
//! по строению, а не по смещению: в изданиях они лежат в разных местах.
//!
//! Перенесены только таблицы, нужные для отрисовки: снаряжение, порядок
//! слотов и анимации огней. Тексты и описания предметов — в Python-эталоде.

use std::path::{Path, PathBuf};

use thiserror::Error;

pub type Result<T> = std::result::Result<T, ExecutableError>;

#[derive(Debug, Error)]
pub enum ExecutableError {
    #[error("таблица {table} не найдена однозначно: {candidates}")]
    Ambiguous {
        table: &'static str,
        candidates: String,
    },
    #[error("{dir}: нет ни одного из {EXECUTABLE_NAMES:?}")]
    NoFile { dir: PathBuf },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub const EXECUTABLE_NAMES: [&str; 2] = ["konung.exe", "konung.res"];

// Описание доспеха или оружия: 44 байта названия, 10 × s16; последнее — набор спрайтов
pub const EQUIPMENT_INFO_SIZE: usize = 44 + 10 * 2;
pub const EQUIPMENT_COUNT: usize = 70;
pub const EQUIPMENT_SETS: usize = 24;
// Порядок слотов снаряжения при отрисовке: 72 строки-перестановки 0–5 по 6 × s32;
// последние 8 — по направлениям для людей без оружия в руках (гипотеза, см. docs)
pub const SLOTS: usize = 6;
pub const DRAW_ORDER_ROW_SIZE: usize = SLOTS * 4;
pub const DRAW_ORDER_ROWS: usize = 72;
pub const DIRECTIONS: usize = 8;

// Анимации огней: 7 пар (первый, последний кадр FLAMES.RES) по 2 × s32,
// подряд от 0 до последнего кадра; номер пары — вид огня у простого объекта
pub const FLAME_KINDS: usize = 7;
pub const FLAME_FRAMES: usize = 74;

/// Палитра каждого набора снаряжения (номер палитры в банке цветов GRAPH.RES).
/// В данных exe её нет — по Slavik resources.cpp:15 (CharEquipPal)
pub const EQUIPMENT_PALETTES: [usize; EQUIPMENT_SETS] = [
    2, 2, 2, 2, 4, 4, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 4, 4, 2, 2, 5, 5, 5, 5,
];

/// Описание доспеха или оружия; номер — Item.info_id предмета.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquipmentInfo {
    pub index: usize,
    /// Название как есть (CP866 в обоих изданиях); в репозиторий не попадает
    pub name: Vec<u8>,
    pub values: [i16; 10],
    /// Набор спрайтов снаряжения GRAPH.RES; −1 — не рисуется
    pub sprite_set: i16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Executable {
    pub equipment: Vec<EquipmentInfo>,
    /// [направление] → слоты Character.equipment в порядке отрисовки
    pub draw_order: [[usize; SLOTS]; DIRECTIONS],
    /// [вид огня] → (первый, последний) кадр FLAMES.RES, оба включены
    pub flame_animations: [(i32, i32); FLAME_KINDS],
}

fn i16_at(data: &[u8], pos: usize) -> i16 {
    i16::from_le_bytes(data[pos..pos + 2].try_into().unwrap())
}

fn i32_at(data: &[u8], pos: usize) -> i32 {
    i32::from_le_bytes(data[pos..pos + 4].try_into().unwrap())
}

fn ambiguous(table: &'static str, found: &[usize]) -> ExecutableError {
    ExecutableError::Ambiguous {
        table,
        candidates: format!(
            "[{}]",
            found
                .iter()
                .map(|start| format!("{start:#x}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Текст записи «текст + нули»: None — запись не такая (пустая тоже None).
fn text(field: &[u8]) -> Option<&[u8]> {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    let (text, rest) = field.split_at(end);
    if text.is_empty() || text[0] < 0x20 || rest.iter().any(|&b| b != 0) {
        return None;
    }
    Some(text)
}

fn is_equipment_table(data: &[u8], start: usize) -> bool {
    (0..EQUIPMENT_COUNT).all(|index| {
        let record = start + index * EQUIPMENT_INFO_SIZE;
        let Some(name) = data.get(record..record + 44) else {
            return false;
        };
        if text(name).is_none() {
            return false;
        }
        let sprite_set = i16_at(data, record + 44 + 9 * 2);
        sprite_set >= -1 && (sprite_set as i32) < EQUIPMENT_SETS as i32
    })
}

fn find_equipment(data: &[u8]) -> Result<usize> {
    let size = EQUIPMENT_COUNT * EQUIPMENT_INFO_SIZE;
    let end = data.len().saturating_sub(size);
    let found = (0..end)
        .step_by(4)
        .filter(|&start| is_equipment_table(data, start))
        .collect::<Vec<_>>();
    if found.len() != 1 {
        return Err(ambiguous("снаряжения", &found));
    }
    Ok(found[0])
}

/// Начало блока из 72 подряд идущих строк-перестановок 0–5.
fn find_draw_order(data: &[u8]) -> Result<usize> {
    let mut found = Vec::new();
    // Строки идут с шагом 24 байта; каждое выравнивание по 4 проверяется отдельно
    for alignment in (0..DRAW_ORDER_ROW_SIZE).step_by(4) {
        let (mut run_start, mut run) = (0usize, 0usize);
        let mut start = alignment;
        while start + DRAW_ORDER_ROW_SIZE < data.len() {
            let row = (0..SLOTS).map(|slot| i32_at(data, start + slot * 4));
            let mut slots: Vec<i32> = row.collect();
            slots.sort_unstable();
            slots.dedup();
            if slots == (0..SLOTS as i32).collect::<Vec<_>>() {
                if run == 0 {
                    run_start = start;
                }
                run += 1;
                start += DRAW_ORDER_ROW_SIZE;
                continue;
            }
            if run == DRAW_ORDER_ROWS {
                found.push(run_start);
            }
            run = 0;
            start += DRAW_ORDER_ROW_SIZE;
        }
    }
    if found.len() != 1 {
        return Err(ambiguous("порядка отрисовки", &found));
    }
    Ok(found[0])
}

fn flame_animations_at(data: &[u8], start: usize) -> Option<[(i32, i32); FLAME_KINDS]> {
    let mut pairs = [(0i32, 0i32); FLAME_KINDS];
    for (kind, pair) in pairs.iter_mut().enumerate() {
        *pair = (
            i32_at(data, start + kind * 8),
            i32_at(data, start + kind * 8 + 4),
        );
    }
    if pairs[0].0 != 0 || pairs[FLAME_KINDS - 1].1 != FLAME_FRAMES as i32 - 1 {
        return None;
    }
    if pairs.iter().any(|&(first, last)| first > last) {
        return None;
    }
    if pairs.windows(2).any(|pair| pair[1].0 != pair[0].1 + 1) {
        return None;
    }
    Some(pairs)
}

/// 7 пар подряд, без пропусков, от кадра 0 до последнего кадра FLAMES.RES.
fn find_flame_animations(data: &[u8]) -> Result<[(i32, i32); FLAME_KINDS]> {
    let end = data.len().saturating_sub(FLAME_KINDS * 8);
    let found = (0..end)
        .step_by(4)
        .filter(|&start| data[start..start + 4] == [0, 0, 0, 0])
        .filter_map(|start| flame_animations_at(data, start).map(|pairs| (start, pairs)))
        .collect::<Vec<_>>();
    if found.len() != 1 {
        return Err(ambiguous(
            "огней",
            &found.iter().map(|&(start, _)| start).collect::<Vec<_>>(),
        ));
    }
    Ok(found[0].1)
}

pub fn parse(data: &[u8]) -> Result<Executable> {
    let start = find_equipment(data)?;
    let equipment = (0..EQUIPMENT_COUNT)
        .map(|index| {
            let record = start + index * EQUIPMENT_INFO_SIZE;
            let name = &data[record..record + 44];
            let text_end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
            let mut values = [0i16; 10];
            for (i, value) in values.iter_mut().enumerate() {
                *value = i16_at(data, record + 44 + i * 2);
            }
            EquipmentInfo {
                index,
                name: name[..text_end].to_vec(),
                values,
                sprite_set: values[9],
            }
        })
        .collect();
    let order_start = find_draw_order(data)?;
    let first = order_start + (DRAW_ORDER_ROWS - DIRECTIONS) * DRAW_ORDER_ROW_SIZE;
    let mut draw_order = [[0usize; SLOTS]; DIRECTIONS];
    for (direction, row) in draw_order.iter_mut().enumerate() {
        for (slot, value) in row.iter_mut().enumerate() {
            *value = i32_at(data, first + direction * DRAW_ORDER_ROW_SIZE + slot * 4) as usize;
        }
    }
    Ok(Executable {
        equipment,
        draw_order,
        flame_animations: find_flame_animations(data)?,
    })
}

/// Исполняемый файл игры в каталоге: `KONUNG.EXE` (ENG) или `konung.res` (RUS).
pub fn find(game_dir: &Path) -> Result<PathBuf> {
    for entry in std::fs::read_dir(game_dir)? {
        let path = entry?.path();
        let matches = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| EXECUTABLE_NAMES.contains(&name.to_lowercase().as_str()));
        if matches {
            return Ok(path);
        }
    }
    Err(ExecutableError::NoFile {
        dir: game_dir.to_path_buf(),
    })
}
