//! Тесты фасада на реальных данных из `original/` (без них пропускаются).

use std::path::PathBuf;

use kn_game::{Game, MapRef, MapSource, SceneOptions};

fn game_roots() -> Vec<(&'static str, PathBuf)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../original/k1");
    let dirs = ["rus", "eng"]
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
        .collect::<Vec<_>>();
    assert!(
        !root.is_dir() || !dirs.is_empty(),
        "{} существует, но ни одного издания с папкой game/ в нём нет: \
         тесты на данных не выполнились бы. Как распаковать — docs/k1/editions.md",
        root.display()
    );
    dirs
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
        let problems = textures
            .iter()
            .zip(&ids)
            .filter_map(|(decoded, id)| match decoded {
                Ok(Some(_)) => None,
                Ok(None) => Some(format!("{id:?}: пустой слот")),
                Err(error) => Some(format!("{id:?}: {error}")),
            })
            .collect::<Vec<_>>();
        assert!(
            problems.is_empty(),
            "{edition}: не декодируются {problems:?}"
        );
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

/// Tutorial-карты описывает `TGame.N`, а не `Game.N`.
///
/// До правки `check` брал первое состояние из `Game.N` для всех источников, и
/// по картам обучения расставлялись персонажи и отряды основной игры.
#[test]
fn tutorial_maps_are_described_by_their_own_state() {
    use k1_data::{characters, game_state, repo::GameDir};

    for (edition, root) in game_roots() {
        let game = Game::open(&root).unwrap();
        let tutorial = game
            .maps()
            .unwrap()
            .into_iter()
            .filter(|map| map.source == MapSource::Tutorial)
            .collect::<Vec<_>>();
        if tutorial.is_empty() {
            continue; // обучения нет в этом издании
        }
        let hero = game.heroes(MapSource::Tutorial).unwrap()[0];
        let dir = GameDir::new(&root);
        let state = game_state::parse(
            &dir.state_bytes(hero, k1_data::repo::TUTORIAL_FILES)
                .unwrap(),
        )
        .unwrap();
        let report = game.check(Some(&tutorial)).unwrap();
        for map_ref in &tutorial {
            let entities = game_state::on_map(&state, map_ref.number as usize, Some(hero as usize));
            let expected = characters::placed(&entities, &state).len();
            let got = report
                .maps
                .iter()
                .find(|check| check.number == map_ref.number)
                .unwrap()
                .characters;
            assert_eq!(
                got, expected,
                "{edition}: tutorial {}: персонажи должны браться из TGame.{hero}",
                map_ref.number
            );
        }
    }
}
