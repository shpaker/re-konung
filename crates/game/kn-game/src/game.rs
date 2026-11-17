//! Game directory, resources and map identities.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use k1_data::{
    game_state::{self, GameState},
    map,
    repo::{FileSet, GAME_FILES, GameDir, TUTORIAL_FILES},
};
use k1_game::resources::Resources;

pub type Result<T> = std::result::Result<T, GameError>;

#[derive(Debug, Error)]
pub enum GameError {
    #[error(transparent)]
    Repo(#[from] k1_data::repo::RepoError),
    #[error(transparent)]
    Resources(#[from] k1_game::resources::ResourcesError),
    #[error(transparent)]
    Map(#[from] map::MapError),
    #[error(transparent)]
    State(#[from] game_state::StateError),
    #[error(transparent)]
    Scene(#[from] k1_game::presentation::scene::SceneError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Часть серии: первая игра или обучение ENG. Позже — K2/K2.5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MapSource {
    Game,
    Tutorial,
}

impl MapSource {
    pub(crate) fn file_set(self) -> FileSet {
        match self {
            MapSource::Game => GAME_FILES,
            MapSource::Tutorial => TUTORIAL_FILES,
        }
    }
}

impl std::fmt::Display for MapSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            MapSource::Game => "game",
            MapSource::Tutorial => "tutorial",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapRef {
    pub source: MapSource,
    pub number: u32,
}

/// Открытая игра: каталог и загруженные ресурсы.
pub struct Game {
    pub(crate) dir: GameDir,
    pub(crate) resources: Resources,
    pub(crate) session_state: Option<(u32, GameState)>,
}

impl Game {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let dir = GameDir::new(root);
        Ok(Self {
            session_state: None,
            resources: Resources::load(&dir)?,
            dir,
        })
    }

    pub fn root(&self) -> &std::path::Path {
        self.dir.root()
    }

    /// Все карты: игра, затем обучение (оно есть только в ENG).
    pub fn maps(&self) -> Result<Vec<MapRef>> {
        let mut maps = self
            .dir
            .map_numbers(GAME_FILES)?
            .into_iter()
            .map(|number| MapRef {
                source: MapSource::Game,
                number,
            })
            .collect::<Vec<_>>();
        maps.extend(
            self.dir
                .map_numbers(TUTORIAL_FILES)?
                .into_iter()
                .map(|number| MapRef {
                    source: MapSource::Tutorial,
                    number,
                }),
        );
        Ok(maps)
    }
}
