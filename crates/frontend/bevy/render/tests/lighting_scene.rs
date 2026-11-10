//! Which lighting material each surface carries on a real map from `original/`
//! (skipped without them): `spawn_scene` hands the pool's materials out by the
//! confirmed surface rules, and a changed [`LightingState`] reaches every
//! material without touching the scene's entity count.

use std::path::{Path, PathBuf};

use bevy::{asset::Assets, prelude::*, sprite_render::SpriteMaterial};
use kn_game::{Game, MapSource, PieceGroup, SceneOptions};
use kn_render::{
    KonungLightMaterial, LightingMaterials, LightingState, ScenePiece, SurfaceLighting,
    TextureCache, Z_STEP, spawn_scene, surface_lighting, update_lighting_uniforms,
};

fn game_roots() -> Vec<(&'static str, PathBuf)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../original/k1");
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

/// The game dir keeps LIGHTS.RES next to the maps; the raw tile-mask bytes go
/// straight into [`LightingMaterials::new`], sizes from the format's constants.
fn lights_res_masks(dir: &Path) -> Option<Vec<(Vec<u8>, u32, u32)>> {
    let path = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.eq_ignore_ascii_case("lights.res"))
        })
        .map(|entry| entry.path())?;
    let res = k1_data::lights_res::parse(&std::fs::read(path).ok()?).ok()?;
    Some(
        res.tiles
            .into_iter()
            .map(|mask| {
                (
                    mask,
                    k1_data::lights_res::TILE_MASK_WIDTH as u32,
                    k1_data::lights_res::TILE_MASK_HEIGHT as u32,
                )
            })
            .collect(),
    )
}

/// What the first map of an edition looks like lit: the scene, the sprite
/// root and the pool, so expectations can name every handle.
#[derive(Resource)]
struct Lit {
    scene: kn_game::Scene,
    root: Entity,
}

/// Spawn request the one-shot system consumes: `spawn_scene` needs `Commands`,
/// which only a system gets reliably.
#[derive(Resource)]
struct PendingSpawn(Option<(kn_game::Scene, TextureCache, LightingMaterials)>);

fn lit_map(edition: &'static str, dir: PathBuf) -> App {
    let game = Game::open(&dir).unwrap();
    let map_ref = game
        .maps()
        .unwrap()
        .into_iter()
        .find(|map| map.source == MapSource::Game)
        .expect("хотя бы одна карта игры");
    let scene = game.scene(map_ref, &SceneOptions::ALL).unwrap();
    let mut cache = TextureCache::default();
    let mut images = Assets::<Image>::default();
    let problems = cache
        .load(&game, &scene.texture_ids(), &mut images)
        .expect("ресурсы карты читаются");
    assert!(
        problems.is_empty(),
        "{edition}: {map_ref:?}: картинки не собрались: {problems:?}"
    );
    let masks = lights_res_masks(&dir).unwrap_or_else(|| panic!("{edition}: LIGHTS.RES не найден"));
    let mut materials = Assets::<KonungLightMaterial>::default();
    let pool = LightingMaterials::new(&mut images, &mut materials, &masks);
    let mut app = App::new();
    // GlobalTransform propagation: the draw order is read back from z.
    app.add_plugins(bevy::transform::TransformPlugin);
    app.insert_resource(images);
    app.insert_resource(materials);
    app.insert_resource(PendingSpawn(Some((scene, cache, pool))));
    app.add_systems(
        Update,
        |mut pending: ResMut<PendingSpawn>, mut commands: Commands| {
            let Some((scene, cache, pool)) = pending.0.take() else {
                return;
            };
            let root = spawn_scene(&mut commands, &scene, &cache, Some(&pool));
            // The pool as its own resource — what a frontend provides for the
            // uniform writer and the creature lighting to read.
            commands.insert_resource(pool);
            commands.insert_resource(Lit { scene, root });
        },
    );
    // First update spawns; the second settles `Children`/`GlobalTransform`.
    app.update();
    app.update();
    app
}

/// Scene pieces as (draw order, sprite entity), the order read back from z.
fn sprites(app: &App, root: Entity) -> Vec<(usize, Entity)> {
    let world = app.world();
    let mut sprites = Vec::new();
    for node in world.get::<Children>(root).unwrap() {
        for sprite in world.get::<Children>(*node).unwrap() {
            assert!(
                world.get::<ScenePiece>(*sprite).is_some(),
                "под узлом группы — не спрайт куска"
            );
            let z = world
                .get::<GlobalTransform>(*sprite)
                .unwrap()
                .translation()
                .z;
            sprites.push(((z / Z_STEP).round() as usize, *sprite));
        }
    }
    sprites
}

/// The scene pieces that `spawn_scene` turns into sprites, keyed by draw order.
fn spawned_pieces(scene: &kn_game::Scene) -> Vec<(usize, &kn_game::Piece)> {
    scene
        .pieces
        .iter()
        .enumerate()
        .filter(|(_, piece)| {
            // Creatures are drawn by their own nodes, not scene sprites.
            piece.creature_id.is_none()
                && (piece.texture.is_some()
                    || piece.animation.iter().any(|animation| {
                        animation.frames.iter().any(|frame| frame.texture.is_some())
                    }))
        })
        .collect()
}

