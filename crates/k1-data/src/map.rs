//! `Konung.N` — карта локации N; `LIGHTS.N` — её освещение
//! (docs/k1/formats/map.md, эталон — `konung/k1/map_file.py`).

use thiserror::Error;

use crate::reader::Reader;

pub const WIDTH: usize = 80;
pub const HEIGHT: usize = 160;
/// Сетка проходимости и света вдвое мельче тайловой
pub const FINE_WIDTH: usize = 160;
pub const FINE_HEIGHT: usize = 320;
pub const MAX_DECORATIONS: usize = 1000;
pub const MAX_OBJECTS: usize = 1000;
pub const DECORATION_SIZE: usize = 16;
pub const OBJECT_SIZE: usize = 36;
pub const FLAME_SLOTS: usize = 20;

pub const MAP_SIZE: usize = WIDTH * HEIGHT * 2
    + FINE_WIDTH * FINE_HEIGHT
    + MAX_DECORATIONS * DECORATION_SIZE
    + MAX_OBJECTS * OBJECT_SIZE;
pub const LIGHTS_SIZE: usize = WIDTH * HEIGHT + FINE_WIDTH * FINE_HEIGHT;

/// Геометрия сетки: ромб 114×64, шаг по x — 116, шаг строки — 32,
/// нечётные строки сдвинуты вправо на половину шага
pub const TILE_STEP_X: i32 = 116;
pub const TILE_STEP_Y: i32 = 32;
pub const ODD_ROW_SHIFT: i32 = TILE_STEP_X / 2;

/// Байт проходимости: младшие 4 бита 0xF — непроходимо, старшие — флаги.
/// В картах дистрибутива только 0x00, 0x0F, 0x40, 0x4F;
/// флаг 0x40 — здесь застревают стрелы (гипотеза)
pub const FOOT_BLOCKED: u8 = 0x0F;
pub const FOOT_STOPS_ARROWS: u8 = 0x40;

/// Число слотов тайлов GRAPH.RES: декорация с тайлом за их пределами — сырая
/// ([Decoration::is_raw]). Временно здесь: переедет в модуль GRAPH.RES при его
/// переносе (эталон — `konung/k1/graph_res.py`).
pub const TILE_SLOTS: i32 = 512;

pub type Result<T> = std::result::Result<T, MapError>;

