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
            None => panic!("ожидался check"),
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
            None => panic!("ожидался check"),
        }
    }
}
