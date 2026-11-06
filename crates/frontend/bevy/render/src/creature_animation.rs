//! Shared creature animation: clips, composite sprites and the per-frame swap.
//!
//! Both frontends — the map viewer and the player — attach this subsystem, so a
//! creature is decoded, spawned and animated the same way in either window and
//! neither keeps its own copy of the pipeline.
//!
//! The boundary this module must keep (ARCHITECTURE.md, "Boundary rules"): the
//! game state and its transitions live in the Bevy-free game layer
//! ([`k1_game::creature_anim::Machine`], reached through
//! [`kn_game::simulation::Simulation`]). This module reads that answer as data
//! and turns it into clips; it never decides that a state-4 special is over or
//! that a creature may enter idle. It also owns no clock: it consumes the scene
//! [`Clock`] the caller already runs, and the simulation only ever moves on the
//! ticks its owner hands it.
//!
//! Frontend-specific concerns stay out: interface panels, captions, overlay
//! toggles, per-creature id labels and the `KN_CREATURE_DEBUG` logging are the
//! viewer's, not the subsystem's.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::prelude::*;
use kn_game::simulation::{CreatureStage, CreatureState, Simulation, WalkState};
use kn_game::{
    CreatureAnimation, CreatureClips, CreatureLayer as GameCreatureLayer, Game, MapRef, Piece,
    PieceGroup, Scene,
};

use crate::GameResource;
use crate::animation::{AnimSet, Animator, Clock, Sequence};
use crate::creatures::{
    CreatureData, CreatureNode, CreatureRoot, CreatureShadow, DepthMap, LayerAnimation, LayerData,
    spawn_creature, update_creature_depth,
};
use crate::scene::{Z_STEP, piece_sequence, piece_size};
use crate::textures::TextureCache;

/// What identifies the step a creature is on, for [`CreatureAnim::locked`].
pub type StepKey = ((i32, i32), Option<(i32, i32)>);

/// A built sequence and the frame it should show when play restarts. `corner`
/// is the clip's base-frame corner in absolute map pixels: a swap re-anchors
/// the animator on `corner` minus the creature's origin, because the clip's
/// frame offsets are relative to that corner, not to the resting piece's.
pub struct Clip {
    pub sequence: Arc<Sequence>,
    pub base: usize,
    pub corner: (i32, i32),
}

/// Resting sequence, one per walk direction, and — for a dynamic creature
/// whose record carries state 4 — the one-shot appearance or transformation
/// clip the state machine plays once on world load (issue #70).
pub struct LayerSequences {
    pub rest: Option<Clip>,
    pub walk: Vec<Option<Clip>>,
    pub special: Option<Clip>,
}

/// Sequences for one creature's pieces: `body` by Characters-group layer index,
/// `shadow` for its ground shadow.
pub struct CreatureSequences {
    pub body: HashMap<usize, LayerSequences>,
    pub shadow: LayerSequences,
}

/// Idle, walk and state-4 [`Sequence`]s per creature, plus the observations the
/// swap needs to stay step-relative and to pay one rebuild per state change.
/// Rebuilt on map change together with [`CreatureSim`].
#[derive(Resource, Default)]
pub struct CreatureAnim {
    pub creatures: HashMap<usize, CreatureSequences>,
    /// Last observed `(from, to)` and `step_elapsed` per creature: the first
    /// frame a step is seen on re-anchors its clips, so the walk cycle runs
    /// step-relative like the original's `Frame` counter (Slavik
    /// `game_play.cpp:3727-3740`) instead of free-running against the global clock.
    pub locked: HashMap<usize, (StepKey, u32)>,
    /// Last animation stage kind (Special vs Idle) per creature (issue #70).
    /// The state machine's own change report fires once per edge, but a reset
    /// recreates the whole machine, so the first observation after a reset must
    /// also count as a change; the subsystem keeps its own snapshot for that.
    pub stages: HashMap<usize, bool>,
    /// Last observed special-frame index of each creature still playing its
    /// special. A decrease under an unchanged [`Sequence`] is a restart the
    /// stage-kind snapshot cannot see — "Reset animation" or a rewind returns a
    /// mid-special creature to an earlier frame without flipping Special↔Idle —
    /// so the swap must re-anchor on it (review of #101, P2-#3). Cleared for a
    /// creature that has left the special, so a later re-entry is not mistaken
    /// for a regression.
    pub special_frames: HashMap<usize, u32>,
}

impl CreatureAnim {
    /// Creatures whose step is new or restarted since the last observation —
    /// their clips must open on the step's base frame. The lock updates here, so
    /// a step re-anchors exactly once, on its first observation.
    ///
    /// A falling `step_elapsed` under an unchanged key is a restart: the world
    /// can cancel a step whose destination became impassable (`cancel_step`) and
    /// order the same `(from, to)` again, and the key alone cannot tell that
    /// from a step still running — without re-anchoring the cycle would keep
    /// running past the halfway point and wrap a cell behind the creature
    /// (review #68).
    pub fn observe_steps(&mut self, states: &[CreatureState]) -> HashSet<usize> {
        let mut restep = HashSet::new();
        for state in states {
            let observed = ((state.pixel, state.step_to_pixel), state.step_elapsed);
            let restarted = match self.locked.get(&state.id) {
                Some(&(last_key, last_elapsed)) => {
                    last_key != observed.0 || observed.1 < last_elapsed
                }
                None => true,
            };
            if restarted {
                restep.insert(state.id);
            }
            // Recorded every observation, not only on a re-anchor: the restart
            // test compares against the highest tick the step was last seen on.
            self.locked.insert(state.id, observed);
        }
        restep
    }

    /// Creatures whose animation stage kind (Special ↔ Idle) changed since the
    /// last observation (issue #70 AC6). The list drives exactly one pose
    /// rebuild per creature per state change: a plain frame advance inside a
    /// running Special is not a change and must not appear here.
    pub fn observe_stages(&mut self, states: &[(usize, bool)]) -> HashSet<usize> {
        let mut changed = HashSet::new();
        for &(id, in_special) in states {
            let previous = self.stages.insert(id, in_special);
            if previous != Some(in_special) {
                changed.insert(id);
            }
        }
        changed
    }

