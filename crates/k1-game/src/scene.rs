//! Список отрисовки карты — упорядоченные куски с ключами текстур.
//!
//! Порядок и позиции повторяют эталон `konung/k1/render.py`: земля, верхний
//! слой, декорации, слой теней, затем объекты с огнями и персонажи,
//! отсортированные по нижнему краю (при равенстве — объекты раньше
//! персонажей, дальше по слоту); оверлеи отладки — поверх всего.
//! Текстовые подписи маркеров не рисуются: в модели сцены нет текста.

use thiserror::Error;

use k1_data::{
    characters::{self, CharacterSprite},
    executable::Executable,
    game_state::{self, MapEntities},
    graph_res::{GraphError, GraphRes},
    map::{self, KonungMap, Lights, MapObject},
    objects_res::ObjectsRes,
};

use crate::textures::{
    FOOT_ARROWS_COLOR, FOOT_BLOCKED_COLOR, FOOT_DIAMOND, MARKER_RADIUS, TextureKey, marker_colors,
};

pub type Result<T> = std::result::Result<T, SceneError>;

#[derive(Debug, Error)]
pub enum SceneError {
    #[error(transparent)]
    Graph(#[from] GraphError),
}

/// Кусок сцены: картинка (ключ текстуры), левый верхний угол на карте.
/// `w`/`h` — None: размер самой текстуры; Some: растянуть (квадраты маркеров).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawPiece {
    pub texture: TextureKey,
    pub x: i32,
    pub y: i32,
    pub w: Option<i32>,
    pub h: Option<i32>,
}

impl DrawPiece {
    fn textured(texture: TextureKey, x: i32, y: i32) -> Self {
        Self {
            texture,
            x,
            y,
            w: None,
            h: None,
        }
    }

    fn solid(color: [u8; 4], x: i32, y: i32, w: i32, h: i32) -> Self {
        Self {
            texture: TextureKey::Solid { color },
            x,
            y,
            w: Some(w),
            h: Some(h),
        }
    }
}

/// Ключ сортировки объектов и персонажей: нижний край картинки, вид
/// (0 — объекты раньше персонажей), слот — как ключ в `render.py`.
type SortKey = (i32, i32, usize);
/// Группа кусков одного объекта или персонажа: сортируется и рисуется целиком
/// (огни объекта остаются сразу над его картинкой).
type DrawGroup = (SortKey, Vec<DrawPiece>);

/// Какие отладочные оверлеи добавить поверх карты.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Overlays {
    /// Проходимость мелкой сетки (красный — непроходимо, синий — застревают стрелы)
    pub foot: bool,
    /// Маски света LIGHTS.N поверх карты
    pub lights: bool,
    /// Маркеры Game.N: отряды, клады, переходы, рабочие места, точка старта
    pub markers: bool,
    /// Персонажи спрайтами из Game.N
    pub characters: bool,
}

/// Данные карты для построения сцены.
pub struct SceneInput<'a> {
    pub map: &'a KonungMap,
    /// LIGHTS.N карты, если есть
    pub lights: Option<&'a Lights>,
    /// Что стоит на карте из Game.N (для маркеров и персонажей)
    pub entities: Option<MapEntities<'a>>,
    pub state: Option<&'a k1_data::game_state::GameState>,
    pub overlays: Overlays,
}

/// Позиция объекта (клетка мелкой сетки) в пикселях карты (render.object_position).
pub fn object_position(x: i16, y: i16) -> (i32, i32) {
    let (x, y) = (i32::from(x), i32::from(y));
    (
        x * map::TILE_STEP_X / 2 - (y & 1) * map::TILE_STEP_X / 4,
        y * 16,
    )
}

