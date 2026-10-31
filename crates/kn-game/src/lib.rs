//! Общий фасад игры: Bevy-независимая модель сцены и доступ к данным.
//!
//! Потребители (`kn-io`, бинарники) видят только этот крейт: [Game] открывает
//! каталог игры, [Game::scene] строит [Scene] — упорядоченный список кусков
//! с непрозрачными ключами текстур, [Game::textures] декодирует их в RGBA.
//! Специфика «Князя 1» спрятана в `k1-game` и `k1-data`; при появлении K2
//! добавится второй бэкенд, не меняя этот API.

use std::path::PathBuf;

use serde::Serialize;
use thiserror::Error;

use k1_data::{
    game_state::{self, GameState},
    map,
    repo::{FileSet, GAME_FILES, GameDir, TUTORIAL_FILES},
};
use k1_game::{resources::Resources, scene::SceneInput, textures::TextureKey};

pub use k1_game::textures::{Rgba, TextureError};

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
    Scene(#[from] k1_game::scene::SceneError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Ключ текстуры сцены: для потребителя непрозрачен, рендер кэширует по нему.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureId(TextureKey);

/// Кусок сцены: textured quad в пикселях карты.
/// `w`/`h` — None: размер текстуры; Some: растянуть.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub texture: TextureId,
    pub x: i32,
    pub y: i32,
    pub w: Option<i32>,
    pub h: Option<i32>,
}

/// Список отрисовки карты: порядок = порядок наложения (painter's algorithm).
#[derive(Debug, Clone, Default)]
pub struct Scene {
    pub pieces: Vec<Piece>,
}

impl Scene {
    /// Уникальные ключи текстур в порядке первого появления.
    pub fn texture_ids(&self) -> Vec<TextureId> {
        let mut seen = std::collections::HashSet::new();
        self.pieces
            .iter()
            .map(|piece| piece.texture)
            .filter(|id| seen.insert(*id))
            .collect()
    }
}

/// Часть серии: первая игра или обучение ENG. Позже — K2/K2.5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MapSource {
    Game,
    Tutorial,
}

