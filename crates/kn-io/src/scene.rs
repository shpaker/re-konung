//! Куски сцены → спрайты Bevy.
//!
//! Модель сцены [`kn_game`] задаёт пиксельные координаты карты с началом в
//! левом верхнем углу и осью y вниз; Bevy рисует y вверх, а спрайт позиционирует
//! по центру. Порядок наложения передаётся через z: прозрачные спрайты 2D
//! сортируются по возрастанию z (`bevy_sprite_render`: `sort_key =
//! FloatOrd(transform.z)`), поэтому каждый следующий кусок сцены выше
//! предыдущего на [`Z_STEP`].

use bevy::prelude::*;
use kn_game::{Piece, Scene};

use crate::textures::TextureCache;

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
    Transform::from_xyz(
        x as f32 + width / 2.0,
        -(y as f32 + height / 2.0),
        order as f32 * Z_STEP,
    )
}

/// Размер спрайта куска: заданный явно или размер текстуры.
pub fn piece_size(piece: &Piece, size: UVec2) -> Vec2 {
    Vec2::new(
        piece.w.unwrap_or(size.x as i32) as f32,
        piece.h.unwrap_or(size.y as i32) as f32,
    )
}

/// Спрайты сцены новым корнем; куски без загруженной текстуры пропускаются.
pub fn spawn_scene(commands: &mut Commands, scene: &Scene, cache: &TextureCache) -> Entity {
    let root = commands
        .spawn((MapRoot, Transform::IDENTITY, Visibility::Visible))
        .id();
    let mut children = Vec::with_capacity(scene.pieces.len());
    for (order, piece) in scene.pieces.iter().enumerate() {
        let Some(entry) = cache.get(&piece.texture) else {
            continue;
        };
        children.push(
            commands
                .spawn((
                    ScenePiece,
                    Sprite {
                        image: entry.handle.clone(),
                        custom_size: Some(piece_size(piece, entry.size)),
                        ..Default::default()
                    },
                    piece_transform(piece.x, piece.y, piece.w, piece.h, entry.size, order),
                ))
                .id(),
        );
    }
    commands.entity(root).add_children(&children);
    root
}

/// Убирает спрайты карты вместе с корнем (потомки — рекурсивно).
pub fn clear_scene(commands: &mut Commands, root: Entity) {
    commands.entity(root).despawn();
}

/// Мировые границы спрайтов сцены: (минимум, максимум) по x и y.
///
/// Номинальный размер карты (`kn_game::Game::map_size`) — это вся сетка 80×160,
/// а нарисовано бывает только её часть, поэтому камеру и её ограничения
/// считают по фактическому содержимому. Куски без загруженной текстуры
/// пропускаются, как в [spawn_scene].
pub fn scene_bounds(scene: &Scene, cache: &TextureCache) -> Option<(Vec2, Vec2)> {
    let mut min = Vec2::splat(f32::MAX);
    let mut max = Vec2::splat(f32::MIN);
    let mut any = false;
    for piece in &scene.pieces {
        let Some(entry) = cache.get(&piece.texture) else {
            continue;
        };
        let size = piece_size(piece, entry.size);
        let left = piece.x as f32;
        let top = piece.y as f32;
        min = min.min(Vec2::new(left, -(top + size.y)));
        max = max.max(Vec2::new(left + size.x, -top));
        any = true;
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
}
