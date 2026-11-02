//! Тесты сцены на реальных данных из `original/` (без них пропускаются).

use std::collections::HashMap;
use std::path::PathBuf;

use k1_data::{
    characters, game_state, map,
    objects_res::{self, FRAMES},
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

/// Список отрисовки вместе с начальным состоянием: без него персонажей нет.
fn draw_with_state(
    game: &GameDir,
    resources: &Resources,
    number: u32,
    overlays: Overlays,
    state: &game_state::GameState,
    hero: usize,
) -> Vec<scene::DrawPiece> {
    let map_bytes = game.map_bytes(number, GAME_FILES).unwrap();
    let konung_map = map::parse(&map_bytes).unwrap();
    let lights_bytes = game.lights_bytes(number, GAME_FILES).unwrap();
    let lights = lights_bytes
        .as_ref()
        .map(|bytes| map::parse_lights(bytes).unwrap());
    let entities = game_state::on_map(state, number as usize, Some(hero));
    let input = SceneInput {
        map: &konung_map,
        lights: lights.as_ref(),
        entities: Some(entities),
        state: Some(state),
        overlays,
    };
    resources
        .with_graphics(|graph, objects| {
            scene::draw_list(&input, graph, Some(objects), &resources.tables).unwrap()
        })
        .unwrap()
}

/// Чем является ключ текстуры кадра, кроме номера кадра: слой анимации не
/// должен перепутаться с другим слоем.
fn layer_signature(texture: &TextureKey) -> String {
    match *texture {
        TextureKey::ObjectFrame { object_id, .. } => format!("объект {object_id}"),
        TextureKey::ObjectShadow { object_id, .. } => format!("тень объекта {object_id}"),
        TextureKey::FlameFrame { .. } => "огонь".to_owned(),
        TextureKey::SpriteImage {
            source, palette, ..
        } => {
            format!("спрайт {source:?} палитра {palette}")
        }
        TextureKey::SpriteShadow { source, .. } => format!("тень {source:?}"),
        other => format!("{other:?}"),
    }
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
            .filter(|piece| matches!(piece.texture, Some(TextureKey::Tile { .. })))
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
                .any(|piece| matches!(piece.texture, Some(TextureKey::ObjectFrame { .. }))),
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
                .any(|piece| matches!(piece.texture, Some(TextureKey::SpriteImage { .. }))),
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
                .filter(|piece| piece.texture == Some(TextureKey::FootDiamond { color }))
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
            .filter(|piece| matches!(piece.texture, Some(TextureKey::LightMask { .. })))
            .count();
        assert_eq!(masks, lit, "{edition}: маски света");
    }
}

#[test]
fn animated_pieces_keep_their_layer_and_base_frame() {
    for (edition, game) in game_dirs() {
        let resources = Resources::load(&game).unwrap();
        let number = game.map_numbers(GAME_FILES).unwrap()[0];
        let hero = game.state_numbers(GAME_FILES).unwrap()[0] as usize;
        let state = game_state::parse(&game.state_bytes(hero as u32, GAME_FILES).unwrap()).unwrap();
        let overlays = Overlays {
            characters: true,
            ..Overlays::default()
        };
        let pieces = draw_with_state(&game, &resources, number, overlays, &state, hero);
        let mut animated = 0;
        for piece in &pieces {
            let Some(animation) = &piece.animation else {
                continue;
            };
            animated += 1;
            // Базовый кадр — тот же кусок, что рисовался и до анимаций: та же
            // картинка в том же углу, поэтому при выключенной анимации картина
            // совпадает с эталоном
            let base = &animation.frames[animation.base];
            assert_eq!(
                base.texture, piece.texture,
                "{edition}: konung.{number}: базовый кадр ≠ картинка куска"
            );
            assert_eq!(
                (base.dx, base.dy),
                (0, 0),
                "{edition}: смещение базового кадра"
            );
            assert!(
                animation.frames.len() >= 2,
                "{edition}: анимация из одного кадра"
            );
            // Слои не перепутаны: все кадры одного куска — один и тот же слой
            let Some(reference) = animation.frames.iter().find_map(|frame| frame.texture) else {
                panic!("{edition}: konung.{number}: у анимированного куска нет ни одного кадра");
            };
            let want = layer_signature(&reference);
            for frame in &animation.frames {
                assert!(frame.ticks >= 1, "{edition}: нулевая длительность кадра");
                if let Some(texture) = frame.texture {
                    assert_eq!(
                        layer_signature(&texture),
                        want,
                        "{edition}: konung.{number}: слой сменился внутри анимации"
                    );
                }
            }
        }
        assert!(
            animated > 0,
            "{edition}: konung.{number}: ни один кусок не анимируется"
        );
    }
}

