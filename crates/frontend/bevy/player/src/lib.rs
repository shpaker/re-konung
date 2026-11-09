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
use k1_data::repo::{GAME_FILES, GameDir};
use k1_game::lighting;
use kn_game::{Game, MapRef, Scene, SceneOptions, TICK_MS};
use kn_input::{ZoomScroll, drag_delta, pan_direction, wheel_steps};
use kn_render::{
    AnimPlugin, CameraInput, CameraState, CreatureAnimationPlugin, CreatureMotion, CreatureSim,
    FineLightGrid, GameResource, KonungLightMaterial, KonungLightingPlugin, LightingMaterials,
    LightingState, MapCamera, MapCameraPlugin, PAN_SPEED, PendingFrame, TextureCache, load_scene,
    ortho_scale,
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
            // The same scene lighting as the viewer: uniforms from the
            // simulation tick, one writer in kn-render.
            KonungLightingPlugin,
        ))
        .insert_resource(GameResource(config.game))
        .init_resource::<TextureCache>()
        .insert_resource(NewGame { hero: config.hero })
        .add_systems(Startup, load_start_map)
        .add_systems(
            Update,
            (camera_control.in_set(CameraInput), update_lighting_state),
        )
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

#[allow(clippy::too_many_arguments)]
fn load_start_map(
    new_game: Res<NewGame>,
    game: Res<GameResource>,
    mut cache: ResMut<TextureCache>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<KonungLightMaterial>>,
    mut commands: Commands,
    mut pending: ResMut<PendingFrame>,
    mut motion: ResMut<CreatureMotion>,
) {
    let Some((map_ref, scene)) = request_start_creatures(&game.0, new_game.hero, &mut motion)
    else {
        return;
    };
    // The lighting of the start map: the pool from LIGHTS.RES and this map's
    // fine grid, the same resources the viewer provides. Uniforms arrive every
    // frame from the simulation tick through `update_lighting_state`.
    let pool = LightingMaterials::new(&mut images, &mut materials, &lights_res_masks(&game.0));
    commands.insert_resource(FineLightGrid(map_fine_lights(&game.0, map_ref)));
    let loaded = load_scene(
        &game.0,
        &scene,
        |_| true,
        &mut cache,
        &mut images,
        Some(&pool),
        &mut commands,
        &mut pending,
    );
    commands.insert_resource(pool);
    match loaded {
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

/// The LIGHTS.RES tile masks of the opened game: raw brightness bytes with
/// their pixel size, ready for [`LightingMaterials::new`]. A game without a
/// readable LIGHTS.RES gets an empty pool — the scene still lights, the tiles
/// just carry no tile mask.
fn lights_res_masks(game: &Game) -> Vec<(Vec<u8>, u32, u32)> {
    let Some(path) = k1_game::resources::find_named(game.root(), "lights.res") else {
        return Vec::new();
    };
    let Ok(res) = k1_data::lights_res::parse(&std::fs::read(path).unwrap_or_default()) else {
        return Vec::new();
    };
    res.tiles
        .into_iter()
        .map(|mask| {
            (
                mask,
                k1_data::lights_res::TILE_MASK_WIDTH as u32,
                k1_data::lights_res::TILE_MASK_HEIGHT as u32,
            )
        })
        .collect()
}

/// The map's LIGHTS.N fine grid — the tile-light index per fine cell, what the
/// creature lighting reads to tell a lit patch from a dark one. `None` when
/// the map has no LIGHTS.N: every cell counts as dark.
fn map_fine_lights(game: &Game, map_ref: MapRef) -> Option<Vec<Vec<u8>>> {
    let bytes = GameDir::new(game.root())
        .lights_bytes(map_ref.number, GAME_FILES)
        .ok()??;
    k1_data::map::parse_lights(&bytes)
        .ok()
        .map(|lights| lights.fine)
}

/// The k1 reading of `k1_game::lighting::params` into the render contract —
/// the same mapping the viewer applies: the ground ambient only inside the
/// ground-light window, the plain ambient always, the ground tile-light colour.
fn lighting_state(params: lighting::LightingParams) -> LightingState {
    LightingState {
        terrain_blend: params.ground_ambient.map_or(Vec3::ZERO, Vec3::from),
        ambient_blend: Vec3::from(params.ambient),
        light_color: Vec3::from(params.ground_light),
    }
}

/// Writes the lighting state for the simulation's current tick. Without a
/// simulation nothing is written: the contract keeps its safe zero default
/// instead of lighting a frozen scene from an invented clock.
fn update_lighting_state(sim: Option<Res<CreatureSim>>, mut state: ResMut<LightingState>) {
    let Some(tick) = sim
        .as_ref()
        .and_then(|sim| sim.sim.as_ref())
        .map(|simulation| simulation.tick_number())
    else {
        return;
    };
    let next = lighting_state(lighting::params(tick, START_SEED));
    if *state != next {
        *state = next;
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