    /// Creatures whose special frame regressed since the last observation: the
    /// same clip is still playing but its frame counter moved backwards, which is
    /// a restart the Special↔Idle snapshot cannot see — "Reset animation" returns
    /// a mid-special creature to frame 0, and a rewind re-derives an earlier
    /// frame (review of #101, P2-#3). Passing only the creatures currently in
    /// `Special`; a creature that left is dropped so its next entry starts clean.
    /// A plain forward advance only raises the frame and is never reported, so
    /// the AC6 one-rebuild-per-change bound holds.
    pub fn observe_specials(&mut self, playing: &[(usize, u32)]) -> HashSet<usize> {
        let mut restarted = HashSet::new();
        let mut seen = HashSet::new();
        for &(id, frame) in playing {
            seen.insert(id);
            if self
                .special_frames
                .get(&id)
                .is_some_and(|&last| frame < last)
            {
                restarted.insert(id);
            }
            self.special_frames.insert(id, frame);
        }
        self.special_frames.retain(|id, _| seen.contains(id));
        restarted
    }
}

/// A sprite under a [`CreatureNode`]: which creature it belongs to and which of
/// its layers it is (or the shadow), so the animation system swaps its sequence.
#[derive(Component)]
pub struct CreatureSprite {
    pub creature_id: usize,
    pub layer: usize,
    pub is_shadow: bool,
}

/// Slots pinning the creature pipeline around [`crate::animation::AnimSet::Sprites`]:
/// clips are chosen before the frame write and nodes placed after it.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CreatureSet {
    /// [`prepare_creatures`]: decodes the map's creatures after a request.
    Prepare,
    /// [`advance_creature_sim`]: catches the simulation up to the scene clock.
    Advance,
    /// [`spawn_creature_nodes`]: spawns each creature's composite node once.
    Spawn,
    /// [`sync_creature_animation`]: picks and installs each sprite's clip.
    Swap,
    /// [`sync_creature_nodes`]: places nodes, shadows and depth.
    Nodes,
}

/// The map's simulation the creature pipeline draws, and where its timeline sits
/// on the scene clock.
///
/// Time is simulation-relative: the clock tick that maps to the simulation's
/// tick 0 is `epoch`, moved by a rebuild or a reset. A forward step is a plain
/// catch-up; a backward step rebuilds and replays to the epoch-relative target,
/// because the engine has no reverse tick. Replaying from the seed is
/// deterministic, so the rewound state equals what forward stepping reached
/// there — which is what keeps a state-4 creature's shown state equal to the
/// state of the selected tick without a reversible simulation (issue #70).
#[derive(Resource, Default)]
pub struct CreatureSim {
    pub sim: Option<Simulation>,
    pub initial_pixels: Vec<(usize, (i32, i32))>,
    pub epoch: u64,
    pub sim_ticks: u64,
    /// Whether this map's creature nodes have been spawned; reset on rebuild so
    /// they are built once.
    pub creatures_spawned: bool,
    /// Root of the creature nodes, despawned on map rebuild. The frontend owns
    /// its visibility through [`CreatureNodesVisible`].
    pub creature_root: Option<Entity>,
    /// Why this map's simulation failed to build, if it did; the frontend
    /// surfaces it instead of silently showing a scene with no creatures.
    pub error: Option<String>,
}

/// Whether the creature nodes are born visible. The viewer drives it from its
/// Characters overlay so a map change with characters off shows nothing for the
/// first frame; the player leaves the default.
#[derive(Resource)]
pub struct CreatureNodesVisible(pub bool);

impl Default for CreatureNodesVisible {
    fn default() -> Self {
        Self(true)
    }
}

/// Depth map built from the scene's static stack boundaries, used for the
/// dynamic z of a moving creature.
#[derive(Resource)]
pub struct CreatureDepth {
    pub map: DepthMap,
}

impl Default for CreatureDepth {
    fn default() -> Self {
        Self {
            map: DepthMap::build(Vec::new().into_iter()),
        }
    }
}

/// What the frontend asked the subsystem to build, and whether it is pending.
///
/// The frontend keeps its own scene building (overlays, options, camera
/// framing) and hands the finished [`Scene`] over once; the subsystem derives
/// everything creature-specific from it.
#[derive(Resource, Default)]
pub struct CreatureMotion {
    pub map: Option<MapRef>,
    pub seed: u64,
    pub scene: Option<Scene>,
    pub rebuild: bool,
}

impl CreatureMotion {
    /// Asks for `map`'s creatures to be built from `scene` at the next run of
    /// [`prepare_creatures`]. `seed` fixes the simulation's spawn so a reset
    /// reproduces the first build exactly.
    pub fn request(&mut self, map: MapRef, seed: u64, scene: Scene) {
        self.map = Some(map);
        self.seed = seed;
        self.scene = Some(scene);
        self.rebuild = true;
    }
}

/// What went wrong or is missing while decoding a map's creature animations.
/// Structured, so each frontend words its own diagnostics panel.
#[derive(Resource, Debug, Default)]
pub struct CreatureAnimReport {
    /// The map's animation data did not decode at all.
    pub error: Option<String>,
    /// Walk frames decoded only partly.
    pub texture_error: Option<String>,
    /// Creatures with no walk frames at all.
    pub without_walk: usize,
    /// Creatures missing walk frames in some directions.
    pub partial_walk: usize,
    /// Creatures the subsystem was asked to animate.
    pub total: usize,
}

