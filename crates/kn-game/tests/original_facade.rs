//! Тесты фасада на реальных данных из `original/` (без них пропускаются).

use std::path::PathBuf;

use kn_game::{Game, MapRef, MapSource, SceneOptions};

fn game_roots() -> Vec<(&'static str, PathBuf)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../original/k1");
    ["rus", "eng"]
        .into_iter()
        .filter_map(|edition| {
            let game = root.join(edition).join("game");
            if game.is_dir() {
                Some((edition, game))
            } else {
                eprintln!("пропуск: нет original/k1/{edition}/game (docs/k1/editions.md)");
                None
            }
        })
        .collect()
}

const FULL: SceneOptions = SceneOptions {
    foot: true,
    lights: true,
    markers: true,
    characters: true,
    hero: None,
};

#[test]
fn maps_cover_game_and_tutorial() {
    for (edition, root) in game_roots() {
        let game = Game::open(&root).unwrap();
        let maps = game.maps().unwrap();
        let tutorial = maps
            .iter()
            .filter(|map| map.source == MapSource::Tutorial)
            .count();
        assert_eq!(
            (maps.len() - tutorial, tutorial),
            (43, if edition == "eng" { 2 } else { 0 }),
            "{edition}: состав карт"
        );
    }
}

#[test]
fn scene_of_first_map_is_full_and_deterministic() {
    for (edition, root) in game_roots() {
        let game = Game::open(&root).unwrap();
        let first = MapRef {
            source: MapSource::Game,
            number: game.maps().unwrap()[0].number,
        };
        let scene = game.scene(first, &FULL).unwrap();
        assert!(!scene.pieces.is_empty(), "{edition}: пустая сцена");
        assert_eq!(
            scene.pieces,
            game.scene(first, &FULL).unwrap().pieces,
            "{edition}: детерминированность"
        );
        let ids = scene.texture_ids();
        let textures = game.textures(&ids).unwrap();
        let missing = textures
            .iter()
            .zip(&ids)
            .filter(|(rgba, _)| rgba.is_none())
            .map(|(_, id)| format!("{id:?}"))
            .collect::<Vec<_>>();
        assert!(missing.is_empty(), "{edition}: не декодируются {missing:?}");
    }
}

#[test]
fn check_reports_no_errors() {
    for (edition, root) in game_roots() {
        let game = Game::open(&root).unwrap();
        let mut maps = vec![game.maps().unwrap()[0]];
        if edition == "eng" {
            maps.push(MapRef {
                source: MapSource::Tutorial,
                number: 4,
            });
        }
        let report = game.check(Some(&maps)).unwrap();
        assert!(report.ok(), "{edition}:\n{report}");
        for map in &report.maps {
            assert!(map.pieces > 0, "{edition}: карта {}", map.number);
            assert!(
                map.unique_textures > 0 && map.missing_textures == 0,
                "{edition}: карта {}",
                map.number
            );
        }
        // Статистика детерминирована
        let again = game.check(Some(&maps)).unwrap();
        let counts = |r: &kn_game::CheckReport| {
            r.maps
                .iter()
                .map(|m| (m.number, m.pieces, m.unique_textures, m.characters))
                .collect::<Vec<_>>()
        };
        assert_eq!(counts(&report), counts(&again), "{edition}: статистика");
    }
}
