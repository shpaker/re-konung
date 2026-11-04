//! Stable creature nodes: one entity per creature, sprites as children.
//!
//! Moving a creature writes `Transform` on its node; layers follow without
//! individual writes. A pose change replaces the children of one creature
//! without touching the map or other creatures. Depth is carried on the node's
//! z and recomputed from the bottom edge via [`DepthMap`].
//!
//! The module knows neither the game rules nor the formats: it takes ready
//! handles and pixel coordinates, the caller decides what a creature is.

use std::sync::Arc;

use bevy::prelude::*;

use crate::animation::{Animator, Sequence};
use crate::scene::Z_STEP;

/// Buckets for the per-creature z sub-slot; large enough that two creatures rarely
/// collide, small enough that the whole span stays under one `Z_STEP`.
const CREATURE_Z_SLOTS: u32 = 1024;

/// Root of all creature nodes, separate from [`MapRoot`] so that a creature
/// change never touches the map sprites.
#[derive(Component, Default)]
pub struct CreatureRoot;

/// One creature's display node. Position and depth live here; child sprites
/// inherit the transform and only carry their local layer offset.
#[derive(Component, Debug)]
pub struct CreatureNode {
    pub id: u32,
    pub bottom: i32,
}

/// A sprite that is one layer of a creature.
#[derive(Component)]
pub struct CreatureLayer;

/// The shadow sprite of a creature: fixed z below all stacks.
#[derive(Component)]
pub struct CreatureShadow;

/// Z boundaries between the shadow band and the stack band, built from the
/// static scene's sorted stacks.
#[derive(Resource, Debug, Clone)]
pub struct DepthMap {
    shadow_z: f32,
    first_stack_z: f32,
    last_stack_z: f32,
    boundaries: Vec<(i32, f32)>,
}

impl DepthMap {
    /// `stacks` are `(bottom, sort_key)` pairs of the **static** pieces a creature
    /// must interleave with — the caller excludes the dynamic creature pieces
    /// themselves. The input arrives in scene-draw order, not bottom order, so it
    /// is sorted here for [`DepthMap::z_for_bottom`]'s binary search; two statics
    /// that share a bottom collapse to one boundary at their higher sort key, so
    /// overlapping stacks resolve to the one drawn on top and never make the
    /// search land between them (review #68).
    pub fn build(stacks: impl Iterator<Item = (i32, f32)>, shadow_z: f32) -> DepthMap {
        let mut sorted: Vec<(i32, f32)> = stacks.collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let mut boundaries: Vec<(i32, f32)> = Vec::with_capacity(sorted.len());
        for (bottom, z) in sorted {
            match boundaries.last_mut() {
                Some(last) if last.0 == bottom => last.1 = last.1.max(z),
                _ => boundaries.push((bottom, z)),
            }
        }
        let first_stack_z = boundaries.first().map_or(0.0, |&(_, z)| z);
        let last_stack_z = boundaries
            .last()
            .map_or(first_stack_z + Z_STEP, |&(_, z)| z + Z_STEP * 10.0);
        DepthMap {
            shadow_z,
            first_stack_z,
            last_stack_z,
            boundaries,
        }
    }

    pub fn z_for_bottom(&self, bottom: i32) -> f32 {
        if self.boundaries.is_empty() {
            return self.first_stack_z;
        }
        match self.boundaries.binary_search_by_key(&bottom, |&(b, _)| b) {
            Ok(i) => self.boundaries[i].1,
            Err(0) => self.first_stack_z,
            Err(i) if i >= self.boundaries.len() => self.last_stack_z,
            Err(i) => {
                let (lo_bottom, lo_z) = self.boundaries[i - 1];
                let (hi_bottom, hi_z) = self.boundaries[i];
                if hi_bottom == lo_bottom {
                    return hi_z;
                }
                let t = (bottom - lo_bottom) as f32 / (hi_bottom - lo_bottom) as f32;
                lo_z + t * (hi_z - lo_z)
            }
        }
    }

    pub fn shadow_z(&self) -> f32 {
        self.shadow_z
    }

    /// A creature's own z: strictly in front of the static it stands on, never
    /// exactly equal (that z-fights), and with a stable per-id sub-slot inside one
    /// `Z_STEP` so two creatures sharing a bottom get a deterministic draw order.
    /// The original's per-creature slot order at equal depth is unconfirmed, so any
    /// stable order satisfies it (review #68).
    pub fn creature_z(&self, bottom: i32, id: u32) -> f32 {
        let base = self.z_for_bottom(bottom);
        let sub = ((id % CREATURE_Z_SLOTS) as f32 / CREATURE_Z_SLOTS as f32) * (Z_STEP * 0.9);
        base + Z_STEP * 0.05 + sub
    }
}

