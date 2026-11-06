//! Куски сцены → спрайты Bevy.
//!
//! Модель сцены [`kn_game`] задаёт пиксельные координаты карты с началом в
//! левом верхнем углу и осью y вниз; Bevy рисует y вверх, а спрайт позиционирует
//! по центру. Порядок наложения передаётся через z: прозрачные спрайты 2D
//! сортируются по возрастанию z (`bevy_sprite_render`: `sort_key =
//! FloatOrd(transform.z)`), поэтому каждый следующий кусок сцены выше
//! предыдущего на [`Z_STEP`].

use std::sync::Arc;

use bevy::prelude::*;
use kn_game::{Game, Piece, PieceGroup, Scene, TextureId};

use crate::animation::{Animator, Frame, Sequence};
use crate::camera::{PendingFrame, frame_scene};
use crate::textures::{TextureCache, TextureEntry};

/// Шаг z между соседними кусками сцены.
pub const Z_STEP: f32 = 0.01;

/// Корень спрайтов одной карты: despawn корня убирает всю карту.
///
/// Спрайты — его потомки, поэтому корню нужны `Transform` и `Visibility`:
/// `propagate_parent_transforms` обходит иерархию от корней с `Transform`, а
/// `sync_simple_transforms` пропускает сущности с `ChildOf`. Без `Transform` у
/// корня потомки не получают `GlobalTransform` и не попадают в рендер.
#[derive(Component, Default)]
pub struct MapRoot;

/// Узел группы кусков: `Visibility::Hidden` на нём прячет всю группу.
///
/// Так переключение оверлея — одна запись компонента вместо пересборки карты:
/// порядок наложения задаёт позиция куска в [`Scene::pieces`], а не состав
/// группы, и скрытая группа ничего в нём не меняет.
///
/// Показанная группа — [`Visibility::Inherited`], а не `Visible`: `Visible`
/// перебивает наследование, и карта, спрятанная целиком на [`MapRoot`], осталась
/// бы на экране.
#[derive(Component, Debug)]
pub struct SceneGroup {
    pub group: PieceGroup,
    /// Мировые границы спрайтов группы со всеми кадрами их анимаций. Нужны, чтобы
    /// включённый слой не оставался за пределами, куда пускают камеру, когда их
    /// пересчитывают без разбора карты.
    pub bounds: Option<(Vec2, Vec2)>,
}

impl SceneGroup {
    /// Узел группы с её границами.
    pub fn new(group: PieceGroup, bounds: Option<(Vec2, Vec2)>) -> Self {
        Self { group, bounds }
    }
}

/// Спрайт одного куска сцены.
///
/// Сам кусок видимость не выбирает: она [`Visibility::Inherited`], а решает её
/// узел группы ([`SceneGroup`]) — плюс [`Animator`], когда картинки текущего
/// кадра нет.
#[derive(Component)]
pub struct ScenePiece;

/// Центр пиксельного прямоугольника куска в мире Bevy.
///
/// Кусок задан левым верхним углом в пикселях карты (y вниз), а спрайт
/// позиционируется по центру и живёт в мире с y вверх. Общая формула для
/// статичных кусков и для кадров анимации ([`crate::animation::frame_view`]):
/// у кадров разные размеры, и центр должен считаться по размеру кадра.
pub fn piece_center(x: f32, y: f32, width: f32, height: f32) -> Vec2 {
    Vec2::new(x + width / 2.0, -(y + height / 2.0))
}

/// Пиксельный прямоугольник куска на карте → позиция и размер спрайта.
///
/// `size` — размер текстуры: им заменяются отсутствующие `w`/`h`. `order` —
/// номер куска в списке отрисовки, из него получается z.
pub fn piece_transform(
    x: i32,
    y: i32,
    w: Option<i32>,
    h: Option<i32>,
    size: UVec2,
    order: usize,
) -> Transform {
    let width = w.unwrap_or(size.x as i32) as f32;
    let height = h.unwrap_or(size.y as i32) as f32;
    let center = piece_center(x as f32, y as f32, width, height);
    Transform::from_xyz(center.x, center.y, order as f32 * Z_STEP)
}

