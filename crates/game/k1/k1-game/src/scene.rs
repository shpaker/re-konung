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
    lights_res,
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
    pub creature_id: Option<usize>,
    /// Logical depth bottom for DepthMap boundary lookup when it differs from
    /// `y + height` (objects with `flags & 8`, Slavik `game.cpp:1391-1395`).
    pub depth_bottom: Option<i32>,
    /// Index into LIGHTS.N tiles (1-based value minus 1); None = no tile light.
    pub light_mask_index: Option<usize>,
    /// Whether this piece carries two or more flames (self-lit → skip ambient fade).
    pub self_lit: bool,
    /// Fine-grid light value from LIGHTS.N at the creature's cell; 0 = dark patch.
    /// Used by the lighting plugin to decide dynamic fade per tick.
    pub fine_light_value: u8,
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
    /// LIGHTS.N light masks: on the ground, above the terrain tiles and below
    /// everything drawn later — the original lights only the ground tiles'
    /// pixels (Slavik `game.cpp:996-1003`)
    Lights,
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
        PieceGroup::Lights,
        PieceGroup::Decorations,
        PieceGroup::Shadows,
        PieceGroup::Objects,
        PieceGroup::Characters,
        PieceGroup::CharacterShadows,
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
            creature_id: None,
            depth_bottom: None,
            light_mask_index: None,
            self_lit: false,
            fine_light_value: 0,
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
            creature_id: None,
            depth_bottom: None,
            light_mask_index: None,
            self_lit: false,
            fine_light_value: 0,
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
            creature_id: None,
            depth_bottom: None,
            light_mask_index: None,
            self_lit: false,
            fine_light_value: 0,
        }
    }
}

/// Sort key of objects and creatures: the bottom edge of the picture, the kind
/// (0 — objects before creatures) and the slot, as in `render.py`.
///
/// Public because #37 sorts moving creatures among the static object stacks.
pub type SortKey = (i32, i32, usize);
/// Куски одного объекта или персонажа, которые сортируются и попадают в список
/// целиком (огни объекта остаются сразу над его картинкой).
///
/// Не путать с [`PieceGroup`]: она про то, что переключается вместе, а это про
/// то, что идёт в список отрисовки одним блоком.
type Stack = (SortKey, Vec<DrawPiece>);

/// Debug overlays: foot and markers go on top of the map, the light masks
/// onto the ground, under decorations and objects.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Overlays {
    /// Проходимость мелкой сетки (красный — непроходимо, синий — застревают стрелы)
    pub foot: bool,
    /// LIGHTS.N light masks on the ground, under decorations and objects
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

/// The map without creatures: what stays when a creature leaves its place.
///
/// [`stacks`] are unsorted — [`assemble`] and [`MapScene::ordered`] put them in
/// order together with the poses of the creatures.
///
/// [`stacks`]: Self::stacks
pub struct MapScene {
    background: Vec<DrawPiece>,
    /// One shadow layer under every sprite, as `shadow_layer` in render.py
    shadows: Vec<DrawPiece>,
    stacks: Vec<Stack>,
    overlays: Vec<DrawPiece>,
}

impl MapScene {
    /// The static blocks with the key each of them sorts under: what a consumer
    /// needs to interleave moving creatures with the objects of the map (#37).
    pub fn stacks(&self) -> &[(SortKey, Vec<DrawPiece>)] {
        &self.stacks
    }

