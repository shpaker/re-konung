//! Sprite geometry rides the creature's own scene composition base, while the
//! node's world position rides the simulation (review of #101, finding P2-#2).
//!
//! The pieces are composed around `(px, py)` by `k1_game::scene::pose_in`, so
//! an absolute piece corner is `base + internal offset`. The plan must store
//! the *internal* offset: reducing against the simulation's initial pixel
//! instead displaces the image on the node by `base − origin` — zero for a
//! creature the scene and the simulation agree on, large for a roaming member
//! the scene stands at the squad's spawn centre while the simulation runs it
//! from its record cell. Synthetic data: no `original/` needed.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use kn_game::{Piece, PieceGroup, Scene};
use kn_render::{
    Clip, CreatureAnim, CreatureSequences, Frame, LayerSequences, Sequence, TextureCache,
    build_creature_spawns,
};

const CREATURE: usize = 7;
/// The base the scene composed this creature around: `compose` placed the
/// body at base + (4, −40) and the shadow at base + (10, −10).
const BASE: (i32, i32) = (100, 200);
/// The simulation's initial pixel for the same creature — a different point,
/// as it is for a roaming member.
const SIM_ORIGIN: (i32, i32) = (158, 216);

fn body() -> Piece {
    Piece {
        texture: None,
        x: BASE.0 + 4,
        y: BASE.1 - 40,
        w: Some(20),
        h: Some(40),
        depth_bottom: None,
        animation: None,
        group: PieceGroup::Characters,
        creature_id: Some(CREATURE),
    }
}

fn shadow() -> Piece {
    Piece {
        texture: None,
        x: BASE.0 + 10,
        y: BASE.1 - 10,
        w: Some(24),
        h: Some(8),
        depth_bottom: None,
        animation: None,
        group: PieceGroup::CharacterShadows,
        creature_id: Some(CREATURE),
    }
}

/// A body whose bottom edge sits at `bottom_edge` (its top is 40 px above), so a
/// caller can place the occlusion line below the base and tell the base-relative
/// height apart from both the simulation-origin distance and the absolute scene
/// bottom.
fn body_with_bottom(bottom_edge: i32) -> Piece {
    Piece {
        y: bottom_edge - 40,
        ..body()
    }
}

/// One-frame resting clip whose base corner is the piece's own corner: the
/// shape `build_creature_anim` produces for a static layer.
fn rest_clip(corner: (i32, i32), size: (f32, f32)) -> Clip {
    Clip {
        sequence: Arc::new(Sequence::new(vec![Frame::new(
            Handle::default(),
            Vec2::new(size.0, size.1),
            Vec2::ZERO,
        )])),
        base: 0,
        corner,
    }
}

/// The roster geometry the renderer builds nodes from (`creature_pieces`),
/// composed around [`BASE`], with the Scene origin recorded for that creature.
/// `pieces` stays empty: the digest source list is irrelevant here and the
/// renderer reads geometry from `creature_pieces` (review #101, P2-1).
fn scene() -> Scene {
    Scene {
        pieces: Vec::new(),
        creature_pieces: vec![body(), shadow()],
        creature_origins: HashMap::from([(CREATURE, BASE)]),
    }
}

/// Resting sequences whose clip corner equals the composed piece corner, the
/// shape the animation build produces for a static layer.
fn anim() -> CreatureAnim {
    CreatureAnim {
        creatures: HashMap::from([(
            CREATURE,
            CreatureSequences {
                body: HashMap::from([(
                    0,
                    LayerSequences {
                        rest: Some(rest_clip((BASE.0 + 4, BASE.1 - 40), (20.0, 40.0))),
                        walk: Vec::new(),
                        special: None,
                    },
                )]),
                shadow: LayerSequences {
                    rest: Some(rest_clip((BASE.0 + 10, BASE.1 - 10), (24.0, 8.0))),
                    walk: Vec::new(),
                    special: None,
                },
            },
        )]),
        locked: HashMap::new(),
        stages: HashMap::new(),
        special_frames: HashMap::new(),
    }
}

