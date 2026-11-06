//! Отображение сцены игр серии «Князь» на Bevy.
//!
//! Тонкий слой над общей моделью [`kn_game::Scene`]: текстуры кэшируются по
//! [`kn_game::TextureId`], куски сцены становятся спрайтами в порядке
//! наложения, камера записывается здесь, а управляется привязками приложений.
//! Игровой логики и знания форматов здесь нет — только Bevy и координаты.

pub mod animation;
pub mod camera;
pub mod creature_animation;
pub mod creatures;
pub mod scene;
pub mod textures;

pub use animation::{
    AnimPlugin, AnimSet, Animator, Clock, Frame, PlayState, Sequence, SpriteView, frame_view,
};
pub use camera::{
    CameraApply, CameraBounds, CameraInput, CameraState, MapCamera, MapCameraPlugin, PAN_SPEED,
    PendingFrame, ZOOM_LEVELS, fitting_zoom, frame_scene, ortho_scale,
};
pub use creature_animation::{
    Clip, CreatureAnim, CreatureAnimReport, CreatureAnimationPlugin, CreatureDepth, CreatureMotion,
    CreatureNodesVisible, CreatureSequences, CreatureSet, CreatureSim, CreatureSpawns,
    CreatureSprite, LayerSequences, StepKey, advance_creature_sim, build_creature_anim,
    build_creature_spawns, build_depth_map, prepare_creatures, reset_creature_animations,
    reset_creature_positions, reset_creature_sim, select_clip, spawn_creature_nodes,
    sync_creature_animation, sync_creature_nodes,
};
pub use creatures::{
    CreatureData, CreatureLayer, CreatureNode, CreatureRoot, CreatureShadow, CreatureSlot,
    DepthMap, LayerAnimation, LayerData, SpawnedCreature, clear_creatures, creature_layer_step,
    despawn_creature, spawn_creature, update_creature_depth,
};
pub use scene::{
    MapRoot, SceneGroup, SceneLoad, ScenePiece, Z_STEP, clear_scene, load_scene, piece_center,
    piece_sequence, piece_size, piece_transform, scene_bounds, spawn_scene,
};
pub use textures::{TextureCache, TextureEntry};

use bevy::prelude::*;

/// Открытая игра как ресурс Bevy: обёртка, потому что [`kn_game::Game`] не знает про ECS.
#[derive(Resource)]
pub struct GameResource(pub kn_game::Game);