/// Список отрисовки карты в порядке наложения.
pub fn draw_list(
    input: &SceneInput<'_>,
    graph: &GraphRes<'_>,
    objects: Option<&ObjectsRes<'_>>,
    tables: &Executable,
) -> Result<Vec<DrawPiece>> {
    let mut pieces = Vec::new();

    // Земля и верхний слой: номера тайлов с единицы, 0 — пусто
    for layer in [&input.map.ground, &input.map.overlay] {
        for (row, cells) in layer.iter().enumerate() {
            for (column, &value) in cells.iter().enumerate() {
                if value != 0 {
                    let (x, y) = map::tile_position(column, row);
                    pieces.push(DrawPiece::textured(
                        TextureKey::Tile {
                            index: (value - 1) as usize,
                        },
                        x,
                        y,
                    ));
                }
            }
        }
    }

    // Декорации: сырые (тайл вне GRAPH.RES) не нарисовать — пропускаем
    for decoration in &input.map.decorations {
        if decoration.tile >= 0 && graph.tile(decoration.tile as usize).is_some() {
            pieces.push(DrawPiece::textured(
                TextureKey::Tile {
                    index: decoration.tile as usize,
                },
                i32::from(decoration.x),
                i32::from(decoration.y),
            ));
        }
    }

    // Объекты и персонажи: собираем группы с ключом сортировки и тени отдельно
    let mut shadows = Vec::new();
    let mut drawn: Vec<DrawGroup> = Vec::new();
    for placed in &input.map.objects {
        object_pieces(placed, objects, tables, &mut shadows, &mut drawn);
    }
    if input.overlays.characters
        && let (Some(entities), Some(state)) = (input.entities.as_ref(), input.state)
        && let Some(objects) = objects
    {
        for (character, x, y) in characters::placed(entities, state) {
            let (px, py) = game_state::character_position(x, y);
            if let Some(sprite) =
                characters::compose(character, px, py, state, graph, objects, tables, None)?
            {
                character_pieces(sprite, &mut shadows, &mut drawn);
            }
        }
    }

    // Тени — один слой под объектами (как shadow_layer в render.py)
    pieces.extend(shadows);

    // Ближе к зрителю то, что ниже на экране; сортировка устойчивая: огни
    // объекта остаются сразу над его картинкой
    drawn.sort_by_key(|group| group.0);
    pieces.extend(drawn.into_iter().flat_map(|(_, group)| group));

    if input.overlays.lights {
        light_pieces(input, &mut pieces);
    }
    if input.overlays.foot {
        foot_pieces(input.map, &mut pieces);
    }
    if input.overlays.markers
        && let Some(entities) = input.entities.as_ref()
    {
        marker_pieces(entities, &mut pieces);
    }
    Ok(pieces)
}

fn object_pieces(
    placed: &MapObject,
    objects: Option<&ObjectsRes<'_>>,
    tables: &Executable,
    shadows: &mut Vec<DrawPiece>,
    drawn: &mut Vec<DrawGroup>,
) {
    let Some(simple) = objects
        .and_then(|objects| objects.simple.get(placed.object_id as usize))
        .and_then(Option::as_ref)
    else {
        return;
    };
    // Кадр вне диапазона или без картинки — берём первый, как render.py
    let mut frame_index = placed.frame as usize;
    if simple
        .frames
        .get(frame_index)
        .and_then(|frame| frame.image.as_ref())
        .is_none()
    {
        frame_index = 0;
    }
    let Some(frame) = simple.frames.get(frame_index) else {
        return;
    };
    let Some(image) = frame.image.as_ref() else {
        return;
    };
    let (x, y) = object_position(placed.x, placed.y);
    let key = (
        y + frame.offset.1 as i32 + image.height as i32,
        0,
        placed.slot,
    );

    let mut group = vec![DrawPiece::textured(
        TextureKey::ObjectFrame {
            object_id: placed.object_id as usize,
            frame: frame_index,
        },
        x + frame.offset.0 as i32,
        y + frame.offset.1 as i32,
    )];
    // Огни объекта — сразу поверх него, первый кадр анимации своего вида
    for flame in &simple.flames {
        if flame.flame_id < 0 {
            continue;
        }
        let Some(&(first, _)) = tables.flame_animations.get(flame.flame_id as usize) else {
            continue;
        };
        group.push(DrawPiece::textured(
            TextureKey::FlameFrame {
                index: first as usize,
            },
            x + i32::from(flame.x),
            y + i32::from(flame.y),
        ));
    }
    if frame.shadow.is_some() {
        shadows.push(DrawPiece::textured(
            TextureKey::ObjectShadow {
                object_id: placed.object_id as usize,
                frame: frame_index,
            },
            x + frame.shadow_offset.0 as i32,
            y + frame.shadow_offset.1 as i32,
        ));
    }
    drawn.push((key, group));
}