/// Размер спрайта куска: заданный явно или размер текстуры.
pub fn piece_size(piece: &Piece, size: UVec2) -> Vec2 {
    Vec2::new(
        piece.w.unwrap_or(size.x as i32) as f32,
        piece.h.unwrap_or(size.y as i32) as f32,
    )
}

/// Кадр модели сцены → кадр последовательности.
///
/// Размер берётся из загруженной картинки кадра, смещение — из данных. Без
/// картинки кадр скрыт: причину уже вернул [`TextureCache::load`], и молча
/// пустой спрайт вместо кадра был бы хуже пропуска.
fn animation_frame(frame: &kn_game::AnimationFrame, entry: Option<&TextureEntry>) -> Frame {
    Frame {
        image: entry.map(|entry| entry.handle.clone()),
        size: entry.map_or(Vec2::ZERO, |entry| {
            Vec2::new(entry.size.x as f32, entry.size.y as f32)
        }),
        offset: Vec2::new(frame.dx as f32, frame.dy as f32),
        ticks: frame.ticks,
    }
}

/// Кадры из модели сцены → последовательность картинок.
///
/// Кадры берутся из кэша целиком: анимированный кусок без какого-то кадра
/// показывал бы пустое место в середине цикла. `None` — показывать нечего ни в
/// одном кадре.
pub fn piece_sequence(
    animation: &kn_game::Animation,
    cache: &TextureCache,
) -> Option<Arc<Sequence>> {
    let frames = animation
        .frames
        .iter()
        .map(|frame| animation_frame(frame, frame.texture.as_ref().and_then(|id| cache.get(id))))
        .collect::<Vec<_>>();
    frames
        .iter()
        .any(|frame| frame.image.is_some())
        .then(|| Arc::new(Sequence::new(frames)))
}

/// Спрайты сцены новым корнем: по узлу на группу, куски без единой загруженной
/// картинки пропускаются.
///
/// Кусок с анимацией получает [`Animator`]: часы у сцены одни, поэтому слои
/// одного объекта или персонажа идут в такт, а базовый кадр (кадр из данных)
/// показан при нуле часов — пока анимация не включена, картина совпадает со
/// статичным рендером.
///
/// Видимость куска — `Inherited`, то есть видимость его узла: переключить
/// оверлей значит скрыть группу, а не пересобрать карту. Порядок наложения при
/// этом не меняется, потому что z даёт номер куска в [`Scene::pieces`], а не
/// соседство в дереве: куски одной группы идут вперемежку с чужими (персонажи с
/// объектами), и каждый остаётся на своём месте.
pub fn spawn_scene(commands: &mut Commands, scene: &Scene, cache: &TextureCache) -> Entity {
    let root = commands
        .spawn((MapRoot, Transform::IDENTITY, Visibility::Visible))
        .id();
    let mut nodes = Vec::with_capacity(PieceGroup::ALL.len());
    for (group, orders) in group_orders(scene) {
        let mut children = Vec::new();
        let mut bounds = None;
        for order in orders {
            let piece = &scene.pieces[order];
            // Creatures are drawn by their stable [`CreatureNode`] children, not as
            // scene sprites: spawning both would give every creature two competing
            // sprites for the same body (review #68).
            if piece.creature_id.is_some() {
                continue;
            }
            let base = piece.texture.as_ref().and_then(|id| cache.get(id));
            let sequence = piece
                .animation
                .as_ref()
                .and_then(|animation| piece_sequence(animation, cache));
            if base.is_none() && sequence.is_none() {
                continue;
            }
            let size = base.map_or(UVec2::ZERO, |entry| entry.size);
            let mut entity = commands.spawn((
                ScenePiece,
                Sprite {
                    image: base.map_or_else(Handle::default, |entry| entry.handle.clone()),
                    custom_size: Some(piece_size(piece, size)),
                    ..Default::default()
                },
                piece_visibility(base),
                piece_transform(piece.x, piece.y, piece.w, piece.h, size, order),
            ));
            if let (Some(sequence), Some(animation)) = (sequence, piece.animation.as_ref()) {
                entity.insert(Animator::with_base(
                    sequence,
                    piece.x,
                    piece.y,
                    animation.base,
                ));
            }
            children.push(entity.id());
            bounds = merge(bounds, piece_bounds(piece, cache));
        }
        if children.is_empty() {
            // Пустой узел группы только множил бы обход иерархии
            continue;
        }
        let node = commands
            .spawn((
                SceneGroup::new(group, bounds),
                Transform::IDENTITY,
                Visibility::Inherited,
            ))
            .id();
        commands.entity(node).add_children(&children);
        nodes.push(node);
    }
    commands.entity(root).add_children(&nodes);
    root
}

