//! Список отрисовки карты — упорядоченные куски с ключами текстур.
//!
//! Порядок и позиции повторяют эталон `konung/k1/render.py`: земля, верхний
//! слой, декорации, слой теней, затем объекты с огнями и персонажи,
//! отсортированные по нижнему краю (при равенстве — объекты раньше
//! персонажей, дальше по слоту); оверлеи отладки — поверх всего.
//! Текстовые подписи маркеров не рисуются: в модели сцены нет текста.

use thiserror::Error;

use k1_data::{
    characters::{self, LayerKey},
    executable::Executable,
    game_state::{self, GameState, MapEntities},
    graph_res::{GraphError, GraphRes},
    map::{self, KonungMap, Lights, MapObject},
    objects_res::{self, ObjectsRes},
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
/// `texture` — None: в базовом кадре картинки нет, кусок скрыт до своего кадра
/// анимации. `animation` — None: кусок статичен. `group` — узел, который
/// скрывает и показывает кусок вместе с собой (оверлей, а не пересборка).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawPiece {
    pub texture: Option<TextureKey>,
    pub x: i32,
    pub y: i32,
    pub w: Option<i32>,
    pub h: Option<i32>,
    pub animation: Option<DrawAnimation>,
    pub group: PieceGroup,
}

/// Кадр анимированного куска: картинка (`None` — слоя в этом кадре не видно) и
/// смещение её угла от угла куска. `ticks` — сколько тиков кадр длится.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawFrame {
    pub texture: Option<TextureKey>,
    pub dx: i32,
    pub dy: i32,
    pub ticks: u32,
}

/// Анимация куска: кадры одного слоя и `base` — кадр, записанный в данных и
/// показываемый, когда анимации нет.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawAnimation {
    pub frames: Vec<DrawFrame>,
    pub base: usize,
}

/// Группа кусков: то, что скрывается и показывается вместе.
///
/// Порядок наложения задаёт позиция в списке отрисовки, а не принадлежность
/// группе, поэтому разнос по группам ничего не переставляет: куски одной группы
/// бывают перемешаны с чужими (персонажи идут вперемежку с объектами).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PieceGroup {
    /// Земля и верхний слой тайлов
    Terrain,
    /// Декорации из GRAPH.RES
    Decorations,
    /// Общие тени карты — под спрайтами
    Shadows,
    /// Картинки объектов вместе с их огнями
    Objects,
    /// Персонажи из `Game.N`
    Characters,
    /// Тени персонажей: без персонажей их показывать нечего
    CharacterShadows,
    /// Маски света `LIGHTS.N`
    Lights,
    /// Проходимость мелкой сетки
    Foot,
    /// Маркеры `Game.N`: отряды, клады, переходы, точки
    Markers,
}

impl PieceGroup {
    /// Все группы в порядке наложения: слой отображения заводит по узлу на
    /// каждую и в этом же порядке раскладывает по ним куски.
    pub const ALL: [PieceGroup; 9] = [
        PieceGroup::Terrain,
        PieceGroup::Decorations,
        PieceGroup::Shadows,
        PieceGroup::Objects,
        PieceGroup::Characters,
        PieceGroup::CharacterShadows,
        PieceGroup::Lights,
        PieceGroup::Foot,
        PieceGroup::Markers,
    ];

    /// Показываются ли куски группы при этих оверлеях.
    ///
    /// Карта и её тени скрыть нельзя; персонажи и их тени переключаются одним
    /// слоем: при выключенных персонажах список отрисовки не содержит ни одной
    /// тени персонажа, и показывать их было бы нечего.
    pub fn is_shown_by(self, overlays: &Overlays) -> bool {
        match self {
            PieceGroup::Terrain
            | PieceGroup::Decorations
            | PieceGroup::Shadows
            | PieceGroup::Objects => true,
            PieceGroup::Characters | PieceGroup::CharacterShadows => overlays.characters,
            PieceGroup::Lights => overlays.lights,
            PieceGroup::Foot => overlays.foot,
            PieceGroup::Markers => overlays.markers,
        }
    }
}

