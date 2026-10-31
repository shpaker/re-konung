//! Тесты сцены на реальных данных из `original/` (без них пропускаются).

use std::path::PathBuf;

use k1_data::{
    game_state, map,
    repo::{GAME_FILES, GameDir},
};
use k1_game::{
    resources::Resources,
    scene::{self, Overlays, SceneInput},
    textures::TextureKey,
};

fn game_dirs() -> Vec<(&'static str, GameDir)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../original/k1");
    let dirs = ["rus", "eng"]
        .into_iter()
        .filter_map(|edition| {
            let game = GameDir::new(root.join(edition).join("game"));
            if game.root().is_dir() {
                Some((edition, game))
            } else {
                eprintln!("пропуск: нет original/k1/{edition}/game (docs/k1/editions.md)");
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(
        !root.is_dir() || !dirs.is_empty(),
        "{} существует, но ни одного издания с папкой game/ в нём нет: \
         тесты на данных не выполнились бы. Как распаковать — docs/k1/editions.md",
        root.display()
    );
    dirs
}

fn draw(
    game: &GameDir,
    resources: &Resources,
    number: u32,
    overlays: Overlays,
) -> Vec<scene::DrawPiece> {
    let map_bytes = game.map_bytes(number, GAME_FILES).unwrap();
    let konung_map = map::parse(&map_bytes).unwrap();
    let lights_bytes = game.lights_bytes(number, GAME_FILES).unwrap();
    let lights = lights_bytes
        .as_ref()
        .map(|bytes| map::parse_lights(bytes).unwrap());
    let input = SceneInput {
        map: &konung_map,
        lights: lights.as_ref(),
        entities: None,
        state: None,
        overlays,
    };
    resources
        .with_graphics(|graph, objects| {
            scene::draw_list(&input, graph, Some(objects), &resources.tables).unwrap()
        })
        .unwrap()
}

#[test]
fn object_position_matches_the_reference_formula() {
    // render.object_position: x·58 − (y&1)·29, y·16
    assert_eq!(scene::object_position(0, 0), (0, 0));
    assert_eq!(scene::object_position(2, 4), (116, 64));
    assert_eq!(scene::object_position(3, 5), (145, 80));
}

#[test]
fn terrain_pieces_cover_every_nonzero_cell() {
    for (edition, game) in game_dirs() {
        let resources = Resources::load(&game).unwrap();
        let number = game.map_numbers(GAME_FILES).unwrap()[0];
        let pieces = draw(&game, &resources, number, Overlays::default());
        let map_bytes = game.map_bytes(number, GAME_FILES).unwrap();
        let konung_map = map::parse(&map_bytes).unwrap();
        let cells = konung_map
            .ground
            .iter()
            .chain(&konung_map.overlay)
            .flatten()
            .filter(|&&value| value != 0)
            .count();
        let tiles = pieces
            .iter()
            .filter(|piece| matches!(piece.texture, TextureKey::Tile { .. }))
            .count();
        // Тайлы земли и верхнего слоя плюс декорации из GRAPH.RES
        let decorations = resources
            .with_graphics(|graph, _| {
                konung_map
                    .decorations
                    .iter()
                    .filter(|d| d.tile >= 0 && graph.tile(d.tile as usize).is_some())
                    .count()
            })
            .unwrap();
        assert_eq!(
            tiles,
            cells + decorations,
            "{edition}: konung.{number}: тайлы = клетки + декорации"
        );
        assert!(
            pieces
                .iter()
                .any(|piece| matches!(piece.texture, TextureKey::ObjectFrame { .. })),
            "{edition}: konung.{number}: объекты рисуются"
        );
    }
}

#[test]
fn draw_list_is_deterministic() {
    for (edition, game) in game_dirs() {
        let resources = Resources::load(&game).unwrap();
        let number = game.map_numbers(GAME_FILES).unwrap()[0];
        let overlays = Overlays {
            foot: true,
            lights: true,
            markers: true,
            characters: true,
        };
        // Персонажам нужны Game.N и сущности карты
        let map_bytes = game.map_bytes(number, GAME_FILES).unwrap();
        let konung_map = map::parse(&map_bytes).unwrap();
        let hero = game.state_numbers(GAME_FILES).unwrap()[0];
        let state = game_state::parse(&game.state_bytes(hero, GAME_FILES).unwrap()).unwrap();
        let entities = game_state::on_map(&state, number as usize, Some(hero as usize));
        let lights_bytes = game.lights_bytes(number, GAME_FILES).unwrap();
        let lights = lights_bytes
            .as_ref()
            .map(|bytes| map::parse_lights(bytes).unwrap());
        let input = SceneInput {
            map: &konung_map,
            lights: lights.as_ref(),
            entities: Some(entities),
            state: Some(&state),
            overlays,
        };
        let first = resources
            .with_graphics(|graph, objects| {
                scene::draw_list(&input, graph, Some(objects), &resources.tables).unwrap()
            })
            .unwrap();
        let second = resources
            .with_graphics(|graph, objects| {
                scene::draw_list(&input, graph, Some(objects), &resources.tables).unwrap()
            })
            .unwrap();
        assert_eq!(first, second, "{edition}: konung.{number}");
        assert!(
            first
                .iter()
                .any(|piece| matches!(piece.texture, TextureKey::SpriteImage { .. })),
            "{edition}: konung.{number}: персонажи рисуются"
        );
    }
}

#[test]
fn overlays_follow_map_data() {
    for (edition, game) in game_dirs() {
        let resources = Resources::load(&game).unwrap();
        let number = game.map_numbers(GAME_FILES).unwrap()[0];
        let overlays = Overlays {
            foot: true,
            lights: true,
            ..Overlays::default()
        };
        let pieces = draw(&game, &resources, number, overlays);
        let map_bytes = game.map_bytes(number, GAME_FILES).unwrap();
        let konung_map = map::parse(&map_bytes).unwrap();
        let blocked = konung_map
            .foot
            .iter()
            .flatten()
            .filter(|&&value| map::is_blocked(value))
            .count();
        let arrows = konung_map
            .foot
            .iter()
            .flatten()
            .filter(|&&value| value & map::FOOT_STOPS_ARROWS != 0)
            .count();
        let foot = |color| {
            pieces
                .iter()
                .filter(|piece| piece.texture == TextureKey::FootDiamond { color })
                .count()
        };
        assert_eq!(
            foot(k1_game::textures::FOOT_BLOCKED_COLOR),
            blocked,
            "{edition}: непроходимые клетки"
        );
        assert_eq!(
            foot(k1_game::textures::FOOT_ARROWS_COLOR),
            arrows,
            "{edition}: клетки стрел"
        );
        let lit = game
            .lights_bytes(number, GAME_FILES)
            .unwrap()
            .map_or(0, |bytes| {
                map::parse_lights(&bytes)
                    .unwrap()
                    .tiles
                    .iter()
                    .flatten()
                    .filter(|&&value| value != 0)
                    .count()
            });
        let masks = pieces
            .iter()
            .filter(|piece| matches!(piece.texture, TextureKey::LightMask { .. }))
            .count();
        assert_eq!(masks, lit, "{edition}: маски света");
    }
}
