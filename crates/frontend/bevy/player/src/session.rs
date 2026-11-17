//! Session build and teardown of the start map: everything between
//! `HeroSelectAction::Choose` and the `Loading` report the watcher submits.
//!
//! [`init`] is the synchronous half of the old startup loader: it requests the
//! hero's creatures from the shared subsystem, builds the lighting pool and the
//! fine grid, and loads the scene. [`teardown`] is its exact inverse and
//! idempotent, so a failed build leaves no half-session behind and a retry
//! starts from a clean world.

use bevy::ecs::world::CommandQueue;
use bevy::prelude::*;
use kn_app::SessionError;
use kn_game::{Game, GameError, MapRef, Scene, SceneOptions};
use kn_lighting::classic::{FineLightGrid, LightingMaterials, install_classic_lighting};
use kn_render::{
    CreatureMotion, CreatureRoot, CreatureSim, GameResource, MapRoot, PendingFrame, TextureCache,
    load_light_masks, load_scene,
};

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

/// Builds the session of `hero`: the creature request, the lighting pool, the
/// fine grid and the scene sprites — all-or-nothing. The hero arrives as the
/// `StartSession` effect's payload (the reducer is where the choice lives); it
/// decides the start map and the on-map position. On success only
/// [`SessionInit`] is inserted (readiness is the watcher's call, not the
/// builder's); on failure everything built along the way is torn back down and
/// the kind is returned for the `Loading` report.
///
/// A texture that fails to decode fails the whole session: a map drawn with
/// holes is not a session. The old startup loader only warned and continued —
/// the all-or-nothing rule of the app plan replaces that.
pub fn init(world: &mut World, hero: u32, draft: kn_app::NewGameDraft) -> Result<(), SessionError> {
    world
        .resource_mut::<GameResource>()
        .0
        .configure_new_game(
            hero,
            draft.class,
            draft.combat_style,
            draft.secondary,
            draft.attributes,
        )
        .map_err(|error| {
            error!("new game configuration failed: {error}");
            SessionError::Scene
        })?;
    let (map_ref, scene) = world.resource_scope::<CreatureMotion, _>(|world, mut motion| {
        let game = world.resource::<GameResource>();
        request_start_creatures(&game.0, hero, &mut motion)
    })?;

    // The lighting of the start map: the pool from LIGHTS.RES and this map's
    // fine grid, the same resources the viewer provides. Uniforms arrive every
    // frame from the simulation tick through `update_lighting_state`.
    let (masks, fine) = {
        let game = world.resource::<GameResource>();
        (load_light_masks(&game.0), game.0.map_light_grid(map_ref))
    };
    install_classic_lighting(world, &masks);
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
            // Decode failures mean the scene would draw with holes — the
            // all-or-nothing rule fails the session here, before the watcher
            // can call it ready. Empty slots are the data's expected gaps:
            // a warning, not damage.
            if loaded.has_decode_errors() {
                for problem in &loaded.problems {
                    error!("start map {map_ref:?}: {problem}");
                }
                error!("start map {map_ref:?}: undecodable textures fail the session");
                teardown(world);
                return Err(SessionError::Textures);
            }
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
    crate::orders::teardown(world);
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
    if let Some(mut game) = world.get_resource_mut::<GameResource>() {
        game.0.clear_new_game();
    }
}