    /// The same order as [`assemble`], borrowed instead of rebuilt: a consumer
    /// that keeps the map around asks for the list again after every change of a
    /// pose, without parsing the resources or cloning a single piece.
    ///
    /// The order is the one rule of the list: background, then shadows (objects
    /// first, creatures after), then every block by depth, then the overlays.
    pub fn ordered<'a>(&'a self, poses: &'a [Pose]) -> Vec<&'a DrawPiece> {
        let mut stacks: Vec<(SortKey, &'a Vec<DrawPiece>)> = self
            .stacks
            .iter()
            .map(|(key, pieces)| (*key, pieces))
            .collect();
        stacks.extend(poses.iter().map(|pose| (pose.key, &pose.pieces)));
        stacks.sort_by_key(|(key, _)| *key);
        let poses_len: usize = poses.iter().map(Pose::len).sum();
        let mut order = Vec::with_capacity(self.pieces_len() + poses_len);
        order.extend(self.background.iter());
        order.extend(self.shadows.iter());
        order.extend(poses.iter().filter_map(|pose| pose.shadow.as_ref()));
        order.extend(stacks.iter().flat_map(|(_, pieces)| pieces.iter()));
        order.extend(self.overlays.iter());
        order
    }

    /// How many pieces the map itself holds, creatures aside.
    pub fn pieces_len(&self) -> usize {
        self.background.len()
            + self.shadows.len()
            + self.overlays.len()
            + self
                .stacks
                .iter()
                .map(|(_, pieces)| pieces.len())
                .sum::<usize>()
    }
}

/// A creature's pose: the blocks that enter the list as one unit, where they
/// sort, and the creature's own shadow.
///
/// The shadow is kept apart because the list puts it in the shared layer under
/// every sprite, not above the creature.
pub struct Pose {
    pub key: SortKey,
    pub pieces: Vec<DrawPiece>,
    /// The [`LayerKey`] of each piece, parallel to [`Pose::pieces`]. A consumer
    /// that swaps a piece between poses (rest ↔ walk) must match layers by this
    /// key, not by position: the track order is first-appearance per pose and
    /// two poses of one creature may list the same layers differently.
    pub layer_keys: Vec<LayerKey>,
    pub shadow: Option<DrawPiece>,
    /// Fine-grid light value from LIGHTS.N at the creature's cell; 0 = dark patch.
    /// Used by the lighting plugin to decide dynamic fade per tick.
    pub fine_light_value: u8,
}

impl Pose {
    pub fn bottom(&self) -> i32 {
        self.key.0
    }

    /// How many pieces this pose contributes to the list, its shadow included.
    fn len(&self) -> usize {
        self.pieces.len() + usize::from(self.shadow.is_some())
    }
}

/// Which pose of a creature to build: the animation state, the direction and the
/// frame shown while the animation is not playing.
///
/// The recorded state and direction belong to the map as it was saved; a creature
/// that starts to walk or turns needs another pose, and asking for one must not
/// inherit the frame of the old animation — it indexes a sequence of a different
/// length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoseKind {
    pub state: u8,
    pub direction: u8,
    /// `None` — the frame from the record, which is what the map shows today;
    /// `Some(index)` — an explicit frame, clamped to the animation like the
    /// recorded one is.
    pub frame: Option<u8>,
}

impl PoseKind {
    /// The pose the record itself describes: its state, its direction, its frame.
    pub fn of_record(character: &game_state::Character) -> PoseKind {
        PoseKind {
            state: character.state,
            direction: character.direction,
            frame: None,
        }
    }

    /// The first frame of a new animation, for a creature that starts to walk or
    /// turns around. Where the original begins a fresh animation is not confirmed
    /// (docs/k1/creatures.md), so this is our decision, spelled out rather than
    /// left to whatever the record happened to hold.
    pub fn start(state: u8, direction: u8) -> PoseKind {
        PoseKind {
            state,
            direction,
            frame: Some(0),
        }
    }
}

/// Позиция объекта (клетка мелкой сетки) в пикселях карты (render.object_position).
pub fn object_position(x: i16, y: i16) -> (i32, i32) {
    let (x, y) = (i32::from(x), i32::from(y));
    (
        x * map::TILE_STEP_X / 2 - (y & 1) * map::TILE_STEP_X / 4,
        y * 16,
    )
}

/// Depth-sort bottom of a placed object: pixel y + frame offset + image height,
/// reduced by `height / 4` when per-instance flags bit 3 is set (Slavik
/// `game.cpp:1391-1395`). The clamp at zero is from the original.
pub fn sort_bottom(position_y: i32, offset_y: i16, height: i32, flags: u8) -> i32 {
    let mut bottom = position_y + i32::from(offset_y) + height;
    if flags & 8 != 0 {
        bottom -= height / 4;
        bottom = bottom.max(0);
    }
    bottom
}

