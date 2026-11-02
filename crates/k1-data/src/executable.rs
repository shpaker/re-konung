//! Таблицы из исполняемого файла игры (docs/k1/formats/executable.md,
//! эталон — `konung/k1/executable.py`).
//!
//! Снаряжение персонажа рисуется наборами спрайтов GRAPH.RES, а какой набор
//! у какого предмета, игра берёт из таблицы в exe. В RUS это `konung.res`
//! (переименованный установщиком PE), в ENG — `KONUNG.EXE`. Таблицы ищутся
//! по строению, а не по смещению: в изданиях они лежат в разных местах.
//!
//! Перенесены таблицы для отрисовки (снаряжение, порядок слотов, анимации
//! огней) и тексты — названия монстров, имена, прозвища и названия карт.
//! Описания предметов (украшения, зелья, прочие) пока только в Python-эталоде.

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

// Тексты — записи фиксированной длины: текст, дальше нули. RUS — CP866, ENG —
// латиница, кроме одного байта CP866 в одном имени; CP866 читает оба.
// Монстры, имена людей и прозвища — сплошной блок из 30 + 171 + 21 записи по
// 32 байта: имя 0 и прозвище 0 пустые, у монстров заполнены первые 20, хвост пуст
pub const NAME_SIZE: usize = 32;
pub const MONSTER_NAMES: usize = 30;
pub const PEOPLE_NAMES: usize = 171;
pub const NICKNAMES: usize = 21;
pub const NAMES_BLOCK: usize = MONSTER_NAMES + PEOPLE_NAMES + NICKNAMES;
// Названия карт: 56 записей по 40 байт, номер записи — номер карты минус 1
pub const MAP_NAME_SIZE: usize = 40;
pub const MAP_NAMES: usize = 56;
// Классы 0x41–0x50 (бит 0x40, виды от 0) называются по виду из таблицы монстров;
// с 0x51 — женщины и дети деревень, они по имени, как люди
pub const MONSTER_CLASS: u8 = 0x40;
pub const FIRST_VILLAGER_CLASS: u8 = 0x51;

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