/// Builds every creature's [`CreatureData`] from its scene pieces, plus the
/// per-creature numbers the depth and the swap need.
#[derive(Resource, Default)]
pub struct CreatureSpawns {
    pub plans: Vec<CreatureData>,
    /// Body height in pixels per creature (`CreatureData::bottom - origin.y`),
    /// used to turn a live cell into an occlusion bottom without a scene lookup.
    pub heights: HashMap<usize, i32>,
    /// Spawn pixel per creature, the origin every clip corner is reduced
    /// against when a swap re-anchors its animator.
    pub origins: HashMap<usize, (i32, i32)>,
}

/// The creature-animation subsystem: its resources, its systems and the ordering
/// they rely on, so a frontend gets the whole pipeline by adding one plugin and
/// cannot break it by listing the systems in another order. What stays with the
/// frontend is only its own additions — the viewer's id labels, its diagnostics —
/// attached after [`CreatureSet::Nodes`].
///
/// The frontend must already provide [`GameResource`], [`TextureCache`],
/// `Assets<Image>` and the scene [`Clock`] (from [`crate::AnimPlugin`]), which is
/// what any window drawing this game has anyway; the subsystem adds no clock of
/// its own.
pub struct CreatureAnimationPlugin;

impl Plugin for CreatureAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CreatureSim>()
            .init_resource::<CreatureAnim>()
            .init_resource::<CreatureAnimReport>()
            .init_resource::<CreatureSpawns>()
            .init_resource::<CreatureDepth>()
            .init_resource::<CreatureMotion>()
            .init_resource::<CreatureNodesVisible>()
            .add_systems(
                Update,
                (
                    prepare_creatures.in_set(CreatureSet::Prepare),
                    advance_creature_sim.in_set(CreatureSet::Advance),
                    spawn_creature_nodes.in_set(CreatureSet::Spawn),
                    sync_creature_animation.in_set(CreatureSet::Swap),
                    sync_creature_nodes.in_set(CreatureSet::Nodes),
                )
                    .chain(),
            )
            .configure_sets(
                Update,
                (
                    CreatureSet::Prepare,
                    CreatureSet::Advance,
                    CreatureSet::Spawn,
                    CreatureSet::Swap,
                )
                    .chain()
                    .after(AnimSet::Clock),
            )
            .configure_sets(
                Update,
                AnimSet::Sprites
                    .after(CreatureSet::Swap)
                    .before(CreatureSet::Nodes),
            );
    }
}

/// Decodes the walk and state-4 frames the scene does not carry and builds the
/// swap sequences; a creature whose walk cannot be decoded simply keeps its
/// record pose. Nothing about that is silent — every reason a walk is missing or
/// partial comes back in the report for the frontend's diagnostics panel
/// (review #68): a creature walking on its rest pose is a rendering defect, not
/// an absence of creatures to explain away.
pub fn build_creature_anim(
    game: &Game,
    map_ref: MapRef,
    creature_pieces: &[Piece],
    cache: &mut TextureCache,
    images: &mut Assets<Image>,
) -> (CreatureAnim, CreatureAnimReport) {
    let animations = match game.creature_animations(map_ref) {
        Ok(animations) => animations,
        Err(error) => {
            return (
                CreatureAnim::default(),
                CreatureAnimReport {
                    error: Some(format!("{error}")),
                    ..default()
                },
            );
        }
    };
    let mut seen = HashSet::new();
    for (_, anim) in &animations {
        for clips in std::iter::once(&anim.rest)
            .chain(anim.walk.iter().flatten())
            .chain(anim.special.iter())
        {
            // Both the layer's static base and every animation frame are drawn.
            seen.extend(clips.layers.iter().filter_map(|layer| layer.base));
            seen.extend(clips.shadow.as_ref().and_then(|layer| layer.base));
            let frames = clips
                .layers
                .iter()
                .map(|layer| layer.animation.as_ref())
                .chain(std::iter::once(
                    clips
                        .shadow
                        .as_ref()
                        .and_then(|layer| layer.animation.as_ref()),
                ));
            for animation in frames.flatten() {
                seen.extend(animation.frames.iter().filter_map(|frame| frame.texture));
            }
        }
    }
    // A creature with neither walk nor special never reaches `creature_animations`
    // and so contributes no clip here, yet its node still shows its resting base
    // image; that texture is only in the roster piece set, so it is decoded too,
    // or the static member would be a blank node (review #101, P2-1).
    for piece in creature_pieces {
        seen.extend(piece.texture);
        if let Some(animation) = &piece.animation {
            seen.extend(animation.frames.iter().filter_map(|frame| frame.texture));
        }
    }
    let texture_error = cache
        .load(game, &seen.into_iter().collect::<Vec<_>>(), images)
        .err()
        .map(|error| error.to_string());
    let without_walk = animations
        .iter()
        .filter(|(_, anim)| anim.walk.iter().all(Option::is_none))
        .count();
    let partial_walk = animations
        .iter()
        .filter(|(_, anim)| {
            anim.walk.iter().any(Option::is_some) && anim.walk.iter().any(Option::is_none)
        })
        .count();
    let report = CreatureAnimReport {
        error: None,
        texture_error,
        without_walk,
        partial_walk,
        total: animations.len(),
    };
    let anim = CreatureAnim {
        creatures: animations
            .into_iter()
            .map(|(id, anim)| (id, build_sequences(&anim, cache)))
            .collect(),
        locked: HashMap::new(),
        stages: HashMap::new(),
        special_frames: HashMap::new(),
    };
    (anim, report)
}

