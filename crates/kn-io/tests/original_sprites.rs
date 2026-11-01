//! Как карта ложится в мир Bevy на реальных данных из `original/` (без них пропускается).
//!
//! Тесты форматов и сверка списков отрисовки не видят того, что делает
//! `spawn_scene`: узлов групп, z по общему номеру куска, наследования видимости.
//! Здесь это проверяется на живом мире Bevy — без окна и рендерера.

use std::collections::HashMap;
use std::path::PathBuf;

use bevy::{
    asset::AssetPlugin, camera::visibility::VisibilityPlugin, mesh::MeshPlugin, prelude::*,
    transform::TransformPlugin,
};
use kn_game::{Game, MapSource, Piece, PieceGroup, Scene, SceneOptions, group_is_shown};
use kn_io::{
    Animator, GameResource, MapRoot, SceneGroup, ScenePiece, TextureCache, Z_STEP, spawn_scene,
};

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

/// Сцена карты и корень её спрайтов после сборки.
#[derive(Resource)]
struct Built {
    scene: Scene,
    root: Option<Entity>,
}

/// Первая карта игры: текстуры докачаны, куски разложены по узлам групп.
///
/// Возвращает мир, сцену (по ней строятся ожидания) и корень спрайтов.
fn spawn_map(edition: &'static str, dir: PathBuf) -> (App, Scene, Entity) {
    let game = Game::open(&dir).unwrap();
    let map_ref = game
        .maps()
        .unwrap()
        .into_iter()
        .find(|map| map.source == MapSource::Game)
        .expect("хотя бы одна карта игры");
    // Все слои разом: так карту собирает просмотрщик
    let scene = game.scene(map_ref, &SceneOptions::ALL).unwrap();
    assert!(
        scene.pieces.len() > 100,
        "{edition}: {map_ref:?}: меньше ста кусков — проверка шла бы по почти пустой карте"
    );
    let texture_ids = scene.texture_ids();
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        // TransformPlugin: без него GlobalTransform не считается, а именно по
        // нему рендер сортирует спрайты
        TransformPlugin,
        AssetPlugin::default(),
        ImagePlugin::default_nearest(),
        MeshPlugin,
        VisibilityPlugin,
    ))
    .insert_resource(GameResource(game))
    .init_resource::<TextureCache>()
    .insert_resource(Built {
        scene: scene.clone(),
        root: None,
    })
    .add_systems(
        Update,
        move |game: Res<GameResource>,
              mut built: ResMut<Built>,
              mut cache: ResMut<TextureCache>,
              mut images: ResMut<Assets<Image>>,
              mut commands: Commands| {
            if built.root.is_some() {
                return;
            }
            // Картинка, которая не собралась, исказила бы сверку ниже: тест
            // обязан упасть с причиной, а не сравнить не то
            let problems = cache
                .load(&game.0, &texture_ids, &mut images)
                .expect("ресурсы карты читаются");
            assert!(
                problems.is_empty(),
                "{edition}: {map_ref:?}: картинки не собрались: {problems:?}"
            );
            built.root = Some(spawn_scene(&mut commands, &built.scene, &cache));
        },
    );
    // Два кадра: команды разворачиваются после системы, а GlobalTransform и
    // видимость расходятся по потомкам в PostUpdate
    app.update();
    app.update();
    let root = app
        .world()
        .resource::<Built>()
        .root
        .expect("карта не собралась");
    (app, scene, root)
}

/// Кусок становится спрайтом, если у него есть хоть одна картинка: базовый кадр
/// или кадр анимации.
fn spawns(piece: &Piece) -> bool {
    piece.texture.is_some()
        || piece
            .animation
            .iter()
            .any(|animation| animation.frames.iter().any(|frame| frame.texture.is_some()))
}

/// Спрайты по узлам групп: группа → (номер куска в списке отрисовки, спрайт).
///
/// Номер читается из z: `piece_transform` ставит `order · Z_STEP`, поэтому z за
/// одно отвечает и за порядок наложения, и за то, что спрайт встал на своё место.
fn sprites(app: &App, root: Entity) -> HashMap<PieceGroup, Vec<(usize, Entity)>> {
    let world = app.world();
    assert!(
        world.get::<MapRoot>(root).is_some(),
        "корень спрайтов карты"
    );
    let mut groups = HashMap::new();
    for node in world.get::<Children>(root).unwrap() {
        let group = world.get::<SceneGroup>(*node).unwrap().0;
        // Без Transform у узла его потомки не получили бы GlobalTransform и не
        // попали бы в рендер — окно осталось бы пустым
        assert!(
            world.get::<GlobalTransform>(*node).is_some(),
            "{group:?}: у узла группы нет GlobalTransform"
        );
        let mut pieces = Vec::new();
        for sprite in world.get::<Children>(*node).unwrap() {
            assert!(
                world.get::<ScenePiece>(*sprite).is_some(),
                "под узлом группы — не спрайт куска"
            );
            let transform = *world.get::<GlobalTransform>(*sprite).unwrap();
            pieces.push((
                (transform.translation().z / Z_STEP).round() as usize,
                *sprite,
            ));
        }
        groups.insert(group, pieces);
    }
    groups
}

