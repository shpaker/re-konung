//! `re-konung` — запуск игры «Князь».
//!
//! Сейчас это заготовка: открывается стартовая карта новой игры тем же
//! конвейером, что и в просмотрщике, камера управляется пользователем.
//! Игрового процесса пока нет — NPC, бой и квесты появятся позже.

use std::path::PathBuf;
use std::process::ExitCode;

use bevy::prelude::*;
use clap::Parser;
use kn_game::{Game, MapRef, MapSource, SceneOptions};
use kn_io::{GameResource, MapCameraPlugin, PendingFrame, TextureCache, frame_scene, spawn_scene};

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
        .run();
    ExitCode::SUCCESS
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
    frame_scene(&scene, &cache, &mut pending);
}
