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

/// Headless-прогон симуляции: существа ходят, печатается отчёт.
pub fn run_simulate(
    cli: &Cli,
    map: Option<u32>,
    all: bool,
    ticks: u64,
    seed: u64,
    format: Format,
) -> ExitCode {
    match simulate(cli, map, all, ticks, seed, format) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// Прогон симуляции по картам: `map` — одна, `all` — все.
pub fn simulate(
    cli: &Cli,
    map: Option<u32>,
    all: bool,
    ticks: u64,
    seed: u64,
    format: Format,
) -> Result<ExitCode, ViewerError> {
    let game = Game::open(&cli.game_dir)?;
    let source: MapSource = cli.source.into();
    let available = game.maps()?;
    let maps = if all {
        available
            .into_iter()
            .filter(|m| m.source == source)
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

    let mut reports = Vec::new();
    for &map_ref in &maps {
        match game.simulation(map_ref, seed) {
            Ok(mut sim) => {
                let creatures = sim.creature_count();
                // Creatures the world refused to place are a data problem, not a
                // silent gap: report each with the reason instead of dropping it.
                let spawn_errors: Vec<String> = sim
                    .spawn_failures()
                    .iter()
                    .map(|(id, error)| format!("roster {id}: {error:?}"))
                    .collect();
                let mut steps = 0usize;
                let mut route_sets = 0usize;
                let errors: Vec<String> = Vec::new();
                for _ in 0..ticks {
                    let events = sim.tick();
                    for event in &events {
                        match event {
                            kn_game::movement::world::Event::StepStarted { .. } => steps += 1,
                            kn_game::movement::world::Event::RouteSet { .. } => route_sets += 1,
                            _ => {}
                        }
                    }
                }
                let states = sim.states();
                let movers = states.iter().filter(|s| s.moving).count();
                reports.push(SimReport {
                    source: map_ref.source,
                    number: map_ref.number,
                    creatures,
                    movers,
                    steps,
                    route_sets,
                    tick: ticks,
                    spawn_errors,
                    errors,
                });
            }
            Err(e) => {
                reports.push(SimReport {
                    source: map_ref.source,
                    number: map_ref.number,
                    creatures: 0,
                    movers: 0,
                    steps: 0,
                    route_sets: 0,
                    tick: ticks,
                    spawn_errors: Vec::new(),
                    errors: vec![e.to_string()],
                });
            }
        }
    }

    match format {
        Format::Text => {
            for r in &reports {
                let spawns = if r.spawn_errors.is_empty() {
                    String::new()
                } else {
                    format!(" · {} not spawned", r.spawn_errors.len())
                };
                println!(
                    "{} {:>2}: {} creatures, {} moved, {} steps, {} routes, {} ticks, {} errors{spawns}",
                    match r.source {
                        MapSource::Game => "game    ",
                        MapSource::Tutorial => "tutorial",
                    },
                    r.number,
                    r.creatures,
                    r.movers,
                    r.steps,
                    r.route_sets,
                    r.tick,
                    r.errors.len(),
                );
                for e in &r.errors {
                    println!("  - {e}");
                }
                for e in &r.spawn_errors {
                    println!("  ! {e}");
                }
            }
        }
        Format::Json => println!("{}", serde_json::to_string_pretty(&reports)?),
    }

    let ok = reports.iter().all(|r| r.errors.is_empty());
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

#[derive(serde::Serialize)]
struct SimReport {
    source: MapSource,
    number: u32,
    creatures: usize,
    movers: usize,
    steps: usize,
    route_sets: usize,
    tick: u64,
    /// Creatures the world refused to place and why, e.g. `"roster 7: ImpassableCell"`.
    spawn_errors: Vec<String>,
    errors: Vec<String>,
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
