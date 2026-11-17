//! Scene model, texture decoding and K1 pose conversion.

use serde::Serialize;

use k1_data::{
    characters::WALK_ANIMATION,
    game_state::{self, GameState},
    graph_res::DIRECTIONS,
    map,
};
use k1_game::presentation::scene::SceneInput;

use crate::{
    Game, GameError, LayerKey, MapRef, PieceGroup, PoseKind, Result, Rgba, TextureError, TextureKey,
};

/// Ключ текстуры сцены: для потребителя непрозрачен, рендер кэширует по нему.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureId(pub(crate) TextureKey);

impl TextureId {
    /// Собирает id из сырого ключа. Сцены получают ключи из данных, а не
    /// руками; путь нужен тестам рендера и диагностическим инструментам.
    pub fn from_key(key: TextureKey) -> Self {
        Self(key)
    }

    /// Whether a texture can contribute visible creature body pixels.
    pub fn is_creature_image(self) -> bool {
        matches!(self.0, TextureKey::SpriteImage { .. })
    }
}

/// Кусок сцены: textured quad в пикселях карты.
/// `w`/`h` — None: размер текстуры; Some: растянуть.
/// `texture` — None: в базовом кадре картинки нет (слой есть не во всех кадрах
/// анимации); кусок скрыт, пока часы не придут к его кадру.
/// `group` — что переключает видимость куска: слой отображения заводит на группу
/// отдельный узел, и куски не пересобираются (см. [`group_is_shown`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub texture: Option<TextureId>,
    pub x: i32,
    pub y: i32,
    pub w: Option<i32>,
    pub h: Option<i32>,
    pub animation: Option<Animation>,
    pub group: PieceGroup,
    pub creature_id: Option<usize>,
    pub depth_bottom: Option<i32>,
    /// Index into LIGHTS.N tiles (1-based value minus 1); None = no tile light.
    pub light_mask_index: Option<usize>,
    /// Whether this piece carries two or more flames (self-lit → skip ambient fade).
    pub self_lit: bool,
    /// Fine-grid light value from LIGHTS.N at the creature's cell; 0 = dark patch.
    /// Used by the lighting plugin to decide dynamic fade per tick.
    pub fine_light_value: u8,
}

impl Piece {
    /// Whether this piece is an object's fire: flames are drawn together with
    /// the object they belong to and are no standing objects of their own —
    /// depth orderers must treat them as part of the object, not as extra
    /// stacks (review #68).
    pub fn is_flame(&self) -> bool {
        let flame = |id: &TextureId| matches!(id.0, TextureKey::FlameFrame { .. });
        self.texture.as_ref().is_some_and(flame)
            || self.animation.as_ref().is_some_and(|animation| {
                animation
                    .frames
                    .iter()
                    .any(|frame| frame.texture.as_ref().is_some_and(flame))
            })
    }
}

/// Показываются ли куски группы при этих опциях.
///
/// Порядок наложения задаёт позиция куска в [`Scene::pieces`], поэтому видимость
/// группы ничего в ней не переставляет: фильтр по этой функции и сборка карты
/// только с включёнными слоями дают одну и ту же картину — держит
/// `k1-game/tests/original_scene.rs`, а что группа доходит до модели —
/// `tests/original_facade.rs`.
pub fn group_is_shown(group: PieceGroup, options: &SceneOptions) -> bool {
    group.is_shown_by(&overlays(options))
}

/// Кадры одного слоя, которые показываются по очереди.
///
/// Длительности — в тиках игры ([`crate::TICK_MS`] миллисекунд), а не в секундах: часы
/// остаются целыми, шаг покадрового просмотра равен тику, а куски с одинаковой
/// фазой идут в такт сами. Слои одного объекта или персонажа получают одинаковые
/// длительности и тот же `base`, поэтому и синхронны.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Animation {
    pub frames: Vec<AnimationFrame>,
    /// Кадр из данных: его показывают при выключенной анимации, и именно по нему
    /// считают порядок наложения
    pub base: usize,
}