/// Один кадр слоя: картинка и её левый верхний угол на карте.
type LayerFrame = Option<(TextureKey, i32, i32)>;

/// Дополняет последовательности всех слоёв до кадра `index`: слой, которого в
/// этом кадре нет, остаётся в них скрытым, а не выпадает из них.
///
/// Без этого тайминг съезжал бы: предмет, пропавший на третьем кадре,
/// подтянул бы четвёртый кадр на место третьего, и рассинхронизировались бы все
/// остальные слои того же персонажа.
fn pad_frames(tracks: &mut [(LayerKey, Vec<LayerFrame>)], index: usize) {
    for (_, frames) in tracks {
        while frames.len() <= index {
            frames.push(None);
        }
    }
}

/// Номера кадров простого объекта, из которых собирается слой, и номер базового
/// кадра внутри этого списка.
///
/// Анимированный объект проигрывает все `count` кадров от нуля. Статичный
/// показывает один — тот, что записан в карте (`placed.frame`), а не первый:
/// объект, оставленный игрой на третьем кадре, иначе рисовался бы нулевым.
fn frame_plan(count: usize, base: usize, animated: bool) -> (Vec<usize>, usize) {
    if animated {
        ((0..count).collect(), base.min(count - 1))
    } else {
        (vec![base], 0)
    }
}

/// Кусок, собранный из кадров одного слоя.
struct Track {
    texture: Option<TextureKey>,
    x: i32,
    y: i32,
    animation: Option<DrawAnimation>,
}

/// Кусок из последовательности кадров одного слоя.
///
/// Началом берётся угол базового кадра — тогда при выключенной анимации картинка
/// стоит там же, где её ставит эталонный `render.py`. Если в базовом кадре
/// картинки нет (слой есть не во всех кадрах), началом становится угол первого
/// кадра, где она есть, а кусок до своего кадра скрыт.
///
/// Кадр без картинки не выбрасывается из последовательности, а остаётся скрытым:
/// пропуск сдвинул бы тайминг остальных кадров. `None` — картинок нет ни в одном
/// кадре, куска не бывает.
fn layer_track(frames: Vec<LayerFrame>, base: usize, ticks: u32) -> Option<Track> {
    if frames.is_empty() {
        return None;
    }
    let base = base.min(frames.len() - 1);
    let anchor = frames[base].or_else(|| frames.iter().find_map(|frame| *frame))?;
    let (_, anchor_x, anchor_y) = anchor;
    // Один кадр — анимировать нечего, и кусок получается тем же, каким был бы
    // статичный кусок с той же картинкой
    let animation = (frames.len() > 1).then(|| DrawAnimation {
        base,
        frames: frames
            .iter()
            .map(|frame| match frame {
                Some((texture, x, y)) => DrawFrame {
                    texture: Some(*texture),
                    dx: x - anchor_x,
                    dy: y - anchor_y,
                    ticks,
                },
                None => DrawFrame {
                    texture: None,
                    dx: 0,
                    dy: 0,
                    ticks,
                },
            })
            .collect(),
    });
    Some(Track {
        texture: if animation.is_some() {
            frames[base].as_ref().map(|frame| frame.0)
        } else {
            Some(anchor.0)
        },
        x: anchor_x,
        y: anchor_y,
        animation,
    })
}

impl DrawPiece {
    fn textured(group: PieceGroup, texture: TextureKey, x: i32, y: i32) -> Self {
        Self {
            texture: Some(texture),
            x,
            y,
            w: None,
            h: None,
            animation: None,
            group,
        }
    }

    /// Кусок из слоя, у которого есть кадры (объект, персонаж, огонь).
    fn track(group: PieceGroup, track: Track) -> Self {
        Self {
            texture: track.texture,
            x: track.x,
            y: track.y,
            w: None,
            h: None,
            animation: track.animation,
            group,
        }
    }