/// Sprite data for one creature layer.
pub struct LayerData {
    pub image: Handle<Image>,
    pub size: Vec2,
    pub offset: Vec2,
}

/// Animation sequence for one creature layer, anchored on the layer's own
/// base-frame corner (local to the node). `apply_animators` positions every
/// frame from this anchor, so the corner a clip was composed around is the
/// only origin that keeps the picture where the data put it.
pub struct LayerAnimation {
    pub sequence: Arc<Sequence>,
    pub base: usize,
    pub anchor: (i32, i32),
}

/// Complete data for spawning a creature.
pub struct CreatureData {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub bottom: i32,
    pub layers: Vec<LayerData>,
    pub animations: Vec<Option<LayerAnimation>>,
    pub shadow: Option<LayerData>,
    /// Walk/rest frames for the shadow, when the original animates it; anchored on
    /// the shadow's own local offset like the body layers.
    pub shadow_animation: Option<LayerAnimation>,
}

fn creature_position(x: i32, y: i32) -> Vec2 {
    Vec2::new(x as f32, -(y as f32))
}

/// The entities a creature spawn created: the node and its ordered children, so
/// the caller can tag each sprite with which layer it is.
pub struct SpawnedCreature {
    pub node: Entity,
    /// Body and equipment children, in the order of [`CreatureData::layers`].
    pub layers: Vec<Entity>,
    pub shadow: Option<Entity>,
}

/// Spawn a creature under `parent`, returning its node and children.
pub fn spawn_creature(
    commands: &mut Commands,
    parent: Entity,
    data: &CreatureData,
    depth: &DepthMap,
) -> SpawnedCreature {
    let pos = creature_position(data.x, data.y);
    let z = depth.creature_z(data.bottom, data.id);

    let node = commands
        .spawn((
            CreatureNode {
                id: data.id,
                bottom: data.bottom,
            },
            Transform::from_xyz(pos.x, pos.y, z),
            Visibility::Inherited,
        ))
        .id();

    let mut body = Vec::with_capacity(data.layers.len());
    let mut shadow_entity: Option<Entity> = None;

    for (index, layer) in data.layers.iter().enumerate() {
        let local_z = index as f32 * Z_STEP;
        let center = Vec2::new(
            layer.offset.x + layer.size.x / 2.0,
            -(layer.offset.y + layer.size.y / 2.0),
        );
        let anim = data.animations.get(index).and_then(|a| a.as_ref());
        let child = if let Some(anim) = anim {
            let seq = Arc::clone(&anim.sequence);
            commands
                .spawn((
                    CreatureLayer,
                    Sprite {
                        image: layer.image.clone(),
                        custom_size: Some(layer.size),
                        ..Default::default()
                    },
                    Visibility::Inherited,
                    Transform::from_xyz(center.x, center.y, local_z),
                    // The node carries the creature's world position, so the
                    // animator anchors on the clip's own base corner: a swap to
                    // another clip must re-anchor the same way (Animator::play_at),
                    // or the new clip renders displaced by the corner difference.
                    Animator::with_base(seq, anim.anchor.0, anim.anchor.1, anim.base),
                ))
                .id()
        } else {
            commands
                .spawn((
                    CreatureLayer,
                    Sprite {
                        image: layer.image.clone(),
                        custom_size: Some(layer.size),
                        ..Default::default()
                    },
                    Visibility::Inherited,
                    Transform::from_xyz(center.x, center.y, local_z),
                ))
                .id()
        };
        body.push(child);
    }

    if let Some(shadow) = &data.shadow {
        let center = Vec2::new(
            shadow.offset.x + shadow.size.x / 2.0,
            -(shadow.offset.y + shadow.size.y / 2.0),
        );
        // Constant local z below the body layers: it rides the node, so the shadow
        // stays under its creature's feet as the node's world z changes and never
        // needs recomputing on a move.
        let anim = data.shadow_animation.as_ref();
        let mut spawn = commands.spawn((
            CreatureShadow,
            Sprite {
                image: shadow.image.clone(),
                custom_size: Some(shadow.size),
                ..Default::default()
            },
            Visibility::Inherited,
            Transform::from_xyz(center.x, center.y, depth.shadow_z()),
        ));
        if let Some(anim) = anim {
            spawn.insert(Animator::with_base(
                Arc::clone(&anim.sequence),
                anim.anchor.0,
                anim.anchor.1,
                anim.base,
            ));
        }
        shadow_entity = Some(spawn.id());
    }

    let mut children = body.clone();
    children.extend(shadow_entity);
    commands.entity(node).add_children(&children);
    commands.entity(parent).add_child(node);
    SpawnedCreature {
        node,
        layers: body,
        shadow: shadow_entity,
    }
}