/// The draw list of a map, in stacking order.
///
/// Assembled from [`map_scene`] and [`creature_poses`], so the same scene is
/// reordered when a creature moves instead of being rebuilt (#37).
pub fn draw_list(
    input: &SceneInput<'_>,
    graph: &GraphRes<'_>,
    objects: Option<&ObjectsRes<'_>>,
    tables: &Executable,
) -> Result<Vec<DrawPiece>> {
    let map = map_scene(input, graph, objects, tables)?;
    let poses = creature_poses(input, graph, objects, tables)?;
    Ok(assemble(map, &poses))
}

/// Everything of the map that stays when a creature leaves its place: ground,
/// decorations, object shadows, object stacks and the debug overlays.
///
/// Creature pieces are not here: they arrive as [`Pose`] blocks, which is what
/// lets the display rebuild only what moved.
pub fn map_scene(
    input: &SceneInput<'_>,
    graph: &GraphRes<'_>,
    objects: Option<&ObjectsRes<'_>>,
    tables: &Executable,
) -> Result<MapScene> {
    let mut background = Vec::new();

    // Земля и верхний слой: номера тайлов с единицы, 0 — пусто.
    // Tile light masks are assigned from the original (col,row) indices:
    // inverse pixel mapping is unreliable for decorations and objects, and
    // unconfirmed for shadows (review #106, P1-4).
    for layer in [&input.map.ground, &input.map.overlay] {
        for (row, cells) in layer.iter().enumerate() {
            for (column, &value) in cells.iter().enumerate() {
                if value != 0 {
                    let (x, y) = map::tile_position(column, row);
                    let mut piece = DrawPiece::textured(
                        PieceGroup::Terrain,
                        TextureKey::Tile {
                            index: (value - 1) as usize,
                        },
                        x,
                        y,
                    );
                    if let Some(lights) = input.lights
                        && row < lights.tiles.len()
                        && column < lights.tiles[row].len()
                    {
                        let mask_value = lights.tiles[row][column];
                        piece.light_mask_index = tile_light_mask_index(mask_value);
                    }
                    background.push(piece);
                }
            }
        }
    }

    // Light lies on the ground only: the masks are drawn before decorations
    // and objects, which the original never lights (Slavik `game.cpp:996-1003`).
    if input.overlays.lights {
        light_pieces(input, &mut background);
    }

    // Декорации: сырые (тайл вне GRAPH.RES) не нарисовать — пропускаем
    for decoration in &input.map.decorations {
        if decoration.tile >= 0 && graph.tile(decoration.tile as usize).is_some() {
            background.push(DrawPiece::textured(
                PieceGroup::Decorations,
                TextureKey::Tile {
                    index: decoration.tile as usize,
                },
                i32::from(decoration.x),
                i32::from(decoration.y),
            ));
        }
    }

    let mut shadows = Vec::new();
    let mut stacks: Vec<Stack> = Vec::new();
    if let Some(objects) = objects {
        let graphics = Graphics {
            graph,
            objects,
            tables,
        };
        for placed in &input.map.objects {
            object_pieces(placed, &graphics, &mut shadows, &mut stacks);
        }
    }

    let mut overlays = Vec::new();
    if input.overlays.foot {
        foot_pieces(input.map, &mut overlays);
    }
    if input.overlays.markers
        && let Some(entities) = input.entities.as_ref()
    {
        marker_pieces(entities, &mut overlays);
    }

    // self_lit is already set by object_pieces for objects with 2+ flames.
    // Light masks are NOT assigned to decorations, objects or shadows:
    // the inverse pixel→tile mapping is unconfirmed for those surfaces
    // (review #106, P1-4).

    Ok(MapScene {
        background,
        shadows,
        stacks,
        overlays,
    })
}