/// Один кадр анимации: картинка (`None` — в этом кадре слоя не видно), смещение
/// её угла от угла куска и длительность в тиках.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationFrame {
    pub texture: Option<TextureId>,
    pub dx: i32,
    pub dy: i32,
    pub ticks: u32,
}

/// Список отрисовки карты: порядок = порядок наложения (painter's algorithm).
#[derive(Debug, Clone, Default)]
pub struct Scene {
    pub pieces: Vec<Piece>,
    /// Resting geometry for every creature the map's roster loads, keyed by
    /// `Piece::creature_id` — the set the creature renderer builds nodes from.
    ///
    /// Kept apart from [`Scene::pieces`] on purpose: `pieces` is the
    /// digest-guarded source draw list, assembled once through
    /// `k1_game::presentation::scene::draw_list` over [`k1_data::characters::placed`], which
    /// under-emits a roaming squad (only its first member). This collection
    /// instead covers every loaded member — roaming 2..n included — by posing
    /// each roster record at its own cell through the same
    /// `character_pose`/`compose` path, so the source list and its digest are
    /// untouched and the nodes no longer drop the members the static list
    /// omits (review #101, P2-1). Empty unless the characters layer is on.
    pub creature_pieces: Vec<Piece>,
    /// The Scene composition origin of each creature's geometry — the cell whose
    /// absolute pixel [`creature_geometry`] composed its resting pieces around —
    /// keyed by `Piece::creature_id` (the record slot, the same id the renderer
    /// builds nodes from).
    ///
    /// The creature renderer reduces every piece offset and animation anchor
    /// against this base, never against the simulation's spawn pixel, so a
    /// sprite's local geometry stays fixed to the creature while the node's
    /// world position comes solely from the simulation — which diverges from the
    /// Scene origin once `place_roaming` re-parks roaming squad members at load
    /// (review #101, P2-2). For a static creature the base equals its simulation
    /// origin, so the reduction is the identity and output is unchanged. A side
    /// table on the facade [`Scene`], never in the digest-guarded `pieces`.
    pub creature_origins: std::collections::HashMap<usize, (i32, i32)>,
}

impl Scene {
    /// Уникальные ключи текстур в порядке первого появления: базовый кадр куска
    /// и все кадры его анимации.
    ///
    /// Кадры нужны целиком: не докачанный кадр означал бы пустой спрайт в
    /// середине цикла, а `check` не увидел бы, что он не декодируется.
    pub fn texture_ids(&self) -> Vec<TextureId> {
        let mut seen = std::collections::HashSet::new();
        let mut ids = Vec::new();
        for piece in &self.pieces {
            let frames = piece
                .animation
                .iter()
                .flat_map(|animation| animation.frames.iter())
                .filter_map(|frame| frame.texture.as_ref());
            for texture in piece.texture.iter().chain(frames) {
                if seen.insert(*texture) {
                    ids.push(*texture);
                }
            }
        }
        ids
    }

    /// Статистика анимаций для диагностики.
    pub fn animation_stats(&self) -> AnimationStats {
        let mut stats = AnimationStats::default();
        for piece in &self.pieces {
            // Видимым кусок считает базовый кадр: без картинки в нём спрайт скрыт
            if piece.texture.is_some() {
                stats.visible += 1;
            }
            if let Some(animation) = &piece.animation {
                stats.pieces += 1;
                stats.frames += animation.frames.len();
            }
        }
        stats
    }
}

/// Анимационная статистика сцены: сколько кусков анимируется, сколько в них
/// кадров и сколько кусков видно в базовом кадре.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct AnimationStats {
    pub pieces: usize,
    pub frames: usize,
    pub visible: usize,
}

/// Что добавить к чистой карте.
#[derive(Debug, Clone, Copy, Default)]
pub struct SceneOptions {
    pub foot: bool,
    pub lights: bool,
    pub markers: bool,
    pub characters: bool,
    /// Номер `Game.N`; None — первый доступный (нужен для маркеров и персонажей)
    pub hero: Option<u32>,
}

impl SceneOptions {
    /// Все отладочные слои разом.
    ///
    /// С ними карта собирается один раз со всеми кусками, а переключение слоя
    /// стоит одну запись видимости группы вместо пересборки тысяч спрайтов
    /// (см. [`group_is_shown`]).
    pub const ALL: SceneOptions = SceneOptions {
        foot: true,
        lights: true,
        markers: true,
        characters: true,
        hero: None,
    };
}

