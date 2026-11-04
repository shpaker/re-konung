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

/// Clearance above the static a creature stands on, in `Z_STEP` units.
const CREATURE_CLEARANCE: f32 = 0.05;

/// The span the per-id tie-break may take, in `Z_STEP` units.
const CREATURE_ID_SPAN: f32 = 0.4;

/// The span all body layers together may take, in `Z_STEP` units. Together with
/// the clearance and the id slot it must stay under one `Z_STEP`: static pieces
/// are one `Z_STEP` apart, so a stack that leaves its slot starts drawing over
/// the next static (review #68).
const CREATURE_LAYER_SPAN: f32 = 0.45;

/// The layer count the layer budget is divided by; K1 gear caps out here, and a
/// fatter stack compresses its step instead ([`spawn_creature`]).
const CREATURE_LAYER_CAP: f32 = 8.0;

/// z between neighbouring body layers.
const CREATURE_LAYER_Z: f32 = Z_STEP * CREATURE_LAYER_SPAN / (CREATURE_LAYER_CAP - 1.0);

/// The z room every creature slot reserves: body clearance, id span and the
/// layer span, with the span's 0.9 spawn clamp divided out. `slot_bounds`
/// clamps interpolated bases so the room is there at every bottom, which is
/// what keeps a stack spawned in one slot under the next static after a move
/// into another — the layer offsets are never rewritten (review #68).
const CREATURE_SLOT_WIDE: f32 =
    Z_STEP * (CREATURE_CLEARANCE + CREATURE_ID_SPAN + CREATURE_LAYER_SPAN / 0.9);

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

/// The shadow sprite of a creature: it rides the creature's slot, strictly
/// under the body and over the static the creature stands on.
#[derive(Component)]
pub struct CreatureShadow;

/// Z boundaries between the static scene's sorted stacks. A creature occupies
/// the slot between two boundaries: [`DepthMap::creature_slot`] carves shadow,
/// body and layer room inside it, all strictly below the next boundary, so a
/// creature draws between the statics it walks between — and its shadow with
/// it, over the ground and under its feet, as the original draws the two
/// together (review #68).
#[derive(Resource, Debug, Clone)]
pub struct DepthMap {
    first_stack_z: f32,
    last_stack_z: f32,
    boundaries: Vec<(i32, f32)>,
}

/// One creature's z slot: world z for the shadow and the body node, and the
/// exclusive `top` ceiling the layer stack above `body` must stay under.
/// `base` is the z of the statics the creature stands in front of.
pub struct CreatureSlot {
    pub base: f32,
    pub shadow: f32,
    pub body: f32,
    pub top: f32,
}