/// Границы одного куска: базовый кадр и все кадры его анимации, которые собрались.
///
/// Берутся все кадры, а не показанный сейчас: спрайт анимации меняет размер и
/// угол, и иначе крупный огонь у края карты оказался бы за границами, которые
/// подбирает `frame_scene`, а включённый слой — недостижимым для камеры.
/// `None` — картинок куска нет ни в одном кадре, спрайта не будет.
fn piece_bounds(piece: &Piece, cache: &TextureCache) -> Option<(Vec2, Vec2)> {
    let mut bounds: Option<(Vec2, Vec2)> = None;
    for (texture, dx, dy) in piece_frames(piece) {
        let Some(entry) = texture.as_ref().and_then(|id| cache.get(id)) else {
            continue;
        };
        let size = piece_size(piece, entry.size);
        let left = (piece.x + dx) as f32;
        let top = (piece.y + dy) as f32;
        bounds = merge(
            bounds,
            Some((
                Vec2::new(left, -(top + size.y)),
                Vec2::new(left + size.x, -top),
            )),
        );
    }
    bounds
}

/// Прямоугольник, покрывающий оба переданных.
fn merge(a: Option<(Vec2, Vec2)>, b: Option<(Vec2, Vec2)>) -> Option<(Vec2, Vec2)> {
    match (a, b) {
        (Some((a_min, a_max)), Some((b_min, b_max))) => Some((a_min.min(b_min), a_max.max(b_max))),
        (bounds, other) => bounds.or(other),
    }
}

/// Куски каждой группы: сама группа и номера её кусков в списке отрисовки.
///
/// Номера общие, а не внутригрупповые: z куска считает [`piece_transform`] по
/// позиции в [`Scene::pieces`], и разнос по узлам не должен переставлять куски,
/// идущие вперемежку с чужими. Групп без кусков в списке нет, порядок — по
/// [`PieceGroup::ALL`].
fn group_orders(scene: &Scene) -> Vec<(PieceGroup, Vec<usize>)> {
    PieceGroup::ALL
        .iter()
        .filter_map(|group| {
            let orders = scene
                .pieces
                .iter()
                .enumerate()
                .filter(|(_, piece)| piece.group == *group)
                .map(|(order, _)| order)
                .collect::<Vec<_>>();
            (!orders.is_empty()).then_some((*group, orders))
        })
        .collect()
}

/// Видимость спрайта куска: есть картинка базового кадра — наследовать узел
/// группы, нет — быть скрытым до своего кадра анимации.
///
/// `Visible` здесь ставить нельзя: он перебивает наследование, и включённая
/// анимация показывала бы куски скрытого оверлея.
fn piece_visibility(base: Option<&TextureEntry>) -> Visibility {
    if base.is_some() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    }
}

/// Убирает спрайты карты вместе с корнем (потомки — рекурсивно).
pub fn clear_scene(commands: &mut Commands, root: Entity) {
    commands.entity(root).despawn();
}

