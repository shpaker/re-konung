//! Слой репозитория: где лежат файлы игры и как их найти
//! (эталон — `konung/k1/map_file.py`, функции работы с папкой игры).

use std::path::{Path, PathBuf};

use thiserror::Error;

pub type Result<T> = std::result::Result<T, RepoError>;

#[derive(Debug, Error)]
pub enum RepoError {
    #[error("{dir}: нет {prefix}.{number}")]
    NotFound {
        dir: PathBuf,
        prefix: String,
        number: u32,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Префиксы имён файлов одного набора карт: карта, её свет, начальные состояния.
///
/// Номер у карты и света — номер локации, у состояния — номер героя.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileSet {
    pub map: &'static str,
    pub lights: &'static str,
    pub state: &'static str,
}

pub const GAME_FILES: FileSet = FileSet {
    map: "konung",
    lights: "lights",
    state: "game",
};

/// Обучение, только в ENG: TUTORIAL.EXE собирает имена TUTORIAL.N, TLIGHTS.N
/// и TGAME.N, а GRAPH.RES, Objects.res, LIGHTS.RES и FLAMES.RES у него общие
/// с игрой. Источник — строки имён файлов в TUTORIAL.EXE (наш эксперимент).
pub const TUTORIAL_FILES: FileSet = FileSet {
    map: "tutorial",
    lights: "tlights",
    state: "tgame",
};

/// Набор, к которому относится файл карты: tutorial.N — обучение, остальное — игра.
pub fn file_set(map_name: &str) -> FileSet {
    let stem = map_name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_lowercase();
    if stem == TUTORIAL_FILES.map {
        TUTORIAL_FILES
    } else {
        GAME_FILES
    }
}

/// Папка установленной игры — содержимое `inst/setup.dat` (docs/k1/editions.md).
#[derive(Debug, Clone)]
pub struct GameDir {
    root: PathBuf,
}

impl GameDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Файл вида `Konung.N` без учёта регистра (в дистрибутиве он разный).
    pub fn find(&self, prefix: &str, number: u32) -> Result<PathBuf> {
        let name = format!("{prefix}.{number}").to_lowercase();
        for entry in std::fs::read_dir(&self.root)? {
            let path = entry?.path();
            let matches = path
                .file_name()
                .and_then(|file_name| file_name.to_str())
                .is_some_and(|file_name| file_name.to_lowercase() == name);
            if matches {
                return Ok(path);
            }
        }
        Err(RepoError::NotFound {
            dir: self.root.clone(),
            prefix: prefix.to_string(),
            number,
        })
    }

    /// Номера файлов вида `prefix.N` в папке, без учёта регистра.
    pub fn numbered(&self, prefix: &str) -> Result<Vec<u32>> {
        let mut numbers = Vec::new();
        for entry in std::fs::read_dir(&self.root)? {
            let Some(name) = entry?
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
            else {
                continue;
            };
            let Some((stem, suffix)) = name.split_once('.') else {
                continue;
            };
            if stem.eq_ignore_ascii_case(prefix)
                && let Ok(number) = suffix.parse::<u32>()
            {
                numbers.push(number);
            }
        }
        numbers.sort_unstable();
        Ok(numbers)
    }

    /// Номера карт `Konung.N` (или `Tutorial.N` обучения) в папке игры.
    pub fn map_numbers(&self, files: FileSet) -> Result<Vec<u32>> {
        self.numbered(files.map)
    }

    /// `LIGHTS.N` карты (`TLights.N` у обучения); у карт 6, 8 и 46 его нет.
    pub fn lights_path(&self, number: u32, files: FileSet) -> Option<PathBuf> {
        self.find(files.lights, number).ok()
    }

    /// Байты карты из набора файлов.
    pub fn map_bytes(&self, number: u32, files: FileSet) -> Result<Vec<u8>> {
        Ok(std::fs::read(self.find(files.map, number)?)?)
    }

    /// Байты света карты; `None` — у карты нет `LIGHTS.N`.
    pub fn lights_bytes(&self, number: u32, files: FileSet) -> Result<Option<Vec<u8>>> {
        match self.lights_path(number, files) {
            Some(path) => Ok(Some(std::fs::read(path)?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    /// Синтетическая папка игры: имена в разном регистре, как в дистрибутиве.
    fn game_dir() -> (TempDir, GameDir) {
        let dir = TempDir::new().unwrap();
        for name in [
            "KONUNG.1",
            "konung.2",
            "Konung.10",
            "LIGHTS.1",
            "lights.2",
            "game.0",
            "Tutorial.4",
            "TLIGHTS.4",
            "TGame.1",
            "notes.txt",
        ] {
            fs::write(dir.path().join(name), b"").unwrap();
        }
        let game = GameDir::new(dir.path());
        (dir, game)
    }

    #[test]
    fn find_ignores_case() {
        let (_dir, game) = game_dir();
        assert_eq!(
            game.find("konung", 1).unwrap().file_name().unwrap(),
            "KONUNG.1"
        );
        assert_eq!(
            game.find("konung", 10).unwrap().file_name().unwrap(),
            "Konung.10"
        );
        assert!(matches!(
            game.find("konung", 3),
            Err(RepoError::NotFound { number: 3, .. })
        ));
    }

    #[test]
    fn numbered_sorts_and_ignores_case() {
        let (_dir, game) = game_dir();
        assert_eq!(game.numbered("konung").unwrap(), [1, 2, 10]);
        assert_eq!(game.numbered("lights").unwrap(), [1, 2]);
        assert_eq!(game.map_numbers(GAME_FILES).unwrap(), [1, 2, 10]);
        assert_eq!(game.map_numbers(TUTORIAL_FILES).unwrap(), [4]);
        assert!(game.numbered("quest").unwrap().is_empty());
    }

    #[test]
    fn lights_path_is_none_when_missing() {
        let (_dir, game) = game_dir();
        assert!(game.lights_path(1, GAME_FILES).is_some());
        assert!(game.lights_path(10, GAME_FILES).is_none());
        assert_eq!(
            game.lights_path(4, TUTORIAL_FILES)
                .unwrap()
                .file_name()
                .unwrap(),
            "TLIGHTS.4"
        );
    }

    #[test]
    fn map_bytes_reads_file() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Konung.7"), b"map data").unwrap();
        let game = GameDir::new(dir.path());
        assert_eq!(game.map_bytes(7, GAME_FILES).unwrap(), b"map data");
        assert_eq!(game.lights_bytes(7, GAME_FILES).unwrap(), None);
    }

    #[test]
    fn file_set_follows_map_name() {
        assert_eq!(file_set("Tutorial.4"), TUTORIAL_FILES);
        assert_eq!(file_set("TUTORIAL.5"), TUTORIAL_FILES);
        for name in ["Konung.4", "KONUNG.12", "konung.1"] {
            assert_eq!(file_set(name), GAME_FILES);
        }
    }
}