/// Update a creature's depth when its bottom edge changed. Returns `true` if written.
pub fn update_creature_depth(
    node: &mut CreatureNode,
    transform: &mut Transform,
    bottom: i32,
    depth: &DepthMap,
) -> bool {
    if node.bottom == bottom {
        return false;
    }
    node.bottom = bottom;
    transform.translation.z = depth.creature_z(bottom, node.id);
    true
}

/// Despawn a creature entity and its children.
pub fn despawn_creature(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).despawn();
}

/// Remove all creatures by despawning the root (children are recursive).
pub fn clear_creatures(commands: &mut Commands, root: Entity) {
    commands.entity(root).despawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_depth() -> DepthMap {
        DepthMap::build(vec![(100, 10.0), (200, 20.0), (300, 30.0)].into_iter(), 1.0)
    }

    #[test]
    fn node_has_layers_as_children() {
        let mut app = test_app();
        let (_root, creature) = spawn_test_creature(&mut app);
        let node = app.world().get::<CreatureNode>(creature).unwrap();
        assert_eq!(node.id, 7);
        assert_eq!(node.bottom, 150);

        let layer_count = count_with::<CreatureLayer>(&mut app);
        assert_eq!(layer_count, 1);
        let shadow_count = count_with::<CreatureShadow>(&mut app);
        assert_eq!(shadow_count, 1);
    }

    #[test]
    fn entities_stable_after_move() {
        let mut app = test_app();
        let (_root, creature) = spawn_test_creature(&mut app);
        let layer = single_entity::<CreatureLayer>(&mut app);

        app.world_mut()
            .entity_mut(creature)
            .get_mut::<Transform>()
            .unwrap()
            .translation = Vec3::new(60.0, -110.0, 15.0);

        assert!(app.world().get::<CreatureNode>(creature).is_some());
        assert!(app.world().get::<Sprite>(layer).is_some());
    }

    #[test]
    fn depth_changes_with_bottom() {
        let depth = mock_depth();
        let mut node = CreatureNode { id: 0, bottom: 100 };
        let mut transform = Transform::from_xyz(0.0, 0.0, depth.z_for_bottom(100));

        assert!(update_creature_depth(
            &mut node,
            &mut transform,
            250,
            &depth
        ));
        assert_eq!(node.bottom, 250);
        assert!(transform.translation.z > depth.z_for_bottom(100));

        assert!(!update_creature_depth(
            &mut node,
            &mut transform,
            250,
            &depth
        ));
    }

    #[test]
    fn depth_map_interpolates_between_boundaries() {
        let depth = mock_depth();
        assert_eq!(depth.z_for_bottom(100), 10.0);
        assert_eq!(depth.z_for_bottom(200), 20.0);
        let mid = depth.z_for_bottom(150);
        assert!((mid - 15.0).abs() < f32::EPSILON);
    }

    #[test]
    fn depth_map_clamps_outside_range() {
        let depth = mock_depth();
        assert_eq!(depth.z_for_bottom(50), 10.0);
        assert_eq!(depth.z_for_bottom(999), depth.last_stack_z);
    }

    #[test]
    fn creature_z_never_sits_on_a_static_z() {
        // At a bottom that coincides with a stack, the creature must be strictly in
        // front, not exactly on the static's z (that z-fights at sub-pixel scale).
        let depth = mock_depth();
        for bottom in [100, 150, 200, 300] {
            let z = depth.creature_z(bottom, 7);
            assert!(
                z > depth.z_for_bottom(bottom),
                "creature z {z} must clear the static z {} at bottom {bottom}",
                depth.z_for_bottom(bottom)
            );
        }
    }

    #[test]
    fn creature_z_orders_equal_depths_by_id() {
        // Two creatures sharing a bottom must not tie: their z differ, deterministically.
        let depth = mock_depth();
        let a = depth.creature_z(200, 5);
        let b = depth.creature_z(200, 6);
        assert_ne!(a, b, "distinct ids must break the z tie");
        assert_eq!(
            a,
            depth.creature_z(200, 5),
            "the order is stable across calls"
        );
    }

    #[test]
    fn equal_bottoms_collapse_to_the_highest_sort_key() {
        // Two statics share a bottom edge: the search must resolve to the one
        // drawn on top (higher sort key), never to a z between the duplicates.
        let depth = DepthMap::build(vec![(100, 10.0), (150, 5.0), (150, 30.0)].into_iter(), 1.0);
        assert_eq!(
            depth.z_for_bottom(150),
            30.0,
            "equal bottom keeps the top stack"
        );
        // Unsorted input still lands correctly because build sorts before search.
        let unsorted =
            DepthMap::build(vec![(300, 30.0), (100, 10.0), (200, 20.0)].into_iter(), 1.0);
        assert_eq!(
            unsorted.z_for_bottom(200),
            20.0,
            "build sorts regardless of input order"
        );
    }

    #[test]
    fn shadow_z_below_stacks() {
        let depth = mock_depth();
        assert!(depth.shadow_z() < depth.first_stack_z);
    }

    #[test]
    fn despawn_removes_entity_and_children() {
        let mut app = test_app();
        let (_root, creature) = spawn_test_creature(&mut app);
        assert_eq!(count_with::<CreatureLayer>(&mut app), 1);

        app.world_mut().entity_mut(creature).despawn();

        assert!(app.world().get::<CreatureNode>(creature).is_none());
        assert_eq!(count_with::<CreatureLayer>(&mut app), 0);
    }

    #[test]
    fn map_untouched_while_creatures_move() {
        let mut app = test_app();
        let map_root = app.world_mut().spawn(crate::scene::MapRoot).id();
        let (_root, creature) = spawn_test_creature(&mut app);
        let map_count_before = count_with::<crate::scene::MapRoot>(&mut app);

        app.world_mut()
            .entity_mut(creature)
            .get_mut::<Transform>()
            .unwrap()
            .translation = Vec3::new(60.0, -60.0, 15.0);

        let map_count_after = count_with::<crate::scene::MapRoot>(&mut app);
        assert_eq!(map_count_before, map_count_after);
        assert!(app.world().get::<crate::scene::MapRoot>(map_root).is_some());
    }

    #[test]
    fn hidden_creature_root_hides_children() {
        let mut app = test_app();
        let (root, creature) = spawn_test_creature(&mut app);
        app.world_mut()
            .entity_mut(root)
            .get_mut::<Visibility>()
            .unwrap()
            .clone_from(&Visibility::Hidden);
        app.update();
        let inherited = app
            .world()
            .get::<InheritedVisibility>(creature)
            .unwrap()
            .get();
        assert!(!inherited);
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app
    }

    fn spawn_test_creature(app: &mut App) -> (Entity, Entity) {
        let depth = mock_depth();
        let z = depth.z_for_bottom(150);
        let root = app
            .world_mut()
            .spawn((CreatureRoot, Visibility::Visible))
            .id();
        let creature = app
            .world_mut()
            .spawn((
                CreatureNode { id: 7, bottom: 150 },
                Transform::from_xyz(100.0, -200.0, z),
                Visibility::Inherited,
            ))
            .id();
        let layer = app
            .world_mut()
            .spawn((
                CreatureLayer,
                Sprite {
                    image: Handle::default(),
                    custom_size: Some(Vec2::new(32.0, 48.0)),
                    ..Default::default()
                },
                Visibility::Inherited,
                Transform::from_xyz(16.0, -24.0, 0.0),
            ))
            .id();
        let shadow = app
            .world_mut()
            .spawn((
                CreatureShadow,
                Sprite {
                    image: Handle::default(),
                    custom_size: Some(Vec2::new(32.0, 16.0)),
                    ..Default::default()
                },
                Visibility::Inherited,
                Transform::from_xyz(16.0, -8.0, 1.0 - z),
            ))
            .id();
        app.world_mut()
            .entity_mut(creature)
            .add_children(&[layer, shadow]);
        app.world_mut().entity_mut(root).add_child(creature);
        (root, creature)
    }

    fn count_with<T: bevy::ecs::component::Component>(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, With<T>>()
            .iter(app.world())
            .count()
    }

    fn single_entity<T: bevy::ecs::component::Component>(app: &mut App) -> Entity {
        app.world_mut()
            .query_filtered::<Entity, With<T>>()
            .single(app.world())
            .unwrap()
    }
}
