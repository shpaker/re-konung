//! Bevy frontend of the «Князь» player: window, camera and the app flow.
//!
//! The app crate (`kn-player`) parses CLI, loads the settings, opens the game,
//! resolves the hero and hands over a concrete [`PlayerConfig`]; this crate
//! builds the Bevy App and runs it. The application core is the Bevy-free
//! `kn-app` reducer — [`flow`] adapts it to `Update`, [`session`] builds and
//! tears down the game session it asks for.

pub mod flow;
pub mod session;

use std::path::PathBuf;

use bevy::{
    input::mouse::{AccumulatedMouseMotion, MouseWheel},
    picking::hover::Hovered,
    prelude::*,
    window::{MonitorSelection, PresentMode, VideoModeSelection, WindowMode},
};
use k1_game::lighting;
use kn_game::TICK_MS;
use kn_input::{ZoomScroll, drag_delta, pan_direction, wheel_steps};
use kn_render::{
    AnimPlugin, CameraInput, CameraState, CreatureAnimationPlugin, CreatureSim, GameResource,
    KonungLightingPlugin, LightingState, MapCamera, MapCameraPlugin, PAN_SPEED, TextureCache,
    ortho_scale, update_lighting_uniforms,
};
use kn_settings::Settings;
use kn_ui::{KonungUiPlugin, pointer_over_ui};

use flow::{PlayerFlowPlugin, screen_is_playing};
use session::NewGame;
pub use session::START_SEED;

/// Everything the player window needs to start.
pub struct PlayerConfig {
    pub game: kn_game::Game,
    pub game_dir: PathBuf,
    pub hero: u32,
    /// The settings the startup loaded (defaults on a missing or unreadable
    /// file); the window is born already in their video mode.
    pub settings: Settings,
    /// Where setting changes persist; `None` — saving will fail with a typed
    /// error (see [`flow::SettingsPath`]).
    pub settings_path: Option<PathBuf>,
    /// Whether the first frame goes through the main menu or straight into a
    /// session (`--new-game`: the menu never renders).
    pub start: StartMode,
}

/// Where the player starts.
pub enum StartMode {
    /// The main menu; a session is built only through the AppFlow pipeline.
    Menu,
    /// Straight into the session: the frontend seeds the flow with
    /// `MainMenuAction::NewGame` before the first frame.
    NewGame,
}

/// Runs the player window until the user closes it.
pub fn run_player(config: PlayerConfig) {
    let fullscreen = config.settings.video.fullscreen;
    let vsync = config.settings.video.vsync;
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: format!("re-konung — {}", config.game_dir.display()),
                    resolution: (1280, 800).into(),
                    mode: if fullscreen {
                        WindowMode::Fullscreen(
                            MonitorSelection::Current,
                            VideoModeSelection::Current,
                        )
                    } else {
                        WindowMode::Windowed
                    },
                    present_mode: if vsync {
                        PresentMode::AutoVsync
                    } else {
                        PresentMode::AutoNoVsync
                    },
                    ..default()
                }),
                ..default()
            })
            .set(ImagePlugin::default_nearest()),
    )
    .add_plugins((
        KonungUiPlugin,
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
    .add_plugins(PlayerFlowPlugin {
        settings: config.settings,
        settings_path: config.settings_path,
    });
    if matches!(config.start, StartMode::NewGame)
        && let Some(mut app_flow) = app.world_mut().get_resource_mut::<flow::AppFlow>()
    {
        // Reduced by the first frame's flush, so the menu root is never
        // spawned and the session build starts immediately.
        app_flow.submit(kn_app::Action::MainMenu(kn_app::MainMenuAction::NewGame));
    }
    app.add_systems(
        Update,
        (
            // Camera input only on the game screen, never over a menu.
            camera_control.in_set(CameraInput).run_if(screen_is_playing),
            // The state is written before the plugin's uniform writer
            // reads it, or a day change reaches the GPU a frame late
            // (review #106).
            update_lighting_state.before(update_lighting_uniforms),
        ),
    );
    app.run();
}

/// The k1 reading of `k1_game::lighting::params` into the render contract —
/// the same mapping the viewer applies: the ambient fade for every surface,
/// the ground tile-light colour and its window gate.
fn lighting_state(params: lighting::LightingParams) -> LightingState {
    LightingState {
        terrain_blend: Vec3::from(params.ambient),
        ambient_blend: Vec3::from(params.ambient),
        light_color: Vec3::from(params.ground_light),
        light_gain: if params.ground_window { 1.0 } else { 0.0 },
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