#[test]
fn a_real_map_materials_exactly_the_pieces_the_rules_name() {
    for (edition, dir) in game_roots() {
        let app = lit_map(edition, dir);
        let lit = app.world().resource::<Lit>();
        let pool = app.world().resource::<LightingMaterials>();
        let by_order: std::collections::HashMap<usize, Entity> =
            sprites(&app, lit.root).into_iter().collect();
        let pieces = spawned_pieces(&lit.scene);
        assert_eq!(
            by_order.len(),
            pieces.len(),
            "{edition}: ровно спрайты кусков с картинками"
        );
        let mut masked_terrain = 0;
        let mut plain_terrain_or_decor = 0;
        let mut ambient_objects = 0;
        let mut unlit = 0;
        for (order, piece) in pieces {
            let material = app
                .world()
                .get::<SpriteMaterial<KonungLightMaterial>>(by_order[&order])
                .map(|material| &material.0);
            match surface_lighting(
                piece.group,
                piece.light_mask_index,
                piece.self_lit,
                piece.is_flame(),
            ) {
                SurfaceLighting::TerrainMasked(index) => {
                    assert_eq!(
                        material,
                        Some(&pool.terrain(Some(index))),
                        "{edition}: кусок №{order}: масочная земля"
                    );
                    masked_terrain += 1;
                }
                SurfaceLighting::TerrainPlain => {
                    assert_eq!(
                        material,
                        Some(&pool.terrain(None)),
                        "{edition}: кусок №{order}: земля или декорация без маски"
                    );
                    plain_terrain_or_decor += 1;
                }
                SurfaceLighting::Ambient => {
                    assert_eq!(
                        material,
                        Some(&pool.ambient()),
                        "{edition}: кусок №{order}: объект под обычным ambient"
                    );
                    ambient_objects += 1;
                }
                SurfaceLighting::None => {
                    assert!(
                        material.is_none(),
                        "{edition}: кусок №{order} ({:?}): огонь, самосвет или оверлей \
                         не должен нести материал",
                        piece.group
                    );
                    unlit += 1;
                }
            }
        }
        assert!(masked_terrain > 0, "{edition}: нет масочной земли");
        assert!(plain_terrain_or_decor > 0, "{edition}: нет простой земли");
        assert!(ambient_objects > 0, "{edition}: нет объектов под ambient");
        // Overlays the scene always builds (Lights at least) are the unlit
        // part; a map with neither overlays nor flames nor shadows would make
        // this vacuous, and shipped maps do have them.
        let overlays = lit
            .scene
            .pieces
            .iter()
            .filter(|piece| {
                matches!(
                    piece.group,
                    PieceGroup::Lights | PieceGroup::Foot | PieceGroup::Markers
                ) && piece.creature_id.is_none()
            })
            .count();
        assert!(
            overlays > 0 && unlit >= overlays,
            "{edition}: оверлеи ({overlays} кусков) должны быть среди {unlit} кусков без материала"
        );
    }
}

/// A changed state — what a simulation-tick change flows through — rewrites
/// the uniforms of the whole pool and leaves the scene's sprite count alone.
#[test]
fn a_changed_lighting_state_rewrites_the_uniforms_and_touches_no_entities() {
    for (edition, dir) in game_roots() {
        let mut app = lit_map(edition, dir);
        app.insert_resource(LightingState::default());
        app.add_systems(Update, update_lighting_uniforms);
        app.update();
        let sprites_before = app
            .world_mut()
            .query::<&ScenePiece>()
            .iter(app.world())
            .count();
        assert!(sprites_before > 0, "{edition}: карта без спрайтов");
        let (terrain_plain, ambient) = {
            let pool = app.world().resource::<LightingMaterials>();
            (pool.terrain(None), pool.ambient())
        };

        // Zero default first: the writer turns it into uniform no-ops.
        {
            let materials = app.world().resource::<Assets<KonungLightMaterial>>();
            assert_eq!(materials.get(&terrain_plain).unwrap().blend(), Vec3::ZERO);
            assert_eq!(materials.get(&ambient).unwrap().blend(), Vec3::ZERO);
        }

        // Night at tick 11000, ground window open: both blends non-zero, the
        // gate open for the masks.
        let night = LightingState {
            terrain_blend: Vec3::new(-0.6, -0.5, -0.7),
            ambient_blend: Vec3::new(-0.4, -0.3, -0.5),
            light_color: Vec3::new(0.1, 0.05, 0.0),
            light_gain: 1.0,
        };
        app.insert_resource(night);
        app.update();
        {
            let materials = app.world().resource::<Assets<KonungLightMaterial>>();
            assert_eq!(
                materials.get(&terrain_plain).unwrap().blend(),
                night.terrain_blend
            );
            assert_eq!(
                materials.get(&ambient).unwrap().blend(),
                night.ambient_blend
            );
            assert_eq!(materials.get(&terrain_plain).unwrap().light_gain(), 1.0);
            assert_eq!(materials.get(&ambient).unwrap().light_gain(), 1.0);
        }

        // Day — the zero triple is the shader's no-op for both surfaces, and
        // the closed window leaves the masks inert without touching the blend.
        let day = LightingState {
            terrain_blend: Vec3::ZERO,
            ambient_blend: Vec3::new(0.02, 0.01, 0.0),
            light_color: Vec3::ZERO,
            light_gain: 0.0,
        };
        app.insert_resource(day);
        app.update();
        {
            let materials = app.world().resource::<Assets<KonungLightMaterial>>();
            assert_eq!(materials.get(&terrain_plain).unwrap().blend(), Vec3::ZERO);
            assert_eq!(materials.get(&ambient).unwrap().blend(), day.ambient_blend);
        }
        let sprites_after = app
            .world_mut()
            .query::<&ScenePiece>()
            .iter(app.world())
            .count();
        assert_eq!(
            sprites_after, sprites_before,
            "{edition}: смена света не пересобирала сцену"
        );
    }
}
