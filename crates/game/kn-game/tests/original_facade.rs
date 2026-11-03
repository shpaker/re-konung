//! Тесты фасада на реальных данных из `original/` (без них пропускаются).

use std::path::PathBuf;

use kn_game::{Game, MapRef, MapSource, Piece, Scene, SceneOptions, group_is_shown};

fn game_roots() -> Vec<(&'static str, PathBuf)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../original/k1");
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

/// Куски сцены со всеми слоями, которые показывает эти опции.
fn shown<'a>(scene: &'a Scene, options: &SceneOptions) -> Vec<&'a Piece> {
    scene
        .pieces
        .iter()
        .filter(|piece| group_is_shown(piece.group, options))
        .collect()
}

/// Группа переходит в модель сцены вместе с куском.
///
/// Дальше она только видимость: `kn_io::spawn_scene` раскладывает куски по узлам
/// групп, и кусок без группы или под чужим переключателем пропал бы с карты
/// молча. Раскладку по слоям проверяет `k1-game/tests/original_scene.rs`, здесь —
/// что фасад её не теряет.
#[test]
fn pieces_carry_their_group() {
    for (edition, root) in game_roots() {
        let game = Game::open(&root).unwrap();
        let map_ref = game.maps().unwrap()[0];
        let every = game.scene(map_ref, &SceneOptions::ALL).unwrap();
        let clean = game.scene(map_ref, &SceneOptions::default()).unwrap();
        assert!(
            every.pieces.len() > clean.pieces.len(),
            "{edition}: {map_ref:?}: со всеми слоями должно быть кусков больше, \
             иначе проверка ничего не доказывает"
        );
        assert_eq!(
            shown(&every, &SceneOptions::default()),
            clean.pieces.iter().collect::<Vec<_>>(),
            "{edition}: {map_ref:?} без слоёв"
        );
    }
}

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
        let scene = game.scene(first, &SceneOptions::ALL).unwrap();
        assert!(!scene.pieces.is_empty(), "{edition}: пустая сцена");
        assert_eq!(
            scene.pieces,
            game.scene(first, &SceneOptions::ALL).unwrap().pieces,
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
