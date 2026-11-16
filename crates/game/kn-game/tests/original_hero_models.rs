//! Verify configured session geometry directly: preview rendering normalizes model fields.

use std::path::PathBuf;

use k1_data::{
    characters::{LayerSource, WALK_ANIMATION},
    game_state,
    repo::{GAME_FILES, GameDir},
};
use k1_game::{
    resources::Resources,
    scene::{DrawPiece, Graphics, Pose, pose_in},
};
use kn_game::{
    CreatureClips, CreatureLayer, Game, LayerKey, MapSource, PieceGroup, PoseKind, SceneOptions,
    TextureId, TextureKey,
};

fn assert_layer(actual: &CreatureLayer, expected: &DrawPiece, key: LayerKey) {
    assert_eq!(actual.key, key);
    assert_eq!(actual.base, expected.texture.map(TextureId::from_key));
    assert_eq!(actual.corner, (expected.x, expected.y));
    match (&actual.animation, &expected.animation) {
        (None, None) => {}
        (Some(actual), Some(expected)) => {
            assert_eq!(actual.base, expected.base);
            assert_eq!(actual.frames.len(), expected.frames.len());
            for (actual, expected) in actual.frames.iter().zip(&expected.frames) {
                assert_eq!(actual.texture, expected.texture.map(TextureId::from_key));
                assert_eq!(
                    (actual.dx, actual.dy, actual.ticks),
                    (expected.dx, expected.dy, expected.ticks)
                );
            }
        }
        _ => panic!("configured layer lost or invented its source animation"),
    }
}

fn assert_pose(actual: &CreatureClips, expected: &Pose, hero: u32, palette: u32) -> usize {
    assert_eq!(actual.layers.len(), expected.pieces.len());
    let mut gear_layers = 0;
    let mut body_seen = false;
    for ((actual, expected), key) in actual
        .layers
        .iter()
        .zip(&expected.pieces)
        .zip(&expected.layer_keys)
    {
        assert_layer(actual, expected, *key);
        match key {
            LayerKey::Body => {
                body_seen = true;
                for texture in
                    expected
                        .texture
                        .into_iter()
                        .chain(expected.animation.iter().flat_map(|animation| {
                            animation.frames.iter().filter_map(|frame| frame.texture)
                        }))
                {
                    assert!(matches!(texture, TextureKey::SpriteImage {
                        source: LayerSource::Model(model), palette: actual_palette, ..
                    } if model == hero as usize && actual_palette == palette));
                }
            }
            LayerKey::Gear { .. } => gear_layers += 1,
            LayerKey::Shadow => panic!("shadow must stay outside body/equipment layers"),
        }
    }
    assert!(body_seen);
    match (&actual.shadow, &expected.shadow) {
        (Some(actual), Some(expected)) => assert_layer(actual, expected, LayerKey::Shadow),
        (None, None) => {}
        _ => panic!("configured model lost or invented its source shadow"),
    }
    gear_layers
}

#[test]
fn all_three_selected_heroes_keep_their_model_palette_and_source_equipment_in_session_clips() {
    let original = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../original/k1");
    let mut editions = 0;
    for edition in ["rus", "eng"] {
        let root = original.join(edition).join("game");
        if !root.is_dir() {
            eprintln!("skip: user-owned {edition} edition absent");
            continue;
        }
        editions += 1;
        let directory = GameDir::new(&root);
        let resources = Resources::load(&directory).unwrap();
        let mut game = Game::open(&root).unwrap();
        assert_eq!(game.heroes(MapSource::Game).unwrap(), [0, 1, 2]);
        for hero in 0..3 {
            let source_bytes = directory.state_bytes(hero, GAME_FILES).unwrap();
            let mut expected_state = game_state::parse(&source_bytes).unwrap();
            let map = game.start_map(hero);
            let record =
                game_state::on_map(&expected_state, map.number as usize, Some(hero as usize))
                    .start
                    .unwrap()
                    .members()
                    .next()
                    .unwrap();
            let occupation = (hero as usize + 1) % kn_game::newgame_classes().len();
            let palette_index = [74, 33, 39][hero as usize];
            let expected = expected_state.characters[record].as_mut().unwrap();
            expected.sprite_set = hero as u8;
            expected.palette_offset = palette_index * 512;
            expected.direction = 5;
            expected.class_id = kn_game::newgame_classes()[occupation].class_id;
            let expected = expected.clone();
            let position =
                game_state::character_position(i32::from(expected.x), i32::from(expected.y));

            game.configure_new_game(hero, occupation, 0, 0, None)
                .unwrap();
            let simulation = game.simulation_with_hero(map, 42, Some(hero)).unwrap();
            assert_eq!(
                simulation.hero_id(),
                Some(record),
                "{edition} hero {hero}: source record"
            );
            let actual_state = simulation
                .states()
                .into_iter()
                .find(|state| state.id == record)
                .unwrap();
            assert_eq!(actual_state.pixel, position);
            let scene = game
                .scene(
                    map,
                    &SceneOptions {
                        characters: true,
                        hero: Some(hero),
                        ..SceneOptions::default()
                    },
                )
                .unwrap();
            let (_, actual) = game
                .creature_animations_with_hero(map, Some(hero))
                .unwrap()
                .into_iter()
                .find(|(id, _)| *id == record)
                .unwrap();
            let checked_gear = resources
                .with_graphics(|graph, objects| {
                    let graphics = Graphics::new(graph, objects, &resources.tables);
                    let rest = pose_in(
                        &expected,
                        position.0,
                        position.1,
                        &expected_state,
                        &graphics,
                        &PoseKind::of_record(&expected),
                    )
                    .unwrap()
                    .unwrap();
                    let expected_textures: Vec<_> = rest
                        .pieces
                        .iter()
                        .map(|piece| piece.texture.map(TextureId::from_key))
                        .collect();
                    // Runtime creature nodes use this roster geometry, not the static draw list.
                    let actual_textures: Vec<_> = scene
                        .creature_pieces
                        .iter()
                        .filter(|piece| {
                            piece.creature_id == Some(record)
                                && piece.group == PieceGroup::Characters
                        })
                        .map(|piece| piece.texture)
                        .collect();
                    assert_eq!(
                        actual_textures, expected_textures,
                        "{edition} hero {hero}: configured scene body/equipment"
                    );
                    let mut gear =
                        assert_pose(&actual.rest, &rest, hero, palette_index as u32 * 256);
                    assert_eq!(actual.walk.len(), 8);
                    for direction in 0..8 {
                        let expected_walk = pose_in(
                            &expected,
                            position.0,
                            position.1,
                            &expected_state,
                            &graphics,
                            &PoseKind::start(WALK_ANIMATION as u8, direction),
                        )
                        .unwrap()
                        .unwrap();
                        gear += assert_pose(
                            actual.walk[direction as usize].as_ref().unwrap(),
                            &expected_walk,
                            hero,
                            palette_index as u32 * 256,
                        );
                    }
                    gear
                })
                .unwrap();
            assert!(
                checked_gear > 0,
                "{edition} hero {hero}: source equipment was not exercised"
            );
            assert_eq!(
                directory.state_bytes(hero, GAME_FILES).unwrap(),
                source_bytes
            );
            game.clear_new_game();
        }
    }
    assert!(
        !original.is_dir() || editions > 0,
        "original/k1 exists but has no supported edition"
    );
}
