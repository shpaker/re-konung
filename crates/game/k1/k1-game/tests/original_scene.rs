//! Тесты сцены на реальных данных из `original/` (без них пропускаются).

use std::collections::HashMap;
use std::fmt::Write as _;
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
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../original/k1");
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
                        .filter_map(|(character, cx, cy)| {
                            let (px, py) = game_state::character_position(cx, cy);
                            scene::pose_in_with_lights(
                                character,
                                px,
                                py,
                                &state,
                                &graphics,
                                &scene::PoseKind::of_record(character),
                                lights.as_ref(),
                                cx,
                                cy,
                            )
                            .ok()
                            .flatten()
                        })
                        .collect::<Vec<_>>();
                    let whole =
                        scene::draw_list(&input, graph, Some(objects), &resources.tables).unwrap();
                    // Two implementations of the same order: `assemble` moves the
                    // map, `ordered` borrows it and can be asked again. Comparing
                    // the borrowed one with the built list is not the list compared
                    // with itself — the pre-split reference does that job elsewhere
                    let map =
                        scene::map_scene(&input, graph, Some(objects), &resources.tables).unwrap();
                    assert_eq!(
                        map.ordered(&poses).into_iter().cloned().collect::<Vec<_>>(),
                        whole,
                        "{edition} konung.{number}: the borrowed order is not the built list"
                    );
                    assert_eq!(
                        map.ordered(&poses).into_iter().cloned().collect::<Vec<_>>(),
                        whole,
                        "{edition} konung.{number}: a second question to the same map changed its answer"
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
                                let pose = scene::pose_in(character, px, py, &state, &graphics,
    &scene::PoseKind::start(state_value, direction))
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

/// FNV-1a over the debug form of every piece: composition, order, position, size,
/// group and each animation frame with its own offsets and tick count. Written by
/// hand so the number does not depend on a hashing implementation, and identical to
/// the one that produced [`PRE_SPLIT_DIGESTS`] — apart from the creature identity
/// tag, which postdates the reference and is not part of the draw list: it prints
/// last, so dropping it from the line reproduces the pre-tag form exactly.
fn digest(pieces: &[scene::DrawPiece]) -> u64 {
    let mut text = String::with_capacity(pieces.len() * 96);
    for piece in pieces {
        let line = format!("{piece:?}");
        let body = line[..line.len() - 1]
            .rsplit_once(", creature_id: ")
            .expect("piece debug must end with the identity tag")
            .0;
        writeln!(&mut text, "{body} }}").unwrap();
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Historical reference: draw-list digests as produced by the code before the
/// map/poses split (`a84da9d`, all overlays, `Game.0`). Preserved so a reviewer
/// can see which maps the flag-8 adjustment reordered; not used by the assertion.
#[allow(dead_code)]
const PRE_SPLIT_DIGESTS: &[(&str, u32, u64)] = &[
    // RUS: konung.<map> -> digest of the whole draw list
    ("rus", 1, 5749883684937406443),
    ("rus", 2, 9161472304994093849),
    ("rus", 3, 15441249324165904371),
    ("rus", 4, 10310950892279433450),
    ("rus", 5, 7865325501130524335),
    ("rus", 6, 1220244666432478341),
    ("rus", 7, 3606969751509788418),
    ("rus", 8, 12187214553015334689),
    ("rus", 10, 17998478349977806281),
    ("rus", 11, 15738468660808415919),
    ("rus", 12, 11506854818216821933),
    ("rus", 14, 2599733937630678180),
    ("rus", 15, 14665066190776868239),
    ("rus", 16, 16933600115205983393),
    ("rus", 17, 11451060058494973695),
    ("rus", 19, 17568150206872617557),
    ("rus", 20, 7472022925656013005),
    ("rus", 21, 7310076132161249349),
    ("rus", 22, 3781409832599483759),
    ("rus", 23, 2516170089819141946),
    ("rus", 24, 11739767581096017853),
    ("rus", 25, 14963309209168605188),
    ("rus", 27, 16318292348713827363),
    ("rus", 28, 12553299292801783248),
    ("rus", 29, 9291741145341910911),
    ("rus", 30, 3020644430475566515),
    ("rus", 32, 16641061535675611849),
    ("rus", 33, 6697558674563883508),
    ("rus", 35, 7017169008861186178),
    ("rus", 36, 2487565378183083243),
    ("rus", 37, 887267549316916178),
    ("rus", 38, 14709114758235585212),
    ("rus", 39, 16483379676538354137),
    ("rus", 40, 15931316204313533989),
    ("rus", 42, 5740726217271924841),
    ("rus", 43, 14536132049177961780),
    ("rus", 44, 8543204414698186456),
    ("rus", 45, 9558525583742928606),
    ("rus", 46, 7151706124225445335),
    ("rus", 47, 10500731466997366681),
    ("rus", 48, 1681298281485412061),
    ("rus", 49, 13226154613141434537),
    ("rus", 50, 12476644280498086465),
    // ENG: konung.<map> -> digest of the whole draw list
    ("eng", 1, 12562467986253598706),
    ("eng", 2, 12522008781678676449),
    ("eng", 3, 15441249324165904371),
    ("eng", 4, 3403604186159680504),
    ("eng", 5, 7865325501130524335),
    ("eng", 6, 1220244666432478341),
    ("eng", 7, 360051675474091955),
    ("eng", 8, 12187214553015334689),
    ("eng", 10, 17998478349977806281),
    ("eng", 11, 7851809796508020556),
    ("eng", 12, 16859731634212215923),
    ("eng", 14, 2599733937630678180),
    ("eng", 15, 14665066190776868239),
    ("eng", 16, 11705496971328505016),
    ("eng", 17, 11451060058494973695),
    ("eng", 19, 12069145880203745133),
    ("eng", 20, 7472022925656013005),
    ("eng", 21, 7310076132161249349),
    ("eng", 22, 4896757579899622059),
    ("eng", 23, 2516170089819141946),
    ("eng", 24, 12387401690749881455),
    ("eng", 25, 14963309209168605188),
    ("eng", 27, 3725805137010609191),
    ("eng", 28, 12553299292801783248),
    ("eng", 29, 12566428361159402877),
    ("eng", 30, 3020644430475566515),
    ("eng", 32, 16641061535675611849),
    ("eng", 33, 5559463842684049742),
    ("eng", 35, 11458689056935783505),
    ("eng", 36, 2487565378183083243),
    ("eng", 37, 14061130096415377861),
    ("eng", 38, 14709114758235585212),
    ("eng", 39, 5892485684375647732),
    ("eng", 40, 15931316204313533989),
    ("eng", 42, 5740726217271924841),
    ("eng", 43, 14536132049177961780),
    ("eng", 44, 8543204414698186456),
    ("eng", 45, 9558525583742928606),
    ("eng", 46, 7151706124225445335),
    ("eng", 47, 10500731466997366681),
    ("eng", 48, 1681298281485412061),
    ("eng", 49, 13226154613141434537),
    ("eng", 50, 12476644280498086465),
];

/// Primary reference: draw-list digest of every map after the flag-8 depth-sort
/// adjustment (Slavik `game.cpp:1391-1395`). Maps where the adjustment changed
/// the order differ from [`PRE_SPLIT_DIGESTS`]; the rest are identical.
const POST_FLAG8_DIGESTS: &[(&str, u32, u64)] = &[
    ("rus", 1, 7751050688764054441),
    ("rus", 2, 9100521088803160881),
    ("rus", 3, 1837156135542868103),
    ("rus", 4, 6232119868689327552),
    ("rus", 5, 9031875089201322691),
    ("rus", 6, 14928976217420981435),
    ("rus", 7, 2012098144737636874),
    ("rus", 8, 369780053321023675),
    ("rus", 10, 16336754863142647349),
    ("rus", 11, 4949236801994068533),
    ("rus", 12, 1547052434354408905),
    ("rus", 14, 16115779341952529302),
    ("rus", 15, 3521583510993450531),
    ("rus", 16, 1588369657751987589),
    ("rus", 17, 2133267109279268587),
    ("rus", 19, 7652071836678500047),
    ("rus", 20, 12709642136215082547),
    ("rus", 21, 17996326481617184763),
    ("rus", 22, 9405981049992134919),
    ("rus", 23, 3790785764411254642),
    ("rus", 24, 18359560027374005249),
    ("rus", 25, 9626325124889049630),
    ("rus", 27, 8119071206318357427),
    ("rus", 28, 2159547171426110132),
    ("rus", 29, 13162320988692396997),
    ("rus", 30, 14537947996400604521),
    ("rus", 32, 491584832894380003),
    ("rus", 33, 9482818873014866218),
    ("rus", 35, 10151279564339630218),
    ("rus", 36, 14891150631671661363),
    ("rus", 37, 2812090491949653406),
    ("rus", 38, 8979626187828735066),
    ("rus", 39, 15261333884600124241),
    ("rus", 40, 16858156007334440001),
    ("rus", 42, 5740726217271924841),
    ("rus", 43, 14536132049177961780),
    ("rus", 44, 8543204414698186456),
    ("rus", 45, 9558525583742928606),
    ("rus", 46, 7151706124225445335),
    ("rus", 47, 9217262604101312185),
    ("rus", 48, 1681298281485412061),
    ("rus", 49, 13226154613141434537),
    ("rus", 50, 12476644280498086465),
    ("eng", 1, 9241924339203940228),
    ("eng", 2, 13937853363716827377),
    ("eng", 3, 1837156135542868103),
    ("eng", 4, 1868218703519829134),
    ("eng", 5, 9031875089201322691),
    ("eng", 6, 14928976217420981435),
    ("eng", 7, 8613486564645303849),
    ("eng", 8, 369780053321023675),
    ("eng", 10, 16336754863142647349),
    ("eng", 11, 3412040631964527610),
    ("eng", 12, 12949463290280154759),
    ("eng", 14, 16115779341952529302),
    ("eng", 15, 3521583510993450531),
    ("eng", 16, 6804888714354781568),
    ("eng", 17, 2133267109279268587),
    ("eng", 19, 9994681615030821233),
    ("eng", 20, 12709642136215082547),
    ("eng", 21, 17996326481617184763),
    ("eng", 22, 17768143932159521339),
    ("eng", 23, 3790785764411254642),
    ("eng", 24, 7069703083486317211),
    ("eng", 25, 9626325124889049630),
    ("eng", 27, 1385033059252792871),
    ("eng", 28, 2159547171426110132),
    ("eng", 29, 13509659145544073095),
    ("eng", 30, 14537947996400604521),
    ("eng", 32, 491584832894380003),
    ("eng", 33, 5030459936296777160),
    ("eng", 35, 4546636124916485053),
    ("eng", 36, 14891150631671661363),
    ("eng", 37, 4443678815409670493),
    ("eng", 38, 8979626187828735066),
    ("eng", 39, 16710547644085996782),
    ("eng", 40, 16858156007334440001),
    ("eng", 42, 5740726217271924841),
    ("eng", 43, 14536132049177961780),
    ("eng", 44, 8543204414698186456),
    ("eng", 45, 9558525583742928606),
    ("eng", 46, 7151706124225445335),
    ("eng", 47, 9217262604101312185),
    ("eng", 48, 1681298281485412061),
    ("eng", 49, 13226154613141434537),
    ("eng", 50, 12476644280498086465),
];

#[test]
fn draw_list_matches_the_digest_taken_before_the_split() {
    for (edition, game) in game_dirs() {
        let resources = Resources::load(&game).unwrap();
        let data = game.state_bytes(0, GAME_FILES).unwrap();
        let state = game_state::parse(&data).unwrap();
        let mut compared = 0;
        for number in game.map_numbers(GAME_FILES).unwrap() {
            let pieces = draw_with_state(&game, &resources, number, EVERY_SWITCH, &state, 0);
            let expected = POST_FLAG8_DIGESTS
                .iter()
                .find(|(name, map, _)| *name == edition && *map == number)
                .map(|(_, _, d)| *d)
                .unwrap_or_else(|| panic!("{edition} konung.{number}: no reference digest"));
            assert_eq!(
                digest(&pieces),
                expected,
                "{edition} konung.{number}: the draw list changed"
            );
            compared += 1;
        }
        let reference = POST_FLAG8_DIGESTS
            .iter()
            .filter(|(name, _, _)| *name == edition)
            .count();
        assert_eq!(
            compared, reference,
            "{edition}: the reference holds {reference} maps, the test walked {compared}"
        );
    }
}

/// Finding 1 of the review: the map must stay usable after its list was taken, so
/// that #37 can reorder it on every change of a pose without rebuilding it.
#[test]
fn a_map_scene_is_reusable_for_another_set_of_poses() {
    for (edition, game) in game_dirs() {
        let resources = Resources::load(&game).unwrap();
        let data = game.state_bytes(0, GAME_FILES).unwrap();
        let state = game_state::parse(&data).unwrap();
        let map_bytes = game.map_bytes(1, GAME_FILES).unwrap();
        let konung_map = map::parse(&map_bytes).unwrap();
        let lights_bytes = game.lights_bytes(1, GAME_FILES).unwrap();
        let lights = lights_bytes
            .as_ref()
            .map(|bytes| map::parse_lights(bytes).unwrap());
        let entities = game_state::on_map(&state, 1, Some(0));
        let input = SceneInput {
            map: &konung_map,
            lights: lights.as_ref(),
            entities: Some(entities),
            state: Some(&state),
            overlays: EVERY_SWITCH,
        };
        resources
            .with_graphics(|graph, objects| {
                let graphics = scene::Graphics::new(graph, objects, &resources.tables);
                let map =
                    scene::map_scene(&input, graph, Some(objects), &resources.tables).unwrap();
                let recorded =
                    scene::creature_poses(&input, graph, Some(objects), &resources.tables).unwrap();
                let static_stacks = map.stacks().len();
                assert!(
                    static_stacks > 0,
                    "{edition} konung.1: the map holds no object to sort creatures among"
                );

                // The same map, asked twice: the first answer is the list the
                // single pass builds, the second is a map where everyone turned to
                // face east and started to walk.
                let first: Vec<scene::DrawPiece> =
                    map.ordered(&recorded).into_iter().cloned().collect();
                assert_eq!(
                    first,
                    scene::draw_list(&input, graph, Some(objects), &resources.tables).unwrap(),
                    "{edition} konung.1: a reused map orders something other than the full pass"
                );
                let turned = characters::placed(input.entities.as_ref().unwrap(), &state)
                    .into_iter()
                    .filter_map(|(character, x, y)| {
                        let (px, py) = game_state::character_position(x, y);
                        scene::pose_in(
                            character,
                            px,
                            py,
                            &state,
                            &graphics,
                            &scene::PoseKind::start(1, 4),
                        )
                        .ok()
                        .flatten()
                    })
                    .collect::<Vec<_>>();
                let second: Vec<scene::DrawPiece> =
                    map.ordered(&turned).into_iter().cloned().collect();
                assert_eq!(
                    map.stacks().len(),
                    static_stacks,
                    "{edition} konung.1: asking for a new list changed the map itself"
                );
                assert!(
                    second != first,
                    "{edition} konung.1: turning every creature left the list untouched"
                );
            })
            .unwrap();
    }
}

/// Finding 2 of the review: a pose of another animation must not inherit the frame
/// index of the record, and the recorded frame stays the default.
#[test]
fn a_pose_frame_is_the_recorded_one_unless_asked_otherwise() {
    for (edition, game) in game_dirs() {
        let resources = Resources::load(&game).unwrap();
        let data = game.state_bytes(0, GAME_FILES).unwrap();
        let state = game_state::parse(&data).unwrap();
        let entities = game_state::on_map(&state, 1, Some(0));
        resources
            .with_graphics(|graph, objects| {
                let graphics = scene::Graphics::new(graph, objects, &resources.tables);
                let (character, x, y) = characters::placed(&entities, &state)
                    .into_iter()
                    .find(|(creature, _, _)| !creature.has_dynamic_sprite())
                    .expect("konung.1 holds no human");
                let (px, py) = game_state::character_position(x, y);
                let record = scene::PoseKind::of_record(character);
                let frames = characters::frame_count(character, graph, objects);

                let recorded = scene::pose_in(character, px, py, &state, &graphics, &record)
                    .unwrap()
                    .expect("the creature has no pose");
                let base = animation_of(&recorded).base;
                assert_eq!(
                    base,
                    usize::from(character.frame).min(frames.saturating_sub(1)),
                    "{edition}: the recorded pose lost its own frame"
                );

                // A fresh animation starts at its first frame, whatever the record
                // said about the old one.
                let started = scene::pose_in(
                    character,
                    px,
                    py,
                    &state,
                    &graphics,
                    &scene::PoseKind::start(record.state, record.direction),
                )
                .unwrap()
                .expect("the creature has no walking pose");
                assert_eq!(animation_of(&started).base, 0, "{edition}");

                // An explicit frame beyond the animation is clamped, not wrapped:
                // the original's own sequence lookup falls back to the first frame
                // (docs/k1/formats/graph-res.md), never to an index outside it.
                let clamped = scene::pose_in(
                    character,
                    px,
                    py,
                    &state,
                    &graphics,
                    &scene::PoseKind {
                        state: record.state,
                        direction: record.direction,
                        frame: Some(u8::MAX),
                    },
                )
                .unwrap()
                .expect("the creature has no pose");
                assert_eq!(
                    animation_of(&clamped).base,
                    frames - 1,
                    "{edition}: frame {frames} - 1 is the last one of this animation"
                );
            })
            .unwrap();
    }
}

/// The first piece of a pose that really animates: a layer of one frame carries no
/// animation at all.
fn animation_of(pose: &scene::Pose) -> &scene::DrawAnimation {
    pose.pieces
        .iter()
        .find_map(|piece| piece.animation.as_ref())
        .expect("a pose of a twelve-frame animation holds no animated layer")
}
