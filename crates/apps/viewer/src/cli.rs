//! Аргументы командной строки просмотрщика.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use kn_game::MapSource;

/// Просмотрщик карт «Князя»: окно с картой и headless-диагностика.
#[derive(Parser, Debug)]
#[command(name = "re-konung-viewer", version, about)]
pub struct Cli {
    /// Каталог установленной игры — содержимое inst/setup.dat
    #[arg(long)]
    pub game_dir: PathBuf,

    /// Номер карты, открытой при запуске
    #[arg(long)]
    pub map: Option<u32>,

    /// Источник карт: игра или обучение (обучение есть только в ENG)
    #[arg(long, value_enum, default_value = "game", global = true)]
    pub source: SourceArg,

    /// Тик, на котором закрепить освещение сцены (без него освещение следует
    /// за тиком симуляции; ночь — например, 11000)
    #[arg(long, global = true)]
    pub light_tick: Option<u64>,

    /// Open with the visible day/night cycle held off ("constant day"): the
    /// ambient stays at its day value while time keeps running. The same
    /// flag the `Constant day` checkbox and the `N` hotkey drive in the window
    #[arg(long, global = true)]
    pub constant_day: bool,

    /// Снять скриншот карты в файл и выйти
    #[arg(long, global = true)]
    pub screenshot: Option<PathBuf>,

