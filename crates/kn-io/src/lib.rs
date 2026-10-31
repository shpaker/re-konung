//! Отображение сцены игр серии «Князь» на Bevy.
//!
//! Тонкий слой над общей моделью [`kn_game::Scene`]: текстуры кэшируются по
//! [`kn_game::TextureId`], куски сцены становятся спрайтами в порядке
//! наложения, камера управляется пользователем. Игровой логики и знания
//! форматов здесь нет — только Bevy и координаты.

pub mod camera;
pub mod scene;
pub mod textures;

pub use camera::{
    CameraBounds, CameraState, MapCamera, MapCameraPlugin, PendingFrame, ZOOM_LEVELS, ZoomScroll,
    fitting_zoom, frame_scene,
};
pub use scene::{
    MapRoot, ScenePiece, Z_STEP, clear_scene, piece_size, piece_transform, scene_bounds,
    spawn_scene,
};
pub use textures::{TextureCache, TextureEntry};

use bevy::prelude::*;

/// Открытая игра как ресурс Bevy: обёртка, потому что [`kn_game::Game`] не знает про ECS.
#[derive(Resource)]
pub struct GameResource(pub kn_game::Game);
