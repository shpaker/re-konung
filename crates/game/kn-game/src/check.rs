//! Headless validation and deterministic map reports.

use serde::Serialize;

use k1_data::{
    game_state::{self, GameState},
    map,
};
use k1_game::presentation::scene::SceneInput;

use crate::scene::piece;
use crate::{Game, GameError, MapRef, MapSource, Result, Scene};

/// Начальное состояние источника карт для проверки: номер `Game.N`/`TGame.N` и
/// разобранный файл. `None` — у источника нет состояния (например, обучения в
/// издании, где его нет).
type SourceState = (MapSource, Option<u32>, Option<GameState>);

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

impl Game {
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
        let overlays = k1_game::presentation::scene::Overlays {
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
        let pieces = k1_game::presentation::scene::draw_list(
            &input,
            graph,
            Some(objects),
            &self.resources.tables,
        )?;
        let scene = Scene {
            pieces: pieces.into_iter().map(piece).collect(),
            // The headless check counts the static draw list only; it renders no
            // creatures, so it needs no roster geometry (review #101, P2-1).
            creature_pieces: Vec::new(),
            creature_origins: std::collections::HashMap::new(),
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
            match k1_game::presentation::textures::decode(
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