/// One creature's clips → sequences, aligned to the scene's layers by
/// [`kn_game::LayerKey`], never by position: see [`pose_matched_layers`].
fn build_sequences(anim: &CreatureAnimation, cache: &TextureCache) -> CreatureSequences {
    let matched = pose_matched_layers(&anim.rest.layers, &anim.walk);
    // The state-4 clip is matched to each resting layer by the same key: the
    // appearance sequence carries the same parts (body, equipment, shadow) as
    // the idle, and an unmatched swap slides one part's clip onto another.
    let special_matched: Vec<Option<&GameCreatureLayer>> = anim
        .rest
        .layers
        .iter()
        .map(|rest_layer| {
            anim.special.as_ref().and_then(|special| {
                special
                    .layers
                    .iter()
                    .find(|layer| layer.key == rest_layer.key)
            })
        })
        .collect();
    let body = anim
        .rest
        .layers
        .iter()
        .enumerate()
        .map(|(index, rest_layer)| {
            let rest = clip_seq(Some(rest_layer), cache);
            let walk = matched[index]
                .iter()
                .map(|layer| clip_seq(*layer, cache))
                .collect();
            let special = clip_seq(special_matched[index], cache);
            (
                index,
                LayerSequences {
                    rest,
                    walk,
                    special,
                },
            )
        })
        .collect();
    let rest = clip_seq(anim.rest.shadow.as_ref(), cache);
    let walk = anim
        .walk
        .iter()
        .map(|dir| {
            dir.as_ref()
                .and_then(|clips| clips.shadow.as_ref())
                .and_then(|layer| clip_seq(Some(layer), cache))
        })
        .collect();
    let special = anim
        .special
        .as_ref()
        .and_then(|clips| clips.shadow.as_ref())
        .and_then(|layer| clip_seq(Some(layer), cache));
    CreatureSequences {
        body,
        shadow: LayerSequences {
            rest,
            walk,
            special,
        },
    }
}

/// For each rest layer, the layer of every walk pose that draws the same part.
///
/// The match is by [`kn_game::LayerKey`]: pose tracks are ordered by first
/// appearance within one pose, so a walk pose may list the same layers in
/// another order, and an index join would bolt one part's walk clip onto
/// another's piece — equipment sliding across the sprite on every step
/// (review #68).
fn pose_matched_layers<'a>(
    rest: &'a [GameCreatureLayer],
    walk: &'a [Option<CreatureClips>],
) -> Vec<Vec<Option<&'a GameCreatureLayer>>> {
    rest.iter()
        .map(|rest_layer| {
            walk.iter()
                .map(|dir| {
                    dir.as_ref().and_then(|clips| {
                        clips
                            .layers
                            .iter()
                            .find(|layer| layer.key == rest_layer.key)
                    })
                })
                .collect()
        })
        .collect()
}

/// Frames of one layer → a swap clip; `None` when no frame decoded.
fn clip_seq(layer: Option<&GameCreatureLayer>, cache: &TextureCache) -> Option<Clip> {
    let layer = layer?;
    let animation = layer.animation.as_ref()?;
    let sequence = piece_sequence(animation, cache)?;
    Some(Clip {
        sequence,
        base: animation.base,
        corner: layer.corner,
    })
}

/// Builds every creature's [`CreatureData`] from its roster creature pieces.
///
/// Geometry comes from [`Scene::creature_pieces`] — the roster-driven set that
/// carries a resting pose for every loaded creature, roaming squad members 2..n
/// included — not from [`Scene::pieces`], the digest-guarded source list that
/// under-emits a squad to its first member (review #101, P2-1). A creature's
/// body/equipment pieces (group `Characters`) and shadow piece
/// (`CharacterShadows`) carry their own geometry; each is placed relative to the
/// creature's origin cell, so the whole set rides the node as it moves. Only
/// creatures that actually have pieces get a plan; roster order keeps the result
/// deterministic.
pub fn build_creature_spawns(
    scene: &Scene,
    cache: &TextureCache,
    anim: &CreatureAnim,
    initial_pixels: &[(usize, (i32, i32))],
) -> CreatureSpawns {
    let mut plans = Vec::new();
    let mut heights = HashMap::new();
    let mut origins = HashMap::new();
    for &(id, origin) in initial_pixels {
        let pieces: Vec<&Piece> = scene
            .creature_pieces
            .iter()
            .filter(|piece| piece.creature_id == Some(id))
            .collect();
        if pieces.is_empty() {
            continue;
        }
        let (plan, height) = build_creature_plan(pieces, cache, anim, id, origin);
        plans.push(plan);
        heights.insert(id, height);
        origins.insert(id, origin);
    }
    CreatureSpawns {
        plans,
        heights,
        origins,
    }
}

/// One creature's pieces → spawn data, with body height reported for depth.
///
/// `bottom` is the lowest body edge (the shadow never sets the occlusion line).
/// Body layers keep their scene order, which indexes [`CreatureSequences::body`].
fn build_creature_plan(
    pieces: Vec<&Piece>,
    cache: &TextureCache,
    anim: &CreatureAnim,
    id: usize,
    origin: (i32, i32),
) -> (CreatureData, i32) {
    let mut layers: Vec<LayerData> = Vec::new();
    let mut shadow: Option<LayerData> = None;
    let mut bottom = i32::MIN;
    for piece in pieces {
        let entry = piece.texture.as_ref().and_then(|id| cache.get(id));
        let size = piece_size(piece, entry.map_or(UVec2::ZERO, |entry| entry.size));
        let layer = LayerData {
            image: entry.map_or_else(Handle::default, |entry| entry.handle.clone()),
            size,
            offset: Vec2::new((piece.x - origin.0) as f32, (piece.y - origin.1) as f32),
        };
        if piece.group == PieceGroup::CharacterShadows {
            shadow = Some(layer);
        } else {
            bottom = bottom.max(piece.y + size.y as i32);
            layers.push(layer);
        }
    }
    let bottom = if bottom == i32::MIN { origin.1 } else { bottom };
    let sequences = anim.creatures.get(&id);
    let animations = layers
        .iter()
        .enumerate()
        .map(|(index, _)| rest_animation(sequences.and_then(|c| c.body.get(&index)), origin))
        .collect();
    let shadow_animation = sequences.and_then(|c| rest_animation(Some(&c.shadow), origin));
    (
        CreatureData {
            id: id as u32,
            x: origin.0,
            y: origin.1,
            bottom,
            layers,
            animations,
            shadow,
            shadow_animation,
        },
        bottom - origin.1,
    )
}

