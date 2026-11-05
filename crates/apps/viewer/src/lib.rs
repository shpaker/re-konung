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

/// Прогон симуляции по картам: `map` — одна карта, `all` — все. Отчёты
/// возвращаются целиком — печатает и решает про код возврата `simulate`.
pub fn simulate_reports(
    cli: &Cli,
    map: Option<u32>,
    all: bool,
    ticks: u64,
    seed: u64,
) -> Result<Vec<SimReport>, ViewerError> {
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
                // The world refused to place these at load: the records
                // carry cells no creature can stand on (docs/k1/creatures.md,
                // «Load-time refusals»). That is a fact about the file, so
                // the report presents it — it does not fail the run.
                let unplaced: Vec<String> = sim
                    .spawn_failures()
                    .iter()
                    .map(|(id, error)| format!("roster {id}: {error:?}"))
                    .collect();
                // Squad members the load rule trimmed for want of room in
                // the spawn area — the original's `GroupSize` shrink, also
                // data, never a refusal.
                let trimmed = sim.load_trimmed().to_vec();
                let mut steps = 0usize;
                let mut route_sets = 0usize;
                // Distinct creatures that started at least one step over the whole
                // run — not the ones caught mid-step at the last tick, which is
                // what `movers` counts (review #68: the two must not be conflated).
                let mut ever_walked: std::collections::HashSet<u32> =
                    std::collections::HashSet::new();
                for _ in 0..ticks {
                    let events = sim.tick();
                    for event in &events {
                        match event {
                            kn_game::movement::world::Event::StepStarted { creature, .. } => {
                                steps += 1;
                                ever_walked.insert(creature.raw());
                            }
                            kn_game::movement::world::Event::RouteSet { .. } => route_sets += 1,
                            _ => {}
                        }
                    }
                }
                // The counters of the finished run: arrivals are counted by
                // the façade from the movement events it saw while ticking.
                let goal_reaches = sim
                    .goal_reaches()
                    .into_iter()
                    .map(|reach| GoalReachCount {
                        id: reach.id,
                        workplace: reach.workplace,
                        walk: reach.walk,
                    })
                    .collect();
                // Orders rejected for a reason other than "no route" are bugs or
                // bad data, never a verdict to fold into a success count.
                let errors: Vec<String> = sim
                    .order_failures()
                    .iter()
                    .map(|(id, error)| format!("order of roster {id}: {error:?}"))
                    .collect();
                let states = sim.states();
                let movers = states.iter().filter(|s| s.moving).count();
                reports.push(SimReport {
                    source: map_ref.source,
                    number: map_ref.number,
                    creatures,
                    ever_moved: ever_walked.len(),
                    movers,
                    steps,
                    route_sets,
                    tick: ticks,
                    unplaced,
                    trimmed,
                    goal_reaches,
                    errors,
                });
            }
            Err(e) => {
                reports.push(SimReport {
                    source: map_ref.source,
                    number: map_ref.number,
                    creatures: 0,
                    ever_moved: 0,
                    movers: 0,
                    steps: 0,
                    route_sets: 0,
                    tick: ticks,
                    unplaced: Vec::new(),
                    trimmed: Vec::new(),
                    goal_reaches: Vec::new(),
                    errors: vec![e.to_string()],
                });
            }
        }
    }
    Ok(reports)
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
    let reports = simulate_reports(cli, map, all, ticks, seed)?;
    match format {
        Format::Text => {
            for r in &reports {
                let spawns = if r.unplaced.is_empty() {
                    String::new()
                } else {
                    format!(" · {} not spawned", r.unplaced.len())
                };
                let trimmed = if r.trimmed.is_empty() {
                    String::new()
                } else {
                    format!(" · {} trimmed", r.trimmed.len())
                };
                let workplace: usize = r.goal_reaches.iter().map(|g| g.workplace).sum();
                let walk: usize = r.goal_reaches.iter().map(|g| g.walk).sum();
                let reaches = if workplace + walk == 0 {
                    String::new()
                } else {
                    format!(" · {workplace} workplace, {walk} walk arrivals")
                };
                println!(
                    "{} {:>2}: {} creatures, {} ever moved, {} moving, {} steps, {} routes, {} ticks, {} errors{spawns}{trimmed}{reaches}",
                    match r.source {
                        MapSource::Game => "game    ",
                        MapSource::Tutorial => "tutorial",
                    },
                    r.number,
                    r.creatures,
                    r.ever_moved,
                    r.movers,
                    r.steps,
                    r.route_sets,
                    r.tick,
                    r.errors.len(),
                );
                for e in &r.errors {
                    println!("  - {e}");
                }
                for e in &r.unplaced {
                    println!("  ! {e}");
                }
                for id in &r.trimmed {
                    println!("  ~ roster {id}: trimmed, no room in its area");
                }
                for g in &r.goal_reaches {
                    println!(
                        "  → roster {}: {} workplace, {} walk arrivals",
                        g.id, g.workplace, g.walk
                    );
                }
            }
        }
        Format::Json => println!("{}", serde_json::to_string_pretty(&reports)?),
    }

    Ok(if reports_ok(&reports) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// A run fails only on a real error — a build failure or an order rejected
/// for a reason other than "no route". What the load made of the records is
/// not an error: refusals and trims are facts about the shipped files, and
/// the report presents them as data (issue #82). Nothing here speaks for the
/// picture: the check runs no renderer, so render correctness is judged only
/// by looking at the viewer or the Bevy continuity test.
fn reports_ok(reports: &[SimReport]) -> bool {
    reports.iter().all(|report| report.errors.is_empty())
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SimReport {
    pub source: MapSource,
    pub number: u32,
    pub creatures: usize,
    /// Creatures that started at least one step during the whole run.
    pub ever_moved: usize,
    /// Creatures mid-step at the last tick — a snapshot, not the run's coverage.
    pub movers: usize,
    pub steps: usize,
    pub route_sets: usize,
    pub tick: u64,
    /// Creatures the world refused to place at load and why, e.g.
    /// `"roster 7: ImpassableCell"` — a fact about the records, not a failure.
    pub unplaced: Vec<String>,
    /// Roster numbers of squad members the load rule trimmed for want of
    /// room in the spawn area — the original's `GroupSize` shrink.
    pub trimmed: Vec<usize>,
    /// Completed confirmed orders per creature (docs/k1/creatures.md,
    /// "Goal reach"): arrivals on the workplace and on walk targets.
    pub goal_reaches: Vec<GoalReachCount>,
    /// Build failures and orders rejected for a reason other than "no route".
    pub errors: Vec<String>,
}

/// One creature's goal-reach counters, as the report carries them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct GoalReachCount {
    pub id: usize,
    pub workplace: usize,
    pub walk: usize,
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

    #[test]
    fn load_refusals_are_data_and_only_errors_fail_a_run() {
        let clean = SimReport {
            source: MapSource::Game,
            number: 1,
            creatures: 3,
            ever_moved: 3,
            movers: 2,
            steps: 5,
            route_sets: 1,
            tick: 10,
            unplaced: Vec::new(),
            trimmed: Vec::new(),
            goal_reaches: Vec::new(),
            errors: Vec::new(),
        };
        assert!(reports_ok(std::slice::from_ref(&clean)));
        // What the load made of the records is presented, not punished:
        // the shipped files carry cells no creature can stand on and
        // squads whose area holds no room for every member (issue #82).
        let refused = SimReport {
            unplaced: vec!["roster 7: ImpassableCell".to_owned()],
            ..clean.clone()
        };
        assert!(
            reports_ok(&[refused]),
            "a load-time refusal is a fact about the file, not a failure"
        );
        let trimmed = SimReport {
            trimmed: vec![5],
            ..clean.clone()
        };
        assert!(reports_ok(&[trimmed]), "a trimmed squad is data too");
        let errored = SimReport {
            errors: vec!["boom".to_owned()],
            ..clean
        };
        assert!(!reports_ok(&[errored]));
    }
}