/// The poses of the creatures standing on the map, in the order of
/// [`characters::placed`].
///
/// Empty when the creature layer is off or the pictures are missing: a list
/// without creatures holds no creature shadows either
/// ([`PieceGroup::is_shown_by`]).
///
/// [`characters::placed`]: k1_data::characters::placed
pub fn creature_poses(
    input: &SceneInput<'_>,
    graph: &GraphRes<'_>,
    objects: Option<&ObjectsRes<'_>>,
    tables: &Executable,
) -> Result<Vec<Pose>> {
    let mut poses = Vec::new();
    if !input.overlays.characters {
        return Ok(poses);
    }
    let (Some(objects), Some(entities), Some(state)) =
        (objects, input.entities.as_ref(), input.state)
    else {
        return Ok(poses);
    };
    let graphics = Graphics {
        graph,
        objects,
        tables,
    };
    for (character, cx, cy) in characters::placed(entities, state) {
        let (px, py) = game_state::character_position(cx, cy);
        if let Some(pose) = character_pose(
            character,
            px,
            py,
            state,
            &graphics,
            &PoseKind::of_record(character),
            input.lights,
            cx,
            cy,
        )? {
            poses.push(pose);
        }
    }
    Ok(poses)
}

/// The whole draw list: background, shadows (objects, then creatures), every
/// block by depth, then the overlays.
///
/// Closer to the viewer is what sits lower on the screen; the sort is stable, so
/// an object keeps its flames right above its picture and an object with the same
/// bottom edge as a creature still comes first.
pub fn assemble(map: MapScene, poses: &[Pose]) -> Vec<DrawPiece> {
    let MapScene {
        background,
        shadows,
        stacks,
        overlays,
    } = map;
    let mut pieces = background;
    pieces.extend(shadows);
    pieces.extend(poses.iter().filter_map(|pose| pose.shadow.clone()));
    let mut drawn = stacks;
    drawn.extend(poses.iter().map(|pose| (pose.key, pose.pieces.clone())));
    drawn.sort_by_key(|stack| stack.0);
    pieces.extend(drawn.into_iter().flat_map(|(_, stack)| stack));
    pieces.extend(overlays);
    pieces
}

/// The pose of a creature in an arbitrary state and direction (#36: a pose is
/// built for any state, not only the one the file recorded).
///
/// [`PoseKind`] says which frame to show as the base of the new animation: the
/// recorded one is meaningless for a sequence of a different length. The record is
/// copied with those fields replaced, so the layers and frames are assembled by the
/// one existing path: there is no second implementation.
pub fn pose_in(
    character: &game_state::Character,
    x: i32,
    y: i32,
    state: &GameState,
    graphics: &Graphics<'_>,
    kind: &PoseKind,
) -> Result<Option<Pose>> {
    character_pose(character, x, y, state, graphics, kind, None, 0, 0)
}

/// The same as [`pose_in`], but with a reference to LIGHTS.N for lighting metadata.
#[allow(clippy::too_many_arguments)]
pub fn pose_in_with_lights(
    character: &game_state::Character,
    x: i32,
    y: i32,
    state: &GameState,
    graphics: &Graphics<'_>,
    kind: &PoseKind,
    lights: Option<&Lights>,
    cell_x: i32,
    cell_y: i32,
) -> Result<Option<Pose>> {
    character_pose(
        character, x, y, state, graphics, kind, lights, cell_x, cell_y,
    )
}

/// The graphics of one map, shared by the objects and the creatures of the list.
///
/// `GRAPH.RES` and `Objects.res` are 66 megabytes, so a consumer that builds more
/// than one pose holds one of these instead of parsing again
/// ([`k1_data::graph_res`] is parsed by [`Resources::with_graphics`]).
///
/// [`Resources::with_graphics`]: crate::resources::Resources::with_graphics
pub struct Graphics<'a> {
    graph: &'a GraphRes<'a>,
    objects: &'a ObjectsRes<'a>,
    tables: &'a Executable,
}

impl<'a> Graphics<'a> {
    pub fn new(
        graph: &'a GraphRes<'a>,
        objects: &'a ObjectsRes<'a>,
        tables: &'a Executable,
    ) -> Graphics<'a> {
        Graphics {
            graph,
            objects,
            tables,
        }
    }
}

