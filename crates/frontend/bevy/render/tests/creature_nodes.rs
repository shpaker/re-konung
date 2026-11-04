//! Creature-node invariants on synthetic data (no `original/`): the whole point of
//! the stable [`kn_render::CreatureNode`] is that the node owns the world position
//! while its sprite children carry only local offsets and per-frame deltas. These
//! drive the real animation and transform systems, so they catch the regressions
//! the review flagged (world jump on an animation frame change, layers not riding
//! the node, shadow above the body) without any game files.

use std::sync::Arc;

use bevy::prelude::*;
use bevy::transform::TransformPlugin;
use kn_render::{
    AnimPlugin, Clock, CreatureData, CreatureNode, DepthMap, Frame, LayerAnimation, LayerData,
    PlayState, Sequence, spawn_creature,
};

/// The entities the spawn system created, kept as plain `Copy` handles.
#[derive(Resource, Default, Clone)]
struct Holder {
    node: Option<Entity>,
    layers: Vec<Entity>,
    shadow: Option<Entity>,
}

/// Two frames of clearly different size and offset, so a frame change moves and
/// resizes the child sprite — the thing that must never touch the node.
fn frames() -> Vec<Frame> {
    vec![
        Frame {
            image: Some(Handle::default()),
            size: Vec2::new(10.0, 10.0),
            offset: Vec2::ZERO,
            ticks: 1,
        },
        Frame {
            image: Some(Handle::default()),
            size: Vec2::new(20.0, 20.0),
            offset: Vec2::new(2.0, 3.0),
            ticks: 1,
        },
    ]
}

fn creature_data() -> CreatureData {
    CreatureData {
        id: 7,
        x: 100,
        y: 200,
        bottom: 150,
        layers: vec![
            LayerData {
                image: Handle::default(),
                size: Vec2::new(10.0, 10.0),
                offset: Vec2::new(5.0, 40.0),
            },
            LayerData {
                image: Handle::default(),
                size: Vec2::new(10.0, 10.0),
                offset: Vec2::new(6.0, 42.0),
            },
        ],
        animations: vec![
            Some(LayerAnimation {
                sequence: Arc::new(Sequence::new(frames())),
                base: 0,
                anchor: (5, 40),
            }),
            None,
        ],
        shadow: Some(LayerData {
            image: Handle::default(),
            size: Vec2::new(20.0, 8.0),
            offset: Vec2::new(0.0, 48.0),
        }),
        shadow_animation: None,
    }
}

/// A world with the animation and transform systems, a paused clock the test
/// drives by hand, and one creature spawned at frame 0 of its resting sequence.
fn scene() -> (App, Holder) {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, AnimPlugin::new(60.0)));
    app.insert_resource(DepthMap::build(std::iter::empty(), -0.01));
    app.world_mut()
        .resource_mut::<Clock>()
        .goto(PlayState::Paused);
    app.init_resource::<Holder>();
    app.add_systems(
        Update,
        |mut commands: Commands,
         mut holder: ResMut<Holder>,
         mut once: Local<bool>,
         depth: Res<DepthMap>| {
            if *once {
                return;
            }
            *once = true;
            // The root needs a Transform so the hierarchy propagates: without it
            // the node is not reached by the transform system and children never
            // follow it — exactly the wiring a real CreatureRoot provides.
            let root = commands.spawn(Transform::IDENTITY).id();
            let spawned = spawn_creature(&mut commands, root, &creature_data(), &depth);
            holder.node = Some(spawned.node);
            holder.layers = spawned.layers;
            holder.shadow = spawned.shadow;
        },
    );
    app.update();
    let holder = app.world().resource::<Holder>().clone();
    assert!(holder.node.is_some(), "creature spawned");
    (app, holder)
}

fn node(holder: &Holder) -> Entity {
    holder.node.expect("node")
}

#[test]
fn an_animation_frame_change_does_not_move_the_node_world_position() {
    let (app, holder) = scene();
    let before = *app.world().get::<Transform>(node(&holder)).unwrap();
    // The node has no Animator, so `apply_animators` cannot touch it.
    assert!(
        app.world()
            .get::<kn_render::Animator>(node(&holder))
            .is_none()
    );

    let mut app = app;
    app.world_mut().resource_mut::<Clock>().ticks = 1;
    app.update();

    let after = *app.world().get::<Transform>(node(&holder)).unwrap();
    assert_eq!(
        after.translation, before.translation,
        "an animation frame must never translate the creature's world node"
    );
    // Yet the animated child did change: its sprite resized to the second frame.
    let child = holder.layers[0];
    assert_eq!(
        app.world().get::<Sprite>(child).unwrap().custom_size,
        Some(Vec2::new(20.0, 20.0)),
        "the child sprite follows the frame, not the node"
    );
}