/// The resting [`LayerAnimation`] of a layer, if it has a resting sequence,
/// anchored on the clip's own base corner relative to the creature's origin —
/// the same coordinates the node lives in.
fn rest_animation(layer: Option<&LayerSequences>, origin: (i32, i32)) -> Option<LayerAnimation> {
    let clip = layer?.rest.as_ref()?;
    Some(LayerAnimation {
        sequence: Arc::clone(&clip.sequence),
        base: clip.base,
        anchor: (clip.corner.0 - origin.0, clip.corner.1 - origin.1),
    })
}

/// Builds a depth map from the scene's sorted pieces for creature z-interpolation.
pub fn build_depth_map(scene: &Scene, cache: &TextureCache) -> DepthMap {
    let stacks: Vec<(i32, f32)> = scene
        .pieces
        .iter()
        .enumerate()
        // Flames ride their object's draw slot; their bottom sits above the
        // object's and their z is higher, so as contributors they would put a
        // creature standing on that row in front of the burning object itself
        // (review #68).
        .filter(|(_, p)| is_depth_relevant(p.group) && !p.is_flame())
        .map(|(order, piece)| {
            let height = piece.h.unwrap_or_else(|| {
                piece
                    .texture
                    .as_ref()
                    .and_then(|id| cache.get(id))
                    .map_or(0, |entry| entry.size.y as i32)
            });
            (piece.y + height, order as f32 * Z_STEP)
        })
        .collect();
    DepthMap::build(stacks.into_iter())
}

/// Only static stacks participate in depth ordering.
///
/// Characters and their shadows are drawn by their own [`CreatureNode`]s and move
/// every frame; putting them in the map would let a creature depth-sort against
/// another creature rather than against the static objects it should hide behind
/// (review #68). Terrain and the ground layers sit below everything by scene order
/// and never occlude a creature, so they are excluded too.
fn is_depth_relevant(group: PieceGroup) -> bool {
    matches!(group, PieceGroup::Objects)
}

/// Decodes the creatures of the scene the frontend handed over and puts the
/// simulation at the current clock tick.
///
/// Runs once per request: a map change in the viewer, world load in the player.
/// The old creature nodes go with the request, because their clips and depth slot
/// belong to the map that produced them.
#[allow(clippy::too_many_arguments)]
pub fn prepare_creatures(
    mut commands: Commands,
    game: Res<GameResource>,
    mut request: ResMut<CreatureMotion>,
    mut cache: ResMut<TextureCache>,
    mut images: ResMut<Assets<Image>>,
    mut sim: ResMut<CreatureSim>,
    mut depth: ResMut<CreatureDepth>,
    mut anim: ResMut<CreatureAnim>,
    mut report: ResMut<CreatureAnimReport>,
    mut spawns: ResMut<CreatureSpawns>,
    clock: Res<Clock>,
) {
    if !request.rebuild {
        return;
    }
    request.rebuild = false;
    let (Some(map_ref), Some(scene)) = (request.map, request.scene.take()) else {
        *report = CreatureAnimReport::default();
        return;
    };
    if let Some(root) = sim.creature_root.take() {
        commands.entity(root).despawn();
    }
    sim.creatures_spawned = false;
    depth.map = build_depth_map(&scene, &cache);
    match game.0.simulation(map_ref, request.seed) {
        Ok(simulation) => {
            sim.error = None;
            sim.initial_pixels = simulation.initial_pixels();
            sim.sim = Some(simulation);
        }
        Err(error) => {
            // The reason names the map, so a frontend's panel says what failed
            // without knowing how the subsystem builds it (review #68).
            sim.error = Some(format!("simulation for {map_ref:?}: {error}"));
            sim.initial_pixels = Vec::new();
            sim.sim = None;
        }
    }
    // The fresh simulation starts at tick 0 from the current clock: the epoch is
    // what keeps a catch-up from replaying the accumulated history as one burst
    // and a later backward step from rewinding to absolute clock time (#68).
    sim.epoch = clock.ticks;
    sim.sim_ticks = 0;
    let (built, built_report) = build_creature_anim(
        &game.0,
        map_ref,
        &scene.creature_pieces,
        &mut cache,
        &mut images,
    );
    *anim = built;
    *report = built_report;
    *spawns = build_creature_spawns(&scene, &cache, &anim, &sim.initial_pixels);
}

/// Rebuilds the map's simulation at tick 0 and pins the epoch to `clock_ticks`.
///
/// The frontend's reset buttons call this: positions return to what the file
/// records and the creature state machine reseeds from the file, while the
/// already-spawned nodes stay — their clips and depth slot still describe this
/// map.
pub fn reset_creature_sim(
    game: &Game,
    map_ref: MapRef,
    seed: u64,
    clock_ticks: u64,
    sim: &mut CreatureSim,
) {
    match game.simulation(map_ref, seed) {
        Ok(simulation) => {
            sim.error = None;
            sim.initial_pixels = simulation.initial_pixels();
            sim.sim = Some(simulation);
        }
        Err(error) => {
            // The reason names the map, so a frontend's panel says what failed
            // without knowing how the subsystem builds it (review #68).
            sim.error = Some(format!("simulation for {map_ref:?}: {error}"));
            sim.initial_pixels = Vec::new();
            sim.sim = None;
        }
    }
    sim.epoch = clock_ticks;
    sim.sim_ticks = 0;
}

/// The frontend's "Reset animation" button: every state-4 special restarts from
/// frame 0 while the walk world and the game clock stay exactly where they are
/// (review of #101, P2-#3). Where [`reset_creature_sim`] reseeds the whole
/// simulation — snapping creatures back to their load cells and the clock back
/// to zero, which is the `Play → Off → Play` full reset — this one only rewinds
/// the animation machine via [`Simulation::restart_all_specials`], so a creature
/// keeps its position and the simulation keeps its accumulated time. The
/// already-spawned nodes stay; the animator re-anchors on the same clip through
/// the special-frame regression [`CreatureAnim::observe_specials`] reports.
pub fn reset_creature_animations(sim: &mut CreatureSim) {
    if let Some(simulation) = sim.sim.as_mut() {
        simulation.restart_all_specials();
    }
}