/// Названия из exe — тексты игры. В репозиторий и тесты они не попадают:
/// поля читаются только локально, а проверки идут по структуре и числам.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Texts {
    /// Монстры по виду `(класс & 0x7F) − 0x41`; хвост таблицы пуст
    pub monsters: Vec<String>,
    /// Имена людей по номеру из байта 0xD4 записи персонажа; запись 0 пуста
    pub names: Vec<String>,
    /// Прозвища по номеру из байта 0xD5; запись 0 пуста, 0 — без прозвища
    pub nicknames: Vec<String>,
    /// Названия карт: [номер карты − 1]
    pub maps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Executable {
    pub equipment: Vec<EquipmentInfo>,
    /// [направление] → слоты Character.equipment в порядке отрисовки
    pub draw_order: [[usize; SLOTS]; DIRECTIONS],
    /// [вид огня] → (первый, последний) кадр FLAMES.RES, оба включены
    pub flame_animations: [(i32, i32); FLAME_KINDS],
    /// Таблицы текстов; если блок не нашёлся однозначно (другое издание) — None,
    /// и без имён карта всё равно рисуется
    pub texts: Option<Texts>,
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
///
/// Управление как в эталоне и в расширении Tiled: годится только непустой текст,
/// где ни один байт не управляющий (все ≥ 0x20), а после текста идут лишь нули.
fn text(field: &[u8]) -> Option<&[u8]> {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    let (text, rest) = field.split_at(end);
    if text.is_empty() || text.iter().any(|&b| b < 0x20) || rest.iter().any(|&b| b != 0) {
        return None;
    }
    Some(text)
}

/// Начало текста записи снаряжения, или None, если записи не подходят.
///
/// Проверка слабее, чем у [text]: и эталон (`_is_equipment_table` в
/// `konung/k1/executable.py`), и расширение Tiled (`isEquipmentTable` в
/// `tiled/konung/lib/k1.mjs`) смотрят только первый байт имени, управляющие
/// байты в середине не отбрасывают таблицу. Строгую [text] здесь применять
/// нельзя: на издании с таким именем Rust отвергнул бы таблицу, которую
/// эталон принимает, а без таблицы снаряжения не разобрались бы вовсе.
///
/// Имя без терминатора (все 44 байта заняты) эталон принимает — здесь тоже;
/// расширение Tiled в этом случае запись отбрасывает, расхождение описано в
/// docs/k1/formats/executable.md.
fn equipment_text_start(field: &[u8]) -> Option<usize> {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    if field.is_empty() || field[0] < 0x20 {
        return None;
    }
    if field[end..].iter().any(|&b| b != 0) {
        return None;
    }
    Some(end)
}

/// CP866 для байтов 0x80–0xFF: 0x80–0x9F — прописные А–Я, 0xA0–0xBF — строчные
/// а–я, 0xC0–0xDF — псевдографика, 0xE0–0xEF — строчные р–я, 0xF0–0xF7 — Ё ё Є є
/// І і Ў ў, 0xF8–0xFF — градус, средняя точка, корень, номерной знак и др.
/// Байты 0x00–0x7F совпадают с ASCII, поэтому отдельной таблицы нет.
const CP866_HIGH: [char; 128] = [
    '\u{410}', '\u{411}', '\u{412}', '\u{413}', '\u{414}', '\u{415}', '\u{416}', '\u{417}',
    '\u{418}', '\u{419}', '\u{41A}', '\u{41B}', '\u{41C}', '\u{41D}', '\u{41E}', '\u{41F}',
    '\u{420}', '\u{421}', '\u{422}', '\u{423}', '\u{424}', '\u{425}', '\u{426}', '\u{427}',
    '\u{428}', '\u{429}', '\u{42A}', '\u{42B}', '\u{42C}', '\u{42D}', '\u{42E}', '\u{42F}',
    '\u{430}', '\u{431}', '\u{432}', '\u{433}', '\u{434}', '\u{435}', '\u{436}', '\u{437}',
    '\u{438}', '\u{439}', '\u{43A}', '\u{43B}', '\u{43C}', '\u{43D}', '\u{43E}', '\u{43F}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}',
    '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255D}', '\u{255C}', '\u{255B}', '\u{2510}',
    '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{255E}', '\u{255F}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{2567}',
    '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}', '\u{2558}', '\u{2552}', '\u{2553}', '\u{256B}',
    '\u{256A}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{258C}', '\u{2590}', '\u{2580}',
    '\u{440}', '\u{441}', '\u{442}', '\u{443}', '\u{444}', '\u{445}', '\u{446}', '\u{447}',
    '\u{448}', '\u{449}', '\u{44A}', '\u{44B}', '\u{44C}', '\u{44D}', '\u{44E}', '\u{44F}',
    '\u{401}', '\u{451}', '\u{404}', '\u{454}', '\u{407}', '\u{457}', '\u{40E}', '\u{45E}',
    '\u{B0}', '\u{2219}', '\u{B7}', '\u{221A}', '\u{2116}', '\u{A4}', '\u{25A0}', '\u{A0}',
];

/// Текст из байтов CP866 — так же, как `bytes.decode("cp866")` в эталоне:
/// 0x00–0x7F как ASCII, старшие байты по таблице выше.
fn decode_cp866(text: &[u8]) -> String {
    text.iter()
        .map(|&byte| {
            if byte < 0x80 {
                char::from(byte)
            } else {
                CP866_HIGH[usize::from(byte) - 0x80]
            }
        })
        .collect()
}

/// Запись номер `index` размером `size` байт, идущая с `start`.
fn record_at(data: &[u8], start: usize, index: usize, size: usize) -> &[u8] {
    let at = start + index * size;
    data.get(at..at + size).unwrap_or(&[])
}

/// Текст записи как строка; пустая или неподходящая запись — пустая строка.
fn decoded_field(field: &[u8]) -> String {
    text(field).map_or_else(String::new, decode_cp866)
}

fn is_equipment_table(data: &[u8], start: usize) -> bool {
    (0..EQUIPMENT_COUNT).all(|index| {
        let record = start + index * EQUIPMENT_INFO_SIZE;
        let Some(name) = data.get(record..record + 44) else {
            return false;
        };
        if equipment_text_start(name).is_none() {
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

/// Блок из 30 названий монстров, 171 имени и 21 прозвища подряд по 32 байта.
///
/// Запись 30 (первое имя) и запись 201 (первое прозвище) обязательны как пустые:
/// так отсекается почти всё сразу. У монстров заполнены первые 20, дальше идут
/// пустые записи, а после первой пустой другой текст уже не допускается.
fn is_names_block(data: &[u8], start: usize) -> bool {
    let empty = [0u8; NAME_SIZE];
    let empty = empty.as_slice();
    let monsters_end = MONSTER_NAMES;
    let nicknames_start = MONSTER_NAMES + PEOPLE_NAMES;
    for index in [monsters_end, nicknames_start] {
        if record_at(data, start, index, NAME_SIZE) != empty {
            return false;
        }
    }
    let mut seen_empty = false;
    for index in 0..NAMES_BLOCK {
        if index == monsters_end || index == nicknames_start {
            continue;
        }
        let field = record_at(data, start, index, NAME_SIZE);
        if index < monsters_end {
            if field == empty {
                if index > 0 {
                    seen_empty = true;
                }
                continue;
            }
            if seen_empty {
                return false;
            }
        }
        if text(field).is_none() {
            return false;
        }
    }
    true
}

/// Начало блока имён и прозвищ; ищется по строению, а не по смещению.
fn find_names(data: &[u8]) -> Result<usize> {
    let end = data.len().saturating_sub(NAMES_BLOCK * NAME_SIZE);
    let found = (0..end)
        .step_by(4)
        .filter(|&start| text(record_at(data, start, 0, NAME_SIZE)).is_some())
        .filter(|&start| is_names_block(data, start))
        .collect::<Vec<_>>();
    if found.len() != 1 {
        return Err(ambiguous("имён", &found));
    }
    Ok(found[0])
}

/// Серии подряд идущих записей размером `size` байт, подходящих под `is_record`:
/// (начало серии, число записей). Шаг записей не кратен 4, поэтому каждое
/// смещение в пределах четырёх байт проверяется отдельно.
fn text_runs(data: &[u8], size: usize, is_record: impl Fn(&[u8]) -> bool) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    for alignment in (0..size).step_by(4) {
        let (mut run_start, mut run) = (0usize, 0usize);
        for start in (alignment..data.len().saturating_sub(size - 1)).step_by(size) {
            if is_record(&data[start..start + size]) {
                if run == 0 {
                    run_start = start;
                }
                run += 1;
                continue;
            }
            if run > 0 {
                runs.push((run_start, run));
            }
            run = 0;
        }
        if run > 0 {
            runs.push((run_start, run));
        }
    }
    runs
}

/// Начало блока названий карт — единственная серия из 56 записей по 40 байт.
fn find_map_names(data: &[u8]) -> Result<usize> {
    let found = text_runs(data, MAP_NAME_SIZE, |field| text(field).is_some())
        .into_iter()
        .filter(|&(_, count)| count == MAP_NAMES)
        .map(|(start, _)| start)
        .collect::<Vec<_>>();
    if found.len() != 1 {
        return Err(ambiguous("названий карт", &found));
    }
    Ok(found[0])
}

/// Тексты игры из exe: названия монстров, имена, прозвища и названия карт.
/// Публично, чтобы неоднозначный поиск давал ошибку с кандидатами, а не молчал.
pub fn parse_texts(data: &[u8]) -> Result<Texts> {
    let start = find_names(data)?;
    let at = |index: usize| decoded_field(record_at(data, start, index, NAME_SIZE));
    let maps_start = find_map_names(data)?;
    Ok(Texts {
        monsters: (0..MONSTER_NAMES).map(at).collect(),
        names: (MONSTER_NAMES..MONSTER_NAMES + PEOPLE_NAMES)
            .map(at)
            .collect(),
        nicknames: (MONSTER_NAMES + PEOPLE_NAMES..NAMES_BLOCK)
            .map(at)
            .collect(),
        maps: (0..MAP_NAMES)
            .map(|index| decoded_field(record_at(data, maps_start, index, MAP_NAME_SIZE)))
            .collect(),
    })
}

/// Строка таблицы по номеру; номер вне таблицы — пустая строка, как в эталоне.
fn entry(table: &[String], index: i32) -> &str {
    usize::try_from(index)
        .ok()
        .and_then(|index| table.get(index))
        .map_or("", String::as_str)
}

/// Имя персонажа, как его собирает игра: у монстров — название вида,
/// у людей, женщин и детей — имя и прозвище через пробел (прозвище 0 — без него).
///
/// Без текстов или с номером вне таблицы — пустая строка. Номера имя и прозвище —
/// байты 0xD4 и 0xD5 записи персонажа (docs/k1/formats/game-state.md).
pub fn character_name(texts: Option<&Texts>, class_id: u8, name_id: u8, nick_id: u8) -> String {
    let Some(texts) = texts else {
        return String::new();
    };
    let kind = i32::from(class_id & 0x7F);
    if class_id & MONSTER_CLASS != 0 && kind < i32::from(FIRST_VILLAGER_CLASS) {
        return entry(&texts.monsters, kind - i32::from(MONSTER_CLASS) - 1).to_string();
    }
    let name = entry(&texts.names, i32::from(name_id));
    if nick_id == 0 {
        return name.to_string();
    }
    let nick = entry(&texts.nicknames, i32::from(nick_id));
    format!("{name} {nick}")
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
        // Как в эталоне: без однозначного блока рисовать можно, поэтому тексты
        // не заводят разбор — None. Эталон вдобавок теряет их, если не нашлись
        // таблицы предметов; в Rust предметов пока нет, и тексты от них не зависят.
        texts: parse_texts(data).ok(),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Запись «текст + нули» фиксированной длины. Тексты синтетические:
    /// строки игры в репозиторий не попадают.
    fn field(text: &str, size: usize) -> Vec<u8> {
        let mut out = vec![0u8; size];
        out[..text.len()].copy_from_slice(text.as_bytes());
        out
    }

    /// Блок имён, какой ищется по строению: заполненные названия монстров и
    /// пустой хвост, пустое имя 0 и пустое прозвище 0, остальное заполнено.
    fn names_block() -> Vec<u8> {
        // Столько заполненных названий монстров, сколько в данных игры
        const FILLED_MONSTERS: usize = 20;
        let mut data = Vec::new();
        for index in 0..NAMES_BLOCK {
            let (prefix, number) = if index < MONSTER_NAMES {
                (String::from("mon"), index)
            } else if index < MONSTER_NAMES + PEOPLE_NAMES {
                (String::from("name"), index - MONSTER_NAMES)
            } else {
                (String::from("nick"), index - MONSTER_NAMES - PEOPLE_NAMES)
            };
            // Хвост названий монстров пуст, как в данных игры, и пустые записи 0
            let empty = (FILLED_MONSTERS..MONSTER_NAMES).contains(&index)
                || number == 0 && index >= MONSTER_NAMES;
            let text = if empty {
                String::new()
            } else {
                format!("{prefix}{number:03}")
            };
            data.extend(field(&text, NAME_SIZE));
        }
        data
    }

    /// Серия названий карт: `count` заполненных записей по 40 байт.
    fn map_names(count: usize) -> Vec<u8> {
        (0..count)
            .flat_map(|index| field(&format!("map{index:02}"), MAP_NAME_SIZE))
            .collect()
    }

    /// exe-подобные байты: блок имён, нули, названия карт, нули.
    fn texts_data(count: usize) -> Vec<u8> {
        let mut data = names_block();
        data.extend(vec![0u8; MAP_NAME_SIZE]);
        data.extend(map_names(count));
        data.extend(vec![0u8; MAP_NAME_SIZE]);
        data
    }

    fn all_texts() -> Texts {
        parse_texts(&texts_data(MAP_NAMES)).unwrap()
    }

    #[test]
    fn a_control_byte_anywhere_makes_the_record_not_text() {
        // Как в эталоне и в расширении Tiled: не только первый байт
        assert_eq!(text(b"ab"), Some(&b"ab"[..]));
        assert_eq!(text(b"ab\0\0"), Some(&b"ab"[..]));
        assert_eq!(text(b""), None);
        assert_eq!(text(b"\0ab"), None);
        assert_eq!(text(b"ab\tcd"), None);
        assert_eq!(text(b"ab\0cd"), None);
    }

    #[test]
    fn equipment_names_are_checked_looser_than_texts() {
        // Управление эталонного _is_equipment_table: только первый байт,
        // поэтому имя с управляющим байтом в середине таблицу не отбрасывает
        assert_eq!(equipment_text_start(b"ab"), Some(2));
        assert_eq!(equipment_text_start(b"ab\0\0"), Some(2));
        assert_eq!(equipment_text_start(b"ab\tcd\0"), Some(5));
        assert_eq!(equipment_text_start(b"ab\0cd"), None);
        assert_eq!(equipment_text_start(b""), None);
        assert_eq!(equipment_text_start(b"\0ab"), None);
        assert_eq!(equipment_text_start(b"\tab\0"), None);
        // Строгая проверка текстов такую запись отвергает — это разные предикаты
        assert_eq!(text(b"ab\tcd\0"), None);
    }

    #[test]
    fn cp866_matches_the_reference_encoding() {
        // 0x80–0x9F — А–Я, 0xA0–0xBF — а–я, 0xE0–0xEF — р–я, 0xF8–0xFF — знак градуса и др.
        assert_eq!(decode_cp866(b"ab"), "ab");
        assert_eq!(decode_cp866(&[0x80, 0x81, 0x9F]), "\u{410}\u{411}\u{42F}");
        assert_eq!(decode_cp866(&[0xA0, 0xE0]), "\u{430}\u{440}");
        assert_eq!(decode_cp866(&[0xF8, 0xFB, 0xFF]), "\u{B0}\u{221A}\u{A0}");
        assert_eq!(decode_cp866(b""), "");
    }

    #[test]
    fn text_tables_are_split_by_their_shapes() {
        let texts = all_texts();
        assert_eq!(texts.monsters.len(), MONSTER_NAMES);
        assert_eq!(texts.names.len(), PEOPLE_NAMES);
        assert_eq!(texts.nicknames.len(), NICKNAMES);
        assert_eq!(texts.maps.len(), MAP_NAMES);
        assert_eq!(texts.monsters[0], "mon000");
        assert_eq!(texts.monsters[19], "mon019");
        assert!(texts.monsters[20..].iter().all(String::is_empty));
        assert_eq!(texts.names[0], "");
        assert_eq!(texts.names[1], "name001");
        assert_eq!(texts.nicknames[0], "");
        assert_eq!(texts.nicknames[NICKNAMES - 1], "nick020");
        assert!(texts.maps.iter().all(|map| !map.is_empty()));
    }

    #[test]
    fn character_name_follows_the_class() {
        let texts = Some(all_texts());
        assert_eq!(character_name(texts.as_ref(), 0x41, 0, 0), "mon000");
        assert_eq!(character_name(texts.as_ref(), 0x4D, 7, 9), "mon012");
        assert_eq!(character_name(texts.as_ref(), 0x51, 2, 0), "name002");
        assert_eq!(
            character_name(texts.as_ref(), 0x51, 2, 3),
            "name002 nick003"
        );
        // Класс 0x40 — вид до первой записи таблицы, как в эталоне: пустая строка
        assert_eq!(character_name(texts.as_ref(), 0x40, 2, 3), "");
        assert_eq!(character_name(texts.as_ref(), 0x02, 255, 0), "");
        assert_eq!(character_name(texts.as_ref(), 0x02, 5, 255), "name005 ");
        assert_eq!(character_name(None, 0x41, 2, 3), "");
    }

    #[test]
    fn ambiguous_names_block_lists_candidates() {
        let second = texts_data(MAP_NAMES).len();
        let mut data = texts_data(MAP_NAMES);
        data.extend(names_block());
        data.extend(vec![0u8; MAP_NAME_SIZE]);
        data.extend(map_names(MAP_NAMES));
        data.extend(vec![0u8; MAP_NAME_SIZE]);
        let error = parse_texts(&data).unwrap_err();
        let ExecutableError::Ambiguous { table, candidates } = error else {
            panic!("ожидалась ошибка неоднозначного поиска, получилась {error}");
        };
        assert_eq!(table, "имён");
        // Оба кандидата называются смещениями, чтобы ошибку можно было разобрать
        assert_eq!(candidates, format!("[0x0, {second:#x}]"));
    }

    #[test]
    fn map_names_need_the_whole_run() {
        // Серия короче или длиннее ожидаемой — это уже не таблица названий карт
        let error = parse_texts(&texts_data(MAP_NAMES - 1))
            .unwrap_err()
            .to_string();
        assert!(error.contains("названий карт"), "{error}");
        let error = parse_texts(&texts_data(MAP_NAMES + 1))
            .unwrap_err()
            .to_string();
        assert!(error.contains("названий карт"), "{error}");
    }

    #[test]
    fn find_reports_a_missing_executable() {
        let dir = tempfile::tempdir().unwrap();
        let error = find(dir.path()).unwrap_err();
        assert!(matches!(error, ExecutableError::NoFile { .. }), "{error}");
        let path = dir.path().join("KONUNG.EXE");
        std::fs::write(&path, b"PE").unwrap();
        assert_eq!(find(dir.path()).unwrap(), path);
    }
}