    fn solid(group: PieceGroup, color: [u8; 4], x: i32, y: i32, w: i32, h: i32) -> Self {
        Self {
            texture: Some(TextureKey::Solid { color }),
            x,
            y,
            w: Some(w),
            h: Some(h),
            animation: None,
            group,
        }
    }
}

/// Ключ сортировки объектов и персонажей: нижний край картинки, вид
/// (0 — объекты раньше персонажей), слот — как ключ в `render.py`.
type SortKey = (i32, i32, usize);
/// Куски одного объекта или персонажа, которые сортируются и попадают в список
/// целиком (огни объекта остаются сразу над его картинкой).
///
/// Не путать с [`PieceGroup`]: она про то, что переключается вместе, а это про
/// то, что идёт в список отрисовки одним блоком.
type Stack = (SortKey, Vec<DrawPiece>);

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
                        PieceGroup::Terrain,
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
                PieceGroup::Decorations,
                TextureKey::Tile {
                    index: decoration.tile as usize,
                },
                i32::from(decoration.x),
                i32::from(decoration.y),
            ));
        }
    }

    // Объекты и персонажи: собираем стеки с ключом сортировки и тени отдельно
    let mut shadows = Vec::new();
    let mut drawn: Vec<Stack> = Vec::new();
    if let Some(objects) = objects {
        let sheets = Sheets {
            graph,
            objects,
            tables,
        };
        for placed in &input.map.objects {
            object_pieces(placed, &sheets, &mut shadows, &mut drawn);
        }
        if input.overlays.characters
            && let (Some(entities), Some(state)) = (input.entities.as_ref(), input.state)
        {
            for (character, x, y) in characters::placed(entities, state) {
                let (px, py) = game_state::character_position(x, y);
                character_pieces(character, px, py, state, &sheets, &mut shadows, &mut drawn)?;
            }
        }
    }

    // Тени — один слой под объектами (как shadow_layer в render.py)
    pieces.extend(shadows);

    // Ближе к зрителю то, что ниже на экране; сортировка устойчивая: огни
    // объекта остаются сразу над его картинкой
    drawn.sort_by_key(|stack| stack.0);
    pieces.extend(drawn.into_iter().flat_map(|(_, stack)| stack));

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

/// Графика одной карты, общая для объектов и персонажей списка отрисовки.
struct Sheets<'a> {
    graph: &'a GraphRes<'a>,
    objects: &'a ObjectsRes<'a>,
    tables: &'a Executable,
}

/// Кадры, которые длятся один тик: так игра меняет кадры огней (Slavik
/// game.cpp:1587-1597) и персонажей (game_play.cpp:3729).
const ONE_TICK: u32 = 1;