impl MapSource {
    fn file_set(self) -> FileSet {
        match self {
            MapSource::Game => GAME_FILES,
            MapSource::Tutorial => TUTORIAL_FILES,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapRef {
    pub source: MapSource,
    pub number: u32,
}

/// Что добавить к чистой карте.
#[derive(Debug, Clone, Copy, Default)]
pub struct SceneOptions {
    pub foot: bool,
    pub lights: bool,
    pub markers: bool,
    pub characters: bool,
    /// Номер `Game.N`; None — первый доступный (нужен для маркеров и персонажей)
    pub hero: Option<u32>,
}

/// Открытая игра: каталог и загруженные ресурсы.
pub struct Game {
    dir: GameDir,
    resources: Resources,
}

impl Game {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let dir = GameDir::new(root);
        Ok(Self {
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

    /// Номера `Game.N` (или `TGame.N`) — начальных состояний новой игры.
    pub fn heroes(&self, source: MapSource) -> Result<Vec<u32>> {
        Ok(self.dir.state_numbers(source.file_set())?)
    }

    /// Сцена карты в порядке наложения; текстуры декодируются отдельно
    /// ([Game::textures]) и кэшируются потребителем.
    pub fn scene(&self, map_ref: MapRef, options: &SceneOptions) -> Result<Scene> {
        let files = map_ref.source.file_set();
        let map_bytes = self.dir.map_bytes(map_ref.number, files)?;
        let konung_map = map::parse(&map_bytes)?;
        let lights_bytes = self.dir.lights_bytes(map_ref.number, files)?;
        let lights = lights_bytes
            .as_ref()
            .map(|bytes| map::parse_lights(bytes))
            .transpose()?;

        let hero = match (options.hero, options.markers || options.characters) {
            (_, false) => None,
            (Some(hero), true) => Some(hero as usize),
            (None, true) => self.heroes(map_ref.source)?.first().map(|&h| h as usize),
        };
        let state_bytes = match hero {
            Some(hero) => Some(self.dir.state_bytes(hero as u32, files)?),
            None => None,
        };
        let state = state_bytes
            .as_ref()
            .map(|bytes| game_state::parse(bytes))
            .transpose()?;
        let entities = state
            .as_ref()
            .map(|state| game_state::on_map(state, map_ref.number as usize, hero));

        let overlays = k1_game::scene::Overlays {
            foot: options.foot,
            lights: options.lights,
            markers: options.markers,
            characters: options.characters,
        };
        let input = SceneInput {
            map: &konung_map,
            lights: lights.as_ref(),
            entities,
            state: state.as_ref(),
            overlays,
        };
        let pieces = self.resources.with_graphics(|graph, objects| {
            k1_game::scene::draw_list(&input, graph, Some(objects), &self.resources.tables)
        })??;
        Ok(Scene {
            pieces: pieces.into_iter().map(piece).collect(),
        })
    }

    /// RGBA-картинки ключей, позиционно параллельно `ids`.
    ///
    /// Наружу уходит и причина: потребитель решает, пропускать ли кусок, но
    /// молча терять картинку из-за битых данных нельзя. Внешняя `Err` — не
    /// разобрались ресурсы игры.
    pub fn textures(&self, ids: &[TextureId]) -> Result<Vec<DecodedTexture>> {
        Ok(self.resources.with_graphics(|graph, objects| {
            ids.iter()
                .map(|id| {
                    k1_game::textures::decode(
                        &id.0,
                        graph,
                        objects,
                        &self.resources.lights,
                        &self.resources.flames,
                    )
                })
                .collect()
        })?)
    }

    /// Headless-проверка карт: сцена строится, все текстуры декодируются,
    /// статистика детерминирована. `None` — все карты.
    ///
    /// Ошибка одной карты попадает в отчёт и не прерывает проверку остальных.
    pub fn check(&self, maps: Option<&[MapRef]>) -> Result<CheckReport> {
        let owned;
        let maps = match maps {
            Some(maps) => maps,
            None => {
                owned = self.maps()?;
                &owned
            }
        };
        // Начальное состояние своё у каждого источника: у игры Game.N, у
        // обучения TGame.N. Брать Game.N для tutorial-карт значило бы расставлять
        // по ним персонажей и отряды другой игры.
        let states = self.states_for(maps)?;
        let mut report = CheckReport { maps: Vec::new() };
        self.resources.with_graphics(|graph, objects| {
            for &map_ref in maps {
                let (hero, state) = states
                    .iter()
                    .find(|(source, ..)| *source == map_ref.source)
                    .map(|(_, hero, state)| (hero.as_ref().copied(), state.as_ref()))
                    .unwrap_or((None, None));
                report.maps.push(self.check_map(
                    map_ref,
                    graph,
                    objects,
                    hero.map(|hero| hero as usize),
                    state,
                ));
            }
            Ok::<_, GameError>(())
        })??;
        Ok(report)
    }

    /// Герой и состояние по каждому источнику карт, который встречается в списке.
    fn states_for(&self, maps: &[MapRef]) -> Result<Vec<SourceState>> {
        let mut states: Vec<SourceState> = Vec::new();
        for map in maps {
            if states.iter().any(|(source, ..)| *source == map.source) {
                continue;
            }
            let files = map.source.file_set();
            let hero = self.heroes(map.source)?.first().copied();
            let state = hero
                .map(|hero| self.dir.state_bytes(hero, files))
                .transpose()?
                .map(|bytes| game_state::parse(&bytes))
                .transpose()?;
            states.push((map.source, hero, state));
        }
        Ok(states)
    }

    fn check_map(
        &self,
        map_ref: MapRef,
        graph: &k1_data::graph_res::GraphRes<'_>,
        objects: &k1_data::objects_res::ObjectsRes<'_>,
        hero: Option<usize>,
        state: Option<&GameState>,
    ) -> MapCheck {
        let mut check = MapCheck::new(map_ref);
        if let Err(error) = self.fill_map_check(map_ref, graph, objects, hero, state, &mut check) {
            check.errors.push(error.to_string());
        }
        check
    }

    fn fill_map_check(
        &self,
        map_ref: MapRef,
        graph: &k1_data::graph_res::GraphRes<'_>,
        objects: &k1_data::objects_res::ObjectsRes<'_>,
        hero: Option<usize>,
        state: Option<&GameState>,
        check: &mut MapCheck,
    ) -> Result<()> {
        let files = map_ref.source.file_set();
        let map_bytes = self.dir.map_bytes(map_ref.number, files)?;
        let konung_map = map::parse(&map_bytes)?;
        check.objects = konung_map.objects.len();
        check.decorations = konung_map.decorations.len();
        check.raw_decorations = konung_map.decorations.iter().filter(|d| d.is_raw()).count();
        let lights_bytes = self.dir.lights_bytes(map_ref.number, files)?;
        check.has_lights = lights_bytes.is_some();
        let lights = lights_bytes
            .as_ref()
            .map(|bytes| map::parse_lights(bytes))
            .transpose()?;
        let entities = state.map(|state| game_state::on_map(state, map_ref.number as usize, hero));
        check.characters = match (&entities, state) {
            (Some(entities), Some(state)) => k1_data::characters::placed(entities, state).len(),
            _ => 0,
        };
        let overlays = k1_game::scene::Overlays {
            foot: false,
            lights: true,
            markers: true,
            characters: true,
        };
        let input = SceneInput {
            map: &konung_map,
            lights: lights.as_ref(),
            entities,
            state,
            overlays,
        };
        let pieces =
            k1_game::scene::draw_list(&input, graph, Some(objects), &self.resources.tables)?;
        check.pieces = pieces.len();
        let scene = Scene {
            pieces: pieces.into_iter().map(piece).collect(),
        };
        let ids = scene.texture_ids();
        check.unique_textures = ids.len();
        let mut missing = 0;
        for id in &ids {
            match k1_game::textures::decode(
                &id.0,
                graph,
                objects,
                &self.resources.lights,
                &self.resources.flames,
            ) {
                Ok(Some(_)) => {}
                // Пустой слот и битые данные — разные случаи: у второго есть
                // причина, и её надо показать, а не молча не дорисовать кусок
                Ok(None) => {
                    missing += 1;
                    check.errors.push(format!("нет картинки: {:?}", id.0));
                }
                Err(error) => {
                    missing += 1;
                    check.errors.push(format!("текстура {:?}: {error}", id.0));
                }
            }
        }
        check.missing_textures = missing;
        Ok(())
    }
}

/// Результат декодирования одного ключа текстуры.
///
/// `Ok(Some)` — картинка есть, `Ok(None)` — слот пуст по данным, `Err` — данные
/// битые и известна причина.
pub type DecodedTexture = std::result::Result<Option<Rgba>, TextureError>;

/// Начальное состояние источника карт для проверки: номер `Game.N`/`TGame.N` и
/// разобранный файл. `None` — у источника нет состояния (например, обучения в
/// издании, где его нет).
type SourceState = (MapSource, Option<u32>, Option<GameState>);

fn piece(from: k1_game::scene::DrawPiece) -> Piece {
    Piece {
        texture: TextureId(from.texture),
        x: from.x,
        y: from.y,
        w: from.w,
        h: from.h,
    }
}

/// Итог headless-проверки одной карты.
#[derive(Debug, Clone, Serialize)]
pub struct MapCheck {
    pub source: MapSource,
    pub number: u32,
    pub pieces: usize,
    pub unique_textures: usize,
    pub missing_textures: usize,
    pub objects: usize,
    pub decorations: usize,
    pub raw_decorations: usize,
    pub characters: usize,
    pub has_lights: bool,
    pub errors: Vec<String>,
}

impl MapCheck {
    fn new(map_ref: MapRef) -> Self {
        Self {
            source: map_ref.source,
            number: map_ref.number,
            pieces: 0,
            unique_textures: 0,
            missing_textures: 0,
            objects: 0,
            decorations: 0,
            raw_decorations: 0,
            characters: 0,
            has_lights: false,
            errors: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CheckReport {
    pub maps: Vec<MapCheck>,
}

impl CheckReport {
    pub fn ok(&self) -> bool {
        self.maps.iter().all(|map| map.errors.is_empty())
    }
}

impl std::fmt::Display for CheckReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for map in &self.maps {
            writeln!(
                f,
                "{} {:>2}: кусков {:>6}, текстур {:>4}, объектов {:>4}, декораций {:>4} (сырых {}), персонажей {:>3}, свет {}, ошибок {}",
                match map.source {
                    MapSource::Game => "game    ",
                    MapSource::Tutorial => "tutorial",
                },
                map.number,
                map.pieces,
                map.unique_textures,
                map.objects,
                map.decorations,
                map.raw_decorations,
                map.characters,
                if map.has_lights {
                    "есть"
                } else {
                    "нет "
                },
                map.errors.len(),
            )?;
            for error in &map.errors {
                writeln!(f, "  - {error}")?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_without_game_files_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Game::open(dir.path()).is_err());
    }

    #[test]
    fn scene_deduplicates_texture_ids_in_order() {
        let scene = Scene {
            pieces: vec![
                Piece {
                    texture: TextureId(TextureKey::Solid {
                        color: [1, 2, 3, 4],
                    }),
                    x: 0,
                    y: 0,
                    w: None,
                    h: None,
                },
                Piece {
                    texture: TextureId(TextureKey::Tile { index: 7 }),
                    x: 0,
                    y: 0,
                    w: None,
                    h: None,
                },
                Piece {
                    texture: TextureId(TextureKey::Solid {
                        color: [1, 2, 3, 4],
                    }),
                    x: 1,
                    y: 1,
                    w: None,
                    h: None,
                },
            ],
        };
        assert_eq!(
            scene.texture_ids(),
            [
                TextureId(TextureKey::Solid {
                    color: [1, 2, 3, 4]
                }),
                TextureId(TextureKey::Tile { index: 7 })
            ]
        );
    }
}