/// The frontend's "Reset positions" button: creatures return to the map's load
/// cells and the walk world restarts, but the creature animation machine keeps
/// playing — a mid-special stays mid-special, and an idle creature stays idle
/// (review of #101, P2-#3). Where [`reset_creature_sim`] rebuilds everything and
/// so reseeds every state-4 special to frame 0 (the animation reset), this one
/// adopts only the fresh movement side via [`Simulation::adopt_positions_from`]
/// and leaves the machine untouched. The already-spawned nodes stay: their clips
/// and depth slot still describe this map, and only their placement changes.
pub fn reset_creature_positions(
    game: &Game,
    map_ref: MapRef,
    seed: u64,
    clock_ticks: u64,
    sim: &mut CreatureSim,
) {
    match game.simulation(map_ref, seed) {
        Ok(fresh) => {
            sim.error = None;
            match sim.sim.as_mut() {
                // Same map and seed: carry the movement side across, keep the machine.
                Some(existing) => existing.adopt_positions_from(fresh),
                // No prior simulation (a previous build failed): install the fresh one.
                None => sim.sim = Some(fresh),
            }
            sim.initial_pixels = sim
                .sim
                .as_ref()
                .map_or_else(Vec::new, Simulation::initial_pixels);
        }
        Err(error) => {
            sim.error = Some(format!("simulation for {map_ref:?}: {error}"));
            sim.initial_pixels = Vec::new();
            sim.sim = None;
        }
    }
    sim.epoch = clock_ticks;
    sim.sim_ticks = 0;
}

/// Advances the simulation to the scene clock, one logical tick per clock tick.
///
/// Forward is a plain catch-up. A backward step rewinds the clock, but the engine
/// has no reverse tick, so the simulation is rebuilt and replayed forward to the
/// epoch-relative target — after a reset the zero point is the reset, so stepping
/// back just after one replays nothing instead of the whole clock (review #68).
/// The creature state machine lives inside the simulation and moves once per
/// `tick`, so a rewound creature shows exactly the state of the tick it landed on
/// (issue #70) without a reversible simulation.
pub fn advance_creature_sim(
    mut sim: ResMut<CreatureSim>,
    request: Res<CreatureMotion>,
    game: Res<GameResource>,
    clock: Res<Clock>,
) {
    if sim.sim.is_none() {
        sim.epoch = clock.ticks;
        sim.sim_ticks = 0;
        return;
    }
    let target = clock.ticks.saturating_sub(sim.epoch);
    if target < sim.sim_ticks {
        // Backward: rebuild and replay forward to the epoch-relative target. This
        // runs only when the clock actually moved behind the simulation, not every
        // frame, so a shallow rewind after a reset (target near 0) is cheap.
        let Some(map_ref) = request.map else {
            return;
        };
        let Some(mut replayed) = game.0.simulation(map_ref, request.seed).ok() else {
            return;
        };
        for _ in 0..target {
            replayed.tick();
        }
        sim.initial_pixels = replayed.initial_pixels();
        sim.sim = Some(replayed);
        sim.sim_ticks = target;
        return;
    }
    while sim.sim_ticks < target {
        let Some(simulation) = sim.sim.as_mut() else {
            break;
        };
        simulation.tick();
        sim.sim_ticks += 1;
    }
}

/// Spawns the creature nodes once per map, tagging each child sprite so
/// [`sync_creature_animation`] can find its layer. Runs after
/// [`prepare_creatures`] laid out the depth map, so every node is born at its
/// correct depth.
pub fn spawn_creature_nodes(
    mut commands: Commands,
    mut sim: ResMut<CreatureSim>,
    spawns: Res<CreatureSpawns>,
    depth: Res<CreatureDepth>,
    visible: Res<CreatureNodesVisible>,
) {
    if sim.creatures_spawned || sim.sim.is_none() {
        return;
    }
    sim.creatures_spawned = true;
    let target = if visible.0 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    let root = commands
        .spawn((CreatureRoot, Transform::IDENTITY, target))
        .id();
    sim.creature_root = Some(root);
    for plan in &spawns.plans {
        let spawned = spawn_creature(&mut commands, root, plan, &depth.map);
        for (layer, child) in spawned.layers.iter().enumerate() {
            commands.entity(*child).insert(CreatureSprite {
                creature_id: plan.id as usize,
                layer,
                is_shadow: false,
            });
        }
        if let Some(shadow) = spawned.shadow {
            commands.entity(shadow).insert(CreatureSprite {
                creature_id: plan.id as usize,
                layer: 0,
                is_shadow: true,
            });
        }
    }
}

/// Which clip a piece shows. The state machine's stage decides first: while a
/// state-4 creature plays its appearance or transformation, its special clip
/// wins regardless of the simulation's walking state, so the sequence reads as
/// a one-shot appearance (issue #70). Otherwise: the walk sequence for the
/// facing direction while walking, else the resting one. A missing walk
/// direction falls back to resting, so an unmapped direction never blanks the
/// creature.
pub fn select_clip(
    layer: &LayerSequences,
    walking: bool,
    direction: u8,
    in_special: bool,
) -> Option<&Clip> {
    if in_special {
        return layer.special.as_ref().or(layer.rest.as_ref());
    }
    if walking && let Some(walk) = layer.walk.get(direction as usize).and_then(Option::as_ref) {
        return Some(walk);
    }
    layer.rest.as_ref()
}