fn object_pieces(
    placed: &MapObject,
    sheets: &Sheets<'_>,
    shadows: &mut Vec<DrawPiece>,
    drawn: &mut Vec<Stack>,
) {
    let Some(simple) = sheets
        .objects
        .simple
        .get(placed.object_id as usize)
        .and_then(Option::as_ref)
    else {
        return;
    };
    // Кадров восемь слотов, а заполнено данными столько, сколько frame_count
    let count = (simple.frame_count as usize).min(simple.frames.len());
    if count == 0 {
        return;
    }
    // Кадр вне диапазона или без картинки — берём первый, как render.py
    let mut base = (placed.frame as usize).min(count - 1);
    if simple.frames[base].image.is_none() {
        base = 0;
    }
    let Some(image) = simple.frames[base].image.as_ref() else {
        return;
    };
    let (x, y) = object_position(placed.x, placed.y);
    let key = (
        y + i32::from(simple.frames[base].offset.1) + image.height as i32,
        0,
        placed.slot,
    );
    let object_id = placed.object_id as usize;
    // Кадр анимированного объекта длится frame_time тиков. Кадров может быть
    // много и без frame_time — такой объект игра не анимирует (и `render.py`, и
    // расширение Tiled берут у него один кадр), поэтому берёмся только за base.
    let animated =
        objects_res::frame_duration_ms(simple).map(|ms| (ms / objects_res::TICK_MS).max(1));
    let (indices, base_index) = frame_plan(count, base, animated.is_some());
    let ticks = animated.unwrap_or(1);
    let picture = |index: usize| {
        let frame = &simple.frames[index];
        frame.image.as_ref().map(|_| {
            (
                TextureKey::ObjectFrame {
                    object_id,
                    frame: index,
                },
                x + i32::from(frame.offset.0),
                y + i32::from(frame.offset.1),
            )
        })
    };
    let shadow = |index: usize| {
        let frame = &simple.frames[index];
        frame.shadow.as_ref().map(|_| {
            (
                TextureKey::ObjectShadow {
                    object_id,
                    frame: index,
                },
                x + i32::from(frame.shadow_offset.0),
                y + i32::from(frame.shadow_offset.1),
            )
        })
    };

    let mut stack = layer_track(
        indices.iter().map(|&index| picture(index)).collect(),
        base_index,
        ticks,
    )
    .map(|track| DrawPiece::track(PieceGroup::Objects, track))
    .into_iter()
    .collect::<Vec<_>>();
    // Огни объекта — сразу поверх него: все кадры анимации своего вида, по тике
    // на кадр. У кадра огня нет своего смещения, угол задаёт точка огня.
    for flame in &simple.flames {
        if flame.flame_id < 0 {
            continue;
        }
        let Some(&(first, last)) = sheets.tables.flame_animations.get(flame.flame_id as usize)
        else {
            continue;
        };
        let first = usize::try_from(first.max(0)).unwrap_or(0);
        let last = usize::try_from(last.max(0)).unwrap_or(first).max(first);
        let (left, top) = (x + i32::from(flame.x), y + i32::from(flame.y));
        let flame_frames = (first..=last)
            .map(|index| Some((TextureKey::FlameFrame { index }, left, top)))
            .collect();
        if let Some(track) = layer_track(flame_frames, 0, ONE_TICK) {
            stack.push(DrawPiece::track(PieceGroup::Objects, track));
        }
    }
    if let Some(track) = layer_track(
        indices.iter().map(|&index| shadow(index)).collect(),
        base_index,
        ticks,
    ) {
        shadows.push(DrawPiece::track(PieceGroup::Shadows, track));
    }
    drawn.push((key, stack));
}