/// Флаги оверлеев для слоя K1: те же четыре переключателя без номера героя.
fn overlays(options: &SceneOptions) -> k1_game::presentation::scene::Overlays {
    k1_game::presentation::scene::Overlays {
        foot: options.foot,
        lights: options.lights,
        markers: options.markers,
        characters: options.characters,
    }
}

/// Результат декодирования одного ключа текстуры.
///
/// `Ok(Some)` — картинка есть, `Ok(None)` — слот пуст по данным, `Err` — данные
/// битые и известна причина.
pub type DecodedTexture = std::result::Result<Option<Rgba>, TextureError>;

pub(crate) fn piece(from: k1_game::presentation::scene::DrawPiece) -> Piece {
    Piece {
        texture: from.texture.map(TextureId),
        x: from.x,
        y: from.y,
        w: from.w,
        h: from.h,
        animation: from.animation.map(animation),
        group: from.group,
        creature_id: from.creature_id,
        depth_bottom: from.depth_bottom,
        light_mask_index: from.light_mask_index,
        self_lit: from.self_lit,
        fine_light_value: from.fine_light_value,
    }
}

/// Кадры слоя из модели сцены: тот же пересчёт, что и в [`piece`], отделён для
/// [`Game::creature_animations`], которое строит позы вне общего списка.
fn animation(from: k1_game::presentation::scene::DrawAnimation) -> Animation {
    Animation {
        base: from.base,
        frames: from
            .frames
            .into_iter()
            .map(|frame| AnimationFrame {
                texture: frame.texture.map(TextureId),
                dx: frame.dx,
                dy: frame.dy,
                ticks: frame.ticks,
            })
            .collect(),
    }
}

/// One creature layer: its own static base and its animation together. `base`
/// is the layer's resting image (the piece texture), `animation` its frames —
/// `None` where the layer never moves. Carrying both from one source lets a
/// consumer build a stable creature node by the layer's position in this list
/// instead of joining animation to scene pieces by a global index (review #68).
///
/// `corner` is the layer's base-frame corner in absolute map pixels, and
/// `key` the sprite part it draws. A consumer swapping a layer between poses
/// must re-anchor on `corner` (every clip's frame offsets are relative to its
/// own base corner) and match layers across poses by `key`, never by position.
#[derive(Debug, Clone)]
pub struct CreatureLayer {
    pub base: Option<TextureId>,
    pub animation: Option<Animation>,
    pub corner: (i32, i32),
    pub key: LayerKey,
}

/// Clips of one creature's pieces: `layers` are the creature's body and
/// equipment in pose order, `shadow` its ground shadow. The list is self-
/// contained — every layer carries its own base and animation.
#[derive(Debug, Clone)]
pub struct CreatureClips {
    pub layers: Vec<CreatureLayer>,
    pub shadow: Option<CreatureLayer>,
}

/// What a creature shows resting, walking and — for the three dynamic classes
/// that record state 4 in `Game.N` — playing their one-shot appearance or
/// transformation animation (docs/research/k1/creatures/animation.md, "State 4 — two meanings
/// under one number", issue #70).
///
/// `walk[direction]` is the walk pose facing that sprite direction, `None`
/// where the creature has no walk frames. `special` is `None` for every
/// human (a human's state 4 is a shot, out of #70's scope) and for a dynamic
/// creature whose sprite carries no state-4 row.
#[derive(Debug, Clone)]
pub struct CreatureAnimation {
    pub rest: CreatureClips,
    pub walk: Vec<Option<CreatureClips>>,
    pub special: Option<CreatureClips>,
    /// Melee poses in the same eight sprite directions as `walk`.
    pub melee: Vec<Option<CreatureClips>>,
    /// Armed idle poses keep the facing selected by a melee action.
    pub armed_rest: Vec<Option<CreatureClips>>,
}

