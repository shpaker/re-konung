//! Bevy frontend of the «Князь» player: window, camera, start-map loading.
//!
//! The app crate (`kn-player`) parses CLI, opens the game, resolves the hero
//! and hands over a concrete [`PlayerConfig`]; this crate builds the Bevy App
//! and runs it.

use std::path::PathBuf;

use bevy::{
    input::mouse::{AccumulatedMouseMotion, MouseWheel},
    picking::hover::Hovered,
    prelude::*,
};
use kn_game::{Game, SceneOptions};
use kn_input::{ZoomScroll, drag_delta, pan_direction, wheel_steps};
use kn_render::{
    CameraInput, CameraState, GameResource, MapCamera, MapCameraPlugin, PAN_SPEED, PendingFrame,
    TextureCache, load_scene, ortho_scale,
};
use kn_ui::pointer_over_ui;

/// Everything the player window needs to start.
pub struct PlayerConfig {
    pub game: Game,
    pub game_dir: PathBuf,
    pub hero: u32,
}

/// Runs the player window until the user closes it.
pub fn run_player(config: PlayerConfig) {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: format!("re-konung — {}", config.game_dir.display()),
                        resolution: (1280, 800).into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(ImagePlugin::default_nearest()),
        )
        .add_plugins(MapCameraPlugin)
        .insert_resource(GameResource(config.game))
        .init_resource::<TextureCache>()
        .insert_resource(NewGame { hero: config.hero })
        .add_systems(Startup, load_start_map)
        .add_systems(Update, camera_control.in_set(CameraInput))
        .run();
}

/// Hero for the new game: the start map and the on-map position depend on it.
#[derive(Resource)]
struct NewGame {
    hero: u32,
}

fn load_start_map(
    new_game: Res<NewGame>,
    game: Res<GameResource>,
    mut cache: ResMut<TextureCache>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    mut pending: ResMut<PendingFrame>,
) {
    let map_ref = game.0.start_map(new_game.hero);
    let options = SceneOptions {
        characters: true,
        hero: Some(new_game.hero),
        ..SceneOptions::default()
    };
    let scene = match game.0.scene(map_ref, &options) {
        Ok(scene) => scene,
        Err(error) => {
            error!("не собралась карта {}: {error}", map_ref.number);
            return;
        }
    };
    match load_scene(
        &game.0,
        &scene,
        |_| true,
        &mut cache,
        &mut images,
        &mut commands,
        &mut pending,
    ) {
        Ok(loaded) => {
            for problem in &loaded.problems {
                warn!("карта {}: {problem}", map_ref.number);
            }
        }
        Err(error) => {
            error!("не загрузились текстуры: {error}");
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn camera_control(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    over_ui_nodes: Query<&Hovered, With<Node>>,
    time: Res<Time>,
    mut state: ResMut<CameraState>,
    camera: Single<(&mut Transform, &Projection), With<MapCamera>>,
    mut scroll: Local<ZoomScroll>,
) {
    let over_ui = pointer_over_ui(over_ui_nodes.iter().map(|hovered| hovered.0));
    let direction = pan_direction(&keys);
    let drag = if over_ui {
        None
    } else {
        drag_delta(&buttons, &motion)
    };
    let steps = wheel_steps(&mut scroll, &mut wheel, over_ui);
    let (mut transform, projection) = camera.into_inner();
    let scale = ortho_scale(projection);
    if direction != Vec2::ZERO {
        transform.translation.x += direction.x * PAN_SPEED * time.delta_secs() * scale;
        transform.translation.y += direction.y * PAN_SPEED * time.delta_secs() * scale;
    }
    if let Some(delta) = drag {
        transform.translation.x -= delta.x * scale;
        transform.translation.y += delta.y * scale;
    }
    if steps != 0 {
        state.step(steps);
    }
}
