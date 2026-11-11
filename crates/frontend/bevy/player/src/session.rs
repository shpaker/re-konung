//! Session build and teardown of the start map: everything between
//! `MainMenuAction::NewGame` and the `Loading` report the watcher submits.
//!
//! [`init`] is the synchronous half of the old startup loader: it requests the
//! hero's creatures from the shared subsystem, builds the lighting pool and the
//! fine grid, and loads the scene. [`teardown`] is its exact inverse and
//! idempotent, so a failed build leaves no half-session behind and a retry
//! starts from a clean world.

use bevy::ecs::world::CommandQueue;
use bevy::prelude::*;
use k1_data::repo::{GAME_FILES, GameDir};
use kn_app::SessionError;
use kn_game::{Game, GameError, MapRef, Scene, SceneOptions};
use kn_render::{
    CreatureMotion, CreatureRoot, CreatureSim, FineLightGrid, GameResource, KonungLightMaterial,
    LightingMaterials, MapRoot, PendingFrame, TextureCache, load_scene,
};

/// Hero for the new game: the start map and the on-map position depend on it.
#[derive(Resource)]
pub struct NewGame {
    pub hero: u32,
}

/// Present from a successful [`init`] until the watcher reports the session's
/// outcome; nothing else builds a session while it is in the world.
#[derive(Resource)]
pub struct SessionInit;

/// Seed of the creature simulation of the start map: fixed, so two runs of the
/// same hero walk the same routes, and the same value the viewer uses for its
/// "Reset positions" — one map, one reproducible spawn in either window.
pub const START_SEED: u64 = 42;

/// Asks the shared creature subsystem for the hero's start map.
///
/// The player's whole creature wiring is this request plus the plugin that
/// answers it: the subsystem builds the simulation, the clips, the depth map and
/// the composite nodes, and drives them off the scene clock. Nothing here
/// decides a creature's state — the game layer does, and the special of a
/// state-4 creature plays once into idle on its own (issue #70).
///
/// Returns the map it asked for together with the scene it built, so the caller
/// draws the same scene it handed over. A failure is typed for the `Loading`
/// report: map bytes that did not load or parse are [`SessionError::Map`],
/// everything else the scene assembly needs (resources, hero state,
/// composition) is [`SessionError::Scene`].
fn request_start_creatures(
    game: &Game,
    hero: u32,
    motion: &mut CreatureMotion,
) -> Result<(MapRef, Scene), SessionError> {
    let map_ref = game.start_map(hero);
    let options = SceneOptions {
        characters: true,
        hero: Some(hero),
        ..SceneOptions::default()
    };
    let scene = game
        .scene(map_ref, &options)
        .map_err(|error| match &error {
            GameError::Map(_) | GameError::Repo(_) | GameError::Io(_) => {
                error!("start map {map_ref:?} of hero {hero} did not load: {error}");
                SessionError::Map
            }
            _ => {
                error!("scene of start map {map_ref:?} (hero {hero}) did not build: {error}");
                SessionError::Scene
            }
        })?;
    motion.request(map_ref, START_SEED, Some(hero), scene.clone());
    Ok((map_ref, scene))
}

/// Builds the session: the creature request, the lighting pool, the fine grid
/// and the scene sprites — all-or-nothing. On success only [`SessionInit`] is
/// inserted (readiness is the watcher's call, not the builder's); on failure
/// everything built along the way is torn back down and the kind is returned
/// for the `Loading` report.
///
/// A texture that fails to decode fails the whole session: a map drawn with
/// holes is not a session. The old startup loader only warned and continued —
/// the all-or-nothing rule of the app plan replaces that.
pub fn init(world: &mut World) -> Result<(), SessionError> {
    let hero = world.resource::<NewGame>().hero;
    let (map_ref, scene) = world.resource_scope::<CreatureMotion, _>(|world, mut motion| {
        let game = world.resource::<GameResource>();
        request_start_creatures(&game.0, hero, &mut motion)
    })?;

    // The lighting of the start map: the pool from LIGHTS.RES and this map's
    // fine grid, the same resources the viewer provides. Uniforms arrive every
    // frame from the simulation tick through `update_lighting_state`.
    let (masks, fine) = {
        let game = world.resource::<GameResource>();
        (lights_res_masks(&game.0), map_fine_lights(&game.0, map_ref))
    };
    world.resource_scope::<Assets<Image>, _>(|world, mut images| {
        world.resource_scope::<Assets<KonungLightMaterial>, _>(|world, mut materials| {
            let pool = LightingMaterials::new(&mut images, &mut materials, &masks);
            world.insert_resource(pool);
        });
    });
    world.insert_resource(FineLightGrid(fine));

    // The pool and the pending camera frame are taken out of the world because
    // `load_scene` wants them by `&mut` next to `Commands` and the game, which
    // between them borrow the world. The standalone command queue is applied
    // immediately, so the session is observable the moment `init` returns —
    // the watcher may run in the very same frame.
    let pool = world.remove_resource::<LightingMaterials>();
    let loaded = world.resource_scope::<TextureCache, _>(|world, mut cache| {
        world.resource_scope::<Assets<Image>, _>(|world, mut images| {
            let mut pending = world.remove_resource::<PendingFrame>().unwrap_or_default();
            let result = {
                let mut queue = CommandQueue::default();
                let result = {
                    let game = world.resource::<GameResource>();
                    let mut commands = Commands::new(&mut queue, world);
                    load_scene(
                        &game.0,
                        &scene,
                        |_| true,
                        &mut cache,
                        &mut images,
                        pool.as_ref(),
                        &mut commands,
                        &mut pending,
                    )
                };
                // The game and the commands borrows ended above; apply now so
                // the spawned tree exists the moment `init` returns.
                queue.apply(world);
                result
            };
            world.insert_resource(pending);
            result
        })
    });
    match loaded {
        Ok(loaded) => {
            for problem in &loaded.problems {
                warn!("start map {map_ref:?}: {problem}");
            }
        }
        Err(error) => {
            error!("textures of start map {map_ref:?} did not decode: {error}");
            teardown(world);
            return Err(SessionError::Textures);
        }
    }
    // The pool left `&mut` scope: back into the world, where the lighting
    // systems read it every frame — without it the window's lit scene panics
    // on the first update after the session appears.
    if let Some(pool) = pool {
        world.insert_resource(pool);
    }
    // Inserted last: from this moment the watcher may observe the request this
    // build just made.
    world.insert_resource(SessionInit);
    Ok(())
}

