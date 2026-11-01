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
use kn_game::{Piece, Scene, TextureId};

use crate::animation::{Animator, Frame, Sequence};
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

/// Спрайт одного куска сцены.
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

/// Спрайты сцены новым корнем; куски без единой загруженной картинки пропускаются.
///
/// Кусок с анимацией получает [`Animator`]: часы у сцены одни, поэтому слои
/// одного объекта или персонажа идут в такт, а базовый кадр (кадр из данных)
/// показан при нуле часов — пока анимация не включена, картина совпадает со
/// статичным рендером.
pub fn spawn_scene(commands: &mut Commands, scene: &Scene, cache: &TextureCache) -> Entity {
    let root = commands
        .spawn((MapRoot, Transform::IDENTITY, Visibility::Visible))
        .id();
    let mut children = Vec::with_capacity(scene.pieces.len());
    for (order, piece) in scene.pieces.iter().enumerate() {
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
            // Куску, у которого в базовом кадре картинки нет, показывать нечего
            // до его кадра анимации
            if base.is_some() {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
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
    }
    commands.entity(root).add_children(&children);
    root
}

/// Убирает спрайты карты вместе с корнем (потомки — рекурсивно).
pub fn clear_scene(commands: &mut Commands, root: Entity) {
    commands.entity(root).despawn();
}

/// Что кусок может показать: базовая картинка с нулевым смещением и каждый кадр
/// анимации с её смещением от угла куска.
///
/// Отдельно от [`scene_bounds`], чтобы список кадров можно было проверить без
/// кэша текстур: ключ `TextureId` для `kn-io` непрозрачен.
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
/// считают по фактическому содержимому. Берутся и кадры анимаций: они могут быть
/// больше базового кадра и сдвинуты от его угла, и иначе крупный огонь у края
/// карты оказался бы за границами, которые подобрать `frame_scene`. Куски без
/// загруженной текстуры пропускаются, как в [spawn_scene].
pub fn scene_bounds(scene: &Scene, cache: &TextureCache) -> Option<(Vec2, Vec2)> {
    let mut min = Vec2::splat(f32::MAX);
    let mut max = Vec2::splat(f32::MIN);
    let mut any = false;
    for piece in &scene.pieces {
        for (texture, dx, dy) in piece_frames(piece) {
            let Some(entry) = texture.as_ref().and_then(|id| cache.get(id)) else {
                continue;
            };
            let size = piece_size(piece, entry.size);
            let left = (piece.x + dx) as f32;
            let top = (piece.y + dy) as f32;
            min = min.min(Vec2::new(left, -(top + size.y)));
            max = max.max(Vec2::new(left + size.x, -top));
            any = true;
        }
    }
    any.then_some((min, max))
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
        };
        // Границы считают по базовому кадру и по каждому кадру анимации:
        // крупный огонь больше своего базового кадра, и без этого он ушёл бы за
        // границы, которые подбирает frame_scene
        assert_eq!(
            piece_frames(&piece).collect::<Vec<_>>(),
            [(None, 0, 0), (None, 0, 0), (None, -4, 7)]
        );
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
