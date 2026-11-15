//! Pixel picking against the same creature layers and uploaded frames that draw.

use bevy::{ecs::system::SystemParam, prelude::*, sprite::Anchor};

use crate::{CreatureShadow, CreatureSprite, TextureCache, textures::AlphaMask};

/// Read after transform and visibility propagation to inspect the rendered frame.
#[derive(SystemParam)]
pub struct CreaturePicker<'w, 's> {
    cache: Res<'w, TextureCache>,
    layers: Query<
        'w,
        's,
        (
            &'static CreatureSprite,
            &'static Sprite,
            &'static GlobalTransform,
            &'static InheritedVisibility,
            &'static Anchor,
        ),
        Without<CreatureShadow>,
    >,
}

impl CreaturePicker<'_, '_> {
    pub fn pick_screen(
        &self,
        point: Vec2,
        camera: &Camera,
        transform: &GlobalTransform,
    ) -> Option<usize> {
        if !camera.logical_viewport_rect()?.contains(point) {
            return None;
        }
        self.pick_world(camera.viewport_to_world_2d(transform, point).ok()?)
    }

    /// Union of this frame's visible body/equipment quads for selection feedback.
    pub fn bounds_world(&self, id: usize) -> Option<Rect> {
        self.layers
            .iter()
            .filter_map(|(tag, sprite, transform, visible, anchor)| {
                if tag.creature_id != id
                    || tag.is_shadow
                    || !visible.get()
                    || sprite.color.alpha() == 0.0
                {
                    return None;
                }
                let mask = self.cache.alpha_mask(sprite.image.id())?;
                if sprite.texture_atlas.is_some() {
                    return None;
                }
                let source = sprite
                    .rect
                    .unwrap_or(Rect::from_corners(Vec2::ZERO, mask.size().as_vec2()));
                let size = sprite.custom_size.unwrap_or(source.size());
                if size.min_element() <= 0.0 || !size.is_finite() {
                    return None;
                }
                let min = (-Vec2::splat(0.5) - anchor.0) * size;
                let max = min + size;
                let points = [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)]
                    .map(|point| transform.transform_point(point.extend(0.0)).truncate());
                Some(points.into_iter().fold(
                    Rect::from_corners(points[0], points[0]),
                    |bounds, point| {
                        Rect::from_corners(bounds.min.min(point), bounds.max.max(point))
                    },
                ))
            })
            .reduce(|a, b| Rect::from_corners(a.min.min(b.min), a.max.max(b.max)))
    }

    pub fn pick_world(&self, point: Vec2) -> Option<usize> {
        self.layers
            .iter()
            .filter_map(|(tag, sprite, transform, visible, anchor)| {
                if tag.is_shadow || !visible.get() || sprite.color.alpha() == 0.0 {
                    return None;
                }
                let mask = self.cache.alpha_mask(sprite.image.id())?;
                layer_hit(point, sprite, transform, anchor, mask)
                    .then_some((tag.creature_id, transform.translation().z))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
            .map(|(id, _)| id)
    }
}

fn layer_hit(
    point: Vec2,
    sprite: &Sprite,
    transform: &GlobalTransform,
    anchor: &Anchor,
    mask: &AlphaMask,
) -> bool {
    // Creature frames use full-image sprites; reject unsupported atlas semantics
    // rather than treating an unrelated texel as a visible body pixel.
    if sprite.texture_atlas.is_some() {
        return false;
    }
    let source = sprite
        .rect
        .unwrap_or(Rect::from_corners(Vec2::ZERO, mask.size().as_vec2()));
    let size = sprite.custom_size.unwrap_or(source.size());
    if size.min_element() <= 0.0 || !size.is_finite() {
        return false;
    }
    let local = transform
        .affine()
        .inverse()
        .transform_point3(point.extend(transform.translation().z))
        .truncate();
    let normalized = local / size + anchor.0 + Vec2::splat(0.5);
    if !normalized.is_finite() || normalized.min_element() < 0.0 || normalized.max_element() >= 1.0
    {
        return false;
    }
    // Local quads are half-open at their positive edges. Quantize first:
    // reversing UVs first would turn the included zero edge into texel N.
    let extent = source.size().as_uvec2();
    if extent.min_element() == 0 {
        return false;
    }
    let mut pixel = (normalized * extent.as_vec2())
        .floor()
        .as_uvec2()
        .min(extent - UVec2::ONE);
    if sprite.flip_x {
        pixel.x = extent.x - 1 - pixel.x;
    }
    if !sprite.flip_y {
        pixel.y = extent.y - 1 - pixel.y;
    }
    pixel += source.min.floor().as_uvec2();
    mask.opaque(pixel)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pick(world: &mut World, point: Vec2) -> Option<usize> {
        let mut state = bevy::ecs::system::SystemState::<CreaturePicker>::new(world);
        state.get(world).unwrap().pick_world(point)
    }

    #[test]
    fn actual_frontmost_layer_wins_and_shadows_hidden_and_transparency_do_not_pick() {
        let mut world = World::new();
        let mut images = Assets::<Image>::default();
        let body = images.add(Image::default());
        let transparent = images.add(Image::default());
        let mut cache = TextureCache::default();
        cache.insert_test_mask(body.id(), AlphaMask::from_rgba(UVec2::ONE, &[0, 0, 0, 255]));
        cache.insert_test_mask(
            transparent.id(),
            AlphaMask::from_rgba(UVec2::ONE, &[0, 0, 0, 0]),
        );
        world.insert_resource(cache);
        let mut spawn = |id, z, shadow, visible, image: Handle<Image>| {
            world
                .spawn((
                    CreatureSprite {
                        creature_id: id,
                        layer: 0,
                        is_shadow: shadow,
                    },
                    Sprite {
                        image,
                        custom_size: Some(Vec2::splat(10.0)),
                        ..default()
                    },
                    GlobalTransform::from_translation(Vec3::new(0.0, 0.0, z)),
                    visible,
                ))
                .id()
        };
        let rear = spawn(100, 1.0, false, InheritedVisibility::VISIBLE, body.clone());
        let front = spawn(1, 2.0, false, InheritedVisibility::VISIBLE, body.clone());
        spawn(2, 10.0, true, InheritedVisibility::VISIBLE, body.clone());
        spawn(3, 11.0, false, InheritedVisibility::HIDDEN, body.clone());
        spawn(4, 12.0, false, InheritedVisibility::VISIBLE, transparent);
        assert_eq!(pick(&mut world, Vec2::ZERO), Some(1));
        world.entity_mut(front).insert(CreatureShadow);
        assert_eq!(pick(&mut world, Vec2::ZERO), Some(100));
        world.entity_mut(rear).insert(InheritedVisibility::HIDDEN);
        assert_eq!(pick(&mut world, Vec2::ZERO), None);
        world.entity_mut(front).remove::<CreatureShadow>();
        world.entity_mut(front).get_mut::<Sprite>().unwrap().image = body;
        assert_eq!(pick(&mut world, Vec2::ZERO), Some(1));
        world
            .entity_mut(front)
            .get_mut::<GlobalTransform>()
            .unwrap()
            .clone_from(&GlobalTransform::from_translation(Vec3::new(
                20.0, 0.0, 2.0,
            )));
        assert_eq!(pick(&mut world, Vec2::ZERO), None);
        assert_eq!(pick(&mut world, Vec2::new(20.0, 0.0)), Some(1));
        let mut state = bevy::ecs::system::SystemState::<CreaturePicker>::new(&mut world);
        assert_eq!(
            state.get(&world).unwrap().bounds_world(1),
            Some(Rect::from_corners(
                Vec2::new(15.0, -5.0),
                Vec2::new(25.0, 5.0)
            ))
        );
        assert_eq!(state.get(&world).unwrap().bounds_world(2), None);
    }

    #[test]
    #[ignore = "manual synthetic workspace cost measurement"]
    fn workspace_inspection_costs() {
        use std::{hint::black_box, time::Instant};
        fn measure(mut run: impl FnMut(), count: u32) -> f64 {
            (0..5)
                .map(|_| {
                    let start = Instant::now();
                    for _ in 0..count {
                        run();
                    }
                    start.elapsed().as_nanos() as f64 / f64::from(count)
                })
                .fold(f64::INFINITY, f64::min)
        }
        let rgba = vec![255; 96 * 128 * 4];
        let mask_ns = measure(
            || {
                black_box(AlphaMask::from_rgba(UVec2::new(96, 128), black_box(&rgba)));
            },
            1000,
        );
        let clone_ns = measure(
            || {
                black_box(black_box(&rgba).clone());
            },
            1000,
        );
        let mut world = World::new();
        let mut images = Assets::<Image>::default();
        let image = images.add(Image::default());
        let mut cache = TextureCache::default();
        cache.insert_test_mask(image.id(), AlphaMask::from_rgba(UVec2::new(96, 128), &rgba));
        world.insert_resource(cache);
        for id in 0..500 {
            world.spawn((
                CreatureSprite {
                    creature_id: id,
                    layer: 0,
                    is_shadow: false,
                },
                Sprite {
                    image: image.clone(),
                    ..default()
                },
                GlobalTransform::from_translation(Vec3::new(
                    (id % 25) as f32 * 32.0,
                    (id / 25) as f32 * 32.0,
                    id as f32,
                )),
                InheritedVisibility::VISIBLE,
            ));
        }
        let mut state = bevy::ecs::system::SystemState::<CreaturePicker>::new(&mut world);
        let picker = state.get(&world).unwrap();
        let pick_ns = measure(
            || {
                black_box(picker.pick_world(black_box(Vec2::new(180.0, 180.0))));
            },
            1000,
        );
        let bounds_ns = measure(
            || {
                black_box(picker.bounds_world(black_box(150)));
            },
            1000,
        );
        println!(
            "WORKSPACE_COST mask_96x128_ns={mask_ns:.0} rgba_clone_ns={clone_ns:.0} pick_500_layers_ns={pick_ns:.0} bounds_500_layers_ns={bounds_ns:.0}"
        );
    }

    #[test]
    fn exact_negative_edges_quantize_before_flipping() {
        let transform = GlobalTransform::IDENTITY;
        for (flip_x, flip_y) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut rgba = [0; 16];
            let x = usize::from(flip_x);
            let y = usize::from(!flip_y);
            rgba[(y * 2 + x) * 4 + 3] = 255;
            let mask = AlphaMask::from_rgba(UVec2::new(2, 2), &rgba);
            let sprite = Sprite {
                flip_x,
                flip_y,
                ..default()
            };
            assert!(
                layer_hit(
                    Vec2::new(-1.0, -1.0),
                    &sprite,
                    &transform,
                    &Anchor::CENTER,
                    &mask
                ),
                "negative corner flip {flip_x}/{flip_y}"
            );
            assert!(!layer_hit(
                Vec2::new(-1.01, -1.0),
                &sprite,
                &transform,
                &Anchor::CENTER,
                &mask
            ));
            assert!(!layer_hit(
                Vec2::new(1.0, -1.0),
                &sprite,
                &transform,
                &Anchor::CENTER,
                &mask
            ));
            assert!(!layer_hit(
                Vec2::new(-1.0, 1.0),
                &sprite,
                &transform,
                &Anchor::CENTER,
                &mask
            ));
            assert!(!layer_hit(
                Vec2::ZERO,
                &sprite,
                &transform,
                &Anchor::CENTER,
                &mask
            ));
        }
    }

    #[test]
    fn screen_pick_respects_nonzero_viewport_dpi_and_equipment_frame_swap() {
        use bevy::camera::{CameraProjection, ComputedCameraValues, RenderTargetInfo, Viewport};
        let mut world = World::new();
        let mut images = Assets::<Image>::default();
        let opaque = images.add(Image::default());
        let clear = images.add(Image::default());
        let mut cache = TextureCache::default();
        cache.insert_test_mask(
            opaque.id(),
            AlphaMask::from_rgba(UVec2::ONE, &[0, 0, 0, 255]),
        );
        cache.insert_test_mask(clear.id(), AlphaMask::from_rgba(UVec2::ONE, &[0, 0, 0, 0]));
        world.insert_resource(cache);
        let spawn = |world: &mut World, id, layer, z| {
            world
                .spawn((
                    CreatureSprite {
                        creature_id: id,
                        layer,
                        is_shadow: false,
                    },
                    Sprite {
                        image: opaque.clone(),
                        custom_size: Some(Vec2::splat(20.0)),
                        ..default()
                    },
                    GlobalTransform::from_translation(Vec3::new(30.0, 40.0, z)),
                    InheritedVisibility::VISIBLE,
                ))
                .id()
        };
        spawn(&mut world, 1, 0, 1.0);
        // An equipment quad can be frontmost even when its body's behind.
        let gear = spawn(&mut world, 2, 1, 3.0);
        spawn(&mut world, 2, 0, 0.0);
        let mut projection = OrthographicProjection::default_2d();
        projection.update(200.0, 100.0);
        let camera = Camera {
            viewport: Some(Viewport {
                physical_position: UVec2::new(200, 100),
                physical_size: UVec2::new(400, 200),
                ..default()
            }),
            computed: ComputedCameraValues {
                clip_from_view: projection.get_clip_from_view(),
                target_info: Some(RenderTargetInfo {
                    physical_size: UVec2::new(1000, 800),
                    scale_factor: 2.0,
                }),
                ..default()
            },
            ..default()
        };
        let transform = GlobalTransform::from_translation(Vec3::new(30.0, 40.0, 0.0));
        let mut state = bevy::ecs::system::SystemState::<CreaturePicker>::new(&mut world);
        assert_eq!(
            state
                .get(&world)
                .unwrap()
                .pick_screen(Vec2::new(200.0, 100.0), &camera, &transform),
            Some(2)
        );
        assert_eq!(
            state
                .get(&world)
                .unwrap()
                .pick_screen(Vec2::new(50.0, 100.0), &camera, &transform),
            None
        );
        world.entity_mut(gear).get_mut::<Sprite>().unwrap().image = clear;
        assert_eq!(
            state
                .get(&world)
                .unwrap()
                .pick_screen(Vec2::new(200.0, 100.0), &camera, &transform),
            Some(1)
        );
    }

    #[test]
    fn alpha_uses_current_geometry_and_flips() {
        let mask = AlphaMask::from_rgba(
            UVec2::new(2, 2),
            &[0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        );
        let mut sprite = Sprite {
            custom_size: Some(Vec2::splat(4.0)),
            ..default()
        };
        let transform = GlobalTransform::from_translation(Vec3::new(10.0, 20.0, 3.0));
        assert!(layer_hit(
            Vec2::new(9.0, 21.0),
            &sprite,
            &transform,
            &Anchor::CENTER,
            &mask
        ));
        assert!(!layer_hit(
            Vec2::new(11.0, 21.0),
            &sprite,
            &transform,
            &Anchor::CENTER,
            &mask
        ));
        sprite.flip_x = true;
        assert!(layer_hit(
            Vec2::new(11.0, 21.0),
            &sprite,
            &transform,
            &Anchor::CENTER,
            &mask
        ));
        assert!(!layer_hit(
            Vec2::new(15.0, 21.0),
            &sprite,
            &transform,
            &Anchor::CENTER,
            &mask
        ));
    }
}