#[test]
fn flame_pieces_cover_the_frames_of_their_kind() {
    // Таблица огней в exe задаёт диапазон кадров вида, оба конца включены
    // (docs/k1/formats/executable.md): кусок огня обязан пройтись по нему целиком
    for (edition, game) in game_dirs() {
        let resources = Resources::load(&game).unwrap();
        let kinds = resources.tables.flame_animations;
        let mut checked = 0;
        for number in game.map_numbers(GAME_FILES).unwrap() {
            let pieces = draw(&game, &resources, number, Overlays::default());
            for piece in &pieces {
                let Some(animation) = &piece.animation else {
                    continue;
                };
                let Some(TextureKey::FlameFrame { index: first }) =
                    animation.frames[animation.base].texture
                else {
                    continue;
                };
                let range = kinds
                    .iter()
                    .find(|(start, _)| *start as usize == first)
                    .unwrap_or_else(|| panic!("{edition}: огонь начинается с кадра {first}"));
                let expected = (range.1.max(range.0) - range.0) as usize + 1;
                assert_eq!(
                    animation.frames.len(),
                    expected,
                    "{edition}: konung.{number}: огонь от кадра {first} за [{}, {}]",
                    range.0,
                    range.1
                );
                for (offset, frame) in animation.frames.iter().enumerate() {
                    assert_eq!(
                        frame.texture,
                        Some(TextureKey::FlameFrame {
                            index: first + offset
                        }),
                        "{edition}: кадры огня идут подряд"
                    );
                    assert_eq!(frame.ticks, 1, "{edition}: огонь меняется каждый тик");
                }
                checked += 1;
            }
            if checked > 0 {
                break;
            }
        }
        assert!(
            checked > 0,
            "{edition}: ни на одной карте нет анимированного огня"
        );
    }
}

#[test]
fn animated_objects_play_all_their_frames_at_their_frame_time() {
    // OBJECTS.RES: у анимированного объекта `frame_count` кадров, кадр меняется
    // раз в `frame_time` тиков (docs/k1/formats/objects-res.md). Объект с
    // несколькими кадрами, но без frame_time игра не анимирует.
    for (edition, game) in game_dirs() {
        let resources = Resources::load(&game).unwrap();
        let expected: HashMap<usize, (usize, u32)> = resources
            .with_graphics(|_, objects| {
                objects
                    .simple
                    .iter()
                    .enumerate()
                    .filter_map(|(object_id, simple)| {
                        let simple = simple.as_ref()?;
                        let duration = objects_res::frame_duration_ms(simple)?;
                        let frames = (simple.frame_count as usize).min(FRAMES);
                        let ticks = (duration / objects_res::TICK_MS).max(1);
                        (frames >= 2).then_some((object_id, (frames, ticks)))
                    })
                    .collect()
            })
            .unwrap();
        let mut checked = 0;
        for number in game.map_numbers(GAME_FILES).unwrap() {
            let pieces = draw(&game, &resources, number, Overlays::default());
            for piece in &pieces {
                let Some(animation) = &piece.animation else {
                    continue;
                };
                let Some(object_id) = (match animation.frames[animation.base].texture {
                    Some(TextureKey::ObjectFrame { object_id, .. })
                    | Some(TextureKey::ObjectShadow { object_id, .. }) => Some(object_id),
                    _ => None,
                }) else {
                    continue;
                };
                let (frames, ticks) = *expected
                    .get(&object_id)
                    .unwrap_or_else(|| panic!("{edition}: объект {object_id} не анимирован"));
                assert_eq!(
                    animation.frames.len(),
                    frames,
                    "{edition}: konung.{number}: объект {object_id}: число кадров"
                );
                assert!(
                    animation.frames.iter().all(|frame| frame.ticks == ticks),
                    "{edition}: konung.{number}: объект {object_id}: длительность кадра"
                );
                checked += 1;
            }
            if checked > 0 {
                break;
            }
        }
        assert!(
            checked > 0,
            "{edition}: ни один анимированный объект не нарисован"
        );
    }
}

/// Все слои сразу.
const EVERY_SWITCH: Overlays = Overlays {
    foot: true,
    lights: true,
    markers: true,
    characters: true,
};

