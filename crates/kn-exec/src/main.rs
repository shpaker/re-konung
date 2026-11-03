//! `re-konung` — запуск игры «Князь».
//!
//! Сейчас это заготовка: открывается стартовая карта новой игры тем же
//! конвейером, что и в просмотрщике, камера управляется пользователем.
//! Игрового процесса пока нет — NPC, бой и квесты появятся позже.

use std::path::PathBuf;
use std::process::ExitCode;

use bevy::{
    input::mouse::{AccumulatedMouseMotion, MouseWheel},
    picking::hover::Hovered,
    prelude::*,
};
use clap::Parser;
use kn_game::{Game, MapRef, MapSource, SceneOptions};
use kn_input::{ZoomScroll, drag_delta, pan_direction, wheel_steps};
use kn_render::{
    CameraInput, CameraState, GameResource, MapCamera, MapCameraPlugin, PAN_SPEED, PendingFrame,
    TextureCache, frame_scene, ortho_scale, pointer_over_ui, spawn_scene,
};

#[derive(Parser, Debug)]
#[command(name = "re-konung", version, about)]
struct Cli {
    /// Каталог установленной игры — содержимое inst/setup.dat
    #[arg(long)]
    game_dir: PathBuf,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let game = match Game::open(&cli.game_dir) {
        Ok(game) => game,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let Some(hero) = game
        .heroes(MapSource::Game)
        .ok()
        .and_then(|h| h.first().copied())
    else {
        eprintln!("{}: нет Game.N — не с чего начать", cli.game_dir.display());
        return ExitCode::FAILURE;
    };

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: format!("re-konung — {}", cli.game_dir.display()),
                        resolution: (1280, 800).into(),
                        ..default()
                    }),
                    ..default()
                })
                // Пиксельная графика: без сглаживания при дробном масштабе
                .set(ImagePlugin::default_nearest()),
        )
        .add_plugins(MapCameraPlugin)
        .insert_resource(GameResource(game))
        .init_resource::<TextureCache>()
        .insert_resource(NewGame { hero })
        .add_systems(Startup, load_start_map)
        .add_systems(Update, camera_control.in_set(CameraInput))
        .run();
    ExitCode::SUCCESS
}

/// Camera-control binding: applies the kn-input mechanisms to this app's camera.
/// All computation lives in kn-input and kn-render; only the wiring is here.
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
        // Screen y points down, world y points up
        transform.translation.y += delta.y * scale;
    }
    if steps != 0 {
        state.step(steps);
    }
}

/// Новая игра: герой и его стартовая карта.
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
    let map_ref: MapRef = game.0.start_map(new_game.hero);
    // Карта так, как её видит игрок: отладочных слоёв нет, но отряд на месте
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
    let ids = scene.texture_ids();
    let problems = match cache.load(&game.0, &ids, &mut images) {
        Ok(problems) => problems,
        Err(error) => {
            error!("не загрузились текстуры: {error}");
            return;
        }
    };
    for problem in &problems {
        warn!("карта {}: {problem}", map_ref.number);
    }
    spawn_scene(&mut commands, &scene, &cache);
    // Игра показывает карту целиком, без отладочных слоёв: границами считается
    // весь список отрисовки
    frame_scene(&scene, &cache, |_| true, &mut pending);
}