/// Swaps each creature sprite's [`crate::animation::Animator`] between the
/// resting, the walking and the state-4 sequence when the simulation's
/// [`WalkState`], its facing direction or the creature's animation stage
/// changes, keeping body, equipment and shadow of one creature in step.
///
/// The swap re-anchors the animator on the clip's own base corner: every clip's
/// frame offsets are relative to that corner, so an animator left on the
/// resting corner would render the new clip displaced by the corner difference
/// — the periodic image jump of review #68. A step seen for the first time
/// re-anchors too, opening the cycle `(elapsed - 1)` frames in: the walk frames
/// carry the travel across a step, so their index must run with the step like the
/// original's `Frame` counter (Slavik `game_play.cpp:3727-3740`), not free-run
/// against the global clock and wrap to the first frame mid-step. A pose is
/// rebuilt at most once per creature and only when the animation stage *kind*
/// flipped — a plain tick advancing the frame counter inside a running Special
/// changes no clip and pays nothing (issue #70 AC6).
pub fn sync_creature_animation(
    sim: Res<CreatureSim>,
    mut anim: ResMut<CreatureAnim>,
    spawns: Res<CreatureSpawns>,
    clock: Res<Clock>,
    mut sprites: Query<(&CreatureSprite, &mut Animator, &Transform)>,
) {
    let Some(ref simulation) = sim.sim else {
        return;
    };
    let states = simulation.states();
    let restep = anim.observe_steps(&states);
    let stage_pairs: Vec<(usize, bool)> = states
        .iter()
        .map(|state| {
            (
                state.id,
                simulation
                    .creature_stage(state.id)
                    .is_some_and(|stage| stage.is_special()),
            )
        })
        .collect();
    let stage_changes = anim.observe_stages(&stage_pairs);
    // A special whose frame counter fell since the last observation is a restart
    // the kind flip cannot see (Reset animation / rewind): it must re-anchor even
    // on the same `Sequence` Arc (review of #101, P2-#3). Only creatures now in
    // Special are offered, so a plain forward advance never trips it.
    let special_playing: Vec<(usize, u32)> = states
        .iter()
        .filter_map(|state| match simulation.creature_stage(state.id) {
            Some(CreatureStage::Special { frame }) => Some((state.id, frame)),
            _ => None,
        })
        .collect();
    let special_resets = anim.observe_specials(&special_playing);
    let facing: HashMap<usize, (bool, u8)> = states
        .iter()
        .map(|state| {
            (
                state.id,
                (
                    state.walk_state == WalkState::Walk,
                    state.direction.unwrap_or(0),
                ),
            )
        })
        .collect();
    let elapsed: HashMap<usize, u32> = states
        .iter()
        .map(|state| (state.id, state.step_elapsed))
        .collect();
    for (tag, mut animator, _transform) in &mut sprites {
        let Some(creature) = anim.creatures.get(&tag.creature_id) else {
            continue;
        };
        // The state machine's answer drives clip choice: while a creature
        // plays its state-4 special (issue #70), the special clip wins over
        // walk and rest regardless of the simulation's walking state.
        let in_special = simulation
            .creature_stage(tag.creature_id)
            .is_some_and(|stage| stage.is_special());
        let special_frame = match simulation.creature_stage(tag.creature_id) {
            Some(CreatureStage::Special { frame }) => Some(frame),
            _ => None,
        };
        let layer = if tag.is_shadow {
            &creature.shadow
        } else {
            match creature.body.get(&tag.layer) {
                Some(layer) => layer,
                None => continue,
            }
        };
        let (walking, direction) = facing.get(&tag.creature_id).copied().unwrap_or((false, 0));
        let clip = select_clip(layer, walking, direction, in_special);
        let Some(clip) = clip else { continue };
        if Arc::ptr_eq(&animator.sequence, &clip.sequence)
            && !(restep.contains(&tag.creature_id) && !in_special)
            && !stage_changes.contains(&tag.creature_id)
            && !special_resets.contains(&tag.creature_id)
        {
            // A step re-anchor is a walk-cycle concern; a state-4 creature
            // re-anchors only when its Arc changes (Special ↔ rest/walk) or
            // when the state machine reports a stage-kind change (issue #70
            // AC6: at most one rebuild per creature per state change).
            continue;
        }
        let Some(&origin) = spawns.origins.get(&tag.creature_id) else {
            continue;
        };
        let elapsed = elapsed.get(&tag.creature_id).copied().unwrap_or(0);
        // Anchor the swap so the current tick shows the correct frame: the
        // special opens on the tick its frame counter started, the walk opens
        // on the tick the step began (elapsed - 1 ago).
        let step_start = if let Some(frame) = special_frame {
            clock.ticks.saturating_sub(u64::from(frame))
        } else {
            clock
                .ticks
                .saturating_sub(u64::from(elapsed.saturating_sub(1)))
        };
        apply_clip(&mut animator, clip, origin, step_start);
    }
}

/// Installs `clip` on `animator`, re-anchored on the clip's own base corner at
/// `at_tick`.
fn apply_clip(animator: &mut Animator, clip: &Clip, origin: (i32, i32), at_tick: u64) {
    animator.play_at(
        Arc::clone(&clip.sequence),
        clip.base,
        at_tick,
        (clip.corner.0 - origin.0, clip.corner.1 - origin.1),
    );
}

