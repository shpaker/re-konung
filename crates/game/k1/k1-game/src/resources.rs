//! Загрузка графических ресурсов игры из каталога `GameDir`.
//!
//! GRAPH.RES и OBJECTS.RES хранятся байтами и разбираются на время операции
//! ([Resources::with_graphics]): их структуры заимствуют буфер, а парсинг
//! быстр относительно однократного построения сцены. Таблицы EXE, маски света
//! и кадры огней владеют своими данными и разбираются один раз.

use std::path::{Path, PathBuf};

use thiserror::Error;

use k1_data::{
    executable::{self, Executable},
    flames_res::{self, FlamesRes},
    graph_res::{self, GraphRes},
    lights_res::{self, LightsRes},
    objects_res::{self, ObjectsRes},
    repo::GameDir,
};

pub type Result<T> = std::result::Result<T, ResourcesError>;

#[derive(Debug, Error)]
pub enum ResourcesError {
    #[error("{dir}: нет {name}")]
    NotFound { dir: PathBuf, name: &'static str },
    #[error(transparent)]
    Executable(#[from] executable::ExecutableError),
    #[error(transparent)]
    Graph(#[from] graph_res::GraphError),
    #[error(transparent)]
    Objects(#[from] objects_res::ObjectsError),
    #[error(transparent)]
    Lights(#[from] lights_res::LightsError),
    #[error(transparent)]
    Flames(#[from] flames_res::FlamesError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Файл с фиксированным именем без учёта регистра (в дистрибутиве он разный).
pub fn find_named(root: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(root).ok()?.flatten().find_map(|entry| {
        let path = entry.path();
        path.file_name()
            .and_then(|file| file.to_str())
            .is_some_and(|file| file.eq_ignore_ascii_case(name))
            .then_some(path)
    })
}

fn read_named(root: &Path, name: &'static str) -> Result<Vec<u8>> {
    let path = find_named(root, name).ok_or_else(|| ResourcesError::NotFound {
        dir: root.to_path_buf(),
        name,
    })?;
    Ok(std::fs::read(path)?)
}

#[derive(Debug)]
pub struct Resources {
    graph_bytes: Vec<u8>,
    objects_bytes: Vec<u8>,
    pub tables: Executable,
    pub lights: LightsRes,
    pub flames: FlamesRes,
}

impl Resources {
    /// GRAPH.RES, Objects.res, LIGHTS.RES, FLAMES.RES и таблицы из EXE.
    pub fn load(game: &GameDir) -> Result<Self> {
        let root = game.root();
        let exe_path = executable::find(root)?;
        Ok(Self {
            graph_bytes: read_named(root, "graph.res")?,
            objects_bytes: read_named(root, "objects.res")?,
            tables: executable::parse(&std::fs::read(exe_path)?)?,
            lights: lights_res::parse(&read_named(root, "lights.res")?)?,
            flames: flames_res::parse(&read_named(root, "flames.res")?)?,
        })
    }

    /// Разобранные GRAPH.RES и OBJECTS.RES на время операции `f`.
    pub fn with_graphics<R>(
        &self,
        f: impl FnOnce(&GraphRes<'_>, &ObjectsRes<'_>) -> R,
    ) -> Result<R> {
        let graph = graph_res::parse(&self.graph_bytes)?;
        let objects = objects_res::parse(&self.objects_bytes)?;
        Ok(f(&graph, &objects))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_named_ignores_case() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("GRAPH.RES"), b"x").unwrap();
        assert_eq!(
            find_named(dir.path(), "graph.res"),
            Some(dir.path().join("GRAPH.RES"))
        );
        assert_eq!(find_named(dir.path(), "objects.res"), None);
    }

    #[test]
    fn load_reports_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let game = GameDir::new(dir.path());
        let error = Resources::load(&game).unwrap_err();
        assert!(
            matches!(
                &error,
                ResourcesError::Executable(executable::ExecutableError::NoFile { .. })
            ),
            "{error}"
        );
    }
}