/// По одному включённому слою: `foot`, `lights`, `markers`, `characters`.
const ONE_SWITCH: [Overlays; 4] = [
    Overlays {
        foot: true,
        lights: false,
        markers: false,
        characters: false,
    },
    Overlays {
        foot: false,
        lights: true,
        markers: false,
        characters: false,
    },
    Overlays {
        foot: false,
        lights: false,
        markers: true,
        characters: false,
    },
    Overlays {
        foot: false,
        lights: false,
        markers: false,
        characters: true,
    },
];

/// Куски списка отрисовки, которые показываются при этом наборе слоёв.
fn shown<'a>(pieces: &'a [scene::DrawPiece], overlays: &Overlays) -> Vec<&'a scene::DrawPiece> {
    pieces
        .iter()
        .filter(|piece| piece.group.is_shown_by(overlays))
        .collect()
}

/// Сколько кусков добавляет слой к чистой карте.
fn layer_pieces(every: &[scene::DrawPiece], overlays: &Overlays) -> usize {
    every
        .iter()
        .filter(|piece| {
            piece.group.is_shown_by(overlays) && !piece.group.is_shown_by(&Overlays::default())
        })
        .count()
}

/// Группа переключается видимостью, а не составом списка отрисовки.
///
/// Карта со всеми слоями, отфильтрованная [`scene::PieceGroup::is_shown_by`],
/// обязана совпасть со списком той же карты, собранной только с этими слоями:
/// иначе чужой переключатель прятал бы кусок карты или оставлял скрытый поверх.
/// Побайтные тесты записи этого не касаются, а на картинке регрессия выглядела
/// бы как пропавший огонь или уехавшая тень.
#[test]
fn group_filter_matches_a_scene_without_the_layer() {
    for (edition, game) in game_dirs() {
        let resources = Resources::load(&game).unwrap();
        let hero = game.state_numbers(GAME_FILES).unwrap()[0] as usize;
        let state = game_state::parse(&game.state_bytes(hero as u32, GAME_FILES).unwrap()).unwrap();
        // Каждый слой сверяем на карте, где он действительно что-то рисует:
        // сверка пустого множества ничего не доказывает
        let mut verified = [false; 4];
        for number in game.map_numbers(GAME_FILES).unwrap() {
            let every = draw_with_state(&game, &resources, number, EVERY_SWITCH, &state, hero);
            let clean =
                draw_with_state(&game, &resources, number, Overlays::default(), &state, hero);
            assert_eq!(
                shown(&every, &Overlays::default()),
                clean.iter().collect::<Vec<_>>(),
                "{edition}: konung.{number} без слоёв"
            );
            for (index, overlays) in ONE_SWITCH.iter().enumerate() {
                if verified[index] || layer_pieces(&every, overlays) == 0 {
                    continue;
                }
                let expected = draw_with_state(&game, &resources, number, *overlays, &state, hero);
                assert_eq!(
                    shown(&every, overlays),
                    expected.iter().collect::<Vec<_>>(),
                    "{edition}: konung.{number} со слоем {overlays:?}"
                );
                verified[index] = true;
            }
        }
        assert!(
            verified.iter().all(|done| *done),
            "{edition}: не все слои нашлись на картах: {verified:?}"
        );
    }
}

/// Criterion 8 of #36: the map without creatures plus the poses built through the
/// public `scene::pose_in` give exactly the same draw list — same composition,
/// order and depth — as the single pass `scene::draw_list`.
#[test]
fn map_plus_poses_gives_the_same_draw_list() {
    for (edition, game) in game_dirs() {
        let resources = Resources::load(&game).unwrap();
        let data = game.state_bytes(0, GAME_FILES).unwrap();
        let state = game_state::parse(&data).unwrap();
        // GRAPH.RES and Objects.res are parsed once per edition: 66 megabytes, and
        // a parse per map would make the test cost more than the check it guards
        let reached = resources
            .with_graphics(|graph, objects| {
                let graphics = scene::Graphics::new(graph, objects, &resources.tables);
                let mut reached = 0;
                for number in game.map_numbers(GAME_FILES).unwrap() {
                    let map_bytes = game.map_bytes(number, GAME_FILES).unwrap();
                    let konung_map = map::parse(&map_bytes).unwrap();
                    let lights_bytes = game.lights_bytes(number, GAME_FILES).unwrap();
                    let lights = lights_bytes
                        .as_ref()
                        .map(|bytes| map::parse_lights(bytes).unwrap());
                    let input = SceneInput {
                        map: &konung_map,
                        lights: lights.as_ref(),
                        entities: Some(game_state::on_map(&state, number as usize, Some(0))),
                        state: Some(&state),
                        overlays: EVERY_SWITCH,
                    };
                    let entities = input.entities.as_ref().unwrap();
                    let poses = characters::placed(entities, &state)
                        .into_iter()
                        .filter_map(|(character, x, y)| {
                            let (px, py) = game_state::character_position(x, y);
                            scene::pose_in(
                                character,
                                px,
                                py,
                                &state,
                                &graphics,
                                character.state,
                                character.direction,
                            )
                            .ok()
                            .flatten()
                        })
                        .collect::<Vec<_>>();
                    let map =
                        scene::map_scene(&input, graph, Some(objects), &resources.tables).unwrap();
                    assert_eq!(
                        scene::draw_list(&input, graph, Some(objects), &resources.tables).unwrap(),
                        scene::assemble(map, &poses),
                        "{edition} konung.{number}: map plus poses do not rebuild the same list"
                    );
                    if !poses.is_empty() {
                        reached += 1;
                    }
                }
                reached
            })
            .unwrap();
        // A guard against a test that silently measured nothing: an edition whose
        // maps hold no creatures would compare empty lists
        assert!(
            reached > 10,
            "{edition}: poses were built on only {reached} maps — the test checked almost nothing"
        );
    }
}