/// Puts each creature node at its live cell and depth.
///
/// The node owns the creature's world position and z; its sprite children carry
/// only local offsets and per-frame deltas, so they follow the node untouched and
/// body, equipment and shadow stay in lockstep (review #68). The node sits on the
/// simulation cell's pixel for the whole step: the glide to the next cell lives in
/// the walk frames' own offsets (docs/k1/movement.md, "Step duration"), so sliding
/// the node along with them would double the travel and snap the image back by a
/// cell at every cycle wrap.
#[allow(clippy::type_complexity)]
pub fn sync_creature_nodes(
    sim: Res<CreatureSim>,
    depth: Res<CreatureDepth>,
    spawns: Res<CreatureSpawns>,
    // `Without<CreatureSprite>` on top of `Without<CreatureShadow>`: the node
    // writes the `Transform` the sprites query reads, and without the filter
    // Bevy cannot prove an entity never carries both (B0001).
    mut nodes: Query<
        (&mut CreatureNode, &mut Transform),
        (Without<CreatureShadow>, Without<CreatureSprite>),
    >,
    // `CreatureShadow` marks only the shadow children, disjoint from the nodes.
    mut shadows: Query<(&CreatureShadow, &ChildOf, &mut Transform)>,
) {
    let Some(ref simulation) = sim.sim else {
        return;
    };
    let states = simulation.states();
    let drawn: HashMap<usize, (i32, i32)> =
        states.iter().map(|state| (state.id, state.pixel)).collect();
    for (mut node, mut transform) in &mut nodes {
        let id = node.id as usize;
        let Some(&(x, y)) = drawn.get(&id) else {
            continue;
        };
        transform.translation.x = x as f32;
        transform.translation.y = -(y as f32);
        // Occlusion bottom from the live cell and this creature's body height.
        let height = spawns.heights.get(&id).copied().unwrap_or(0);
        let bottom = (y + height).max(0);
        update_creature_depth(&mut node, &mut transform, bottom, &depth.map);
    }
    // The shadow rides the node but holds its slot's shadow z — over the
    // ground the creature stands on, under its feet — so its local z cancels
    // the node's body z, which `update_creature_depth` rewrites whenever the
    // bottom edge moves (review #68).
    for (_, parent, mut transform) in &mut shadows {
        let Ok(node) = nodes.get(parent.0) else {
            continue;
        };
        let slot = depth.map.creature_slot(node.0.bottom, node.0.id);
        let target = slot.shadow - node.1.translation.z;
        if transform.translation.z != target {
            transform.translation.z = target;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kn_game::{CreatureClips, CreatureLayer, LayerKey};

    use super::*;
    use crate::animation::Frame;

    /// The walk cycle is opened at the tick the step started, not the tick it was
    /// first observed: `elapsed - 1` ticks after the base frame, so the shown
    /// frame runs with the step like the original's per-tick `Frame` counter.
    #[test]
    fn apply_clip_opens_the_frame_the_step_has_reached() {
        let frames = (0..12)
            .map(|_| Frame::new(Handle::default(), Vec2::new(8.0, 8.0), Vec2::ZERO))
            .collect();
        let clip = Clip {
            sequence: Arc::new(Sequence::new(frames)),
            base: 0,
            corner: (0, 0),
        };
        let mut animator = Animator::new(Arc::clone(&clip.sequence), 0, 0);
        let clock_ticks = 100u64;
        let elapsed = 3u32;
        apply_clip(
            &mut animator,
            &clip,
            (0, 0),
            clock_ticks.saturating_sub(u64::from(elapsed - 1)),
        );
        assert_eq!(
            clip.sequence.frame_at(clock_ticks, animator.phase),
            (elapsed - 1) as usize,
            "three ticks into a step shows its third frame"
        );
    }

    /// A swap re-anchors the animator on the clip's own corner reduced to the
    /// creature's origin: leaving the resting anchor there displaced the image
    /// by the corner difference — the periodic jump review #68 is about.
    #[test]
    fn apply_clip_reanchors_on_the_clips_own_corner() {
        let rest_sequence = Arc::new(Sequence::new(Vec::new()));
        let walk_sequence = Arc::new(Sequence::new(Vec::new()));
        let mut animator = Animator::with_base(Arc::clone(&rest_sequence), 4, 40, 0);
        let walk = Clip {
            sequence: Arc::clone(&walk_sequence),
            base: 0,
            corner: (62, 26),
        };

        apply_clip(&mut animator, &walk, (4, 10), 9);

        assert_eq!(
            (animator.x, animator.y),
            (58, 16),
            "the anchor is the clip corner in the creature's local coordinates"
        );
        assert!(Arc::ptr_eq(&animator.sequence, &walk_sequence));
        assert_eq!(animator.shown(), None, "the swap forces a sprite rewrite");
        assert_eq!(
            walk_sequence.frame_at(9, animator.phase),
            walk.base,
            "the clip opens on its base frame at the current tick"
        );
    }

    /// A walk pose may list the same layers in another order than the resting
    /// one (tracks are ordered by first appearance per pose), so a join by
    /// position would bolt the gear's walk clip onto the body's piece and the
    /// body's onto the gear's — equipment sliding across the sprite on every
    /// step. The join must follow [`LayerKey`].
    #[test]
    fn walk_layers_join_to_their_part_not_their_position() {
        let rest = [
            data_layer(LayerKey::Body, (0, 0)),
            data_layer(LayerKey::Gear { slot: 1 }, (4, 40)),
        ];
        // Direction 0 lists the layers reversed, direction 1 has no gear at all.
        let reversed = CreatureClips {
            layers: vec![
                data_layer(LayerKey::Gear { slot: 1 }, (6, 44)),
                data_layer(LayerKey::Body, (2, 1)),
            ],
            shadow: None,
        };
        let gearless = CreatureClips {
            layers: vec![data_layer(LayerKey::Body, (3, 2))],
            shadow: None,
        };
        let walk = [Some(reversed), Some(gearless), None];

        let matched = pose_matched_layers(&rest, &walk);
        assert_eq!(matched.len(), 2);
        assert_eq!(
            matched[0][0].map(|layer| layer.key),
            Some(LayerKey::Body),
            "the body's walk clip must be the body's, not the first listed"
        );
        assert_eq!(
            matched[0][0].map(|layer| layer.corner),
            Some((2, 1)),
            "the body's clip keeps the walk pose's own corner"
        );
        assert_eq!(
            matched[1][0].map(|layer| layer.key),
            Some(LayerKey::Gear { slot: 1 }),
            "the gear's walk clip must be the gear's, not the second listed"
        );
        assert!(
            matched[1][1].is_none(),
            "a pose without the part contributes no clip"
        );
        assert!(
            matched[0][2].is_none(),
            "a missing pose contributes nothing"
        );
    }

    fn data_layer(key: LayerKey, corner: (i32, i32)) -> CreatureLayer {
        CreatureLayer {
            base: None,
            animation: None,
            corner,
            key,
        }
    }
}