/// Что кусок может показать: базовая картинка с нулевым смещением и каждый кадр
/// анимации с её смещением от угла куска.
///
/// Отдельно от [`scene_bounds`], чтобы список кадров можно было проверить без
/// кэша текстур: ключ `TextureId` для `kn-render` непрозрачен.
fn piece_frames(piece: &Piece) -> impl Iterator<Item = (Option<TextureId>, i32, i32)> {
    std::iter::once((piece.texture, 0, 0)).chain(
        piece
            .animation
            .iter()
            .flat_map(|animation| animation.frames.iter())
            .map(|frame| (frame.texture, frame.dx, frame.dy)),
    )
}

/// Мировые границы спрайтов сцены: (минимум, максимум) по x и y.
///
/// Номинальный размер карты (`kn_game::Game::map_size`) — это вся сетка 80×160,
/// а нарисовано бывает только её часть, поэтому камеру и её ограничения
/// считают по фактическому содержимому. Куски без загруженной текстуры
/// пропускаются, как в [spawn_scene].
///
/// `shown` — какие куски сейчас видно: карта собирается целиком со всеми
/// оверлеями, а наводить камеру на спрятанные под [`SceneGroup`] ромбы
/// проходимости или маски света незачем. Как именно группа оказывается видимой,
/// решает вызывающий: `kn-render` про оверлеи ничего не знает.
pub fn scene_bounds(
    scene: &Scene,
    cache: &TextureCache,
    shown: impl Fn(&Piece) -> bool,
) -> Option<(Vec2, Vec2)> {
    scene
        .pieces
        .iter()
        .filter(|piece| shown(piece))
        .fold(None, |bounds, piece| {
            merge(bounds, piece_bounds(piece, cache))
        })
}

/// Result of a successful scene load.
pub struct SceneLoad {
    pub root: Entity,
    pub unique_textures: usize,
    pub problems: Vec<String>,
}

