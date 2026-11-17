use k1_data::game_state;

#[test]
fn constructor_configuration_reaches_all_runtime_consumers() {
    for edition in ["rus", "eng"] {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../original/k1")
            .join(edition)
            .join("game");
        if !root.is_dir() {
            continue;
        }
        let mut game = super::Game::open(&root).unwrap();
        for hero in 0..3 {
            let bytes = game
                .dir
                .state_bytes(hero, k1_data::repo::GAME_FILES)
                .unwrap();
            let original = k1_data::game_state::parse(&bytes).unwrap();
            let baseline = game
                .simulation_with_hero(game.start_map(hero), 42, Some(hero))
                .unwrap();
            for class in 0..4 {
                for combat in 0..2 {
                    for secondary in 0..3 {
                        game.configure_new_game(hero, class, combat, secondary, None)
                            .unwrap();
                        let loaded = game
                            .session_or_original(
                                &bytes,
                                super::MapSource::Game,
                                Some(hero as usize),
                            )
                            .unwrap();
                        let slot = k1_data::game_state::on_map(
                            &loaded,
                            k1_data::game_state::start_map(hero as usize),
                            Some(hero as usize),
                        )
                        .start
                        .unwrap()
                        .members()
                        .next()
                        .unwrap();
                        let character = loaded.characters[slot].as_ref().unwrap();
                        assert_eq!(character.class_id, super::newgame_classes()[class].class_id);
                        assert_eq!(
                            character.attributes[0],
                            super::newgame_classes()[class].agility
                        );
                        assert_eq!(character.skills[2], if combat == 1 { 100 } else { 0 });
                        assert_eq!(character.skills[4], if secondary == 2 { 100 } else { 50 });
                        assert_eq!(character.name_id, [10, 124, 151][hero as usize]);
                        assert_eq!(character.level, 1);
                        assert_eq!(
                            character.equipment,
                            original.characters[slot].as_ref().unwrap().equipment
                        );
                        assert_eq!(
                            character.weapon_slot,
                            original.characters[slot].as_ref().unwrap().weapon_slot
                        );
                        for (index, npc) in original.characters.iter().enumerate() {
                            if index != slot {
                                assert_eq!(&loaded.characters[index], npc);
                            }
                        }
                        let serialized = k1_data::game_state::serialize(&loaded).unwrap();
                        let item_bytes =
                            k1_data::game_state::ITEM_SLOTS * k1_data::game_state::ITEM_SIZE;
                        assert!(
                            serialized[..item_bytes] == bytes[..item_bytes],
                            "item bytes stay intact, including NaNs"
                        );
                    }
                }
            }
            let allocation = newgame_allocation(0, None)
                .unwrap()
                .adjusted(0, 2, -1)
                .unwrap()
                .adjusted(0, 0, 1)
                .unwrap();
            game.configure_new_game(hero, 0, 1, 2, Some(allocation.values))
                .unwrap();
            let configured = &game.session_state.as_ref().unwrap().1;
            let slot = game_state::on_map(
                configured,
                game_state::start_map(hero as usize),
                Some(hero as usize),
            )
            .start
            .unwrap()
            .members()
            .next()
            .unwrap();
            let character = configured.characters[slot].as_ref().unwrap();
            assert_eq!(character.attributes[0], 4);
            assert_eq!(character.attributes[6], 14);
            assert_eq!(character.free_points, 3);
            let menu = game.hero_preview(hero).unwrap();
            let playing = game.configured_hero_preview(hero).unwrap();
            assert_eq!((menu.width, menu.height), (playing.width, playing.height));
            assert!(
                menu.rgba == playing.rgba,
                "menu and session equipment must render identically"
            );
            assert!(
                game.configure_new_game(hero, 0, 0, 0, Some([99; 4]))
                    .is_err()
            );
            let map = game.start_map(hero);
            let scene = game
                .scene(
                    map,
                    &super::SceneOptions {
                        characters: true,
                        hero: Some(hero),
                        ..Default::default()
                    },
                )
                .unwrap();
            assert!(!scene.creature_pieces.is_empty());
            assert!(
                !game
                    .creature_animations_with_hero(map, Some(hero))
                    .unwrap()
                    .is_empty()
            );
            let configured = game.simulation_with_hero(map, 42, Some(hero)).unwrap();
            assert_eq!(
                configured.spawn_failures(),
                baseline.spawn_failures(),
                "constructor adds no placement failures"
            );
            game.clear_new_game();
            let restored = game
                .session_or_original(&bytes, super::MapSource::Game, Some(hero as usize))
                .unwrap();
            assert!(k1_data::game_state::serialize(&restored).unwrap() == bytes);

            assert_eq!(
                std::fs::read(root.join(format!("Game.{hero}"))).unwrap(),
                bytes
            );
        }
    }
}

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
        creature_id: None,
        depth_bottom: None,
        light_mask_index: None,
        self_lit: false,
        fine_light_value: 0,
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
fn flame_pieces_are_recognised_as_part_of_their_object() {
    // Flames share their object's draw slot, so a depth orderer must not
    // see them as extra stacks (review #68); detection covers the base
    // texture and every animation frame.
    assert!(piece(TextureKey::FlameFrame { index: 5 }).is_flame());
    assert!(
        !piece(TextureKey::Solid {
            color: [1, 2, 3, 4],
        })
        .is_flame()
    );
    let animated = Piece {
        animation: Some(Animation {
            frames: vec![frame(Some(TextureKey::FlameFrame { index: 1 }))],
            base: 0,
        }),
        ..piece(TextureKey::Solid {
            color: [1, 2, 3, 4],
        })
    };
    assert!(animated.is_flame());
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
        creature_pieces: Vec::new(),
        creature_origins: std::collections::HashMap::new(),
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
        creature_pieces: Vec::new(),
        creature_origins: std::collections::HashMap::new(),
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
        creature_pieces: Vec::new(),
        creature_origins: std::collections::HashMap::new(),
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
            creature_id: None,
            depth_bottom: None,
            light_mask_index: None,
            self_lit: false,
            fine_light_value: 0,
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
        creature_pieces: Vec::new(),
        creature_origins: std::collections::HashMap::new(),
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
            Lights,
            Decorations,
            Shadows,
            Objects,
            Characters,
            CharacterShadows,
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
        [Terrain, Lights, Decorations, Shadows, Objects],
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