impl DepthMap {
    /// `stacks` are `(bottom, sort_key)` pairs of the **static** pieces a creature
    /// must interleave with — the caller excludes the dynamic creature pieces
    /// themselves, and pieces that are not standing objects of their own: an
    /// object's flames ride the object's draw slot, and their higher bottom
    /// would otherwise put a creature on that row in front of the object they
    /// burn on (review #68). The input arrives in scene-draw order, not bottom
    /// order, so it is sorted here for [`DepthMap::z_for_bottom`]'s binary
    /// search; two statics that share a bottom collapse to one boundary at
    /// their higher sort key, so overlapping stacks resolve to the one drawn
    /// on top and never make the search land between them (review #68).
    pub fn build(stacks: impl Iterator<Item = (i32, f32)>) -> DepthMap {
        let mut sorted: Vec<(i32, f32)> = stacks.collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let mut boundaries: Vec<(i32, f32)> = Vec::with_capacity(sorted.len());
        for (bottom, z) in sorted {
            match boundaries.last_mut() {
                Some(last) if last.0 == bottom => last.1 = last.1.max(z),
                // A row drawn under the previous boundary's stack cannot drop
                // the boundary below it without breaking the monotonicity every
                // bracketing below assumes — and it adds nothing: anything in
                // front of the higher stack is in front of it in z, so the row
                // simply takes the previous boundary's slot (review #68).
                Some(last) if z <= last.1 => {}
                Some(_) => boundaries.push((bottom, z)),
                None => boundaries.push((bottom, z)),
            }
        }
        let first_stack_z = boundaries.first().map_or(0.0, |&(_, z)| z);
        let last_stack_z = boundaries
            .last()
            .map_or(first_stack_z + Z_STEP, |&(_, z)| z + Z_STEP * 10.0);
        DepthMap {
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

    /// The slot bounds around `bottom`: the z the creature draws just behind
    /// (`base`, the interpolated statics' z it stands in front of) and the
    /// exclusive z of the next stack (`next`).
    fn slot_bounds(&self, bottom: i32) -> (f32, f32) {
        if self.boundaries.is_empty() {
            return (self.first_stack_z, self.first_stack_z + Z_STEP);
        }
        match self.boundaries.binary_search_by_key(&bottom, |&(b, _)| b) {
            Ok(i) => (
                self.boundaries[i].1,
                self.boundaries
                    .get(i + 1)
                    .map_or(self.last_stack_z, |&(_, z)| z),
            ),
            // A bottom below every static: the creature draws before all of them.
            Err(0) => (self.first_stack_z - Z_STEP, self.first_stack_z),
            // Past the last static: one free slot above it, as in `z_for_bottom`.
            Err(i) if i >= self.boundaries.len() => (self.last_stack_z, self.last_stack_z + Z_STEP),
            Err(i) => {
                let (lo_bottom, lo_z) = self.boundaries[i - 1];
                let (hi_bottom, hi_z) = self.boundaries[i];
                if hi_bottom == lo_bottom {
                    return (hi_z, hi_z + Z_STEP);
                }
                let t = (bottom - lo_bottom) as f32 / (hi_bottom - lo_bottom) as f32;
                // An unclamped interpolation sits arbitrarily close to `hi_z`,
                // leaving no room for the body between the two statics; inside
                // the last `CREATURE_SLOT_WIDE` the base stops. Both statics
                // still bracket the creature there — every z in the interval
                // draws the same picture — so the clamp costs nothing visible.
                let base = (lo_z + t * (hi_z - lo_z))
                    .max(lo_z)
                    .min(hi_z - CREATURE_SLOT_WIDE);
                (base, hi_z)
            }
        }
    }

    /// A creature's z slot: strictly in front of the static it stands on, never
    /// exactly equal (that z-fights), with a stable per-id sub-slot inside the
    /// slot so two creatures sharing a bottom get a deterministic draw order.
    /// The original's per-creature slot order at equal depth is unconfirmed, so
    /// any stable order satisfies it (review #68).
    ///
    /// The body offset shrinks with the slot: an interpolated base sits
    /// arbitrarily close to `next`, and a fixed budget would push body, layers
    /// or shadow past the occluding static there (review #68). The shadow takes
    /// half the way from the base to the body — always over the ground the
    /// creature stands on and under its feet, the order the original draws.
    pub fn creature_slot(&self, bottom: i32, id: u32) -> CreatureSlot {
        let (base, next) = self.slot_bounds(bottom);
        let sub = (id % CREATURE_Z_SLOTS) as f32 / CREATURE_Z_SLOTS as f32;
        let body = base
            + (Z_STEP * (CREATURE_CLEARANCE + sub * CREATURE_ID_SPAN))
                .min((next - base) * (0.55 + 0.35 * sub));
        CreatureSlot {
            base,
            shadow: base + (body - base) * 0.5,
            body,
            top: next,
        }
    }

    /// A creature node's world z: the body of its [`Self::creature_slot`].
    pub fn creature_z(&self, bottom: i32, id: u32) -> f32 {
        self.creature_slot(bottom, id).body
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

/// z step between neighbouring body layers of a `layer_count` tall stack,
/// shrunk so the whole stack above `body` ends strictly under the slot's
/// `top`: a narrow slot leaves less room than the nominal budget (review #68).
pub fn creature_layer_step(top: f32, body: f32, layer_count: usize) -> f32 {
    if layer_count < 2 {
        return 0.0;
    }
    let last = (layer_count - 1) as f32;
    CREATURE_LAYER_Z
        .min(Z_STEP * CREATURE_LAYER_SPAN / last)
        .min((top - body) * 0.9 / last)
}

/// Spawn a creature under `parent`, returning its node and children.
pub fn spawn_creature(
    commands: &mut Commands,
    parent: Entity,
    data: &CreatureData,
    depth: &DepthMap,
) -> SpawnedCreature {
    let pos = creature_position(data.x, data.y);
    let slot = depth.creature_slot(data.bottom, data.id);
    let z = slot.body;

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

    // Layers ride the node, whose z is the slot's body, and the node's z is
    // recomputed by `update_creature_depth` — so the local offsets must keep the
    // whole stack inside the creature's slot between two boundary pieces. One
    // `Z_STEP` is the distance between static pieces; a per-layer step of a full
    // `Z_STEP` let the second layer draw over the next static (review #68).
    // A move never rewrites the offsets, so the step is bounded twice: by the
    // slot it is spawned in, and by the nominal span — and `slot_bounds`
    // reserves the nominal span's room at every bottom, so a stack spawned in
    // any slot stays under the next static of every other one.
    let layer_step = creature_layer_step(slot.top, z, data.layers.len());

    for (index, layer) in data.layers.iter().enumerate() {
        let local_z = index as f32 * layer_step;
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
        // The shadow rides the node but holds its own slot z — over the ground
        // the creature stands on, under its feet — so its local z cancels the
        // node's body z and leaves the slot's shadow z (review #68).
        // `sync_creature_nodes` keeps it so after every depth recomputation.
        let anim = data.shadow_animation.as_ref();
        let mut spawn = commands.spawn((
            CreatureShadow,
            Sprite {
                image: shadow.image.clone(),
                custom_size: Some(shadow.size),
                ..Default::default()
            },
            Visibility::Inherited,
            Transform::from_xyz(center.x, center.y, slot.shadow - z),
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
        DepthMap::build(vec![(100, 10.0), (200, 20.0), (300, 30.0)].into_iter())
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
        let depth = DepthMap::build(vec![(100, 10.0), (150, 5.0), (150, 30.0)].into_iter());
        assert_eq!(
            depth.z_for_bottom(150),
            30.0,
            "equal bottom keeps the top stack"
        );
        // Unsorted input still lands correctly because build sorts before search.
        let unsorted = DepthMap::build(vec![(300, 30.0), (100, 10.0), (200, 20.0)].into_iter());
        assert_eq!(
            unsorted.z_for_bottom(200),
            20.0,
            "build sorts regardless of input order"
        );
        // An out-of-order contributor (a flame riding its object's draw slot,
        // say) must not make the boundaries drop: the binary search and the
        // interpolation both assume z grows with bottom (review #68).
        let inverted = DepthMap::build(vec![(100, 10.0), (150, 20.0), (200, 15.0)].into_iter());
        let z = |b: i32| inverted.z_for_bottom(b);
        assert!(z(100) <= z(150) && z(150) <= z(200), "monotonic");
        assert!(z(125) <= z(175), "monotonic between boundaries too");
    }

    #[test]
    fn the_slot_orders_shadow_body_and_layers_inside_the_boundaries() {
        // The shadow rides the creature's slot — over the static the creature
        // stands on, under its feet — and body plus a full layer stack stays
        // under the next boundary for any bottom and id, interpolated bases
        // included (review #68).
        let depth = mock_depth();
        for bottom in [50, 100, 150, 199, 200, 250, 300, 400] {
            for id in [0, 5, CREATURE_Z_SLOTS - 1] {
                let (base, next) = depth.slot_bounds(bottom);
                let slot = depth.creature_slot(bottom, id);
                assert!(
                    slot.shadow > base && slot.shadow < slot.body,
                    "bottom {bottom} id {id}: shadow {} must sit between the \
                     base {base} and the body {}",
                    slot.shadow,
                    slot.body
                );
                let step = creature_layer_step(slot.top, slot.body, CREATURE_LAYER_CAP as usize);
                let top = slot.body + (CREATURE_LAYER_CAP - 1.0) * step;
                assert!(
                    slot.body < top && top < next,
                    "bottom {bottom} id {id}: stack [{}..{top}] must stay in \
                     [{base}..{next})",
                    slot.body
                );
            }
        }
    }

    #[test]
    fn the_whole_stack_stays_under_the_next_boundary() {
        // The old layer step was a full `Z_STEP` — the distance between static
        // pieces — so the second layer drew over the next stack. Clearance,
        // id slot and every layer must fit inside one slot below the next
        // boundary z for any bottom and id (review #68).
        let depth = mock_depth();
        // `last_stack_z` is a sentinel above the last boundary, not a drawn
        // piece: past it only the lower bound matters.
        let next = |bottom: i32| match bottom {
            b if b <= 150 => 20.0,
            b if b <= 250 => 30.0,
            _ => depth.last_stack_z + Z_STEP,
        };
        for bottom in [50, 100, 150, 200, 250, 300, 400] {
            for id in [0, 5, CREATURE_Z_SLOTS - 1] {
                let top =
                    depth.creature_z(bottom, id) + (CREATURE_LAYER_CAP - 1.0) * CREATURE_LAYER_Z;
                assert!(
                    top < next(bottom),
                    "bottom {bottom} id {id}: stack top {top} must stay under {}",
                    next(bottom)
                );
            }
        }
    }

    #[test]
    fn a_narrow_slot_keeps_every_layer_under_the_next_static() {
        // A bottom interpolating just below the next boundary must still fit
        // the whole stack: the slot reserves its room at every bottom and the
        // spawn path additionally clamps against the slot's top (review #68).
        let depth = DepthMap::build(vec![(50, 1.0), (60, 1.01), (200, 20.0)].into_iter());
        let mut app = test_app();
        let data = CreatureData {
            id: 7,
            x: 0,
            y: 0,
            bottom: 59,
            layers: (0..5)
                .map(|_| LayerData {
                    image: Handle::default(),
                    size: Vec2::ZERO,
                    offset: Vec2::ZERO,
                })
                .collect(),
            animations: (0..5).map(|_| None).collect(),
            shadow: None,
            shadow_animation: None,
        };
        let (node, layers) = {
            let mut commands = app.world_mut().commands();
            let root = commands.spawn(CreatureRoot).id();
            let spawned = spawn_creature(&mut commands, root, &data, &depth);
            (spawned.node, spawned.layers)
        };
        app.world_mut().flush();
        let node_z = app.world().get::<Transform>(node).unwrap().translation.z;
        for layer in layers {
            let local = app.world().get::<Transform>(layer).unwrap().translation.z;
            assert!(
                node_z + local < 1.01,
                "layer world z {} must stay under the next boundary 1.01",
                node_z + local
            );
        }
    }

    #[test]
    fn production_spawn_places_the_shadow_in_its_slot() {
        // The shadow rides the node, so its local z cancels the node's body z
        // down to the slot's shadow z — the same formula the viewer's depth
        // sync keeps after every recompute (review #68).
        let mut app = test_app();
        let depth = mock_depth();
        let data = CreatureData {
            id: 7,
            x: 0,
            y: 0,
            bottom: 150,
            layers: (0..3)
                .map(|_| LayerData {
                    image: Handle::default(),
                    size: Vec2::ZERO,
                    offset: Vec2::ZERO,
                })
                .collect(),
            animations: (0..3).map(|_| None).collect(),
            shadow: Some(LayerData {
                image: Handle::default(),
                size: Vec2::ZERO,
                offset: Vec2::ZERO,
            }),
            shadow_animation: None,
        };
        let (node, layers, shadow) = {
            let mut commands = app.world_mut().commands();
            let root = commands.spawn(CreatureRoot).id();
            let spawned = spawn_creature(&mut commands, root, &data, &depth);
            (spawned.node, spawned.layers, spawned.shadow.unwrap())
        };
        app.world_mut().flush();
        let node_z = app.world().get::<Transform>(node).unwrap().translation.z;
        let shadow_local = app.world().get::<Transform>(shadow).unwrap().translation.z;
        let slot = depth.creature_slot(150, 7);
        assert!(
            (node_z + shadow_local - slot.shadow).abs() < 1e-6,
            "shadow world z {} must sit at the slot's {}",
            node_z + shadow_local,
            slot.shadow
        );
        let (base, next) = depth.slot_bounds(150);
        assert!(
            slot.shadow > base && slot.shadow < node_z,
            "the shadow must ride above the ground ({base}) and below the \
             body ({node_z}), got {}",
            slot.shadow
        );
        let locals: Vec<f32> = layers
            .iter()
            .map(|entity| app.world().get::<Transform>(*entity).unwrap().translation.z)
            .collect();
        assert!(
            locals.windows(2).all(|pair| pair[0] < pair[1]),
            "layers must be strictly ordered: {locals:?}"
        );
        let top = node_z + locals.iter().copied().fold(0.0f32, f32::max);
        assert!(
            top > base && top < next,
            "the stack ({base}..{next}) must hold the whole creature, top {top}"
        );
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
        // The production formulas: the node at the slot's body, the shadow's
        // local z cancelling it down to the slot's shadow z.
        let slot = depth.creature_slot(150, 7);
        let z = slot.body;
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
                Transform::from_xyz(16.0, -8.0, slot.shadow - z),
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