/// Criterion 7 of #36: a pose is built for every direction and for the rest and
/// walk states on real data of both editions, and its layers keep their count and
/// their length wherever the creature looks.
///
/// Composing a frame decodes its picture, so the test takes a few maps rather
/// than all 43: the point is that any state and direction is buildable, not that
/// every creature was measured.
#[test]
fn poses_build_for_every_direction_and_both_locomotion_states() {
    const REST: u8 = 0;
    const WALK: u8 = 1;
    // Map 1 carries armed humans, 17 the monsters of roaming groups, 19 villagers
    // with their workplaces
    let maps = [1, 17, 19];
    for (edition, game) in game_dirs() {
        let resources = Resources::load(&game).unwrap();
        let data = game.state_bytes(0, GAME_FILES).unwrap();
        let state = game_state::parse(&data).unwrap();
        resources
            .with_graphics(|graph, objects| {
                let graphics = scene::Graphics::new(graph, objects, &resources.tables);
                for number in maps {
                    let entities = game_state::on_map(&state, number, Some(0));
                    // Dynamic set 3 has no walk frames at all: it is not a pose the
                    // game builds while moving (docs/k1/movement.md)
                    let built = characters::placed(&entities, &state)
                        .into_iter()
                        .filter(|(character, _, _)| {
                            !(character.has_dynamic_sprite() && character.sprite_set == 3)
                        })
                        .collect::<Vec<_>>();
                    assert!(
                        !built.is_empty(),
                        "{edition} konung.{number}: no creature to build a pose from"
                    );
                    for (character, x, y) in &built {
                        let (px, py) = game_state::character_position(*x, *y);
                        for state_value in [REST, WALK] {
                            let mut layers = None;
                            for direction in 0..8u8 {
                                let pose = scene::pose_in(
                                    character,
                                    px,
                                    py,
                                    &state,
                                    &graphics,
                                    state_value,
                                    direction,
                                )
                                .unwrap()
                                .unwrap_or_else(|| {
                                    panic!(
                                        "{edition} konung.{number}: creature {} has no pose in state {state_value} direction {direction}",
                                        character.slot
                                    )
                                });
                                let mut record = (*character).clone();
                                record.state = state_value;
                                record.direction = direction;
                                let frames = characters::frame_count(&record, graph, objects);
                                for piece in &pose.pieces {
                                    // A layer of one frame is a static piece:
                                    // layer_track gives it no animation
                                    let length = piece
                                        .animation
                                        .as_ref()
                                        .map_or(1, |animation| animation.frames.len());
                                    assert_eq!(
                                        length, frames,
                                        "{edition} konung.{number}: creature {} state {state_value} direction {direction}: a layer holds {length} frames, the animation has {frames}",
                                        character.slot
                                    );
                                }
                                layers = match layers {
                                    None => Some(pose.pieces.len()),
                                    Some(count) => {
                                        assert_eq!(
                                            count,
                                            pose.pieces.len(),
                                            "{edition} konung.{number}: creature {} state {state_value} changed its layers between directions",
                                            character.slot
                                        );
                                        Some(count)
                                    }
                                };
                            }
                        }
                    }
                }
            })
            .unwrap();
    }
}