#[test]
fn body_equipment_and_shadow_ride_the_node() {
    let (app, holder) = scene();
    let mut app = app;
    let children: Vec<Entity> = holder.layers.iter().copied().chain(holder.shadow).collect();
    let before: Vec<Vec3> = children
        .iter()
        .map(|e| {
            app.world()
                .get::<GlobalTransform>(*e)
                .unwrap()
                .translation()
        })
        .collect();

    app.world_mut()
        .get_mut::<Transform>(node(&holder))
        .unwrap()
        .translation += Vec3::new(29.0, -16.0, 3.0);
    app.update(); // GlobalTransform propagation runs in PostUpdate

    for (child, before) in children.iter().zip(&before) {
        let after = app
            .world()
            .get::<GlobalTransform>(*child)
            .unwrap()
            .translation();
        let delta = after - before;
        assert_eq!(delta.x, 29.0, "child {child:?} x must track the node");
        assert_eq!(delta.y, -16.0, "child {child:?} y must track the node");
    }
}

#[test]
fn the_shadow_sits_below_the_body_layers() {
    let (app, holder) = scene();
    let shadow = holder.shadow.expect("shadow spawned");
    let shadow_z = app.world().get::<Transform>(shadow).unwrap().translation.z;
    for layer in &holder.layers {
        let body_z = app.world().get::<Transform>(*layer).unwrap().translation.z;
        assert!(
            shadow_z < body_z,
            "shadow local z {shadow_z} must stay below body layer z {body_z}"
        );
    }
}

#[test]
fn body_layers_keep_their_scene_order_depth() {
    let (app, holder) = scene();
    let first = app
        .world()
        .get::<Transform>(holder.layers[0])
        .unwrap()
        .translation
        .z;
    let second = app
        .world()
        .get::<Transform>(holder.layers[1])
        .unwrap()
        .translation
        .z;
    assert!(
        second > first,
        "later body layers must sit above earlier ones so equipment reads over the body"
    );
}

#[test]
fn the_node_is_a_parented_creature_node() {
    let (app, holder) = scene();
    let node = node(&holder);
    assert!(app.world().get::<CreatureNode>(node).is_some());
    // Parented under a root, so hiding that root hides the whole creature.
    assert!(app.world().get::<ChildOf>(node).is_some());
}

#[test]
fn a_clip_swap_reanchors_the_image_and_never_moves_the_node() {
    // The #68 jump: a walk clip is composed around its own base corner, so
    // swapping the sequence must move the anchor with it. Left on the resting
    // anchor the sprite renders displaced by the corner difference — the
    // periodic one-cell image jump — while the node itself must stay put.
    let (mut app, holder) = scene();
    let child = holder.layers[0];
    let before = *app.world().get::<Transform>(node(&holder)).unwrap();

    // A walk clip anchored at (33, 26): base frame 12×24 at dx=dy=0.
    let walk = Arc::new(Sequence::new(vec![Frame {
        image: Some(Handle::default()),
        size: Vec2::new(12.0, 24.0),
        offset: Vec2::ZERO,
        ticks: 1,
    }]));
    {
        let mut animator = app
            .world_mut()
            .get_mut::<kn_render::Animator>(child)
            .unwrap();
        animator.play_at(Arc::clone(&walk), 0, 7, (33, 26));
    }
    app.update();

    let after = *app.world().get::<Transform>(node(&holder)).unwrap();
    assert_eq!(
        after.translation, before.translation,
        "a clip swap is a display change, not a world move"
    );
    let transform = app.world().get::<Transform>(child).unwrap();
    assert_eq!(
        (transform.translation.x, transform.translation.y),
        (39.0, -38.0),
        "the base frame sits on the new clip's own corner, not the resting one"
    );
    let global = *app.world().get::<GlobalTransform>(child).unwrap();
    assert_eq!(
        (global.translation().x, global.translation().y),
        (139.0, -238.0),
        "the image rides the node, composited over the new anchor"
    );
}