#[allow(clippy::too_many_arguments)]
fn character_pieces(
    character: &game_state::Character,
    x: i32,
    y: i32,
    state: &GameState,
    sheets: &Sheets<'_>,
    shadows: &mut Vec<DrawPiece>,
    drawn: &mut Vec<Stack>,
) -> Result<()> {
    let count = characters::frame_count(character, sheets.graph, sheets.objects);
    if count == 0 {
        return Ok(());
    }
    // Кадр из записи персонажа; вне последовательности — первый, как sequence_frame
    let base = (character.frame as usize).min(count - 1);
    // Слои собираются по устойчивому ключу: предмет без картинки в части кадров
    // сдвинул бы позиции, и снаряжение разъехалось бы по кадрам
    let mut tracks: Vec<(LayerKey, Vec<LayerFrame>)> = Vec::new();
    let mut shadows_by_frame: Vec<LayerFrame> = Vec::with_capacity(count);
    let mut bottoms: Vec<Option<i32>> = Vec::with_capacity(count);
    for index in 0..count {
        let sprite = characters::compose(
            character,
            x,
            y,
            state,
            sheets.graph,
            sheets.objects,
            sheets.tables,
            Some(index),
        )?;
        // Нет картинки тела — нет и всего спрайта (как `compose` в Python), но
        // кадр остаётся: на его тике персонаж просто скрыт
        bottoms.push(
            sprite
                .as_ref()
                .and_then(|sprite| sprite.layers.first())
                .map(|layer| layer.y + layer.image.height as i32),
        );
        shadows_by_frame.push(
            sprite
                .as_ref()
                .and_then(|sprite| sprite.shadow.as_ref())
                .map(|shadow| {
                    (
                        TextureKey::SpriteShadow {
                            source: shadow.source,
                            image_id: shadow.image_id as u16,
                        },
                        shadow.x,
                        shadow.y,
                    )
                }),
        );
        for layer in sprite.iter().flat_map(|sprite| sprite.layers.iter()) {
            let frame = Some((
                TextureKey::SpriteImage {
                    source: layer.source,
                    image_id: layer.image_id as u16,
                    palette: layer.palette_offset as u32,
                },
                layer.x,
                layer.y,
            ));
            match tracks.iter_mut().find(|(key, _)| *key == layer.key) {
                Some((_, frames)) => frames.push(frame),
                None => {
                    let mut frames = vec![None; index];
                    frames.push(frame);
                    tracks.push((layer.key, frames));
                }
            }
        }
        pad_frames(&mut tracks, index);
    }
    // Сортировка по нижнему краю модели базового кадра (render.py); если в нём
    // тела нет — по первому кадру, где оно есть
    let Some(bottom) = bottoms
        .get(base)
        .copied()
        .flatten()
        .or_else(|| bottoms.iter().find_map(|bottom| *bottom))
    else {
        return Ok(());
    };
    let mut stack = Vec::new();
    for (_, frames) in tracks {
        if let Some(track) = layer_track(frames, base, ONE_TICK) {
            stack.push(DrawPiece::track(PieceGroup::Characters, track));
        }
    }
    if let Some(track) = layer_track(shadows_by_frame, base, ONE_TICK) {
        shadows.push(DrawPiece::track(PieceGroup::CharacterShadows, track));
    }
    drawn.push(((bottom, 1, character.slot), stack));
    Ok(())
}