fn character_pieces(
    sprite: CharacterSprite,
    shadows: &mut Vec<DrawPiece>,
    drawn: &mut Vec<DrawGroup>,
) {
    let key = (sprite.bottom(), 1, sprite.slot);
    let group = sprite
        .layers
        .iter()
        .map(|layer| {
            DrawPiece::textured(
                TextureKey::SpriteImage {
                    source: layer.source,
                    image_id: layer.image_id as u16,
                    palette: layer.palette_offset as u32,
                },
                layer.x,
                layer.y,
            )
        })
        .collect();
    if let Some(shadow) = sprite.shadow {
        shadows.push(DrawPiece::textured(
            TextureKey::SpriteShadow {
                source: shadow.source,
                image_id: shadow.image_id as u16,
            },
            shadow.x,
            shadow.y,
        ));
    }
    drawn.push((key, group));
}

fn light_pieces(input: &SceneInput<'_>, pieces: &mut Vec<DrawPiece>) {
    let Some(lights) = input.lights else { return };
    for (row, values) in lights.tiles.iter().enumerate() {
        for (column, &value) in values.iter().enumerate() {
            if value != 0 {
                let (x, y) = map::tile_position(column, row);
                pieces.push(DrawPiece::textured(
                    TextureKey::LightMask {
                        index: (value - 1) as usize,
                    },
                    x,
                    y,
                ));
            }
        }
    }
}

fn foot_pieces(map: &KonungMap, pieces: &mut Vec<DrawPiece>) {
    // Ромбы не перекрываются, поэтому каждый слой рисуется сразу с прозрачностью;
    // два прохода, как в render.draw_foot: сначала непроходимые, потом стрелы
    let (half_x, half_y) = (FOOT_DIAMOND.0 / 2, FOOT_DIAMOND.1 / 2);
    for (color, marked) in [
        (FOOT_BLOCKED_COLOR, map::is_blocked as fn(u8) -> bool),
        (FOOT_ARROWS_COLOR, |value| {
            value & map::FOOT_STOPS_ARROWS != 0
        }),
    ] {
        for (y, row) in map.foot.iter().enumerate() {
            for (x, &value) in row.iter().enumerate() {
                if marked(value) {
                    let (cx, cy) = game_state::character_position(x as i32, y as i32);
                    pieces.push(DrawPiece::textured(
                        TextureKey::FootDiamond { color },
                        cx - half_x,
                        cy - half_y,
                    ));
                }
            }
        }
    }
}

fn marker_pieces(entities: &MapEntities<'_>, pieces: &mut Vec<DrawPiece>) {
    fn point(pieces: &mut Vec<DrawPiece>, color: [u8; 4], x: i32, y: i32) {
        let (px, py) = game_state::character_position(x, y);
        pieces.push(DrawPiece::textured(
            TextureKey::MarkerRing { color },
            px - MARKER_RADIUS,
            py - MARKER_RADIUS,
        ));
    }

    fn rect(pieces: &mut Vec<DrawPiece>, color: [u8; 4], area: &game_state::Rect) {
        // Контур толщиной 3 (как PIL rectangle width=3) — четыре сплошных куска
        let (left, top, right, bottom) = game_state::rect_pixels(area);
        let width = right - left + 1;
        let height = bottom - top + 1;
        pieces.push(DrawPiece::solid(color, left, top, width, 3));
        pieces.push(DrawPiece::solid(color, left, bottom - 2, width, 3));
        pieces.push(DrawPiece::solid(color, left, top, 3, height));
        pieces.push(DrawPiece::solid(color, right - 2, top, 3, height));
    }

    if let Some(start) = entities.start {
        point(
            pieces,
            marker_colors::START,
            start.spawn.left as i32,
            start.spawn.top as i32,
        );
    }
    for group in &entities.groups {
        if group.is_roaming() {
            rect(pieces, marker_colors::LEASH, &group.leash);
            rect(pieces, marker_colors::SPAWN, &group.spawn);
        }
    }
    // Персонажи-маркеры не рисуются: они видны спрайтами (как в render.py
    // при objects is not None)
    for (_, job) in &entities.jobs {
        point(
            pieces,
            marker_colors::JOB,
            i32::from(job.x),
            i32::from(job.y),
        );
    }
    for loot in &entities.loot {
        point(
            pieces,
            marker_colors::LOOT,
            i32::from(loot.x),
            i32::from(loot.y),
        );
    }
    for exit in &entities.exits {
        rect(pieces, marker_colors::EXIT, &exit.zone);
    }
    for arrival in &entities.arrivals {
        point(
            pieces,
            marker_colors::ARRIVAL,
            i32::from(arrival.arrival_x),
            i32::from(arrival.arrival_y),
        );
    }
}