/// The pose's own clips, in the order the scene lists the creature's pieces:
/// body then equipment, the shadow kept apart.
fn clips_of(pose: k1_game::presentation::scene::Pose) -> CreatureClips {
    CreatureClips {
        layers: pose
            .pieces
            .into_iter()
            .zip(pose.layer_keys)
            .map(|(piece, key)| creature_layer(piece, key))
            .collect(),
        shadow: pose
            .shadow
            .map(|piece| creature_layer(piece, LayerKey::Shadow)),
    }
}

fn creature_layer(piece: k1_game::presentation::scene::DrawPiece, key: LayerKey) -> CreatureLayer {
    CreatureLayer {
        base: piece.texture.map(TextureId),
        animation: piece.animation.map(animation),
        corner: (piece.x, piece.y),
        key,
    }
}

/// The resting [`k1_game::presentation::scene::Pose`] a creature shows when neither walking nor playing a
/// special: its idle pair. A state-4 record rests on state 6 (a dynamic
/// creature's idle, docs/research/k1/creatures/animation.md) so its static base and clips match the
/// idle it returns to; every other record rests on the state its file carries.
///
/// Shared by [`Game::creature_animations`] and [`creature_geometry`] so the
/// node's static geometry and its resting clips come from one pose, never two
/// that could list the layers differently.
#[allow(clippy::too_many_arguments)]
fn creature_rest_pose(
    character: &game_state::Character,
    x: i32,
    y: i32,
    state: &GameState,
    graphics: &k1_game::presentation::scene::Graphics<'_>,
    lights: Option<&map::Lights>,
    cell_x: i32,
    cell_y: i32,
) -> Option<k1_game::presentation::scene::Pose> {
    let rest_state = if character.state == k1_game::creature_anim::SPECIAL_STATE {
        6
    } else {
        character.state
    };
    let kind = PoseKind {
        state: rest_state,
        direction: character.direction,
        frame: Some(0),
    };
    if let Some(lights) = lights {
        k1_game::presentation::scene::pose_in_with_lights(
            character,
            x,
            y,
            state,
            graphics,
            &kind,
            Some(lights),
            cell_x,
            cell_y,
        )
    } else {
        k1_game::presentation::scene::pose_in(character, x, y, state, graphics, &kind)
    }
    .ok()
    .flatten()
}

/// Resting geometry for every creature `map_number` loads, in roster order,
/// keyed by `Piece::creature_id`.
///
/// Driven by [`k1_game::creatures::roster`] — the set the simulation actually
/// loads — not by [`k1_data::characters::placed`], which under-emits a roaming
/// squad to its first member: each member is posed at its own record cell
/// through [`creature_rest_pose`], the same path the static scene uses, so the
/// whole squad gets node geometry while the digest-guarded `draw_list` stays
/// untouched (review #101, P2-1). A creature whose resting pose composes to
/// nothing contributes no piece, matching the renderer's "no pieces, no node"
/// policy.
fn creature_geometry(
    state: &GameState,
    map_number: usize,
    hero: Option<usize>,
    graphics: &k1_game::presentation::scene::Graphics<'_>,
    lights: Option<&map::Lights>,
) -> (Vec<Piece>, std::collections::HashMap<usize, (i32, i32)>) {
    let roster = k1_game::creatures::roster(state, map_number, hero);
    let mut pieces = Vec::new();
    let mut origins = std::collections::HashMap::new();
    for creature in &roster {
        let Some(character) = state.characters.get(creature.id).and_then(Option::as_ref) else {
            continue;
        };
        let (px, py) =
            game_state::character_position(i32::from(creature.cell.0), i32::from(creature.cell.1));
        let Some(pose) = creature_rest_pose(
            character,
            px,
            py,
            state,
            graphics,
            lights,
            i32::from(creature.cell.0),
            i32::from(creature.cell.1),
        ) else {
            continue;
        };
        let before = pieces.len();
        pieces.extend(pose.pieces.into_iter().map(piece));
        if let Some(shadow) = pose.shadow {
            pieces.push(piece(shadow));
        }
        // Keyed by the record slot — the `Piece::creature_id` the renderer joins
        // on, not the roster index — so a piece and its origin share one key. Only
        // creatures that contributed geometry are recorded, matching the
        // renderer's "no pieces, no node" policy.
        if pieces.len() > before {
            origins.insert(character.slot, (px, py));
        }
    }
    (pieces, origins)
}