fn light_pieces(input: &SceneInput<'_>, pieces: &mut Vec<DrawPiece>) {
    let Some(lights) = input.lights else { return };
    for (row, values) in lights.tiles.iter().enumerate() {
        for (column, &value) in values.iter().enumerate() {
            if value != 0 {
                let (x, y) = map::tile_position(column, row);
                pieces.push(DrawPiece::textured(
                    PieceGroup::Lights,
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
                        PieceGroup::Foot,
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
            PieceGroup::Markers,
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
        pieces.push(DrawPiece::solid(
            PieceGroup::Markers,
            color,
            left,
            top,
            width,
            3,
        ));
        pieces.push(DrawPiece::solid(
            PieceGroup::Markers,
            color,
            left,
            bottom - 2,
            width,
            3,
        ));
        pieces.push(DrawPiece::solid(
            PieceGroup::Markers,
            color,
            left,
            top,
            3,
            height,
        ));
        pieces.push(DrawPiece::solid(
            PieceGroup::Markers,
            color,
            right - 2,
            top,
            3,
            height,
        ));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn key(index: u8) -> TextureKey {
        TextureKey::Solid {
            color: [index, 0, 0, 255],
        }
    }

    fn picture(index: u8, x: i32, y: i32) -> LayerFrame {
        Some((key(index), x, y))
    }

    #[test]
    fn a_static_object_shows_the_frame_from_the_map() {
        // Кадров много, но frame_time = 0: объект не анимируется, и рисовать
        // надо записанный в карте кадр, а не первый
        let (indices, base) = frame_plan(8, 3, false);
        assert_eq!(indices, [3]);
        assert_eq!(base, 0);
    }

    #[test]
    fn an_animated_object_plays_every_frame() {
        let (indices, base) = frame_plan(4, 2, true);
        assert_eq!(indices, [0, 1, 2, 3]);
        assert_eq!(base, 2);
        // Кадр из данных может быть за пределами frame_count
        let (indices, base) = frame_plan(2, 7, true);
        assert_eq!(indices, [0, 1]);
        assert_eq!(base, 1);
    }

    #[test]
    fn base_frame_anchors_the_piece() {
        // Кусок стоит в углу базового кадра, а смещения кадров отсчитываются от
        // него: при выключенной анимации картинка там же, где её рисует render.py
        let frames = vec![picture(1, 100, 200), picture(2, 104, 196)];
        let track = layer_track(frames, 0, 1).unwrap();
        assert_eq!((track.x, track.y), (100, 200));
        assert_eq!(track.texture, Some(key(1)));
        let animation = track.animation.unwrap();
        assert_eq!(animation.base, 0);
        assert_eq!(
            animation
                .frames
                .iter()
                .map(|frame| (frame.dx, frame.dy))
                .collect::<Vec<_>>(),
            [(0, 0), (4, -4)]
        );
    }

    #[test]
    fn hidden_frame_stays_in_the_sequence() {
        // Слой есть не во всех кадрах: пропущенный кадр остаётся скрытым, и
        // тайминг следующих не сдвигается
        let frames = vec![picture(1, 10, 10), None, picture(3, 12, 10)];
        let track = layer_track(frames, 0, 2).unwrap();
        let animation = track.animation.unwrap();
        assert_eq!(animation.frames.len(), 3);
        assert_eq!(
            animation
                .frames
                .iter()
                .map(|frame| (frame.texture.is_some(), frame.ticks))
                .collect::<Vec<_>>(),
            [(true, 2), (false, 2), (true, 2)]
        );
        assert_eq!((animation.frames[1].dx, animation.frames[1].dy), (0, 0));
    }

    #[test]
    fn missing_base_falls_back_to_the_first_picture() {
        // В базовом кадре картинки нет: началом становится угол первого кадра,
        // где она есть, а сам кусок скрыт до своего кадра
        let frames = vec![None, picture(2, 30, 40), picture(3, 32, 40)];
        let track = layer_track(frames, 0, 1).unwrap();
        assert_eq!((track.x, track.y), (30, 40));
        assert_eq!(track.texture, None);
        let animation = track.animation.unwrap();
        assert_eq!(animation.base, 0);
        assert_eq!(animation.frames[0].texture, None);
        assert_eq!((animation.frames[1].dx, animation.frames[1].dy), (0, 0));
        assert_eq!((animation.frames[2].dx, animation.frames[2].dy), (2, 0));
    }

    #[test]
    fn one_frame_is_a_static_piece() {
        // Один кадр — анимировать нечего, и кусок не отличается от статичного
        let track = layer_track(vec![picture(1, 5, 6)], 0, 1).unwrap();
        assert!(track.animation.is_none());
        assert_eq!((track.x, track.y), (5, 6));
        assert_eq!(track.texture, Some(key(1)));
    }

    #[test]
    fn a_layer_without_pictures_is_not_a_piece() {
        assert!(layer_track(vec![None, None], 0, 1).is_none());
        assert!(layer_track(Vec::new(), 0, 1).is_none());
    }

    #[test]
    fn pad_frames_keeps_every_layer_the_same_length() {
        // Тело пропущено на индексах 1 и 2, снаряжение появилось только ко
        // второму кадру: длины последовательностей остаются равными, иначе
        // кадры съехали бы по таймингу
        let mut tracks = vec![
            (LayerKey::Body, vec![picture(1, 0, 0)]),
            (
                LayerKey::Gear { slot: 2 },
                vec![None, None, picture(2, 1, 0)],
            ),
        ];
        pad_frames(&mut tracks, 2);
        assert_eq!(tracks[0].1, vec![picture(1, 0, 0), None, None]);
        assert_eq!(tracks[1].1.len(), 3);
    }

    #[test]
    fn base_index_outside_the_track_clamps_to_the_last_frame() {
        // Кадр из данных может быть за пределами последовательности
        let frames = vec![picture(1, 0, 0), picture(2, 1, 0)];
        let track = layer_track(frames, 9, 1).unwrap();
        let animation = track.animation.unwrap();
        assert_eq!(animation.base, 1);
        assert_eq!(animation.frames[1].dx, 0);
    }
}
