//! `re-konung` — the «Князь» player.
//!
//! Bootstrap order: settings first (they shape the window before it exists),
//! then the game and its heroes, then the Bevy frontend (`kn-fe-player`). A
//! session is never built here — only the frontend's AppFlow pipeline builds
//! one.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use kn_fe_player::{PlayerConfig, StartMode, run_player};
use kn_game::{Game, MapSource};
use kn_settings::{LoadError, Settings, default_path, defaults};

#[derive(Parser, Debug)]
#[command(name = "re-konung", version, about)]
struct Cli {
    /// Installed game directory, containing the unpacked inst/setup.dat files
    #[arg(long, env = "RE_KONUNG_GAME_DIR")]
    game_dir: PathBuf,
    /// Start straight into a new game, skipping the main menu
    #[arg(long)]
    new_game: bool,
    /// Settings file path; defaults to config_dir()/re-konung/settings.json
    #[arg(long, env = "RE_KONUNG_SETTINGS")]
    settings: Option<PathBuf>,
}

/// The settings the app starts on, and where later changes persist.
///
/// A missing file is a first run: defaults, silently. A file that cannot be
/// read is diagnosed and replaced by defaults, but never touched — the user's
/// bytes survive for a fix or a newer binary. An unresolvable default path
/// means saving will fail with a typed error when it is attempted.
fn bootstrap_settings(path: Option<PathBuf>) -> (Settings, Option<PathBuf>) {
    let Some(path) = path.or_else(default_path) else {
        eprintln!(
            "no settings path: the OS config directory is unresolvable, \
             running on defaults and saving will fail"
        );
        return (defaults(), None);
    };
    match kn_settings::load(&path) {
        Ok(settings) => (settings, Some(path)),
        Err(LoadError::Missing) => (defaults(), Some(path)),
        Err(LoadError::Corrupted) => {
            eprintln!(
                "settings at {} are corrupted; running on defaults, the file is left as is",
                path.display()
            );
            (defaults(), Some(path))
        }
        Err(LoadError::UnsupportedVersion(version)) => {
            eprintln!(
                "settings at {} are of a newer format version ({version}); \
                 running on defaults, the file is left as is",
                path.display()
            );
            (defaults(), Some(path))
        }
        Err(LoadError::Io(error)) => {
            eprintln!(
                "settings at {} cannot be read: {error}; running on defaults, \
                 the file is left as is",
                path.display()
            );
            (defaults(), Some(path))
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let (settings, settings_path) = bootstrap_settings(cli.settings);
    let game = match Game::open(&cli.game_dir) {
        Ok(game) => game,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let heroes = match game.heroes(MapSource::Game) {
        Ok(heroes) if !heroes.is_empty() => heroes,
        Ok(_) => {
            eprintln!("{}: нет Game.N — не с чего начать", cli.game_dir.display());
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    run_player(PlayerConfig {
        game,
        game_dir: cli.game_dir,
        heroes,
        settings,
        settings_path,
        start: if cli.new_game {
            StartMode::NewGame
        } else {
            StartMode::Menu
        },
    });
    ExitCode::SUCCESS
}