/// Shared scene-display pipeline: textures → sprites → camera frame.
///
/// Does NOT remove the previous scene — the caller keeps the old root and
/// clears it only after this returns `Ok` (the current `rebuild_scene` semantics).
pub fn load_scene(
    game: &Game,
    scene: &Scene,
    shown: impl Fn(&Piece) -> bool,
    cache: &mut TextureCache,
    images: &mut Assets<Image>,
    commands: &mut Commands,
    pending: &mut PendingFrame,
) -> kn_game::Result<SceneLoad> {
    let ids = scene.texture_ids();
    let unique_textures = ids.len();
    let problems = cache.load(game, &ids, images)?;
    let root = spawn_scene(commands, scene, cache);
    frame_scene(scene, cache, shown, pending);
    Ok(SceneLoad {
        root,
        unique_textures,
        problems,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNIT: UVec2 = UVec2::new(1, 1);

    #[test]
    fn pixel_coordinates_flip_y_and_center() {
        // Кусок 10×20 в левом верхнем углу карты: центр (5, −10)
        let transform = piece_transform(0, 0, Some(10), Some(20), UNIT, 0);
        assert_eq!(transform.translation, Vec3::new(5.0, -10.0, 0.0));
    }

    #[test]
    fn texture_size_is_used_when_piece_has_none() {
        let size = UVec2::new(114, 64);
        let transform = piece_transform(100, 200, None, None, size, 0);
        assert_eq!(transform.translation, Vec3::new(157.0, -232.0, 0.0));
    }

    #[test]
    fn later_pieces_are_higher() {
        assert!(
            piece_transform(0, 0, Some(2), Some(2), UNIT, 3)
                .translation
                .z
                > piece_transform(0, 0, Some(2), Some(2), UNIT, 2)
                    .translation
                    .z
        );
    }

    #[test]
    fn negative_offsets_flip_with_the_axis() {
        // У объектов и спрайтов бывают x, y < 0 (render.blit их не отбрасывает):
        // центр куска 67×88 в (−30, −76) — это (3.5, 32) в мире Bevy
        let transform = piece_transform(-30, -76, Some(67), Some(88), UNIT, 0);
        assert_eq!(transform.translation, Vec3::new(3.5, 32.0, 0.0));
    }

    fn model_frame(dx: i32, dy: i32, ticks: u32) -> kn_game::AnimationFrame {
        kn_game::AnimationFrame {
            texture: None,
            dx,
            dy,
            ticks,
        }
    }

    #[test]
    fn frame_without_texture_is_hidden() {
        // Кадр, картинка которого не загружена, не должен становиться пустым
        // спрайтом: он скрыт, а причину вернул загрузчик текстур
        let frame = animation_frame(&model_frame(3, -2, 4), None);
        assert!(frame.image.is_none());
        assert_eq!(frame.size, Vec2::ZERO);
        assert_eq!(frame.offset, Vec2::new(3.0, -2.0));
        assert_eq!(frame.ticks, 4);
    }

    #[test]
    fn frame_size_follows_its_texture() {
        let entry = TextureEntry {
            handle: Handle::default(),
            size: UVec2::new(114, 64),
        };
        let frame = animation_frame(&model_frame(0, 0, 1), Some(&entry));
        assert_eq!(frame.size, Vec2::new(114.0, 64.0));
        assert!(frame.image.is_some());
    }

    #[test]
    fn bounds_walk_every_animation_frame() {
        let piece = Piece {
            texture: None,
            x: 10,
            y: 20,
            w: None,
            h: None,
            animation: Some(kn_game::Animation {
                base: 0,
                frames: vec![
                    kn_game::AnimationFrame {
                        texture: None,
                        dx: 0,
                        dy: 0,
                        ticks: 1,
                    },
                    kn_game::AnimationFrame {
                        texture: None,
                        dx: -4,
                        dy: 7,
                        ticks: 1,
                    },
                ],
            }),
            group: PieceGroup::Objects,
            creature_id: None,
            depth_bottom: None,
        };
        // Границы считают по базовому кадру и по каждому кадру анимации:
        // крупный огонь больше своего базового кадра, и без этого он ушёл бы за
        // границы, которые подбирает frame_scene
        assert_eq!(
            piece_frames(&piece).collect::<Vec<_>>(),
            [(None, 0, 0), (None, 0, 0), (None, -4, 7)]
        );
    }

    fn group_piece(group: PieceGroup) -> Piece {
        Piece {
            texture: None,
            x: 0,
            y: 0,
            w: None,
            h: None,
            animation: None,
            group,
            creature_id: None,
            depth_bottom: None,
        }
    }

    #[test]
    fn interleaved_pieces_keep_their_draw_numbers() {
        // Куски одной группы идут вперемежку с чужими (персонажи между
        // объектами), и узел группы обязан сохранить общие номера: по ним
        // считается z, и перестановка уронила бы огонь под дерево
        let scene = Scene {
            creature_pieces: Vec::new(),
            creature_origins: std::collections::HashMap::new(),
            pieces: vec![
                group_piece(PieceGroup::Terrain),
                group_piece(PieceGroup::Objects),
                group_piece(PieceGroup::Characters),
                group_piece(PieceGroup::Objects),
                group_piece(PieceGroup::Markers),
            ],
        };
        assert_eq!(
            group_orders(&scene),
            vec![
                (PieceGroup::Terrain, vec![0]),
                (PieceGroup::Objects, vec![1, 3]),
                (PieceGroup::Characters, vec![2]),
                (PieceGroup::Markers, vec![4]),
            ]
        );
    }

    #[test]
    fn a_loaded_base_inherits_the_group_visibility() {
        // Кусок с картинкой не становится `Visible`: видимость выбирает узел его
        // группы, а `Visible` перебивает наследование
        let entry = TextureEntry {
            handle: Handle::default(),
            size: UVec2::new(8, 8),
        };
        assert_eq!(piece_visibility(Some(&entry)), Visibility::Inherited);
        assert_eq!(piece_visibility(None), Visibility::Hidden);
    }

    #[test]
    fn sequence_without_any_picture_is_not_built() {
        // Кусок, у которого не загружен ни один кадр, не получает аниматора:
        // спрайт и так скрыт
        let animation = kn_game::Animation {
            base: 0,
            frames: vec![model_frame(0, 0, 1), model_frame(1, 0, 1)],
        };
        assert!(piece_sequence(&animation, &TextureCache::default()).is_none());
    }
}
