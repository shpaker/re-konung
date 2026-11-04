//! Отображение сцены игр серии «Князь» на Bevy.
//!
//! Тонкий слой над общей моделью [`kn_game::Scene`]: текстуры кэшируются по
//! [`kn_game::TextureId`], куски сцены становятся спрайтами в порядке
//! наложения, камера записывается здесь, а управляется привязками приложений.
//! Игровой логики и знания форматов здесь нет — только Bevy и координаты.

pub mod animation;
pub mod camera;
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
pub use creatures::{
    CreatureData, CreatureLayer, CreatureNode, CreatureRoot, CreatureShadow, DepthMap,
    LayerAnimation, LayerData, SpawnedCreature, clear_creatures, despawn_creature, spawn_creature,
    update_creature_depth,
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
