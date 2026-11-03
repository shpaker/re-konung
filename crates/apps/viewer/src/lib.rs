//! The viewer app: CLI resolution and the headless check.
//!
//! The window itself lives in `kn-fe-viewer`; this crate opens the game,
//! resolves the startup selection and runs the headless diagnostics. The check
//! never creates a Bevy App, a window or a GPU context (the binary still links
//! Bevy transitively, which is fine).

pub mod cli;

use std::process::ExitCode;

use kn_fe_viewer::ViewerConfig;
use kn_game::{Game, MapRef, MapSource};
use thiserror::Error;

use crate::cli::{Cli, Format};

#[derive(Debug, Error)]
pub enum ViewerError {
    #[error(transparent)]
    Game(#[from] kn_game::GameError),
    #[error("{dir}: нет карт источника {map_source}")]
    NoMaps {
        dir: std::path::PathBuf,
        map_source: MapSource,
    },
    #[error("{map_source} {number}: карты нет в {dir}")]
    NoSuchMap {
        dir: std::path::PathBuf,
        map_source: MapSource,
        number: u32,
    },
    #[error("нужно --map N или --all")]
    MapNotSelected,
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Opens the game and resolves the startup selection; errors keep the old CLI messages.
pub fn viewer_config(cli: &Cli) -> Result<ViewerConfig, ViewerError> {
    let game = Game::open(&cli.game_dir)?;
    let maps = game.maps()?;
    if maps.is_empty() {
        return Err(ViewerError::NoMaps {
            dir: cli.game_dir.clone(),
            map_source: cli.source.into(),
        });
    }
    // Неверный номер карты — ошибка, а не тихий откат на первую
    let selected = cli.initial_map(&maps);
    if selected.is_none()
        && let Some(number) = cli.map
    {
        return Err(ViewerError::NoSuchMap {
            dir: cli.game_dir.clone(),
            map_source: cli.source.into(),
            number,
        });
    }
    Ok(ViewerConfig {
        game,
        game_dir: cli.game_dir.clone(),
        maps,
        selected: selected.unwrap_or(0),
    })
}

/// Headless-проверка карт: без окна, GPU и рендерера.
pub fn run_check(cli: &Cli, map: Option<u32>, all: bool, format: Format) -> ExitCode {
    match check(cli, map, all, format) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// Отчёт по картам источника: `map` — одна карта, `all` — все.
pub fn check(
    cli: &Cli,
    map: Option<u32>,
    all: bool,
    format: Format,
) -> Result<ExitCode, ViewerError> {
    let game = Game::open(&cli.game_dir)?;
    let source: MapSource = cli.source.into();
    let available = game.maps()?;
    let maps = if all {
        available
            .into_iter()
            .filter(|map| map.source == source)
            .collect::<Vec<_>>()
    } else {
        let number = map.ok_or(ViewerError::MapNotSelected)?;
        let map_ref = MapRef { source, number };
        if !available.contains(&map_ref) {
            return Err(ViewerError::NoSuchMap {
                dir: cli.game_dir.clone(),
                map_source: source,
                number,
            });
        }
        vec![map_ref]
    };
    if maps.is_empty() {
        return Err(ViewerError::NoMaps {
            dir: cli.game_dir.clone(),
            map_source: source,
        });
    }
    let report = game.check(Some(&maps))?;
    match format {
        Format::Text => print!("{report}"),
        Format::Json => println!("{}", serde_json::to_string_pretty(&report)?),
    }
    Ok(if report.ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_opens_only_a_real_game_dir() {
        let dir = tempfile::tempdir().unwrap();
        let cli = Cli {
            game_dir: dir.path().to_path_buf(),
            map: None,
            source: crate::cli::SourceArg::Game,
            command: None,
        };
        assert!(matches!(
            check(&cli, Some(1), false, Format::Text),
            Err(ViewerError::Game(_))
        ));
    }
}
