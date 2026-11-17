//! Shared creature animation: clips, composite sprites and the per-frame swap.
//!
//! Both frontends — the map viewer and the player — attach this subsystem, so a
//! creature is decoded, spawned and animated the same way in either window and
//! neither keeps its own copy of the pipeline.
//!
//! The boundary this module must keep (ARCHITECTURE.md, "Boundary rules"): the
//! game state and its transitions live in the Bevy-free game layer
//! ([`kn_game::session::Simulation`]). This module reads that answer as data
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
use std::time::{Duration, Instant};

use bevy::prelude::*;
use kn_game::{
    CreatureAnimation, CreatureClips, CreatureLayer as GameCreatureLayer, Game, MapRef, Piece,
    PieceGroup, Scene,
};
use kn_game::{
    session::CreatureStage, session::CreatureState, session::Simulation, session::WalkState,
};

use crate::GameResource;
use crate::animation::{AnimSet, Animator, Clock, Sequence};
use crate::creatures::{
    CreatureData, CreatureNode, CreatureRoot, CreatureShadow, DepthMap, LayerAnimation, LayerData,
    spawn_creature, update_creature_depth,
};
use crate::lighting::{attach_creature_lighting, update_creature_lighting};
use crate::scene::{Z_STEP, piece_sequence, piece_size};
use crate::textures::TextureCache;
use kn_lighting::{LightingSet, classic::LightingMaterials};

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
    pub melee: Vec<Option<Clip>>,
    pub armed_rest: Vec<Option<Clip>>,
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
    /// Last animation stage kind per creature, excluding its frame counter.
    /// The state machine's own change report fires once per edge, but a reset
    /// recreates the whole machine, so the first observation after a reset must
    /// also count as a change; the subsystem keeps its own snapshot for that.
    pub stages: HashMap<usize, u8>,
    /// Last observed one-shot frame index of each creature still playing. A decrease under an unchanged [`Sequence`] is a restart the
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
        let stages: Vec<_> = states
            .iter()
            .map(|&(id, special)| {
                (
                    id,
                    if special {
                        CreatureStage::Special { frame: 0 }
                    } else {
                        CreatureStage::Idle
                    },
                )
            })
            .collect();
        self.observe_animation_stages(&stages)
    }

    pub fn observe_animation_stages(
        &mut self,
        states: &[(usize, CreatureStage)],
    ) -> HashSet<usize> {
        let mut changed = HashSet::new();
        for &(id, stage) in states {
            let kind = stage.kind();
            if self.stages.insert(id, kind) != Some(kind) {
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
    /// The `Game.N` the running simulation, its clips and the Scene were built
    /// for (`None` — the first). A rebuild — reset or a backward replay — reloads
    /// this same hero so the creature geometry never drifts onto another hero's
    /// state (review #101, finding P2-2).
    pub hero: Option<u32>,
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

/// Optional per-frame scheduling budget for simulation catch-up. Frontends that
/// omit it retain unlimited catch-up; no target tick or gameplay rule is capped.
#[derive(Resource, Debug, Clone, Copy)]
pub struct SimulationCatchUpBudget {
    pub max_ticks: u64,
    pub max_duration: Option<Duration>,
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
    /// Which `Game.N` the frontend composed the Scene for; the simulation and the
    /// clips must load the same hero's state, so it rides the request (`None` —
    /// the first, the viewer's default; review #101, P2-2).
    pub hero: Option<u32>,
    pub scene: Option<Scene>,
    pub rebuild: bool,
}

impl CreatureMotion {
    /// Asks for `map`'s creatures to be built from `scene` at the next run of
    /// [`prepare_creatures`]. `seed` fixes the simulation's spawn so a reset
    /// reproduces the first build exactly, and `hero` must be the same `Game.N`
    /// the frontend built `scene` from, so geometry, roster and clips stay on one
    /// starting state (review #101, P2-2).
    pub fn request(&mut self, map: MapRef, seed: u64, hero: Option<u32>, scene: Scene) {
        self.map = Some(map);
        self.seed = seed;
        self.hero = hero;
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
    /// Body height in pixels per creature, measured from the creature's Scene
    /// composition base (`scene_bottom − base.y`), so `sync_creature_nodes` adds
    /// it to the live node row to get the occlusion bottom without a scene
    /// lookup; it equals `CreatureData::bottom − origin.y` (review #101, P2-1).
    pub heights: HashMap<usize, i32>,
    /// Spawn pixel per creature, the origin the occlusion bottom and depth are
    /// measured against (the simulation's own position for that creature).
    pub origins: HashMap<usize, (i32, i32)>,
    /// Scene composition origin per creature — the base every piece offset and
    /// animation anchor is reduced against (`Scene::creature_origins`), kept
    /// apart from [`Self::origins`] so the sprite's local geometry stays fixed to
    /// the creature even when the simulation places it elsewhere than the Scene
    /// composed it (review #101, P2-2). Falls back to the origin, i.e. the
    /// identity, for a creature the Scene carries no origin for.
    pub bases: HashMap<usize, (i32, i32)>,
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
                update_creature_lighting
                    .after(CreatureSet::Nodes)
                    .before(LightingSet::Apply),
            )
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
    hero: Option<u32>,
    creature_pieces: &[Piece],
    cache: &mut TextureCache,
    images: &mut Assets<Image>,
) -> (CreatureAnim, CreatureAnimReport) {
    let animations = match game.creature_animations_with_hero(map_ref, hero) {
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
            .chain(anim.melee.iter().flatten())
            .chain(anim.armed_rest.iter().flatten())
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

/// Stable rest-first layer union across all poses, joined by part identity.
fn build_sequences(anim: &CreatureAnimation, cache: &TextureCache) -> CreatureSequences {
    let mut keys: Vec<_> = anim.rest.layers.iter().map(|layer| layer.key).collect();
    for pose in anim
        .walk
        .iter()
        .chain(anim.melee.iter())
        .chain(anim.armed_rest.iter())
        .flatten()
        .chain(anim.special.iter())
    {
        for layer in &pose.layers {
            if !keys.contains(&layer.key) {
                keys.push(layer.key);
            }
        }
    }
    let body =
        keys.into_iter()
            .enumerate()
            .map(|(index, key)| {
                let rest_layer = layer_with_key(&anim.rest, key);
                let rest = clip_seq(rest_layer, cache).or_else(|| Some(blank_clip()));
                let walk = anim
                    .walk
                    .iter()
                    .map(|pose| {
                        pose.as_ref()
                            .and_then(|pose| clip_seq(layer_with_key(pose, key), cache))
                    })
                    .collect();
                let special = anim.special.as_ref().map(|pose| {
                    clip_seq(layer_with_key(pose, key), cache).unwrap_or_else(blank_clip)
                });
                let armed_rest = anim
                    .armed_rest
                    .iter()
                    .map(|pose| {
                        pose.as_ref().map(|pose| {
                            clip_seq(layer_with_key(pose, key), cache).unwrap_or_else(blank_clip)
                        })
                    })
                    .collect();
                let melee = anim
                    .melee
                    .iter()
                    .map(|pose| {
                        pose.as_ref().map(|pose| {
                            clip_seq(layer_with_key(pose, key), cache).unwrap_or_else(blank_clip)
                        })
                    })
                    .collect();
                (
                    index,
                    LayerSequences {
                        rest,
                        walk,
                        special,
                        melee,
                        armed_rest,
                    },
                )
            })
            .collect();
    let rest = clip_seq(anim.rest.shadow.as_ref(), cache).or_else(|| Some(blank_clip()));
    let walk = anim
        .walk
        .iter()
        .map(|pose| {
            pose.as_ref()
                .and_then(|clips| clip_seq(clips.shadow.as_ref(), cache))
        })
        .collect();
    let special = anim
        .special
        .as_ref()
        .map(|clips| clip_seq(clips.shadow.as_ref(), cache).unwrap_or_else(blank_clip));
    let armed_rest = anim
        .armed_rest
        .iter()
        .map(|pose| {
            pose.as_ref()
                .map(|clips| clip_seq(clips.shadow.as_ref(), cache).unwrap_or_else(blank_clip))
        })
        .collect();
    let melee = anim
        .melee
        .iter()
        .map(|pose| {
            pose.as_ref()
                .map(|clips| clip_seq(clips.shadow.as_ref(), cache).unwrap_or_else(blank_clip))
        })
        .collect();
    CreatureSequences {
        body,
        shadow: LayerSequences {
            rest,
            walk,
            special,
            melee,
            armed_rest,
        },
    }
}

fn layer_with_key(clips: &CreatureClips, key: kn_game::LayerKey) -> Option<&GameCreatureLayer> {
    clips.layers.iter().find(|layer| layer.key == key)
}

fn blank_clip() -> Clip {
    Clip {
        sequence: Arc::new(Sequence::new(vec![crate::animation::Frame {
            image: None,
            size: Vec2::ZERO,
            offset: Vec2::ZERO,
            ticks: 1,
        }])),
        base: 0,
        corner: (0, 0),
    }
}

/// For each rest layer, the layer of every walk pose that draws the same part.
///
/// The match is by [`kn_game::LayerKey`]: pose tracks are ordered by first
/// appearance within one pose, so a walk pose may list the same layers in
/// another order, and an index join would bolt one part's walk clip onto
/// another's piece — equipment sliding across the sprite on every step
/// (review #68).
#[cfg(test)]
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
    let (sequence, base) = if let Some(animation) = &layer.animation {
        (piece_sequence(animation, cache)?, animation.base)
    } else {
        let texture = layer.base.and_then(|texture| cache.get(&texture))?;
        (
            Arc::new(Sequence::new(vec![crate::animation::Frame::new(
                texture.handle.clone(),
                texture.size.as_vec2(),
                Vec2::ZERO,
            )])),
            0,
        )
    };
    Some(Clip {
        sequence,
        base,
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
    let mut bases = HashMap::new();
    for &(id, origin) in initial_pixels {
        let pieces: Vec<&Piece> = scene
            .creature_pieces
            .iter()
            .filter(|piece| piece.creature_id == Some(id))
            .collect();
        if pieces.is_empty() {
            continue;
        }
        // The Scene composed this creature's pieces around its own origin cell;
        // reduce the local geometry against that, never against the simulation
        // origin, so the sprite stays put while the node rides the simulation
        // (review #101, P2-2). A creature the Scene carries no origin for keeps
        // the historical behaviour: base == origin, the identity reduction.
        let base = scene.creature_origins.get(&id).copied().unwrap_or(origin);
        let (plan, height) = build_creature_plan(pieces, cache, anim, id, origin, base);
        plans.push(plan);
        heights.insert(id, height);
        origins.insert(id, origin);
        bases.insert(id, base);
    }
    CreatureSpawns {
        plans,
        heights,
        origins,
        bases,
    }
}

/// One creature's pieces → spawn data, with body height reported for depth.
///
/// `bottom` is the lowest body edge (the shadow never sets the occlusion line).
/// Body layers keep their scene order, which indexes [`CreatureSequences::body`].
///
/// Piece offsets and resting anchors are reduced against `base` (the Scene
/// composition origin), while the node's world position rides the simulation
/// `origin`. Body height is the lowest body edge measured from `base` (the
/// shadow never sets it), so the returned height and `CreatureData::bottom` stay
/// correct when the Scene composed a creature elsewhere than the simulation
/// places it (`base != origin`, review #101, finding P2-1): `bottom` is
/// `origin.y + height`, the occlusion line at the creature's live row. For a
/// static creature `base == origin` and this reduces to the scene bottom, the
/// historical value.
fn build_creature_plan(
    pieces: Vec<&Piece>,
    cache: &TextureCache,
    anim: &CreatureAnim,
    id: usize,
    origin: (i32, i32),
    base: (i32, i32),
) -> (CreatureData, i32) {
    let mut layers: Vec<LayerData> = Vec::new();
    let mut shadow: Option<LayerData> = None;
    // The lowest body edge in absolute scene coordinates; the shadow never
    // contributes, so it never sets the occlusion line.
    let mut scene_bottom = i32::MIN;
    for piece in pieces {
        let entry = piece.texture.as_ref().and_then(|id| cache.get(id));
        let size = piece_size(piece, entry.map_or(UVec2::ZERO, |entry| entry.size));
        let layer = LayerData {
            image: entry.map_or_else(Handle::default, |entry| entry.handle.clone()),
            size,
            offset: Vec2::new((piece.x - base.0) as f32, (piece.y - base.1) as f32),
        };
        if piece.group == PieceGroup::CharacterShadows {
            shadow = Some(layer);
        } else {
            scene_bottom = scene_bottom.max(piece.y + size.y as i32);
            layers.push(layer);
        }
    }
    // Height above the Scene composition base, not the simulation origin: the
    // occlusion line must follow the creature's live row, so `bottom` is the
    // origin row plus this height rather than the absolute scene bottom — the
    // old code kept the pre-relocation position's offset once #99 moved a member.
    let height = if scene_bottom == i32::MIN {
        0
    } else {
        scene_bottom - base.1
    };
    let bottom = origin.1 + height;
    let sequences = anim.creatures.get(&id);
    if let Some(sequences) = sequences {
        // Pose-only equipment needs an entity before its first visible frame.
        for _ in layers.len()..sequences.body.len() {
            layers.push(LayerData {
                image: Handle::default(),
                size: Vec2::ZERO,
                offset: Vec2::ZERO,
            });
        }
        if shadow.is_none() && sequences.shadow.melee.iter().any(Option::is_some) {
            shadow = Some(LayerData {
                image: Handle::default(),
                size: Vec2::ZERO,
                offset: Vec2::ZERO,
            });
        }
    }
    let animations = layers
        .iter()
        .enumerate()
        .map(|(index, _)| rest_animation(sequences.and_then(|c| c.body.get(&index)), base))
        .collect();
    let shadow_animation = sequences.and_then(|c| rest_animation(Some(&c.shadow), base));
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
        height,
    )
}

/// The resting [`LayerAnimation`] of a layer, if it has a resting sequence,
/// anchored on the clip's own base corner reduced to the creature's Scene
/// composition origin — the same local coordinates its piece offset lives in, so
/// the resting clip and the static piece land on one spot regardless of where the
/// simulation places the node (review #101, P2-2).
fn rest_animation(layer: Option<&LayerSequences>, base: (i32, i32)) -> Option<LayerAnimation> {
    let clip = layer?.rest.as_ref()?;
    Some(LayerAnimation {
        sequence: Arc::clone(&clip.sequence),
        base: clip.base,
        anchor: (clip.corner.0 - base.0, clip.corner.1 - base.1),
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
            // A flag-8 object carries its own occlusion bottom (`depth_bottom`),
            // the original's per-object depth-sort adjustment (review #103); it
            // wins over the plain texture bottom so a creature standing behind
            // such an object hides at the line the original uses.
            (
                piece.depth_bottom.unwrap_or(piece.y + height),
                order as f32 * Z_STEP,
            )
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
    debug: Option<ResMut<crate::DebugReplay>>,
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
    // Remember the hero the Scene was composed for: every later rebuild (reset,
    // backward replay) reloads this same `Game.N` (review #101, P2-2).
    sim.hero = request.hero;
    depth.map = build_depth_map(&scene, &cache);
    match game
        .0
        .simulation_with_hero(map_ref, request.seed, request.hero)
    {
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
    if let Some(mut debug) = debug {
        debug.start(&game.0, map_ref, request.seed, request.hero);
    }
    let (built, built_report) = build_creature_anim(
        &game.0,
        map_ref,
        request.hero,
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
    match game.simulation_with_hero(map_ref, seed, sim.hero) {
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
    match game.simulation_with_hero(map_ref, seed, sim.hero) {
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
    budget: Option<Res<SimulationCatchUpBudget>>,
    debug: Option<ResMut<crate::DebugReplay>>,
) {
    if sim.sim.is_none() {
        sim.epoch = clock.ticks;
        sim.sim_ticks = 0;
        return;
    }
    let target = clock.ticks.saturating_sub(sim.epoch);
    if let Some(mut debug) = debug {
        debug.advance(&mut sim, &game.0, target, budget.as_deref());
        return;
    }
    let hero = sim.hero;
    advance_to_target(&mut sim, target, budget.as_deref(), || {
        let map_ref = request
            .map
            .ok_or_else(|| "cannot replay simulation without a map".to_owned())?;
        game.0
            .simulation_with_hero(map_ref, request.seed, hero)
            .map_err(|error| format!("simulation replay for {map_ref:?}: {error}"))
    });
}

fn advance_to_target(
    sim: &mut CreatureSim,
    target: u64,
    budget: Option<&SimulationCatchUpBudget>,
    rebuild: impl FnOnce() -> Result<Simulation, String>,
) {
    if target < sim.sim_ticks {
        match rebuild() {
            Ok(replayed) => {
                sim.initial_pixels = replayed.initial_pixels();
                sim.sim = Some(replayed);
                sim.sim_ticks = 0;
                sim.error = None;
            }
            Err(error) => {
                sim.error = Some(error);
                return;
            }
        }
    }
    let started = budget.and_then(|budget| budget.max_duration.map(|_| Instant::now()));
    let mut advanced = 0;
    while sim.sim_ticks < target {
        let Some(simulation) = sim.sim.as_mut() else {
            break;
        };
        simulation.tick();
        sim.sim_ticks += 1;
        advanced += 1;
        // Even a zero budget permits one tick, so a seek can always progress.
        if budget.is_some_and(|budget| {
            advanced >= budget.max_ticks.max(1)
                || started
                    .zip(budget.max_duration)
                    .is_some_and(|(start, duration)| start.elapsed() >= duration)
        }) {
            break;
        }
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
    lighting: Option<Res<LightingMaterials>>,
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
            attach_creature_lighting(&mut commands.entity(*child), lighting.as_deref());
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

fn select_stage_clip(
    layer: &LayerSequences,
    walking: bool,
    direction: u8,
    stage: CreatureStage,
    armed: bool,
) -> Option<&Clip> {
    match stage {
        CreatureStage::Melee { .. } => layer.melee.get(direction as usize).and_then(Option::as_ref),
        CreatureStage::Idle if armed && !walking => layer
            .armed_rest
            .get(direction as usize)
            .and_then(Option::as_ref)
            .or(layer.rest.as_ref()),
        _ => select_clip(layer, walking, direction, stage.is_special()),
    }
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
    debug: Option<Res<crate::DebugReplay>>,
    mut sprites: Query<(&CreatureSprite, &mut Animator, &Transform)>,
) {
    let Some(ref simulation) = sim.sim else {
        return;
    };
    let states = simulation.states();
    let restep = anim.observe_steps(&states);
    let stage_pairs: Vec<_> = states
        .iter()
        .map(|state| {
            (
                state.id,
                simulation
                    .creature_stage(state.id)
                    .unwrap_or(CreatureStage::Idle),
            )
        })
        .collect();
    let stage_changes = anim.observe_animation_stages(&stage_pairs);
    let special_playing: Vec<_> = stage_pairs
        .iter()
        .filter_map(|&(id, stage)| stage.frame().map(|frame| (id, frame)))
        .collect();
    let special_resets = anim.observe_specials(&special_playing);
    let directed: HashSet<_> = states
        .iter()
        .filter(|state| state.direction.is_some())
        .map(|state| state.id)
        .collect();
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
    let progress: HashMap<usize, (u32, u32)> = states
        .iter()
        .map(|state| (state.id, (state.step_elapsed, state.step_ticks)))
        .collect();
    let replay_anchors: HashMap<usize, u64> = debug
        .as_ref()
        .map(|debug| {
            states
                .iter()
                .map(|state| {
                    (
                        state.id,
                        sim.epoch.saturating_add(debug.animation_origin(
                            state,
                            simulation.creature_stage(state.id),
                            simulation.tick_number(),
                        )),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    for (tag, mut animator, _transform) in &mut sprites {
        let Some(creature) = anim.creatures.get(&tag.creature_id) else {
            continue;
        };
        let stage = simulation
            .creature_stage(tag.creature_id)
            .unwrap_or(CreatureStage::Idle);
        let acting = stage != CreatureStage::Idle;
        let special_frame = stage.frame();
        let layer = if tag.is_shadow {
            &creature.shadow
        } else {
            match creature.body.get(&tag.layer) {
                Some(layer) => layer,
                None => continue,
            }
        };
        let (walking, direction) = facing.get(&tag.creature_id).copied().unwrap_or((false, 0));
        let clip = select_stage_clip(
            layer,
            walking,
            direction,
            stage,
            directed.contains(&tag.creature_id)
                && simulation.creature_shown_state(tag.creature_id) == Some(0),
        );
        let Some(clip) = clip else { continue };
        let (elapsed, step_ticks) = progress.get(&tag.creature_id).copied().unwrap_or((0, 0));
        let sampled_frame = if let CreatureStage::Melee { frame } = stage {
            Some(
                clip.sequence
                    .frame_at(u64::from(frame), clip.sequence.phase_for(clip.base)),
            )
        } else {
            (walking
                && !acting
                && layer
                    .walk
                    .get(direction as usize)
                    .is_some_and(Option::is_some))
            .then(|| {
                let point = sampled_step_tick(clip.sequence.ticks, elapsed, step_ticks);
                clip.sequence
                    .frame_at(point, clip.sequence.phase_for(clip.base))
            })
        };
        animator.sampled_frame = sampled_frame;
        let replay_anchor = replay_anchors.get(&tag.creature_id).copied();
        if Arc::ptr_eq(&animator.sequence, &clip.sequence)
            && !(restep.contains(&tag.creature_id) && !acting)
            && !stage_changes.contains(&tag.creature_id)
            && !special_resets.contains(&tag.creature_id)
            && replay_anchor
                .is_none_or(|anchor| animator.phase == clip.sequence.phase_at(clip.base, anchor))
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
        // Re-anchor against the Scene composition origin, not the simulation
        // origin, so a clip's frame lands at the same node-local spot its resting
        // piece occupies even after the simulation moved the node elsewhere
        // (review #101, P2-2); a creature with no recorded base keeps the
        // identity, base == origin.
        let base = spawns
            .bases
            .get(&tag.creature_id)
            .copied()
            .unwrap_or(origin);
        // Anchor the swap so the current tick shows the correct frame: the
        // special opens on the tick its frame counter started, the walk opens
        // on the tick the step began (elapsed - 1 ago).
        let step_start = if let Some(anchor) = replay_anchor {
            anchor
        } else if let Some(frame) = special_frame {
            clock.ticks.saturating_sub(u64::from(frame))
        } else {
            clock
                .ticks
                .saturating_sub(u64::from(elapsed.saturating_sub(1)))
        };
        apply_clip(&mut animator, clip, base, step_start);
        animator.sampled_frame = sampled_frame;
    }
}

fn sampled_step_tick(sequence_ticks: u64, elapsed: u32, step_ticks: u32) -> u64 {
    let last_step_tick = step_ticks.saturating_sub(1);
    let spent = elapsed.saturating_sub(1).min(last_step_tick);
    // Include both ends of the clip when a shorter step skips source frames.
    (u128::from(spent) * u128::from(sequence_ticks.saturating_sub(1))
        / u128::from(last_step_tick.max(1))) as u64
}

/// Installs `clip` on `animator`, re-anchored on the clip's own base corner
/// reduced to the creature's Scene composition origin `base` at `at_tick`.
fn apply_clip(animator: &mut Animator, clip: &Clip, base: (i32, i32), at_tick: u64) {
    animator.play_at(
        Arc::clone(&clip.sequence),
        clip.base,
        at_tick,
        (clip.corner.0 - base.0, clip.corner.1 - base.1),
    );
}

/// Puts each creature node at its live cell and depth.
///
/// The node owns the creature's world position and z; its sprite children carry
/// only local offsets and per-frame deltas, so they follow the node untouched and
/// body, equipment and shadow stay in lockstep (review #68). The node sits on the
/// simulation cell's pixel for the whole step: the glide to the next cell lives in
/// the walk frames' own offsets (docs/research/k1/movement.md, "Step duration"), so sliding
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

    fn material_handle(
        pool: &kn_lighting::classic::LightingMaterials,
        surface: kn_lighting::classic::ClassicSurface,
    ) -> bevy::prelude::Handle<kn_lighting::classic::KonungLightMaterial> {
        use bevy::{ecs::world::CommandQueue, prelude::*, sprite_render::SpriteMaterial};
        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let id = {
            let mut commands = Commands::new(&mut queue, &world);
            let mut entity = commands.spawn_empty();
            pool.attach(&mut entity, surface);
            entity.id()
        };
        queue.apply(&mut world);
        world
            .get::<SpriteMaterial<kn_lighting::classic::KonungLightMaterial>>(id)
            .unwrap()
            .0
            .clone()
    }

    use std::sync::Arc;

    use kn_game::{CreatureClips, CreatureLayer, LayerKey};

    use super::*;
    use crate::animation::Frame;

    #[test]
    fn melee_and_armed_idle_follow_the_live_direction_and_their_own_frames() {
        let clip = |offset| Clip {
            sequence: Arc::new(Sequence::new(
                (0..3)
                    .map(|frame| {
                        Frame::new(
                            Handle::default(),
                            Vec2::ONE,
                            Vec2::new((offset + frame) as f32, 0.0),
                        )
                    })
                    .collect(),
            )),
            base: 0,
            corner: (offset, 0),
        };
        let layer = LayerSequences {
            rest: Some(clip(1)),
            walk: vec![],
            special: Some(clip(10)),
            melee: (0..8).map(|direction| Some(clip(20 + direction))).collect(),
            armed_rest: (0..8).map(|direction| Some(clip(40 + direction))).collect(),
        };
        let attack =
            select_stage_clip(&layer, false, 5, CreatureStage::Melee { frame: 2 }, true).unwrap();
        assert_eq!(attack.corner, (25, 0));
        assert_eq!(attack.sequence.frame_at(2, 0), 2);
        let idle = select_stage_clip(&layer, false, 5, CreatureStage::Idle, true).unwrap();
        assert_eq!(idle.corner, (45, 0));
        assert_eq!(
            select_stage_clip(&layer, false, 5, CreatureStage::Idle, false)
                .unwrap()
                .corner,
            (1, 0)
        );
        assert_eq!(
            select_stage_clip(&layer, false, 5, CreatureStage::Special { frame: 1 }, true)
                .unwrap()
                .corner,
            (10, 0)
        );
        let mut tracker = CreatureAnim::default();
        assert_eq!(
            tracker.observe_animation_stages(&[(7, CreatureStage::Melee { frame: 0 })]),
            HashSet::from([7])
        );
        assert!(
            tracker
                .observe_animation_stages(&[(7, CreatureStage::Melee { frame: 2 })])
                .is_empty()
        );
        assert_eq!(
            tracker.observe_animation_stages(&[(7, CreatureStage::Idle)]),
            HashSet::from([7])
        );
    }

    #[test]
    fn attack_only_equipment_gets_a_hidden_rest_entity_and_missing_parts_stay_hidden() {
        let body = data_layer(LayerKey::Body, (0, 0));
        let gear = data_layer(LayerKey::Gear { slot: 1 }, (5, 8));
        let animations = CreatureAnimation {
            rest: CreatureClips {
                layers: vec![body.clone()],
                shadow: None,
            },
            walk: vec![],
            special: None,
            armed_rest: vec![],
            melee: vec![
                Some(CreatureClips {
                    layers: vec![gear, body],
                    shadow: None,
                }),
                Some(CreatureClips {
                    layers: vec![],
                    shadow: None,
                }),
            ],
        };
        let sequences = build_sequences(&animations, &TextureCache::default());
        assert_eq!(sequences.body.len(), 2);
        let gear = sequences.body.get(&1).unwrap();
        assert!(
            gear.rest.as_ref().unwrap().sequence.frames[0]
                .image
                .is_none()
        );
        assert!(
            gear.melee[1].as_ref().unwrap().sequence.frames[0]
                .image
                .is_none()
        );
        let anim = CreatureAnim {
            creatures: HashMap::from([(7, sequences)]),
            ..default()
        };
        let (plan, _) =
            build_creature_plan(vec![], &TextureCache::default(), &anim, 7, (10, 10), (0, 0));
        assert_eq!(plan.layers.len(), 2);
        assert!(plan.animations.iter().all(Option::is_some));
    }

    #[test]
    fn melee_body_equipment_and_shadow_sample_simulation_frames_even_when_clock_differs() {
        use k1_data::{
            game_state::{Character, GameState, Group, Rect},
            map::KonungMap,
        };
        let mut characters = vec![None; 10];
        characters[7] = Some(Character {
            slot: 7,
            x: 10,
            y: 10,
            group: 0,
            state: 6,
            direction: 2,
            ..default()
        });
        characters[9] = Some(Character {
            slot: 9,
            x: 11,
            y: 10,
            group: 1,
            class_id: 0x41,
            ..default()
        });
        let fresh = || {
            Simulation::new(
                &KonungMap {
                    ground: vec![],
                    overlay: vec![],
                    foot: vec![],
                    decorations: vec![],
                    objects: vec![],
                },
                &GameState {
                    items: vec![],
                    loot: vec![],
                    exits: vec![],
                    villages: vec![],
                    characters: characters.clone(),
                    groups: vec![
                        Some(Group {
                            slot: 0,
                            first_character: 7,
                            size: 1,
                            kind: 0x20,
                            ..default()
                        }),
                        Some(Group {
                            slot: 1,
                            first_character: 9,
                            size: 1,
                            kind: 0x41,
                            map_id: 1,
                            spawn: Rect {
                                left: 11,
                                right: 11,
                                top: 10,
                                bottom: 10,
                            },
                            ..default()
                        }),
                    ],
                },
                1,
                Some(0),
                vec![[12; 8]; 2],
                vec![0; 2],
                vec![],
                42,
                None,
            )
        };
        let mut simulation = fresh();
        simulation.configure_melee_frames(vec![[3; 8], [0; 8]]);
        simulation.order_hero_attack(9).unwrap();
        let sequence = Arc::new(Sequence::new(
            (0..3)
                .map(|frame| Frame::new(Handle::default(), Vec2::ONE, Vec2::new(frame as f32, 0.0)))
                .collect(),
        ));
        let layer = || LayerSequences {
            rest: Some(blank_clip()),
            walk: vec![],
            special: None,
            armed_rest: (0..8)
                .map(|direction| {
                    Some(Clip {
                        sequence: Arc::clone(&sequence),
                        base: 0,
                        corner: (20 + direction, 30),
                    })
                })
                .collect(),
            melee: (0..8)
                .map(|_| {
                    Some(Clip {
                        sequence: Arc::clone(&sequence),
                        base: 0,
                        corner: (5, 6),
                    })
                })
                .collect(),
        };
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, crate::AnimPlugin::new(60.0)))
            .insert_resource(CreatureSim {
                sim: Some(simulation),
                ..default()
            })
            .insert_resource(CreatureAnim {
                creatures: HashMap::from([(
                    7,
                    CreatureSequences {
                        body: HashMap::from([(0, layer()), (1, layer())]),
                        shadow: layer(),
                    },
                )]),
                ..default()
            })
            .insert_resource(CreatureSpawns {
                origins: HashMap::from([(7, (0, 0))]),
                ..default()
            })
            .add_systems(Update, sync_creature_animation.before(AnimSet::Sprites));
        app.world_mut()
            .resource_mut::<Clock>()
            .goto(crate::PlayState::Paused);
        let entities: Vec<_> = (0..3)
            .map(|part| {
                app.world_mut()
                    .spawn((
                        CreatureSprite {
                            creature_id: 7,
                            layer: part.min(1),
                            is_shadow: part == 2,
                        },
                        Animator::new(Arc::clone(&sequence), 0, 0),
                        Sprite::default(),
                        Transform::default(),
                        Visibility::Inherited,
                    ))
                    .id()
            })
            .collect();
        for tick in 1..=8 {
            let frame = {
                let mut sim = app.world_mut().resource_mut::<CreatureSim>();
                let simulation = sim.sim.as_mut().unwrap();
                simulation.tick();
                let Some(CreatureStage::Melee { frame }) = simulation.creature_stage(7) else {
                    panic!("expected a repeating melee action at tick {tick}");
                };
                frame
            };
            app.world_mut().resource_mut::<Clock>().ticks = tick * 10;
            app.update();
            for &entity in &entities {
                let animator = app.world().get::<Animator>(entity).unwrap();
                assert_eq!(animator.shown(), Some(frame as usize));
                assert_eq!((animator.x, animator.y), (5, 6));
            }
            app.update();
            for &entity in &entities {
                assert_eq!(
                    app.world().get::<Animator>(entity).unwrap().shown(),
                    Some(frame as usize)
                );
            }
        }
        {
            let mut sim = app.world_mut().resource_mut::<CreatureSim>();
            let simulation = sim.sim.as_mut().unwrap();
            simulation.adopt_positions_from(fresh());
            assert_eq!(simulation.creature_stage(7), Some(CreatureStage::Idle));
            assert_eq!(simulation.creature_shown_state(7), Some(0));
            assert_eq!(
                simulation
                    .states()
                    .iter()
                    .find(|state| state.id == 7)
                    .unwrap()
                    .direction,
                Some(2)
            );
        }
        app.update();
        for &entity in &entities {
            let animator = app.world().get::<Animator>(entity).unwrap();
            assert_eq!((animator.x, animator.y), (22, 30));
        }
        app.world_mut().resource_mut::<CreatureSim>().sim = Some(fresh());
        app.world_mut()
            .resource_mut::<Clock>()
            .goto(crate::PlayState::Off);
        app.update();
        for &entity in &entities {
            let animator = app.world().get::<Animator>(entity).unwrap();
            assert_eq!((animator.x, animator.y), (0, 0));
            assert!(animator.sequence.frames[0].image.is_none());
        }
    }

    #[test]
    fn shortened_steps_sample_the_entire_walk_cycle_without_changing_normal_steps() {
        assert_eq!(
            (1..=6)
                .map(|elapsed| sampled_step_tick(12, elapsed, 6))
                .collect::<Vec<_>>(),
            [0, 2, 4, 6, 8, 11]
        );
        assert_eq!(
            (1..=12)
                .map(|elapsed| sampled_step_tick(12, elapsed, 12))
                .collect::<Vec<_>>(),
            (0..12).collect::<Vec<_>>()
        );
        assert_eq!(sampled_step_tick(12, 1, 1), 0);
        assert_eq!(sampled_step_tick(0, 0, 0), 0);
    }

    #[test]
    fn mode_changes_finish_the_current_clip_and_sample_body_equipment_and_shadow_together() {
        use k1_data::{
            game_state::{Character, GameState, Group},
            map::KonungMap,
        };
        use kn_isometric::Cell;
        use kn_movement::MovementMode;
        let mut characters = vec![None; 8];
        characters[7] = Some(Character {
            slot: 7,
            x: 8,
            y: 8,
            group: 0,
            ..default()
        });
        let simulation = Simulation::new(
            &KonungMap {
                ground: vec![],
                overlay: vec![],
                foot: vec![],
                decorations: vec![],
                objects: vec![],
            },
            &GameState {
                items: vec![],
                groups: vec![Some(Group {
                    slot: 0,
                    first_character: 7,
                    size: 1,
                    kind: 0x20,
                    ..default()
                })],
                loot: vec![],
                exits: vec![],
                characters,
                villages: vec![],
            },
            1,
            Some(0),
            vec![[12; 8]],
            vec![0],
            vec![],
            42,
            None,
        );
        assert_eq!(simulation.hero_id(), Some(7));
        let sequence = Arc::new(Sequence::new(
            (0..12)
                .map(|index| {
                    Frame::new(
                        Handle::default(),
                        Vec2::splat(8.0),
                        Vec2::new(index as f32 * 58.0 / 12.0, 0.0),
                    )
                })
                .collect(),
        ));
        let clip = || Clip {
            sequence: Arc::clone(&sequence),
            base: 0,
            corner: (0, 0),
        };
        let layer = || LayerSequences {
            rest: Some(Clip {
                sequence: Arc::new(Sequence::new(vec![Frame::new(
                    Handle::default(),
                    Vec2::splat(8.0),
                    Vec2::ZERO,
                )])),
                base: 0,
                corner: (0, 0),
            }),
            walk: (0..8).map(|_| Some(clip())).collect(),
            special: None,
            melee: Vec::new(),
            armed_rest: Vec::new(),
        };
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, crate::AnimPlugin::new(60.0)))
            .insert_resource(CreatureSim {
                sim: Some(simulation),
                ..default()
            })
            .insert_resource(CreatureAnim {
                creatures: HashMap::from([(
                    7,
                    CreatureSequences {
                        body: HashMap::from([(0, layer()), (1, layer())]),
                        shadow: layer(),
                    },
                )]),
                ..default()
            })
            .insert_resource(CreatureSpawns {
                origins: HashMap::from([(7, (0, 0))]),
                ..default()
            })
            .add_systems(
                Update,
                sync_creature_animation
                    .after(AnimSet::Clock)
                    .before(AnimSet::Sprites),
            );
        app.world_mut()
            .resource_mut::<Clock>()
            .goto(crate::PlayState::Paused);
        for (layer, shadow) in [(0, false), (1, false), (0, true)] {
            app.world_mut().spawn((
                CreatureSprite {
                    creature_id: 7,
                    layer,
                    is_shadow: shadow,
                },
                Animator::new(Arc::clone(&sequence), 0, 0),
                Sprite::default(),
                Transform::default(),
                Visibility::Inherited,
            ));
        }
        let target = Cell::new(12, 8);
        app.world_mut()
            .resource_mut::<CreatureSim>()
            .sim
            .as_mut()
            .unwrap()
            .order_hero_move(target)
            .unwrap();
        let mut prior_x = f32::NEG_INFINITY;
        let mut saw_run_step = false;
        let mut returned_to_walk = false;
        for tick in 1..=55 {
            if tick == 4 {
                app.world_mut()
                    .resource_mut::<CreatureSim>()
                    .sim
                    .as_mut()
                    .unwrap()
                    .order_hero_move_with_mode(target, MovementMode::Run)
                    .unwrap();
            }
            if tick == 17 {
                app.world_mut()
                    .resource_mut::<CreatureSim>()
                    .sim
                    .as_mut()
                    .unwrap()
                    .order_hero_move(target)
                    .unwrap();
            }
            app.world_mut()
                .resource_mut::<CreatureSim>()
                .sim
                .as_mut()
                .unwrap()
                .tick();
            app.world_mut().resource_mut::<Clock>().step(1);
            app.update();
            let state = app
                .world()
                .resource::<CreatureSim>()
                .sim
                .as_ref()
                .unwrap()
                .states()[0];
            if tick <= 12 {
                assert_eq!(state.step_ticks, 12);
            }
            if (13..=18).contains(&tick) {
                assert_eq!(state.step_ticks, 6);
                saw_run_step = true;
            }
            if tick >= 19 && state.moving {
                assert_eq!(state.step_ticks, 12);
                returned_to_walk = true;
            }
            let expected =
                sampled_step_tick(sequence.ticks, state.step_elapsed, state.step_ticks) as usize;
            let mut sprites = app.world_mut().query::<(&Animator, &Transform)>();
            for (animator, transform) in sprites.iter(app.world()) {
                if state.moving {
                    assert_eq!(animator.sampled_frame, Some(expected));
                    assert_eq!(animator.shown(), Some(expected));
                } else {
                    assert_eq!(animator.sampled_frame, None);
                }
                let x = state.pixel.0 as f32 + transform.translation.x;
                assert!(x >= prior_x, "tick {tick}: {x} < {prior_x}");
            }
            let mut sprites = app.world_mut().query::<&Transform>();
            prior_x =
                state.pixel.0 as f32 + sprites.iter(app.world()).next().unwrap().translation.x;
        }
        assert!(saw_run_step && returned_to_walk);
        assert_eq!(app.world().resource::<Clock>().rate(), 1.0);
        assert_eq!(
            app.world()
                .resource::<CreatureSim>()
                .sim
                .as_ref()
                .unwrap()
                .hero_goal_reaches(),
            1
        );
    }

    fn synthetic_simulation() -> Simulation {
        use k1_data::{
            game_state::{Character, GameState, Group},
            map::KonungMap,
        };
        Simulation::new(
            &KonungMap {
                ground: Vec::new(),
                overlay: Vec::new(),
                foot: Vec::new(),
                decorations: Vec::new(),
                objects: Vec::new(),
            },
            &GameState {
                items: Vec::new(),
                groups: vec![
                    None,
                    Some(Group {
                        slot: 1,
                        first_character: 2,
                        size: 1,
                        map_id: 1,
                        kind: 0x53,
                        ..default()
                    }),
                ],
                loot: Vec::new(),
                exits: Vec::new(),
                characters: vec![
                    None,
                    None,
                    Some(Character {
                        slot: 2,
                        x: 5,
                        y: 5,
                        group: 1,
                        state: 4,
                        class_id: 0x46,
                        ..default()
                    }),
                ],
                villages: Vec::new(),
            },
            1,
            None,
            vec![[2; 8]],
            vec![12],
            Vec::new(),
            42,
            None,
        )
    }

    #[test]
    fn creature_layers_keep_the_material_type_across_light_boundaries() {
        use bevy::sprite_render::SpriteMaterial;
        use kn_lighting::classic::{
            ClassicSurface, FineLightGrid, KonungLightMaterial, add_classic_surface_system,
        };

        let simulation = synthetic_simulation();
        let state = simulation.states().into_iter().next().unwrap();
        let mut images = Assets::<Image>::default();
        let mut materials = Assets::<KonungLightMaterial>::default();
        let pool = LightingMaterials::new(&mut images, &mut materials, &[]);
        let ambient = material_handle(&pool, ClassicSurface::Ambient);
        let neutral = material_handle(&pool, ClassicSurface::Moving { cell: None });
        let layer = || LayerData {
            image: Handle::default(),
            size: Vec2::splat(8.0),
            offset: Vec2::new(3.0, 4.0),
        };
        let sequence = Arc::new(Sequence::new(vec![Frame {
            image: Some(Handle::default()),
            size: Vec2::splat(8.0),
            offset: Vec2::ZERO,
            ticks: 1,
        }]));
        let mut app = App::new();
        app.insert_resource(CreatureSim {
            sim: Some(simulation),
            ..default()
        })
        .insert_resource(CreatureSpawns {
            plans: vec![CreatureData {
                id: state.id as u32,
                x: 100,
                y: 200,
                bottom: 208,
                layers: vec![layer(), layer()],
                animations: vec![
                    Some(LayerAnimation {
                        sequence: Arc::clone(&sequence),
                        base: 0,
                        anchor: (3, 4),
                    }),
                    None,
                ],
                shadow: Some(layer()),
                shadow_animation: None,
            }],
            ..default()
        })
        .init_resource::<CreatureDepth>()
        .insert_resource(CreatureNodesVisible(false))
        .insert_resource(pool)
        .add_systems(
            Update,
            (spawn_creature_nodes, update_creature_lighting)
                .chain()
                .before(LightingSet::Apply),
        );

        add_classic_surface_system(&mut app);

        // Even before a grid is supplied, hidden and static layers are born with
        // the custom material, before the renderer can extract a standard one.
        app.update();
        let mut query = app
            .world_mut()
            .query::<(Entity, &CreatureSprite, &Transform)>();
        let sprites: Vec<_> = query
            .iter(app.world())
            .map(|(entity, tag, transform)| (entity, tag.is_shadow, *transform))
            .collect();
        assert_eq!(sprites.len(), 3);
        for &(entity, shadow, _) in &sprites {
            let material = app
                .world()
                .get::<SpriteMaterial<KonungLightMaterial>>(entity);
            assert_eq!(
                material.map(|material| &material.0),
                (!shadow).then_some(&neutral)
            );
        }
        let root = app.world().resource::<CreatureSim>().creature_root.unwrap();
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Hidden)
        );

        for value in [0, 7, 0, 7, 0] {
            let mut grid = vec![vec![0; state.cell.0 as usize + 1]; state.cell.1 as usize + 1];
            grid[state.cell.1 as usize][state.cell.0 as usize] = value;
            app.insert_resource(FineLightGrid(Some(grid)));
            app.update();
            let expected = if value == 0 { &ambient } else { &neutral };
            for &(entity, shadow, transform) in &sprites {
                assert_eq!(app.world().get::<Transform>(entity), Some(&transform));
                let material = app
                    .world()
                    .get::<SpriteMaterial<KonungLightMaterial>>(entity);
                assert_eq!(
                    material.map(|material| &material.0),
                    (!shadow).then_some(expected)
                );
                if let Some(animator) = app.world().get::<Animator>(entity) {
                    assert!(Arc::ptr_eq(&animator.sequence, &sequence));
                }
            }
            // A stable cell must not keep dirtying materials every frame.
            app.update();
            let mut changed = app
                .world_mut()
                .query_filtered::<Entity, Changed<SpriteMaterial<KonungLightMaterial>>>();
            assert_eq!(changed.iter(app.world()).count(), 0);
        }
        let unavailable = sprites.iter().find(|(_, shadow, _)| !shadow).unwrap().0;
        app.world_mut()
            .get_mut::<CreatureSprite>(unavailable)
            .unwrap()
            .creature_id = usize::MAX;
        app.insert_resource(FineLightGrid(Some(vec![
            vec![9; state.cell.0 as usize + 1];
            state.cell.1 as usize + 1
        ])));
        app.update();
        assert_eq!(
            app.world()
                .get::<SpriteMaterial<KonungLightMaterial>>(unavailable)
                .unwrap()
                .0,
            ambient
        );
        let prior: Vec<_> = sprites
            .iter()
            .filter_map(|(entity, _, _)| {
                app.world()
                    .get::<SpriteMaterial<KonungLightMaterial>>(*entity)
                    .map(|material| (*entity, material.0.clone()))
            })
            .collect();
        app.world_mut().resource_mut::<CreatureSim>().sim = None;
        app.insert_resource(FineLightGrid(Some(vec![
            vec![0; state.cell.0 as usize + 1];
            state.cell.1 as usize + 1
        ])));
        app.update();
        for (entity, handle) in prior {
            assert_eq!(
                app.world()
                    .get::<SpriteMaterial<KonungLightMaterial>>(entity)
                    .unwrap()
                    .0,
                handle
            );
            assert_eq!(
                app.world().get::<ClassicSurface>(entity),
                Some(&ClassicSurface::Moving { cell: None })
            );
        }
    }

    #[test]
    fn catch_up_budget_advances_incrementally_and_unlimited_keeps_existing_behavior() {
        let mut sim = CreatureSim {
            sim: Some(synthetic_simulation()),
            ..default()
        };
        let budget = SimulationCatchUpBudget {
            max_ticks: 3,
            max_duration: None,
        };
        advance_to_target(&mut sim, 10, Some(&budget), || unreachable!());
        assert_eq!(sim.sim_ticks, 3);
        assert_eq!(sim.sim.as_ref().unwrap().tick_number(), 3);
        advance_to_target(&mut sim, 10, Some(&budget), || unreachable!());
        assert_eq!(sim.sim_ticks, 6);
        advance_to_target(&mut sim, 10, None, || unreachable!());
        assert_eq!(sim.sim_ticks, 10);
        assert_eq!(sim.sim.as_ref().unwrap().tick_number(), 10);
    }

    #[test]
    fn backward_catch_up_rebuilds_once_and_continues_to_the_same_final_state() {
        let mut sim = CreatureSim {
            sim: Some(synthetic_simulation()),
            ..default()
        };
        advance_to_target(&mut sim, 20, None, || unreachable!());
        let budget = SimulationCatchUpBudget {
            max_ticks: 3,
            max_duration: None,
        };
        let mut builds = 0;
        advance_to_target(&mut sim, 8, Some(&budget), || {
            builds += 1;
            Ok(synthetic_simulation())
        });
        assert_eq!(builds, 1);
        assert_eq!(sim.sim_ticks, 3);
        while sim.sim_ticks < 8 {
            advance_to_target(&mut sim, 8, Some(&budget), || panic!("replayed twice"));
        }
        let mut direct = CreatureSim {
            sim: Some(synthetic_simulation()),
            ..default()
        };
        advance_to_target(&mut direct, 8, None, || unreachable!());
        assert_eq!(
            sim.sim.as_ref().unwrap().tick_number(),
            direct.sim.as_ref().unwrap().tick_number()
        );
        assert_eq!(
            sim.sim.as_ref().unwrap().game_time(),
            direct.sim.as_ref().unwrap().game_time()
        );
        assert_eq!(
            sim.sim.as_ref().unwrap().creature_stage(2),
            direct.sim.as_ref().unwrap().creature_stage(2)
        );
        assert_eq!(
            sim.sim.as_ref().unwrap().creature_stage(2),
            Some(CreatureStage::Special { frame: 8 })
        );
    }

    #[test]
    fn zero_budgets_still_make_progress_and_replay_errors_are_reported() {
        let mut sim = CreatureSim {
            sim: Some(synthetic_simulation()),
            ..default()
        };
        let budget = SimulationCatchUpBudget {
            max_ticks: 0,
            max_duration: Some(Duration::ZERO),
        };
        advance_to_target(&mut sim, 10, Some(&budget), || unreachable!());
        assert_eq!(sim.sim_ticks, 1);
        let time_only = SimulationCatchUpBudget {
            max_ticks: 100,
            max_duration: Some(Duration::ZERO),
        };
        advance_to_target(&mut sim, 10, Some(&time_only), || unreachable!());
        assert_eq!(sim.sim_ticks, 2);
        advance_to_target(&mut sim, 0, Some(&budget), || Err("bad replay data".into()));
        assert_eq!(sim.error.as_deref(), Some("bad replay data"));
        assert_eq!(sim.sim_ticks, 2);
        advance_to_target(&mut sim, 0, Some(&budget), || Ok(synthetic_simulation()));
        assert_eq!(sim.sim_ticks, 0);
        assert!(sim.error.is_none());
    }

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