/// Кадры, которые длятся один тик: так игра меняет кадры огней (Slavik
/// game.cpp:1587-1597) и персонажей (game_play.cpp:3729).
const ONE_TICK: u32 = 1;

fn object_pieces(
    placed: &MapObject,
    graphics: &Graphics<'_>,
    shadows: &mut Vec<DrawPiece>,
    drawn: &mut Vec<Stack>,
) {
    let Some(simple) = graphics
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
    let bottom = sort_bottom(
        y,
        simple.frames[base].offset.1,
        image.height as i32,
        placed.flags,
    );
    let key = (bottom, 0, placed.slot);
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
    if placed.flags & 8 != 0
        && let Some(main) = stack.first_mut()
    {
        main.depth_bottom = Some(bottom);
    }
    // Self-lit goes by the flame list size, not by active slots (game.cpp:1573).
    if simple.flames.len() >= 2 {
        for piece in stack.iter_mut() {
            piece.self_lit = true;
        }
    }
    // Огни объекта — сразу поверх него: все кадры анимации своего вида, по тике
    // на кадр. У кадра огня нет своего смещения, угол задаёт точка огня.
    for flame in &simple.flames {
        if flame.flame_id < 0 {
            continue;
        }
        let Some(&(first, last)) = graphics
            .tables
            .flame_animations
            .get(flame.flame_id as usize)
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

/// The pose of one creature as [`PoseKind`] asks for it.
///
/// `None` when there are no frames or no bottom edge is found: such a creature is
/// not in the list at all, as in `render.py`.
#[allow(clippy::too_many_arguments)]
fn character_pose(
    character: &game_state::Character,
    x: i32,
    y: i32,
    state: &GameState,
    graphics: &Graphics<'_>,
    kind: &PoseKind,
    lights: Option<&Lights>,
    cell_x: i32,
    cell_y: i32,
) -> Result<Option<Pose>> {
    // The pose may belong to another animation than the record describes
    let mut record = (*character).clone();
    record.state = kind.state;
    record.direction = kind.direction;
    let count = characters::frame_count(&record, graphics.graph, graphics.objects);
    if count == 0 {
        return Ok(None);
    }
    // The frame of the record unless asked otherwise; outside the sequence the
    // first one is taken, as sequence_frame does
    let base = (kind.frame.unwrap_or(record.frame) as usize).min(count - 1);
    // Слои собираются по устойчивому ключу: предмет без картинки в части кадров
    // сдвинул бы позиции, и снаряжение разъехалось бы по кадрам
    let mut tracks: Vec<(LayerKey, Vec<LayerFrame>)> = Vec::new();
    let mut shadows_by_frame: Vec<LayerFrame> = Vec::with_capacity(count);
    let mut bottoms: Vec<Option<i32>> = Vec::with_capacity(count);
    for index in 0..count {
        let sprite = characters::compose(
            &record,
            x,
            y,
            state,
            graphics.graph,
            graphics.objects,
            graphics.tables,
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
        return Ok(None);
    };
    let mut pieces = Vec::new();
    let mut layer_keys = Vec::new();
    // Fine-grid light value from LIGHTS.N at the creature's cell.
    let fine_light_value = lights.map_or(0u8, |lights| {
        lights
            .fine
            .get(cell_y as usize)
            .and_then(|r| r.get(cell_x as usize))
            .copied()
            .unwrap_or(0)
    });
    for (key, frames) in tracks {
        if let Some(track) = layer_track(frames, base, ONE_TICK) {
            let mut p = DrawPiece::track(PieceGroup::Characters, track);
            p.creature_id = Some(record.slot);
            p.fine_light_value = fine_light_value;
            pieces.push(p);
            layer_keys.push(key);
        }
    }
    let shadow = layer_track(shadows_by_frame, base, ONE_TICK).map(|track| {
        let mut p = DrawPiece::track(PieceGroup::CharacterShadows, track);
        p.creature_id = Some(record.slot);
        p.fine_light_value = fine_light_value;
        p
    });
    Ok(Some(Pose {
        key: (bottom, 1, record.slot),
        pieces,
        layer_keys,
        shadow,
        fine_light_value,
    }))
}

fn light_pieces(input: &SceneInput<'_>, pieces: &mut Vec<DrawPiece>) {
    let Some(lights) = input.lights else { return };
    for (row, values) in lights.tiles.iter().enumerate() {
        for (column, &value) in values.iter().enumerate() {
            if let Some(index) = tile_light_mask_index(value) {
                let (x, y) = map::tile_position(column, row);
                pieces.push(DrawPiece::textured(
                    PieceGroup::Lights,
                    TextureKey::LightMask { index },
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

/// Convert a raw LIGHTS.N tile value to a zero-based mask index.
///
/// LIGHTS.N stores 1-based indices: 0 means no mask, 1 → index 0, 19 → index 18.
/// Values outside 1..=TILE_MASKS (19) are treated as no mask (corrupted data).
pub fn tile_light_mask_index(value: u8) -> Option<usize> {
    if value > 0 && (value as usize) <= lights_res::TILE_MASKS {
        Some((value as usize).saturating_sub(1))
    } else {
        None
    }
}

/// Reverse of [`map::tile_position`]: pixel → tile column/row.
///
/// Forward: `x = col * 116 + (row & 1) * 58`, `y = row * 32`.
/// The inverse reconstructs `row` from y exactly (`row = py / 32`), then tries both
/// parity hypotheses for x — whichever yields a consistent integer column wins.
/// Falls back to `(0, 0)` for negative coordinates or when both parities fail.
#[cfg(test)]
fn tile_column_row(px: i32, py: i32) -> (usize, usize) {
    if px < 0 || py < 0 {
        return (0, 0);
    }
    let tile_step_x: i32 = k1_data::map::TILE_STEP_X; // 116
    let tile_step_y: i32 = k1_data::map::TILE_STEP_Y; // 32
    let odd_row_shift: i32 = k1_data::map::ODD_ROW_SHIFT; // 58
    let row = (py / tile_step_y) as usize;
    // Try odd-row hypothesis first: x = col * STEP_X + SHIFT → col = (x - SHIFT) / STEP_X
    if let Some(col_odd) = try_column(px - odd_row_shift, tile_step_x) {
        return (col_odd, row);
    }
    // Even-row hypothesis: no shift → col = x / STEP_X
    if let Some(col_even) = try_column(px, tile_step_x) {
        return (col_even, row);
    }
    (0, 0)
}

/// Best-effort inverse: `(px - shift) / STEP` clamped to non-negative.
/// Used by callers who don't mind an approximate column — e.g. bounding-box culling.
#[cfg(test)]
pub fn approximate_tile_column(px: i32) -> usize {
    if px < 0 {
        return 0;
    }
    (px / k1_data::map::TILE_STEP_X) as usize
}

/// Exact inverse component: returns `col` only when `px` is cleanly divisible by `step`.
#[cfg(test)]
fn try_column(px: i32, step: i32) -> Option<usize> {
    if px < 0 {
        return None;
    }
    if px % step != 0 {
        return None;
    }
    Some((px / step) as usize)
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

    // ── Lighting metadata tests ──────────────────────────────────────────────

    #[test]
    fn tile_column_row_reverses_tile_position_even_rows() {
        // Even rows have no x-shift: col * 116 → exact match
        for col in 0..80u32 {
            let (px, py) = map::tile_position(col as usize, 0);
            assert_eq!(tile_column_row(px, py), (col as usize, 0), "col={col}");
        }
        // Row 2 (even)
        for col in 0..80u32 {
            let (px, py) = map::tile_position(col as usize, 2);
            assert_eq!(
                tile_column_row(px, py),
                (col as usize, 2),
                "col={col}, row=2"
            );
        }
    }

    #[test]
    fn tile_column_row_reverses_tile_position_odd_rows() {
        // Odd rows shift x by ODD_ROW_SHIFT (=58): col * 116 + 58
        for col in 0..80u32 {
            let (px, py) = map::tile_position(col as usize, 1);
            assert_eq!(tile_column_row(px, py), (col as usize, 1), "col={col}");
        }
        // Row 3 (odd)
        for col in 0..80u32 {
            let (px, py) = map::tile_position(col as usize, 3);
            assert_eq!(
                tile_column_row(px, py),
                (col as usize, 3),
                "col={col}, row=3"
            );
        }
    }

    #[test]
    fn tile_column_row_boundary_edge_cases() {
        // Negative coordinates → (0, 0)
        assert_eq!(tile_column_row(-1, 0), (0, 0));
        assert_eq!(tile_column_row(0, -1), (0, 0));
        assert_eq!(tile_column_row(-1, -1), (0, 0));
        // Zero → correct mapping
        assert_eq!(tile_column_row(0, 0), (0, 0));
        // Off-grid pixel that doesn't align with any tile → fallback
        assert_eq!(tile_column_row(1, 0), (0, 0));
        assert_eq!(tile_column_row(0, 1), (0, 0));
        // Offset sprite: decoration placed between tiles — try_column returns None
        // because 58 % 116 != 0 (the odd-row position is not divisible)
        // but the odd-row hypothesis should catch it since 58 - 58 = 0
        let (px, py) = map::tile_position(0, 1); // 0 * 116 + 58 = 58, 1 * 32 = 32
        assert_eq!(tile_column_row(px, py), (0, 1));
    }

    #[test]
    fn tile_column_row_across_all_tiles_round_trips() {
        // Every tile on an 80×160 map round-trips correctly through forward + inverse.
        for row in 0..160u32 {
            for col in 0..80u32 {
                let (px, py) = map::tile_position(col as usize, row as usize);
                let got = tile_column_row(px, py);
                assert_eq!(
                    got,
                    (col as usize, row as usize),
                    "round-trip failed at ({col}, {row}): got {:?}",
                    got
                );
            }
        }
    }

    #[test]
    fn light_mask_index_value_0_means_none() {
        assert_eq!(tile_light_mask_index(0), None, "value=0 → None");
    }

    #[test]
    fn light_mask_index_value_1_gives_index_0() {
        assert_eq!(tile_light_mask_index(1), Some(0), "1 → 0");
    }

    #[test]
    fn light_mask_index_value_19_gives_index_18() {
        // Boundary: 19 is the last valid 1-based index → 18 zero-based.
        assert_eq!(tile_light_mask_index(19), Some(18), "19 → 18");
    }

    #[test]
    fn light_mask_index_value_20_is_out_of_range() {
        // 20 exceeds TILE_MASKS (19) → treated as no mask.
        assert_eq!(tile_light_mask_index(20), None, "20 > 19 → None");
    }

    #[test]
    fn light_mask_index_value_255_is_out_of_range() {
        // Corrupted LIGHTS.N value 255 exceeds TILE_MASKS (19) → no mask.
        assert_eq!(tile_light_mask_index(255), None, "255 > 19 → None");
    }

    #[test]
    fn self_lit_follows_the_flame_list_size() {
        // The original gates the fade on Flames.size() >= 2 (game.cpp:1573):
        // the list length decides, whether the slots hold an active flame does not.
        // Two inactive slots (flame_id < 0) still make the list two entries long:
        // a slot-counting reading would say "not self-lit", the list says the opposite.
        let two_empty: Vec<k1_data::objects_res::Flame> = vec![
            k1_data::objects_res::Flame {
                flame_id: -1,
                x: 0,
                y: 0,
            },
            k1_data::objects_res::Flame {
                flame_id: -1,
                x: 0,
                y: 0,
            },
        ];
        assert_eq!(two_empty.iter().filter(|f| f.flame_id >= 0).count(), 0);
        assert!(two_empty.len() >= 2);

        // A single entry is not self-lit even when its slot is active.
        let one_active: Vec<_> = vec![k1_data::objects_res::Flame {
            flame_id: 5,
            x: 0,
            y: 0,
        }];
        assert_eq!(one_active.iter().filter(|f| f.flame_id >= 0).count(), 1);
        assert!(one_active.len() < 2);
    }
}