#[derive(Debug, Error)]
pub enum MapError {
    #[error("карта {actual} байт, ожидалось {MAP_SIZE}")]
    WrongMapSize { actual: usize },
    #[error("свет {actual} байт, ожидалось {LIGHTS_SIZE}")]
    WrongLightsSize { actual: usize },
    #[error("слой {layer}: {detail}")]
    Geometry { layer: &'static str, detail: String },
    #[error("декораций {count}, максимум {MAX_DECORATIONS}")]
    TooManyDecorations { count: usize },
    #[error("объектов {count}, максимум {MAX_OBJECTS}")]
    TooManyObjects { count: usize },
    #[error("сырая декорация: слот {slot} занят или вне 0–{max}")]
    RawSlotConflict { slot: usize, max: usize },
    #[error("получилось {actual} байт вместо {expected}: неверный размер слоёв")]
    WrongSerializedSize { actual: usize, expected: usize },
    #[error(transparent)]
    Read(#[from] crate::reader::ReadError),
}

/// Тайл из GRAPH.RES, поставленный в произвольную точку карты (в пикселях).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoration {
    pub slot: usize,
    pub tile: i32,
    pub x: i16,
    pub y: i16,
    pub unknown: [u8; 8],
}

impl Decoration {
    /// Запись с тайлом за пределами GRAPH.RES: её не нарисовать, она хранится
    /// как есть и пишется обратно в свой слот, а не подряд с обычными
    /// (docs/k1/formats/map.md, «Запись»).
    pub fn is_raw(&self) -> bool {
        self.tile >= TILE_SLOTS
    }
}

/// Объект из OBJECTS.RES (деревья, дома, анимации) в пикселях карты.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapObject {
    pub slot: usize,
    pub object_id: i32,
    pub x: i16,
    pub y: i16,
    pub flames: [u8; FLAME_SLOTS],
    pub frame_time: i16,
    pub flags: u8,
    pub frame: u8,
    pub unknown: [u8; 4],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KonungMap {
    /// Номера тайлов GRAPH.RES с единицы, 0 — пусто; [строка][столбец]
    pub ground: Vec<Vec<u8>>,
    pub overlay: Vec<Vec<u8>>,
    /// Байт проходимости на клетку мелкой сетки [строка][столбец]
    pub foot: Vec<Vec<u8>>,
    pub decorations: Vec<Decoration>,
    pub objects: Vec<MapObject>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lights {
    /// Номер маски LIGHTS.RES + 1 на клетку тайловой сетки, 0 — света нет
    pub tiles: Vec<Vec<u8>>,
    /// Тот же номер на 4 клетки мелкой сетки в ромбе тайла ([fine_cells]), у края — 0
    pub fine: Vec<Vec<u8>>,
}

/// Левый верхний угол тайла клетки на картинке карты.
pub fn tile_position(column: usize, row: usize) -> (i32, i32) {
    let (column, row) = (column as i32, row as i32);
    (
        column * TILE_STEP_X + (row & 1) * ODD_ROW_SHIFT,
        row * TILE_STEP_Y,
    )
}

pub fn is_blocked(foot: u8) -> bool {
    foot & FOOT_BLOCKED == FOOT_BLOCKED
}

/// Четыре клетки мелкой сетки в ромбе тайла: верхняя, левая, правая, нижняя.
///
/// Ромб клетки мелкой сетки — 58 × 32, вчетверо меньше ромба тайла, и каждая
/// клетка лежит ровно в одном тайле. Клетки за краем мелкой сетки тоже
/// возвращаются — их отбрасывает вызывающий.
pub fn fine_cells(column: usize, row: usize) -> [(usize, usize); 4] {
    let (x, y) = (2 * column + (row & 1), 2 * row);
    [(x, y), (x, y + 1), (x + 1, y + 1), (x, y + 2)]
}

pub fn parse(data: &[u8]) -> Result<KonungMap> {
    if data.len() != MAP_SIZE {
        return Err(MapError::WrongMapSize { actual: data.len() });
    }
    let mut reader = Reader::new(data);

    let mut ground = Vec::with_capacity(HEIGHT);
    let mut overlay = Vec::with_capacity(HEIGHT);
    for _ in 0..HEIGHT {
        let row = reader.bytes(WIDTH * 2)?;
        ground.push(row.iter().step_by(2).copied().collect());
        overlay.push(row.iter().skip(1).step_by(2).copied().collect());
    }

    let mut foot = Vec::with_capacity(FINE_HEIGHT);
    for _ in 0..FINE_HEIGHT {
        foot.push(reader.bytes(FINE_WIDTH)?.to_vec());
    }

    let mut decorations = Vec::new();
    for slot in 0..MAX_DECORATIONS {
        let tile = reader.s32()?;
        let unknown: [u8; 8] = reader.bytes(8)?.try_into().unwrap();
        let x = reader.s16()?;
        let y = reader.s16()?;
        if tile >= 0 {
            decorations.push(Decoration {
                slot,
                tile,
                x,
                y,
                unknown,
            });
        }
    }

    let mut objects = Vec::new();
    for slot in 0..MAX_OBJECTS {
        let start = reader.pos();
        let object_id = reader.s32()?;
        if object_id < 0 {
            reader.seek(start + OBJECT_SIZE);
            continue;
        }
        let unknown: [u8; 4] = reader.bytes(4)?.try_into().unwrap();
        let flames: [u8; FLAME_SLOTS] = reader.bytes(FLAME_SLOTS)?.try_into().unwrap();
        let frame_time = reader.s16()?;
        let x = reader.s16()?;
        let y = reader.s16()?;
        let flags = reader.u8()?;
        let frame = reader.u8()?;
        objects.push(MapObject {
            slot,
            object_id,
            x,
            y,
            flames,
            frame_time,
            flags,
            frame,
            unknown,
        });
    }

    Ok(KonungMap {
        ground,
        overlay,
        foot,
        decorations,
        objects,
    })
}

/// Декорации по слотам файла: сырые ([Decoration::is_raw]) — в свой `slot`,
/// обычные — по порядку списка в свободные слоты с нулевого; `None` — пустой слот.
pub fn decoration_slots(decorations: &[Decoration]) -> Result<Vec<Option<Decoration>>> {
    if decorations.len() > MAX_DECORATIONS {
        return Err(MapError::TooManyDecorations {
            count: decorations.len(),
        });
    }
    let mut slots: Vec<Option<Decoration>> = vec![None; MAX_DECORATIONS];
    for decoration in decorations {
        if !decoration.is_raw() {
            continue;
        }
        if decoration.slot >= MAX_DECORATIONS || slots[decoration.slot].is_some() {
            return Err(MapError::RawSlotConflict {
                slot: decoration.slot,
                max: MAX_DECORATIONS - 1,
            });
        }
        slots[decoration.slot] = Some(decoration.clone());
    }
    // Свободных слотов хватает: обычных декораций не больше
    // MAX_DECORATIONS минус занятые сырыми
    let free: Vec<usize> = (0..MAX_DECORATIONS)
        .filter(|&slot| slots[slot].is_none())
        .collect();
    for (slot, decoration) in free
        .into_iter()
        .zip(decorations.iter().filter(|d| !d.is_raw()))
    {
        slots[slot] = Some(decoration.clone());
    }
    Ok(slots)
}

/// Точная геометрия сетки перед записью: `rows` строк по `width` клеток.
/// Размер байтов в файле сам по себе её не гарантирует: строки 81 и 79
/// дают верный суммарный размер, но границы строк были бы смещены.
fn check_grid(layer: &'static str, grid: &[Vec<u8>], rows: usize, width: usize) -> Result<()> {
    if grid.len() != rows {
        return Err(MapError::Geometry {
            layer,
            detail: format!("ожидалось {rows} строк, а {}", grid.len()),
        });
    }
    for (row, cells) in grid.iter().enumerate() {
        if cells.len() != width {
            return Err(MapError::Geometry {
                layer,
                detail: format!("в строке {row} ожидалось {width} клеток, а {}", cells.len()),
            });
        }
    }
    Ok(())
}

/// Карта обратно в формат `Konung.N`.
///
/// Объекты и обычные декорации пишутся подряд с нулевого слота в порядке
/// списков, сырые декорации — в свой слот ([decoration_slots]), остальные
/// слоты — байты 0xFF: так устроены все карты дистрибутива и обучения,
/// и parse → serialize возвращает их байт в байт.
pub fn serialize(konung_map: &KonungMap) -> Result<Vec<u8>> {
    let decorations = decoration_slots(&konung_map.decorations)?;
    if konung_map.objects.len() > MAX_OBJECTS {
        return Err(MapError::TooManyObjects {
            count: konung_map.objects.len(),
        });
    }
    check_grid("ground", &konung_map.ground, HEIGHT, WIDTH)?;
    check_grid("overlay", &konung_map.overlay, HEIGHT, WIDTH)?;
    check_grid("foot", &konung_map.foot, FINE_HEIGHT, FINE_WIDTH)?;

    let mut out: Vec<u8> = Vec::with_capacity(MAP_SIZE);
    for (ground_row, overlay_row) in konung_map.ground.iter().zip(&konung_map.overlay) {
        for (&ground, &overlay) in ground_row.iter().zip(overlay_row) {
            out.push(ground);
            out.push(overlay);
        }
    }
    for row in &konung_map.foot {
        out.extend_from_slice(row);
    }

    for decoration in &decorations {
        match decoration {
            None => out.extend_from_slice(&[0xFF; DECORATION_SIZE]),
            Some(decoration) => {
                out.extend_from_slice(&decoration.tile.to_le_bytes());
                out.extend_from_slice(&decoration.unknown);
                out.extend_from_slice(&decoration.x.to_le_bytes());
                out.extend_from_slice(&decoration.y.to_le_bytes());
            }
        }
    }

    for placed in &konung_map.objects {
        out.extend_from_slice(&placed.object_id.to_le_bytes());
        out.extend_from_slice(&placed.unknown);
        out.extend_from_slice(&placed.flames);
        out.extend_from_slice(&placed.frame_time.to_le_bytes());
        out.extend_from_slice(&placed.x.to_le_bytes());
        out.extend_from_slice(&placed.y.to_le_bytes());
        out.push(placed.flags);
        out.push(placed.frame);
    }
    out.extend(std::iter::repeat_n(
        0xFF,
        OBJECT_SIZE * (MAX_OBJECTS - konung_map.objects.len()),
    ));

    if out.len() != MAP_SIZE {
        return Err(MapError::WrongSerializedSize {
            actual: out.len(),
            expected: MAP_SIZE,
        });
    }
    Ok(out)
}

pub fn parse_lights(data: &[u8]) -> Result<Lights> {
    if data.len() != LIGHTS_SIZE {
        return Err(MapError::WrongLightsSize { actual: data.len() });
    }
    let mut reader = Reader::new(data);
    let mut tiles = Vec::with_capacity(HEIGHT);
    for _ in 0..HEIGHT {
        tiles.push(reader.bytes(WIDTH)?.to_vec());
    }
    let mut fine = Vec::with_capacity(FINE_HEIGHT);
    for _ in 0..FINE_HEIGHT {
        fine.push(reader.bytes(FINE_WIDTH)?.to_vec());
    }
    Ok(Lights { tiles, fine })
}

/// Мелкий блок `LIGHTS.N` из тайлового: так он устроен во всех файлах дистрибутива.
pub fn lights_fine(tiles: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut fine = vec![vec![0u8; FINE_WIDTH]; FINE_HEIGHT];
    for (row, values) in tiles.iter().enumerate() {
        for (column, &value) in values.iter().enumerate() {
            for (x, y) in fine_cells(column, row) {
                if x < FINE_WIDTH && y < FINE_HEIGHT {
                    fine[y][x] = value;
                }
            }
        }
    }
    fine
}

/// Свет обратно в `LIGHTS.N`; мелкий блок пересчитывается из тайлового.
pub fn serialize_lights(lights: &Lights) -> Result<Vec<u8>> {
    check_grid("tiles", &lights.tiles, HEIGHT, WIDTH)?;
    let mut out: Vec<u8> = Vec::with_capacity(LIGHTS_SIZE);
    for row in &lights.tiles {
        out.extend_from_slice(row);
    }
    for row in lights_fine(&lights.tiles) {
        out.extend_from_slice(&row);
    }
    if out.len() != LIGHTS_SIZE {
        return Err(MapError::WrongSerializedSize {
            actual: out.len(),
            expected: LIGHTS_SIZE,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// Пустая карта: нули в слоях, 0xFF в декорациях и объектах,
    /// как в test_serialize_keeps_edits (tests/k1/test_maps.py).
    fn empty_map_bytes() -> Vec<u8> {
        let tail = MAX_DECORATIONS * DECORATION_SIZE + MAX_OBJECTS * OBJECT_SIZE;
        let mut data = vec![0u8; MAP_SIZE - tail];
        data.extend(std::iter::repeat_n(0xFF, tail));
        data
    }

    fn decoration(slot: usize, tile: i32, x: i16) -> Decoration {
        Decoration {
            slot,
            tile,
            x,
            y: -1,
            unknown: [0; 8],
        }
    }

    #[test]
    fn wrong_size_is_an_error() {
        assert!(matches!(
            parse(&[0u8; MAP_SIZE - 1]),
            Err(MapError::WrongMapSize { .. })
        ));
        assert!(matches!(
            parse_lights(&[0u8; LIGHTS_SIZE + 1]),
            Err(MapError::WrongLightsSize { .. })
        ));
    }

    #[test]
    fn empty_map_round_trips() {
        let data = empty_map_bytes();
        let konung_map = parse(&data).unwrap();
        assert_eq!(konung_map.ground.len(), HEIGHT);
        assert_eq!(konung_map.ground[0].len(), WIDTH);
        assert_eq!(konung_map.foot.len(), FINE_HEIGHT);
        assert!(konung_map.decorations.is_empty());
        assert!(konung_map.objects.is_empty());
        assert_eq!(serialize(&konung_map).unwrap(), data);
    }

    #[test]
    fn serialize_keeps_edits() {
        let mut konung_map = parse(&empty_map_bytes()).unwrap();
        konung_map.ground[3][5] = 7;
        konung_map.decorations.push(Decoration {
            slot: 0,
            tile: 230,
            x: 100,
            y: -20,
            unknown: [0; 8],
        });
        konung_map.objects.push(MapObject {
            slot: 0,
            object_id: 117,
            x: 27,
            y: 33,
            flames: [0xFF; FLAME_SLOTS],
            frame_time: 0,
            flags: 0,
            frame: 0,
            unknown: [0; 4],
        });
        let edited = parse(&serialize(&konung_map).unwrap()).unwrap();
        assert_eq!(edited.ground[3][5], 7);
        let decorations: Vec<_> = edited
            .decorations
            .iter()
            .map(|d| (d.tile, d.x, d.y))
            .collect();
        assert_eq!(decorations, [(230, 100, -20)]);
        let objects: Vec<_> = edited
            .objects
            .iter()
            .map(|o| (o.object_id, o.x, o.y))
            .collect();
        assert_eq!(objects, [(117, 27, 33)]);
    }

    #[test]
    fn serialize_checks_grid_geometry() {
        // 81 клетка в строке 0 ground и 79 в строке 1 overlay: суммарный
        // размер верный, но границы строк были бы смещены
        let mut konung_map = parse(&empty_map_bytes()).unwrap();
        konung_map.ground[0].push(0);
        konung_map.overlay[1].pop();
        assert!(matches!(
            serialize(&konung_map),
            Err(MapError::Geometry {
                layer: "ground",
                ..
            })
        ));

        // число строк и ширина строки foot
        let mut konung_map = parse(&empty_map_bytes()).unwrap();
        konung_map.foot.remove(7);
        assert!(matches!(
            serialize(&konung_map),
            Err(MapError::Geometry { layer: "foot", .. })
        ));
        let mut konung_map = parse(&empty_map_bytes()).unwrap();
        konung_map.foot[10].push(0);
        assert!(matches!(
            serialize(&konung_map),
            Err(MapError::Geometry { layer: "foot", .. })
        ));

        // обе сетки тайлов короткие на строку, ground и overlay поровну —
        // парность не спасает, нужна точная высота
        let mut konung_map = parse(&empty_map_bytes()).unwrap();
        konung_map.ground.pop();
        konung_map.overlay.pop();
        assert!(matches!(
            serialize(&konung_map),
            Err(MapError::Geometry {
                layer: "ground",
                ..
            })
        ));

        let lights = Lights {
            tiles: vec![vec![0u8; WIDTH - 1]; HEIGHT],
            fine: vec![],
        };
        assert!(matches!(
            serialize_lights(&lights),
            Err(MapError::Geometry { layer: "tiles", .. })
        ));
    }

    #[test]
    fn raw_decoration_keeps_its_slot() {
        let raw = decoration(994, 0x00FF_FFFF, 0);
        let decorations = [decoration(0, 5, 10), raw.clone(), decoration(1, 7, 20)];
        let slots = decoration_slots(&decorations).unwrap();
        assert_eq!(slots[0], Some(decorations[0].clone()));
        assert_eq!(slots[1], Some(decorations[2].clone()));
        assert_eq!(slots[994], Some(raw.clone()));
        assert_eq!(slots.iter().flatten().count(), 3);

        // Удаление, добавление и перестановка обычных декораций сырую не сдвигают
        let mut konung_map = parse(&empty_map_bytes()).unwrap();
        konung_map.decorations = vec![decoration(0, 3, 1), raw.clone(), decoration(0, 4, 2)];
        let edited = parse(&serialize(&konung_map).unwrap()).unwrap();
        let placed: Vec<_> = edited
            .decorations
            .iter()
            .map(|d| (d.slot, d.tile))
            .collect();
        assert_eq!(placed, [(0, 3), (1, 4), (994, 0x00FF_FFFF)]);

        konung_map.decorations = vec![decoration(0, 4, 2), raw.clone()];
        let edited = parse(&serialize(&konung_map).unwrap()).unwrap();
        let placed: Vec<_> = edited
            .decorations
            .iter()
            .map(|d| (d.slot, d.tile))
            .collect();
        assert_eq!(placed, [(0, 4), (994, 0x00FF_FFFF)]);

        // Обычные обходят занятый слот, а два сырых в одном слоте — ошибка
        let mut many: Vec<Decoration> = (0..MAX_DECORATIONS - 1)
            .map(|_| decoration(0, 1, 0))
            .collect();
        many.push(raw.clone());
        assert_eq!(
            decoration_slots(&many).unwrap()[995].as_ref().unwrap().tile,
            1
        );
        assert!(matches!(
            decoration_slots(&[raw.clone(), decoration(994, 0x0100_0000, 0)]),
            Err(MapError::RawSlotConflict { slot: 994, .. })
        ));
        many.push(decoration(0, 1, 0));
        assert!(matches!(
            decoration_slots(&many),
            Err(MapError::TooManyDecorations { .. })
        ));
    }

    #[test]
    fn foot_flags() {
        assert!(is_blocked(FOOT_BLOCKED));
        assert!(is_blocked(FOOT_BLOCKED | FOOT_STOPS_ARROWS));
        assert!(!is_blocked(0x00));
        assert!(!is_blocked(FOOT_STOPS_ARROWS));
    }

    #[test]
    fn fine_cells_split_tiles_without_overlap() {
        let mut owners = HashSet::new();
        for row in 0..HEIGHT {
            for column in 0..WIDTH {
                for (x, y) in fine_cells(column, row) {
                    if x < FINE_WIDTH && y < FINE_HEIGHT {
                        assert!(owners.insert((x, y)), "клетка ({x}, {y}) в двух тайлах");
                    }
                }
            }
        }
        let mut orphans = HashSet::new();
        for y in 0..FINE_HEIGHT {
            for x in 0..FINE_WIDTH {
                if !owners.contains(&(x, y)) {
                    orphans.insert((x, y));
                }
            }
        }
        // Без тайла — только нечётные клетки верхней строки и левый край
        // нечётных строк тайлов
        let expected: HashSet<(usize, usize)> = (1..FINE_WIDTH)
            .step_by(2)
            .map(|x| (x, 0))
            .chain((3..FINE_HEIGHT).step_by(4).map(|y| (0, y)))
            .collect();
        assert_eq!(orphans, expected);
    }

    #[test]
    fn tile_position_shifts_odd_rows() {
        assert_eq!(tile_position(0, 0), (0, 0));
        assert_eq!(tile_position(1, 0), (TILE_STEP_X, 0));
        assert_eq!(tile_position(0, 1), (ODD_ROW_SHIFT, TILE_STEP_Y));
        assert_eq!(
            tile_position(2, 3),
            (2 * TILE_STEP_X + ODD_ROW_SHIFT, 3 * TILE_STEP_Y)
        );
    }

    #[test]
    fn lights_round_trip_and_fine_follows_tiles() {
        let tiles = vec![vec![7u8; WIDTH]; HEIGHT];
        let fine = lights_fine(&tiles);
        assert_eq!(fine[0][0], 7);
        // Нечётные клетки верхней строки не принадлежат ни одному тайлу
        assert_eq!(fine[0][1], 0);
        let lights = Lights { tiles, fine };
        let data = serialize_lights(&lights).unwrap();
        assert_eq!(data.len(), LIGHTS_SIZE);
        assert_eq!(parse_lights(&data).unwrap(), lights);
        // Мелкий блок при записи пересчитывается из тайлового: поле fine игнорируется
        let stale_fine = Lights {
            tiles: vec![vec![3u8; WIDTH]; HEIGHT],
            fine: vec![],
        };
        assert_eq!(
            serialize_lights(&stale_fine).unwrap(),
            serialize_lights(&parse_lights(&serialize_lights(&stale_fine).unwrap()).unwrap())
                .unwrap()
        );
    }
}