#[test]
fn sprite_offsets_are_internal_geometry_not_distance_to_the_simulation() {
    let spawns = build_creature_spawns(
        &scene(),
        &TextureCache::default(),
        &anim(),
        &[(CREATURE, SIM_ORIGIN)],
    );
    let plan = spawns
        .plans
        .iter()
        .find(|plan| plan.id as usize == CREATURE)
        .unwrap_or_else(|| panic!("creature #{CREATURE} must get a plan"));

    // The body: piece − base, the offset `compose` gave it. Reducing against
    // the simulation origin would give (104−158, 160−216) = (−54, −56).
    assert_eq!(
        plan.layers[0].offset,
        Vec2::new(4.0, -40.0),
        "the body offset is the internal geometry (piece − scene base), not (piece − simulation origin)"
    );
    let shadow = plan
        .shadow
        .as_ref()
        .unwrap_or_else(|| panic!("the shadow must ride the plan"));
    assert_eq!(
        shadow.offset,
        Vec2::new(10.0, -10.0),
        "the shadow offset too: piece − base"
    );

    // The resting anchor follows the same reduction: the clip corner is an
    // absolute scene corner, and the node carries the world position.
    let animation = plan.animations[0]
        .as_ref()
        .unwrap_or_else(|| panic!("the body layer must keep its resting animation"));
    assert_eq!(
        animation.anchor,
        (4, -40),
        "the anchor is the clip corner reduced by the scene base"
    );
    let shadow_animation = plan
        .shadow_animation
        .as_ref()
        .unwrap_or_else(|| panic!("the shadow must keep its resting animation"));
    assert_eq!(shadow_animation.anchor, (10, -10));

    // The world semantics stay with the simulation: the node places from the
    // simulation origin, and the body's bottom edge lies exactly on the base
    // (`piece.y + h = 200 = base.y`), so its height above the base is 0 and the
    // occlusion bottom sits on the live node row (`origin.y`), not the absolute
    // scene bottom. Reporting the scene bottom (200) here — the P2-1 bug — would
    // put the creature at the wrong depth once #99 relocated it off its base.
    assert_eq!(
        (plan.x, plan.y),
        SIM_ORIGIN,
        "the node places from the simulation origin, not the scene base"
    );
    assert_eq!(
        spawns.heights.get(&CREATURE).copied(),
        Some(0),
        "a body whose bottom edge sits on the base has height 0 above the base"
    );
    assert_eq!(
        plan.bottom, SIM_ORIGIN.1,
        "the occlusion bottom is the live row (origin.y) plus the height above the base"
    );
}

/// A creature the Scene carries no origin for keeps the historical identity:
/// the offset and anchor reduce against the simulation origin itself, so a
/// scene built outside the roster creature path is unchanged (review #101,
/// P2-2 — the fallback must not regress pre-P2 callers).
#[test]
fn a_creature_without_a_recorded_scene_base_falls_back_to_the_origin() {
    let mut scene = scene();
    scene.creature_origins = HashMap::new();
    let spawns = build_creature_spawns(
        &scene,
        &TextureCache::default(),
        &anim(),
        &[(CREATURE, SIM_ORIGIN)],
    );
    let plan = spawns
        .plans
        .iter()
        .find(|plan| plan.id as usize == CREATURE)
        .expect("the creature still gets a plan");
    // Without a base the reduction is against the simulation origin: the body
    // corner (104, 160) less SIM_ORIGIN (158, 216).
    assert_eq!(
        plan.layers[0].offset,
        Vec2::new(
            (BASE.0 + 4 - SIM_ORIGIN.0) as f32,
            (BASE.1 - 40 - SIM_ORIGIN.1) as f32
        ),
        "no recorded base reduces by the simulation origin (identity with the pre-P2 path)"
    );
    assert_eq!(
        spawns.bases.get(&CREATURE).copied(),
        Some(SIM_ORIGIN),
        "the base defaults to the origin"
    );
}

/// The occlusion bottom follows the creature's live row even when the body's
/// bottom edge sits below the base and the simulation relocated the creature
/// (`base != origin`): the height is measured from the base, and `bottom` is the
/// simulation row plus that height — never the absolute scene bottom, and never
/// the height measured from the simulation origin (review #101, finding P2-1).
#[test]
fn a_body_below_the_base_keeps_its_depth_on_the_relocated_row() {
    // base.y = 200, origin.y = 216 (relocated 16 px down); the body's bottom edge
    // sits 20 px below the base at 220 → height above the base 20, occlusion
    // bottom 216 + 20 = 236. The P2-1 bug reported the absolute scene bottom
    // (220) and a height from the origin (220 − 216 = 4).
    let mut scene = scene();
    scene.creature_pieces = vec![body_with_bottom(BASE.1 + 20), shadow()];
    let spawns = build_creature_spawns(
        &scene,
        &TextureCache::default(),
        &anim(),
        &[(CREATURE, SIM_ORIGIN)],
    );
    let plan = spawns
        .plans
        .iter()
        .find(|plan| plan.id as usize == CREATURE)
        .unwrap_or_else(|| panic!("creature #{CREATURE} must get a plan"));
    assert_eq!(
        spawns.heights.get(&CREATURE).copied(),
        Some(20),
        "height is measured from the base (220 − 200), not the simulation origin \
         (220 − 216 = 4), which would mis-occlude a relocated creature"
    );
    assert_eq!(
        plan.bottom,
        SIM_ORIGIN.1 + 20,
        "the occlusion bottom rides the live row (origin.y) plus the body height \
         above its base, not the absolute scene bottom (220)"
    );
}