/// The animation state a walking creature draws: the walk row of its sprite
/// table (docs/research/k1/movement.md), shared with [`k1_data::characters::walk_frame_counts`].
const WALK_STATE: u8 = WALK_ANIMATION as u8;

impl Game {
    /// Сцена карты в порядке наложения; текстуры декодируются отдельно
    /// ([Game::textures]) и кэшируются потребителем.
    pub fn scene(&self, map_ref: MapRef, options: &SceneOptions) -> Result<Scene> {
        let files = map_ref.source.file_set();
        let map_bytes = self.dir.map_bytes(map_ref.number, files)?;
        let konung_map = map::parse(&map_bytes)?;
        let lights_bytes = self.dir.lights_bytes(map_ref.number, files)?;
        let lights = lights_bytes
            .as_ref()
            .map(|bytes| map::parse_lights(bytes))
            .transpose()?;

        let hero = if options.markers || options.characters {
            self.hero_for(map_ref.source, options.hero)?
        } else {
            None
        };
        let state_bytes = match hero {
            Some(hero) => Some(self.dir.state_bytes(hero as u32, files)?),
            None => None,
        };
        let state = state_bytes
            .as_ref()
            .map(|bytes| self.session_or_original(bytes, map_ref.source, hero))
            .transpose()?;
        let entities = state
            .as_ref()
            .map(|state| game_state::on_map(state, map_ref.number as usize, hero));

        let input = SceneInput {
            map: &konung_map,
            lights: lights.as_ref(),
            entities,
            state: state.as_ref(),
            overlays: overlays(options),
        };
        let (pieces, creature_pieces, creature_origins) =
            self.resources.with_graphics(|graph, objects| {
                let draw = k1_game::presentation::scene::draw_list(
                    &input,
                    graph,
                    Some(objects),
                    &self.resources.tables,
                )?;
                // The roster-driven creature geometry rides along with the scene but
                // never enters `draw_list`, so the digest-guarded source list is
                // untouched; built only when the characters layer is on.
                let (creatures, origins) = match (options.characters, state.as_ref()) {
                    (true, Some(state)) => {
                        let graphics = k1_game::presentation::scene::Graphics::new(
                            graph,
                            objects,
                            &self.resources.tables,
                        );
                        creature_geometry(
                            state,
                            map_ref.number as usize,
                            hero,
                            &graphics,
                            lights.as_ref(),
                        )
                    }
                    _ => (Vec::new(), std::collections::HashMap::new()),
                };
                Ok::<_, GameError>((draw, creatures, origins))
            })??;
        Ok(Scene {
            pieces: pieces.into_iter().map(piece).collect(),
            creature_pieces,
            creature_origins,
        })
    }

    /// Idle and walk clips per creature for the first available hero's state.
    pub fn creature_animations(&self, map_ref: MapRef) -> Result<Vec<(usize, CreatureAnimation)>> {
        self.creature_animations_with_hero(map_ref, None)
    }