/// The exact inverse of [`init`], idempotent: a second run finds nothing and
/// changes nothing. Despawns the map tree and the creature nodes, removes the
/// session's resources and clears the subsystem's request state.
///
/// [`TextureCache`] is deliberately kept: decoded textures are reusable assets,
/// not session state — a retry must not re-decode the map it just showed. The
/// subsystem's derived resources (`CreatureAnim`, `CreatureSpawns`,
/// `CreatureDepth`) are kept too: they are rebuilt by the next request and read
/// only while a simulation runs, which teardown just removed.
pub fn teardown(world: &mut World) {
    // Direct despawns, not commands: teardown must be observably complete the
    // moment it returns (callers may run it twice or read the world right
    // after), and a queued despawn would apply only at the next sync point.
    let mut roots = world.query_filtered::<Entity, Or<(With<MapRoot>, With<CreatureRoot>)>>();
    let roots: Vec<Entity> = roots.iter(world).collect();
    for root in roots {
        // Recursive: the scene groups, sprites and creature nodes are children.
        world.despawn(root);
    }
    *world.resource_mut::<CreatureSim>() = CreatureSim::default();
    *world.resource_mut::<CreatureMotion>() = CreatureMotion::default();
    world.remove_resource::<FineLightGrid>();
    world.remove_resource::<LightingMaterials>();
    world.remove_resource::<SessionInit>();
}

/// The LIGHTS.RES tile masks of the opened game: raw brightness bytes with
/// their pixel size, ready for [`LightingMaterials::new`]. A game without a
/// readable LIGHTS.RES gets an empty pool — the scene still lights, the tiles
/// just carry no tile mask. Every way the file can fail warns with its path
/// and reason: an empty pool reached silently reads as a broken night scene
/// (review #106).
fn lights_res_masks(game: &Game) -> Vec<(Vec<u8>, u32, u32)> {
    let Some(path) = k1_game::resources::find_named(game.root(), "lights.res") else {
        warn!(
            "LIGHTS.RES not found in {}; the scene lights without tile masks",
            game.root().display()
        );
        return Vec::new();
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            warn!(
                "LIGHTS.RES unreadable at {}: {error}; the scene lights without tile masks",
                path.display()
            );
            return Vec::new();
        }
    };
    let res = match k1_data::lights_res::parse(&bytes) {
        Ok(res) => res,
        Err(error) => {
            warn!(
                "LIGHTS.RES unparsable at {}: {error}; the scene lights without tile masks",
                path.display()
            );
            return Vec::new();
        }
    };
    res.tiles
        .into_iter()
        .map(|mask| {
            (
                mask,
                k1_data::lights_res::TILE_MASK_WIDTH as u32,
                k1_data::lights_res::TILE_MASK_HEIGHT as u32,
            )
        })
        .collect()
}

/// The map's LIGHTS.N fine grid — the tile-light index per fine cell, what the
/// creature lighting reads to tell a lit patch from a dark one. `None` when
/// the map has no LIGHTS.N: every cell counts as dark.
fn map_fine_lights(game: &Game, map_ref: MapRef) -> Option<Vec<Vec<u8>>> {
    let bytes = GameDir::new(game.root())
        .lights_bytes(map_ref.number, GAME_FILES)
        .ok()??;
    k1_data::map::parse_lights(&bytes)
        .ok()
        .map(|lights| lights.fine)
}