    /// Run N measured frames after the warm-up, print one `BENCH …` line to
    /// stdout and exit; a benchmark run never captures a screenshot
    #[arg(
        long,
        global = true,
        value_parser = clap::value_parser!(u32).range(1..),
        conflicts_with = "screenshot"
    )]
    pub benchmark: Option<u32>,

    /// Run the measured window until SECS seconds of frame time have
    /// accumulated, print one `BENCH …` line to stdout and exit; the frame
    /// count becomes an output, so the 60-second protocol stays the same
    /// recipe on any hardware. Never captures a screenshot
    #[arg(
        long = "benchmark-secs",
        global = true,
        value_parser = clap::value_parser!(u64).range(1..),
        conflicts_with_all = ["benchmark", "screenshot"],
    )]
    pub benchmark_secs: Option<u64>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Проверка без окна, GPU и рендерера: сцена строится, текстуры декодируются
    Check {
        /// Номер карты
        #[arg(long, conflicts_with = "all")]
        map: Option<u32>,

        /// Все карты источника
        #[arg(long)]
        all: bool,

        /// Формат отчёта
        #[arg(long, value_enum, default_value = "text")]
        format: Format,
    },

    /// Headless-прогон симуляции: существа ходят, отчёт по тикам
    Simulate {
        /// Номер карты
        #[arg(long, conflicts_with = "all")]
        map: Option<u32>,

        /// Все карты источника
        #[arg(long)]
        all: bool,

        /// Число тиков прогона
        #[arg(long, default_value = "600")]
        ticks: u64,

        /// Seed для детерминизма
        #[arg(long, default_value = "42")]
        seed: u64,

        /// Формат отчёта
        #[arg(long, value_enum, default_value = "text")]
        format: Format,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceArg {
    Game,
    Tutorial,
}

impl From<SourceArg> for MapSource {
    fn from(source: SourceArg) -> Self {
        match source {
            SourceArg::Game => MapSource::Game,
            SourceArg::Tutorial => MapSource::Tutorial,
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Text,
    Json,
}

impl Cli {
    /// Карта, выбранная при запуске; None — первая карта источника.
    pub fn initial_map(&self, maps: &[kn_game::MapRef]) -> Option<usize> {
        let source: MapSource = self.source.into();
        maps.iter().position(|map| {
            map.source == source && self.map.is_none_or(|number| map.number == number)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kn_game::MapRef;

    fn maps() -> Vec<MapRef> {
        vec![
            MapRef {
                source: MapSource::Game,
                number: 1,
            },
            MapRef {
                source: MapSource::Game,
                number: 2,
            },
            MapRef {
                source: MapSource::Tutorial,
                number: 4,
            },
        ]
    }

    fn cli(args: &[&str]) -> Cli {
        let mut all = vec!["re-konung-viewer"];
        all.extend_from_slice(args);
        Cli::parse_from(all)
    }

    #[test]
    fn screenshot_flags_parse_global() {
        let parsed = cli(&[
            "--game-dir",
            "g",
            "--map",
            "17",
            "--light-tick",
            "11000",
            "--screenshot",
            "out/plugin-night.png",
        ]);
        assert_eq!(parsed.light_tick, Some(11000));
        assert_eq!(
            parsed.screenshot,
            Some(PathBuf::from("out/plugin-night.png"))
        );
        // The subcommands stay reachable with the flags present
        match cli(&["--game-dir", "g", "--light-tick", "5", "check", "--all"]).command {
            Some(Command::Check { all, .. }) => assert!(all),
            other => panic!("expected check, got {other:?}"),
        }
    }

    #[test]
    fn initial_map_follows_source_and_number() {
        assert_eq!(cli(&["--game-dir", "g"]).initial_map(&maps()), Some(0));
        assert_eq!(
            cli(&["--game-dir", "g", "--map", "2"]).initial_map(&maps()),
            Some(1)
        );
        assert_eq!(
            cli(&["--game-dir", "g", "--source", "tutorial"]).initial_map(&maps()),
            Some(2)
        );
    }

    #[test]
    fn unknown_map_falls_back_to_none() {
        assert_eq!(
            cli(&["--game-dir", "g", "--map", "99"]).initial_map(&maps()),
            None
        );
    }

    #[test]
    fn check_subcommand_takes_its_own_map() {
        match cli(&["--game-dir", "g", "check", "--map", "7"]).command {
            Some(Command::Check { map, all, format }) => {
                assert_eq!(map, Some(7));
                assert!(!all);
                assert_eq!(format, Format::Text);
            }
            other => panic!("expected check, got {other:?}"),
        }
    }

    #[test]
    fn check_all_conflicts_with_map() {
        let parsed = Cli::try_parse_from([
            "re-konung-viewer",
            "--game-dir",
            "g",
            "check",
            "--all",
            "--map",
            "1",
        ]);
        assert!(parsed.is_err());
    }

    #[test]
    fn json_format_is_parsed() {
        match cli(&["--game-dir", "g", "check", "--all", "--format", "json"]).command {
            Some(Command::Check { format, all, .. }) => {
                assert_eq!(format, Format::Json);
                assert!(all);
            }
            other => panic!("expected check, got {other:?}"),
        }
    }

    #[test]
    fn benchmark_flag_parses_and_stays_separate_from_capture() {
        let parsed = cli(&["--game-dir", "g", "--map", "17", "--benchmark", "600"]);
        assert_eq!(parsed.benchmark, Some(600));
        // A benchmark run must not capture: the pair is a CLI error up front
        let both = Cli::try_parse_from([
            "re-konung-viewer",
            "--game-dir",
            "g",
            "--benchmark",
            "600",
            "--screenshot",
            "out/x.png",
        ]);
        assert!(both.is_err(), "--benchmark conflicts with --screenshot");
        // Zero frames measure nothing: rejected at the CLI, not at runtime
        let zero = Cli::try_parse_from(["re-konung-viewer", "--game-dir", "g", "--benchmark", "0"]);
        assert!(zero.is_err(), "--benchmark 0 is not a run");
        // The subcommands stay reachable with the flag present
        match cli(&["--game-dir", "g", "--benchmark", "10", "check", "--all"]).command {
            Some(Command::Check { all, .. }) => assert!(all),
            other => panic!("expected check, got {other:?}"),
        }
    }

    #[test]
    fn benchmark_secs_flag_parses_and_stays_separate() {
        // The 60-second gate protocol (#98): one recipe, duration-bound, so
        // the frame count is an output and not part of the command.
        let parsed = cli(&["--game-dir", "g", "--map", "17", "--benchmark-secs", "60"]);
        assert_eq!(parsed.benchmark_secs, Some(60));
        // One bound per run: frames and seconds are alternatives
        let both = Cli::try_parse_from([
            "re-konung-viewer",
            "--game-dir",
            "g",
            "--benchmark",
            "600",
            "--benchmark-secs",
            "60",
        ]);
        assert!(both.is_err(), "--benchmark-secs conflicts with --benchmark");
        // A benchmark run must not capture either
        let capture = Cli::try_parse_from([
            "re-konung-viewer",
            "--game-dir",
            "g",
            "--benchmark-secs",
            "60",
            "--screenshot",
            "out/x.png",
        ]);
        assert!(
            capture.is_err(),
            "--benchmark-secs conflicts with --screenshot"
        );
        // A window shorter than one frame measures nothing: rejected up front
        let zero = Cli::try_parse_from([
            "re-konung-viewer",
            "--game-dir",
            "g",
            "--benchmark-secs",
            "0",
        ]);
        assert!(zero.is_err(), "--benchmark-secs 0 is not a run");
        // The subcommands stay reachable with the flag present
        match cli(&[
            "--game-dir",
            "g",
            "--benchmark-secs",
            "60",
            "check",
            "--all",
        ])
        .command
        {
            Some(Command::Check { all, .. }) => assert!(all),
            other => panic!("expected check, got {other:?}"),
        }
    }

    #[test]
    fn constant_day_flag_parses_global() {
        // The capture/benchmark hook for the switch: the same state the panel
        // checkbox and `N` drive, pinned for a headless run.
        let parsed = cli(&[
            "--game-dir",
            "g",
            "--map",
            "17",
            "--light-tick",
            "11000",
            "--constant-day",
            "--screenshot",
            "out/constant-day-night.png",
        ]);
        assert!(parsed.constant_day);
        assert_eq!(parsed.light_tick, Some(11000));
        // Default: the cycle runs.
        assert!(!cli(&["--game-dir", "g"]).constant_day);
        // The subcommands stay reachable with the flag present
        match cli(&["--game-dir", "g", "--constant-day", "check", "--all"]).command {
            Some(Command::Check { all, .. }) => assert!(all),
            other => panic!("expected check, got {other:?}"),
        }
    }

    #[test]
    fn simulate_subcommand_parses_defaults() {
        match cli(&["--game-dir", "g", "simulate", "--map", "19"]).command {
            Some(Command::Simulate {
                map,
                all,
                ticks,
                seed,
                format,
            }) => {
                assert_eq!(map, Some(19));
                assert!(!all);
                assert_eq!(ticks, 600);
                assert_eq!(seed, 42);
                assert_eq!(format, Format::Text);
            }
            other => panic!("expected simulate, got {other:?}"),
        }
    }
}
