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
use kn_game::{Game, MapRef, Scene, SceneOptions, TICK_MS};
use kn_input::{ZoomScroll, drag_delta, pan_direction, wheel_steps};
use kn_render::{
    AnimPlugin, CameraInput, CameraState, CreatureAnimationPlugin, CreatureMotion, GameResource,
    MapCamera, MapCameraPlugin, PAN_SPEED, PendingFrame, TextureCache, load_scene, ortho_scale,
};
use kn_ui::pointer_over_ui;

/// Everything the player window needs to start.
pub struct PlayerConfig {
    pub game: Game,
    pub game_dir: PathBuf,
    pub hero: u32,
}

/// Seed of the creature simulation of the start map: fixed, so two runs of the
/// same hero walk the same routes, and the same value the viewer uses for its
/// "Reset positions" — one map, one reproducible spawn in either window.
pub const START_SEED: u64 = 42;

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
        .add_plugins((
            MapCameraPlugin,
            // The same clock and the same creature subsystem as the viewer: the
            // picture of a creature is built in one place (issue #70).
            AnimPlugin::new(TICK_MS as f32),
            CreatureAnimationPlugin,
        ))
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

/// Asks the shared creature subsystem for the hero's start map.
///
/// The player's whole creature wiring is this request plus the plugin that
/// answers it: the subsystem builds the simulation, the clips, the depth map and
/// the composite nodes, and drives them off the scene clock. Nothing here
/// decides a creature's state — the game layer does, and the special of a
/// state-4 creature plays once into idle on its own (issue #70).
///
/// Returns the map it asked for together with the scene it built, so the caller
/// draws the same scene it handed over, or `None` when the scene did not build.
pub fn request_start_creatures(
    game: &Game,
    hero: u32,
    motion: &mut CreatureMotion,
) -> Option<(MapRef, Scene)> {
    let map_ref = game.start_map(hero);
    let options = SceneOptions {
        characters: true,
        hero: Some(hero),
        ..SceneOptions::default()
    };
    let scene = match game.scene(map_ref, &options) {
        Ok(scene) => scene,
        Err(error) => {
            error!("не собралась карта {}: {error}", map_ref.number);
            return None;
        }
    };
    motion.request(map_ref, START_SEED, Some(hero), scene.clone());
    Some((map_ref, scene))
}

fn load_start_map(
    new_game: Res<NewGame>,
    game: Res<GameResource>,
    mut cache: ResMut<TextureCache>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    mut pending: ResMut<PendingFrame>,
    mut motion: ResMut<CreatureMotion>,
) {
    let Some((map_ref, scene)) = request_start_creatures(&game.0, new_game.hero, &mut motion)
    else {
        return;
    };
    match load_scene(
        &game.0,
        &scene,
        |_| true,
        &mut cache,
        &mut images,
        None,
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