/// Номера кусков каждой группы в списке отрисовки, в порядке этого списка.
fn expected_orders(scene: &Scene) -> HashMap<PieceGroup, Vec<usize>> {
    let mut orders: HashMap<PieceGroup, Vec<usize>> = HashMap::new();
    for (order, piece) in scene.pieces.iter().enumerate() {
        if spawns(piece) {
            orders.entry(piece.group).or_default().push(order);
        }
    }
    orders
}

#[test]
fn every_group_of_a_real_map_gets_its_own_node() {
    for (edition, dir) in game_roots() {
        let (app, scene, root) = spawn_map(edition, dir);
        let groups = sprites(&app, root);
        // Узел — на каждую группу с кусками, в порядке PieceGroup::ALL
        let want = PieceGroup::ALL
            .into_iter()
            .filter(|group| {
                scene
                    .pieces
                    .iter()
                    .any(|piece| piece.group == *group && spawns(piece))
            })
            .collect::<Vec<_>>();
        let got = PieceGroup::ALL
            .into_iter()
            .filter(|group| groups.contains_key(group))
            .collect::<Vec<_>>();
        assert_eq!(got, want, "{edition}: узлы групп");
        // Спрайты группы стоят на z своих номеров в общем списке: разнос по
        // узлам не переставил наложение
        for (group, want) in &expected_orders(&scene) {
            let got = groups
                .get(group)
                .map(|pieces| pieces.iter().map(|(order, _)| *order).collect::<Vec<_>>())
                .unwrap_or_default();
            assert_eq!(&got, want, "{edition}: {group:?}: номера кусков");
        }
        // Кусок с картинкой наследует видимость группы, кусок без неё скрыт сам
        for (group, pieces) in &groups {
            for (order, sprite) in pieces {
                let want = if scene.pieces[*order].texture.is_some() {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                assert_eq!(
                    app.world().get::<Visibility>(*sprite),
                    Some(&want),
                    "{edition}: {group:?}: кусок №{order}"
                );
            }
        }
    }
}

#[test]
fn a_hidden_group_hides_only_its_own_sprites() {
    for (edition, dir) in game_roots() {
        let (mut app, _, root) = spawn_map(edition, dir);
        let groups = sprites(&app, root);
        let clean = SceneOptions::default();
        let switched = PieceGroup::ALL
            .into_iter()
            .filter(|group| groups.contains_key(group) && !group_is_shown(*group, &clean))
            .collect::<Vec<_>>();
        assert!(
            !switched.is_empty(),
            "{edition}: на карте нет ни одного переключемого слоя"
        );
        let nodes = app.world().get::<Children>(root).unwrap().to_vec();
        for node in nodes {
            let group = app.world().get::<SceneGroup>(node).unwrap().0;
            if switched.contains(&group) {
                *app.world_mut().get_mut::<Visibility>(node).unwrap() = Visibility::Hidden;
            }
        }
        // Один кадр: Hidden расходится по потомкам в PostUpdate
        app.update();
        let mut hidden = 0;
        for group in &switched {
            for (_, sprite) in &groups[group] {
                assert!(
                    !app.world()
                        .get::<InheritedVisibility>(*sprite)
                        .unwrap()
                        .get(),
                    "{edition}: {group:?}: спрайт скрытой группы виден"
                );
                hidden += 1;
            }
        }
        for (group, pieces) in &groups {
            if switched.contains(group) {
                continue;
            }
            for (_, sprite) in pieces {
                assert!(
                    app.world()
                        .get::<InheritedVisibility>(*sprite)
                        .unwrap()
                        .get(),
                    "{edition}: {group:?}: спрайт открытой группы скрылся"
                );
            }
        }
        assert!(hidden > 0, "{edition}: скрывать было нечего");
    }
}

#[test]
fn animated_pieces_keep_their_animators() {
    for (edition, dir) in game_roots() {
        let (app, scene, root) = spawn_map(edition, dir);
        let groups = sprites(&app, root);
        // Аниматор нужен каждому куску с картинкой в кадрах анимации: переезд
        // спрайта под узел группы его не потерял
        let want = scene
            .pieces
            .iter()
            .filter(|piece| {
                piece
                    .animation
                    .iter()
                    .any(|animation| animation.frames.iter().any(|frame| frame.texture.is_some()))
            })
            .count();
        let got = groups
            .values()
            .flatten()
            .filter(|(_, sprite)| app.world().get::<Animator>(*sprite).is_some())
            .count();
        assert!(want > 0, "{edition}: на карте нет анимированных кусков");
        assert_eq!(
            got, want,
            "{edition}: аниматор потерялся при разносе по группам"
        );
    }
}