    /// The same, for a chosen hero: `hero` selects the `Game.N` the clips are
    /// decoded from, `None` the first. Keyed by `Piece::creature_id` (the
    /// character slot); the layer order matches the creature's Characters-group
    /// pieces in the scene built from the SAME hero, so a consumer can swap an
    /// [`Animator`] sequence by piece position when [`WalkState`] changes, and a
    /// caller must pass the hero its Scene used (review #101, finding P2-2).
    pub fn creature_animations_with_hero(
        &self,
        map_ref: MapRef,
        hero: Option<u32>,
    ) -> Result<Vec<(usize, CreatureAnimation)>> {
        let files = map_ref.source.file_set();
        let hero = self.hero_for(map_ref.source, hero)?;
        let state_bytes = self.dir.state_bytes(hero.unwrap_or(0) as u32, files)?;
        let state = self.session_or_original(&state_bytes, map_ref.source, hero)?;
        Ok(self.resources.with_graphics(|graph, objects| {
            let graphics =
                k1_game::presentation::scene::Graphics::new(graph, objects, &self.resources.tables);
            let roster = k1_game::creatures::roster(&state, map_ref.number as usize, hero);
            let mut result = Vec::new();
            // Every loaded roster creature, not `characters::placed`: a roaming
            // squad's members 2..n live in the roster and the simulation but are
            // omitted there, so the clips — and the nodes built from them — must
            // be driven by the roster to cover them (review #101, P2-1).
            for creature in &roster {
                let Some(character) = state.characters.get(creature.id).and_then(Option::as_ref)
                else {
                    continue;
                };
                let (px, py) = game_state::character_position(
                    i32::from(creature.cell.0),
                    i32::from(creature.cell.1),
                );
                // A state-4 record's resting clip is its idle pair (state 6 for
                // a dynamic creature, docs/research/k1/creatures/animation.md); the record's own
                // state 4 becomes the `special` clip the viewer swaps to while
                // the machine plays it. Every other record shows the state its
                // file carries.
                let is_special = character.state == k1_game::creature_anim::SPECIAL_STATE;
                let rest =
                    match creature_rest_pose(character, px, py, &state, &graphics, None, 0, 0) {
                        Some(pose) => clips_of(pose),
                        None => continue,
                    };
                let walk: Vec<Option<CreatureClips>> = (0..DIRECTIONS as u8)
                    .map(|direction| {
                        k1_game::presentation::scene::pose_in(
                            character,
                            px,
                            py,
                            &state,
                            &graphics,
                            &PoseKind::start(WALK_STATE, direction),
                        )
                        .ok()
                        .flatten()
                        .map(clips_of)
                    })
                    .collect();
                let armed_rest: Vec<Option<CreatureClips>> = (0..DIRECTIONS as u8)
                    .map(|direction| {
                        k1_game::presentation::scene::pose_in(
                            character,
                            px,
                            py,
                            &state,
                            &graphics,
                            &PoseKind::start(0, direction),
                        )
                        .ok()
                        .flatten()
                        .map(clips_of)
                    })
                    .collect();
                let melee: Vec<Option<CreatureClips>> = (0..DIRECTIONS as u8)
                    .map(|direction| {
                        k1_game::presentation::scene::pose_in(
                            character,
                            px,
                            py,
                            &state,
                            &graphics,
                            &PoseKind::start(k1_game::creature_anim::MELEE_STATE, direction),
                        )
                        .ok()
                        .flatten()
                        .map(clips_of)
                    })
                    .collect();
                // Only a state-4 record can have a special; a human's state 4 is
                // the shot, which is out of #70's scope and must not be routed
                // through the appearance clip (docs/research/k1/creatures/animation.md).
                let special = is_special
                    .then(|| {
                        k1_game::presentation::scene::pose_in(
                            character,
                            px,
                            py,
                            &state,
                            &graphics,
                            &PoseKind {
                                state: k1_game::creature_anim::SPECIAL_STATE,
                                direction: character.direction,
                                frame: Some(0),
                            },
                        )
                        .ok()
                        .flatten()
                        .map(clips_of)
                    })
                    .flatten();
                if walk.iter().any(Option::is_some)
                    || melee.iter().any(Option::is_some)
                    || special.is_some()
                {
                    result.push((
                        character.slot,
                        CreatureAnimation {
                            rest,
                            walk,
                            special,
                            melee,
                            armed_rest,
                        },
                    ));
                }
            }
            result
        })?)
    }

    /// RGBA-картинки ключей, позиционно параллельно `ids`.
    ///
    /// Наружу уходит и причина: потребитель решает, пропускать ли кусок, но
    /// молча терять картинку из-за битых данных нельзя. Внешняя `Err` — не
    /// разобрались ресурсы игры.
    pub fn textures(&self, ids: &[TextureId]) -> Result<Vec<DecodedTexture>> {
        Ok(self.resources.with_graphics(|graph, objects| {
            ids.iter()
                .map(|id| {
                    k1_game::presentation::textures::decode(
                        &id.0,
                        graph,
                        objects,
                        &self.resources.lights,
                        &self.resources.flames,
                    )
                })
                .collect()
        })?)
    }
}
