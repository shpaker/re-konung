//! `re-konung` — the «Князь» player.
//!
//! Opens the game, resolves the hero from the starting state and hands over a
//! [`PlayerConfig`] to the Bevy frontend (`kn-fe-player`).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use kn_fe_player::{PlayerConfig, run_player};
use kn_game::{Game, MapSource};

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

    run_player(PlayerConfig {
        game,
        game_dir: cli.game_dir,
        hero,
    });
    ExitCode::SUCCESS
}
