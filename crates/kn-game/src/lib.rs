//! Общий фасад игры: Bevy-независимая модель сцены и доступ к данным.
//!
//! Потребители (`kn-io`, бинарники) видят только этот крейт: [Game] открывает
//! каталог игры, [Game::scene] строит [Scene] — упорядоченный список кусков
//! с непрозрачными ключами текстур, [Game::textures] декодирует их в RGBA.
//! Специфика «Князя 1» спрятана в `k1-game` и `k1-data`; при появлении K2
//! добавится второй бэкенд, не меняя этот API.
//!
//! [movement] — the shared movement engine: it depends on neither K1 nor the
//! display, and builds the grid, the routes and the per-tick step from data
//! brought to it from outside.

pub mod movement;
pub mod step_durations;

use std::path::PathBuf;

use serde::Serialize;
use thiserror::Error;

use k1_data::{
    game_state::{self, GameState},
    map,
    repo::{FileSet, GAME_FILES, GameDir, TUTORIAL_FILES},
};
use k1_game::{resources::Resources, scene::SceneInput, textures::TextureKey};

pub use k1_data::objects_res::TICK_MS;
pub use k1_game::scene::PieceGroup;
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
/// `texture` — None: в базовом кадре картинки нет (слой есть не во всех кадрах
/// анимации); кусок скрыт, пока часы не придут к его кадру.
/// `group` — что переключает видимость куска: слой отображения заводит на группу
/// отдельный узел, и куски не пересобираются (см. [`group_is_shown`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub texture: Option<TextureId>,
    pub x: i32,
    pub y: i32,
    pub w: Option<i32>,
    pub h: Option<i32>,
    pub animation: Option<Animation>,
    pub group: PieceGroup,
}

/// Показываются ли куски группы при этих опциях.
///
/// Порядок наложения задаёт позиция куска в [`Scene::pieces`], поэтому видимость
/// группы ничего в ней не переставляет: фильтр по этой функции и сборка карты
/// только с включёнными слоями дают одну и ту же картину — держит
/// `k1-game/tests/original_scene.rs`, а что группа доходит до модели —
/// `tests/original_facade.rs`.
pub fn group_is_shown(group: PieceGroup, options: &SceneOptions) -> bool {
    group.is_shown_by(&overlays(options))
}

/// Кадры одного слоя, которые показываются по очереди.
///
/// Длительности — в тиках игры ([`TICK_MS`] миллисекунд), а не в секундах: часы
/// остаются целыми, шаг покадрового просмотра равен тику, а куски с одинаковой
/// фазой идут в такт сами. Слои одного объекта или персонажа получают одинаковые
/// длительности и тот же `base`, поэтому и синхронны.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Animation {
    pub frames: Vec<AnimationFrame>,
    /// Кадр из данных: его показывают при выключенной анимации, и именно по нему
    /// считают порядок наложения
    pub base: usize,
}

/// Один кадр анимации: картинка (`None` — в этом кадре слоя не видно), смещение
/// её угла от угла куска и длительность в тиках.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationFrame {
    pub texture: Option<TextureId>,
    pub dx: i32,
    pub dy: i32,
    pub ticks: u32,
}

/// Список отрисовки карты: порядок = порядок наложения (painter's algorithm).
#[derive(Debug, Clone, Default)]
pub struct Scene {
    pub pieces: Vec<Piece>,
}

impl Scene {
    /// Уникальные ключи текстур в порядке первого появления: базовый кадр куска
    /// и все кадры его анимации.
    ///
    /// Кадры нужны целиком: не докачанный кадр означал бы пустой спрайт в
    /// середине цикла, а `check` не увидел бы, что он не декодируется.
    pub fn texture_ids(&self) -> Vec<TextureId> {
        let mut seen = std::collections::HashSet::new();
        let mut ids = Vec::new();
        for piece in &self.pieces {
            let frames = piece
                .animation
                .iter()
                .flat_map(|animation| animation.frames.iter())
                .filter_map(|frame| frame.texture.as_ref());
            for texture in piece.texture.iter().chain(frames) {
                if seen.insert(*texture) {
                    ids.push(*texture);
                }
            }
        }
        ids
    }

    /// Статистика анимаций для диагностики.
    pub fn animation_stats(&self) -> AnimationStats {
        let mut stats = AnimationStats::default();
        for piece in &self.pieces {
            // Видимым кусок считает базовый кадр: без картинки в нём спрайт скрыт
            if piece.texture.is_some() {
                stats.visible += 1;
            }
            if let Some(animation) = &piece.animation {
                stats.pieces += 1;
                stats.frames += animation.frames.len();
            }
        }
        stats
    }
}

/// Анимационная статистика сцены: сколько кусков анимируется, сколько в них
/// кадров и сколько кусков видно в базовом кадре.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct AnimationStats {
    pub pieces: usize,
    pub frames: usize,
    pub visible: usize,
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

impl std::fmt::Display for MapSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            MapSource::Game => "game",
            MapSource::Tutorial => "tutorial",
        })
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

impl SceneOptions {
    /// Все отладочные слои разом.
    ///
    /// С ними карта собирается один раз со всеми кусками, а переключение слоя
    /// стоит одну запись видимости группы вместо пересборки тысяч спрайтов
    /// (см. [`group_is_shown`]).
    pub const ALL: SceneOptions = SceneOptions {
        foot: true,
        lights: true,
        markers: true,
        characters: true,
        hero: None,
    };
}

/// Флаги оверлеев для слоя K1: те же четыре переключателя без номера героя.
fn overlays(options: &SceneOptions) -> k1_game::scene::Overlays {
    k1_game::scene::Overlays {
        foot: options.foot,
        lights: options.lights,
        markers: options.markers,
        characters: options.characters,
    }
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

        let input = SceneInput {
            map: &konung_map,
            lights: lights.as_ref(),
            entities,
            state: state.as_ref(),
            overlays: overlays(options),
        };
        let pieces = self.resources.with_graphics(|graph, objects| {
            k1_game::scene::draw_list(&input, graph, Some(objects), &self.resources.tables)
        })??;
        Ok(Scene {
            pieces: pieces.into_iter().map(piece).collect(),
        })
    }

    /// Карта, на которой начинается новая игра героя.
    pub fn start_map(&self, hero: u32) -> MapRef {
        MapRef {
            source: MapSource::Game,
            number: game_state::start_map(hero as usize) as u32,
        }
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
        let scene = Scene {
            pieces: pieces.into_iter().map(piece).collect(),
        };
        let ids = scene.texture_ids();
        let stats = scene.animation_stats();
        check.pieces = scene.pieces.len();
        check.unique_textures = ids.len();
        check.animated_pieces = stats.pieces;
        check.animation_frames = stats.frames;
        check.visible_pieces = stats.visible;
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
        texture: from.texture.map(TextureId),
        x: from.x,
        y: from.y,
        w: from.w,
        h: from.h,
        animation: from.animation.map(|animation| Animation {
            base: animation.base,
            frames: animation
                .frames
                .into_iter()
                .map(|frame| AnimationFrame {
                    texture: frame.texture.map(TextureId),
                    dx: frame.dx,
                    dy: frame.dy,
                    ticks: frame.ticks,
                })
                .collect(),
        }),
        group: from.group,
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
    /// Куски с анимацией и суммарное число их кадров: кадры тоже декодируются,
    /// и битый кадр — ошибка, а не пустой спрайт в середине цикла
    pub animated_pieces: usize,
    pub animation_frames: usize,
    /// Куски с картинкой в базовом кадре — то, что видно при выключенной
    /// анимации. Их число не изменилось с появлением анимаций: слои, которых нет
    /// в базовом кадре, добавляются скрытыми
    pub visible_pieces: usize,
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
            animated_pieces: 0,
            animation_frames: 0,
            visible_pieces: 0,
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
                "{} {:>2}: кусков {:>6} (видимых {:>6}), текстур {:>4}, анимаций {:>4} (кадров {:>5}), объектов {:>4}, декораций {:>4} (сырых {}), персонажей {:>3}, свет {}, ошибок {}",
                match map.source {
                    MapSource::Game => "game    ",
                    MapSource::Tutorial => "tutorial",
                },
                map.number,
                map.pieces,
                map.visible_pieces,
                map.unique_textures,
                map.animated_pieces,
                map.animation_frames,
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

    fn piece(texture: TextureKey) -> Piece {
        Piece {
            texture: Some(TextureId(texture)),
            x: 0,
            y: 0,
            w: None,
            h: None,
            animation: None,
            group: PieceGroup::Terrain,
        }
    }

    fn frame(texture: Option<TextureKey>) -> AnimationFrame {
        AnimationFrame {
            texture: texture.map(TextureId),
            dx: 0,
            dy: 0,
            ticks: 1,
        }
    }

    #[test]
    fn open_without_game_files_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Game::open(dir.path()).is_err());
    }

    #[test]
    fn scene_deduplicates_texture_ids_in_order() {
        let solid = TextureKey::Solid {
            color: [1, 2, 3, 4],
        };
        let scene = Scene {
            pieces: vec![
                piece(solid),
                piece(TextureKey::Tile { index: 7 }),
                Piece {
                    x: 1,
                    y: 1,
                    ..piece(solid)
                },
            ],
        };
        assert_eq!(
            scene.texture_ids(),
            [TextureId(solid), TextureId(TextureKey::Tile { index: 7 })]
        );
    }

    #[test]
    fn texture_ids_include_every_animation_frame() {
        // Базовый кадр идёт первым (он же кадр из файла), затем остальные кадры;
        // общий кадр двух кусков не дублируется
        let shared = TextureKey::FlameFrame { index: 3 };
        let first = TextureKey::FlameFrame { index: 1 };
        let second = TextureKey::FlameFrame { index: 2 };
        let animation = || Animation {
            base: 0,
            frames: vec![frame(Some(first)), frame(Some(second)), frame(Some(shared))],
        };
        let scene = Scene {
            pieces: vec![
                Piece {
                    animation: Some(animation()),
                    ..piece(first)
                },
                Piece {
                    animation: Some(animation()),
                    ..piece(first)
                },
            ],
        };
        assert_eq!(
            scene.texture_ids(),
            [TextureId(first), TextureId(second), TextureId(shared)]
        );
        assert_eq!(
            scene.animation_stats(),
            AnimationStats {
                pieces: 2,
                frames: 6,
                visible: 2,
            }
        );
    }

    #[test]
    fn hidden_base_still_declares_its_frames() {
        // Слой, у которого в базовом кадре картинки нет, всё равно обязан
        // получить текстуры своих кадров
        let body = TextureKey::SpriteImage {
            source: k1_data::characters::LayerSource::Model(4),
            image_id: 11,
            palette: 2,
        };
        let gear = TextureKey::SpriteImage {
            source: k1_data::characters::LayerSource::Equipment(1),
            image_id: 5,
            palette: 3,
        };
        let scene = Scene {
            pieces: vec![Piece {
                texture: None,
                x: 0,
                y: 0,
                w: None,
                h: None,
                animation: Some(Animation {
                    base: 0,
                    frames: vec![frame(None), frame(Some(gear)), frame(Some(body))],
                }),
                group: PieceGroup::Characters,
            }],
        };
        assert_eq!(scene.texture_ids(), [TextureId(gear), TextureId(body)]);
        assert_eq!(
            scene.animation_stats(),
            AnimationStats {
                pieces: 1,
                frames: 3,
                visible: 0,
            }
        );
    }

    #[test]
    fn static_scene_has_no_animation() {
        let scene = Scene {
            pieces: vec![piece(TextureKey::Tile { index: 1 })],
        };
        assert_eq!(
            scene.animation_stats(),
            AnimationStats {
                pieces: 0,
                frames: 0,
                visible: 1,
            }
        );
    }

    /// Группы, которые остаются видны при этих опциях.
    fn shown(options: &SceneOptions) -> Vec<PieceGroup> {
        PieceGroup::ALL
            .into_iter()
            .filter(|group| group_is_shown(*group, options))
            .collect()
    }

    #[test]
    fn each_overlay_switches_only_its_own_groups() {
        use PieceGroup::*;
        // Списки названы вручную: проверка на данных (`tests/original_scene.rs` в
        // k1-game) сравнивает куски, а не группы, и группа, записанная под чужой
        // переключатель, осталась бы незамеченной на карте без таких кусков
        assert_eq!(
            shown(&SceneOptions::default()),
            [Terrain, Decorations, Shadows, Objects],
            "чистая карта"
        );
        assert_eq!(
            shown(&SceneOptions::ALL),
            [
                Terrain,
                Decorations,
                Shadows,
                Objects,
                Characters,
                CharacterShadows,
                Lights,
                Foot,
                Markers,
            ],
            "все слои"
        );
        assert_eq!(
            shown(&SceneOptions {
                foot: true,
                ..SceneOptions::default()
            }),
            [Terrain, Decorations, Shadows, Objects, Foot],
            "один слой проходимости"
        );
        // Тени персонажей переключаются вместе с самими персонажами
        assert_eq!(
            shown(&SceneOptions {
                characters: true,
                ..SceneOptions::default()
            }),
            [
                Terrain,
                Decorations,
                Shadows,
                Objects,
                Characters,
                CharacterShadows
            ],
            "один слой персонажей"
        );
        assert_eq!(
            shown(&SceneOptions {
                lights: true,
                ..SceneOptions::default()
            }),
            [Terrain, Decorations, Shadows, Objects, Lights],
            "один слой света"
        );
        assert_eq!(
            shown(&SceneOptions {
                markers: true,
                ..SceneOptions::default()
            }),
            [Terrain, Decorations, Shadows, Objects, Markers],
            "один слой маркеров"
        );
    }
}
