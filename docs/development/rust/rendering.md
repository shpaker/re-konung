# Bevy scene rendering

Bevy 0.20 renders the common kn-game scene. Window/GPU access is needed for
interactive rendering, not the headless logic tests. Original data and rules
arrive through the facade; [architecture](../../../ARCHITECTURE.md) owns boundaries.

## Static sprites and animation

- Map pixels have y down and a top-left origin; Bevy sprites use centers and y
  up. A piece `(x,y,w,h)` maps to `(x+w/2, −(y+h/2))` (`piece_transform`).
  Negative positions remain valid, matching Python render.blit.
- Transparent sprites sort by ascending z (`bevy_sprite_render` uses
  FloatOrd(transform.z)); piece n gets `n · Z_STEP`.
- TextureCache creates one Image per TextureId. A map's roughly 6000–13000
  pieces share about 250–300 base images, up to about 500 with animation frames.
  Nearest filtering preserves pixel art.
- Animation uses Sprite/Handle<Image> with a Sequence, not bevy_animation's
  reflected property curves. Bevy's own sprite-sheet example similarly updates
  frames manually. One Clock per map counts ticks; sub_tick carries fractional
  frame time (essential with 60 ms ticks on a 120 Hz display).
- Playing advances, Paused holds, Off zeroes the clock and shows the base.
  Sprite fields change only on a frame change. piece_center recomputes the
  center for the new image size, using the same formula as static pieces;
  different-sized K1 frames must not shift their top-left anchor.

## Visibility and bounds

MapRoot → SceneGroup → sprites forms the hierarchy. Roots and groups need
Transform and Visibility: parent propagation reaches children only through a
rooted Transform chain; sync_simple_transforms skips ChildOf entities. Missing
transforms leave entities present but not rendered.

Sprites inherit visibility, except empty frames which are Hidden. Animators
restore Inherited rather than Visible, which would override a hidden group.
The viewer builds SceneOptions::ALL once and changes a group's visibility;
Rebuild is only for map changes. The facade defines which group follows which
overlay; tree adjacency never replaces draw-list z-order.

SceneGroup stores bounds including all animation frames. sync_groups rebuilds
CameraBounds from visible groups: fine-grid overlays extend past rendered tiles.
It preserves camera position/zoom; only a new map calls frame_scene. Both group
bounds and scene_bounds use piece_bounds, checked by
`group_bounds_cover_the_pieces_a_layer_shows`.

## Camera and input seam

ZOOM_LEVELS is discrete, scale = 1 / zoom, preserving pixel alignment at whole
zoom values. kn-input owns pan_direction, drag_delta, wheel_steps and ZoomScroll;
kn-ui owns pointer_over_ui. Each frontend binds input in CameraInput; the render
plugin orders CameraApply afterwards: apply_pending_frame then apply_zoom.
The binding is about 20 lines; computations and constants have one owner.

A wheel notch or about 60 pixels of trackpad swipe advances one step.
ZoomScroll.push accumulates; take emits at most MAX_ZOOM_STEPS_PER_FRAME,
carrying the remainder to later frames. ZOOM_ACCUM_LIMIT bounds excess. Above
UI, discard the whole queue so a gesture cannot zoom later when the pointer
leaves the panel. Pointer capture reads Hovered's boolean value, not merely the
component's presence; panel roots also carry it to cover gaps between controls.

frame_scene uses content bounds rather than the whole 80×160 grid, chooses
fitting_zoom and defers to PendingFrame while the camera spawns. Applying the
pending frame before zoom reaches the projection in the same update
(`a_framed_scene_reaches_the_projection_on_the_same_frame`). Pan is WASD/arrows
or left drag outside UI. These mechanics implement the
[product principles](../design.md); user controls are in the
[viewer guide](../../guides/viewer.md).

## Creature nodes

A creature is one stable kn_render::CreatureNode under CreatureRoot. It owns
world position; body/equipment/shadow sprites carry local offsets and frame
deltas. sync_creature_nodes is the only writer of world position. A second
independently moving sprite model would double-count motion; the former
CreaturePiece/update_shadow_depth path was removed for that reason.

- **The pipeline is one plugin in `kn-render`, and both windows add it (issue #70).** `CreatureAnimationPlugin` owns the resources (`CreatureSim`, `CreatureAnim`, `CreatureSpawns`, `CreatureDepth`, `CreatureMotion`, `CreatureAnimReport`, `CreatureNodesVisible`), the five systems and the ordering between them and the scene clock, so a frontend gets the whole thing by adding the plugin and cannot mis-order it. A frontend asks for a map with `CreatureMotion::request(map, seed, scene)` and provides what any window drawing this game has anyway — `GameResource`, `TextureCache`, `Assets<Image>` and the `Clock` of `AnimPlugin`; the subsystem adds no clock of its own. The viewer's `#id` labels and its diagnostics panel are its own additions, attached after `CreatureSet::Nodes`; the player's whole creature wiring is one request (`kn_fe_player::request_start_creatures`, same seed as the viewer's "Reset positions", so one map spawns the same way in either window). Before that request existed the player drew no creatures at all: `spawn_scene` skips the pieces that carry a `creature_id`, so the composite nodes were simply never built there — covered by `crates/frontend/bevy/player/tests/creatures.rs`.
- **The node owns the world, the children own the frame.** The node gets
  `x, −y` from the simulation and its depth z; `apply_animators` writes each animated
  child's centre relative to that child's own base offset (`Animator::with_base`), never
  a world centre, so the two no longer double-count a move. The node carries no
  `Animator`, so a frame change resizes and repositions a sprite *inside* the creature
  but cannot translate the creature in the world
  (`an_animation_frame_change_does_not_move_the_node_world_position`,
  `body_equipment_and_shadow_ride_the_node`). Both root and node carry a `Transform`
  because `propagate_parent_transforms` reaches children only down a chain rooted at a
  `Transform`.
- **The node, sprite and label queries are provably disjoint.** `sync_creature_nodes` moves the
  node and its shadow in one system, both taking `&mut Transform`, so its node query carries
  `Without<CreatureShadow>` *and* `Without<CreatureSprite>` — the sprite transforms are what the
  animation pass writes. The `#id` labels are the viewer's own (`sync_creature_labels`, riding the
  body image rather than the cell), and that query carries `Without<CreatureSprite>` *and*
  `Without<CreatureNode>` for the same reason, because Bevy otherwise rejects the overlap with a
  `B0001` panic that crashed the running viewer before anything moved (`creature_node_and_label_queries_are_disjoint`).
- **Layers keep their identity by `LayerKey`, not by position.**
  `kn_game::CreatureClips` hands each layer as `CreatureLayer { base, animation, corner, key }`:
  a layer carrying its resting image, its frames, the absolute corner its base frame was
  composed at and the sprite part it draws (`Pose::layer_keys` in `k1-game` carries the key
  out of the pose build). Within one pose the subsystem still builds the node by the layer's
  place in the list — scene pieces and resting clips come from the same build and agree
  (`the_rest_clips_are_anchored_where_the_scene_draws_the_layer`). Across poses the match
  is by `key` (`pose_matched_layers`): track order is first-appearance *per pose*, so a
  walk pose may list the same layers differently and an index join bolts one part's walk
  clip onto another's piece (`walk_layers_join_to_their_part_not_their_position`). Body
  and equipment keep the scene's pose order in their local z, so equipment reads over the
  body (`body_layers_keep_their_scene_order_depth`).
- **A clip swap re-anchors on the clip's own base corner; the old anchor was the jump.**
  Every clip's frame `dx/dy` are offsets from *that clip's* base-frame corner
  (`layer_track` in `k1-game`), and the rest and walk poses of one creature are composed
  around different corners. Swapping the sequence while leaving the `Animator` on the
  resting corner rendered the new clip displaced by the corner difference — the periodic
  one-cell image jump at every direction or rest/walk change. `Animator::play_at` now takes
  the anchor and moves it with the sequence (`apply_clip` reduces the clip's absolute
  corner to the creature's local coordinates); the frame change and the anchor change are
  one atomic write (`play_at_reanchors_the_piece_on_the_new_clip_corner`,
  `a_clip_swap_reanchors_the_image_and_never_moves_the_node`,
  `apply_clip_reanchors_on_the_clips_own_corner`). The node is never touched by a swap —
  the jump was always the image, never the world position.
- **A one-frame resting layer would block the walk swap — a structural gap with no data hit.**
  `layer_track` builds an `animation` only for multi-frame tracks and `clip_seq` requires
  one, so a layer resting on a single frame spawns without an `Animator`, and
  `sync_creature_animation` — which queries `&mut Animator` — could never swap a walk
  clip onto it. Scanned both editions over every map: no creature has a static rest layer
  whose same-`LayerKey` walk layer is animated (body or shadow), so nothing is patched
  blindly; the chain is recorded here so a future format surprise finds the gap named.
- **Depth comes from the static stacks only, and a creature's shadow rides its slot.**
  `DepthMap` is built from `PieceGroup::Objects` bottoms (`is_depth_relevant` matches only
  `Objects`), never from characters, terrain or markers — those must not decide what a
  creature stands behind. Flames are excluded too: a fire is drawn together with its
  object, and as a contributor its higher bottom would put a creature on the fire's row
  in front of the object it burns on (rus map 1 had exactly one such inversion).
  Equal bottoms collapse to the highest sort key, so two statics sharing an edge resolve
  to the one drawn on top; a contributor lower than the previous boundary adds nothing
  and never drops it — the search and the interpolation both assume z grows with bottom
  (`depth_map_uses_only_static_object_stacks`, `equal_bottoms_collapse_to_the_highest_sort_key`,
  `depth_boundaries_stay_monotonic_on_every_map`,
  `flame_pieces_are_recognised_as_part_of_their_object`).
  A creature then occupies the slot between two boundaries (`DepthMap::creature_slot`):
  the shadow sits above the statics under its feet and strictly under the body — the
  original draws the pair together, while the earlier fixed band below the whole map
  buried the shadow under the ground. Body and every layer stay under the next boundary
  at any bottom: an interpolated base reserves the stack's room, so a stack spawned in
  one slot never draws over the next static after moving into another — the layer
  offsets are never rewritten (`the_slot_orders_shadow_body_and_layers_inside_the_boundaries`,
  `a_narrow_slot_keeps_every_layer_under_the_next_static`,
  `production_spawn_places_the_shadow_in_its_slot`,
  `a_reset_returns_nodes_animators_shadows_and_labels_home`).
- **The visual step is offset by one tick; the logical tick is not.** A walk step
  commits at `step_elapsed == 1`, which is faithful to the original (Slavik increments the
  frame counter before the first render, so the first *visible* frame is index 1) and is
  kept as the logical contract the engine and tests see. The bug was in the display
  interpolation, which read that `1` as "one tick already spent" and jumped a whole step
  forward the instant a step began. `smooth_pixel` maps visual progress to
  `(step_elapsed − 1 + sub_tick)`, clamped to `[0, step_ticks]` — continuous at both step
  boundaries (`one_tick_into_a_four_tick_step_sits_a_quarter_across`, and the
  1/2/4-tick continuity test `movement_is_continuous_across_step_boundaries`).
- **The simulation's own direction is single-valued per step; the display is what bounced.**
  A creature carries one `direction` for the whole step, so it never flickers between
  neighbours on its own — the earlier left/right jitter was purely the interpolation above,
  now fixed. To tell the two apart at runtime, `KN_CREATURE_DEBUG=1` logs per creature id:
  the sim tick, cell, step from/to, `step_elapsed`, the interpolated pixel, the rendered
  node position and its `GlobalTransform`; the viewer's `sync_creature_labels` adds every
  sprite's chosen clip — named against the clips the subsystem built (`clip_identity`) —
  its base corner, anchor, phase and shown frame with its local offset and the world
  position as it will be drawn. Comparing the sprite's world with the node's separates a
  node jump from an image jump without guessing. The logging is the viewer's own: the
  shared subsystem logs nothing, so `KN_CREATURE_DEBUG` stays a diagnostics feature
  rather than something the player inherits.
- **The creature pipeline is ordered by its own plugin, not by the scheduler.**
  `CreatureAnimationPlugin` registers its systems in `CreatureSet::Prepare` → `Advance` → `Spawn`
  → `Swap` → `Nodes`, chained, with the first four `after(AnimSet::Clock)` and
  `AnimSet::Sprites` configured between `CreatureSet::Swap` and `CreatureSet::Nodes`: the clock
  settles, the simulation catches up to it, clips are chosen, sprites are written, then the nodes
  are placed — never one frame stale by scheduler accident, and never a clip applied after the
  nodes were read. A frontend that listed the systems itself could break that order, which is why
  the chain lives in the plugin both windows add; the viewer only pins its own chain
  `.after(AnimSet::Clock).before(CreatureSet::Prepare)` and hangs its labels after
  `CreatureSet::Nodes`.
- **A map change and a reset both pin the epoch; a backward `Step` rebuilds and replays.**
  `rebuild_scene` (through `CreatureMotion::request`) and `reset_positions` (through
  `reset_creature_sim`) pin `CreatureSim::epoch` to the current `Clock.ticks`
  so a fresh simulation does not re-run the whole tick history in one frame
  (`map_change_pins_the_epoch_so_history_is_not_replayed`). The engine has no reverse
  tick, so `advance_creature_sim` rewinds by rebuilding the map's simulation and replaying it
  forward to the epoch-relative target; the replay already lands at `clock.ticks`, so the
  epoch is pinned there — pinning it to 0 let the catch-up loop tick the fresh simulation
  a second `clock.ticks` times and every creature overshot
  (`a_backward_step_rewinds_the_simulation_to_the_clock_not_beyond_it`,
  `replay_simulation_matches_forward_stepping`). The state machine lives inside the
  simulation and moves once per `tick`, so a rewound creature — a creature in the middle of
  its state-4 special included — shows exactly the state of the tick it landed on, with no
  reversible simulation (#70, `rewinding_the_clock_replays_to_the_positions_a_forward_run_reached`).
  Measured cost of the worst
  rewind — a 30 000-tick (30 game-minute) session on the real first map, full
  rules and engine per tick: 306 ms release (10.2 µs/tick), 4.3 s debug, on an
  Apple M-series (`replaying_a_long_session_stays_a_bargain`, release numbers
  from `cargo test --profile release`). A rewind is a one-off after the press,
  not a per-frame cost, and a reset pins the epoch so rewinding just past it
  replays almost nothing.
- **A pose rebuild is paid per state change, never per frame (issue #70 AC6).**
  The state-4 special is a per-creature state machine in the game layer
  (`k1_game::creature_anim::Machine`) that owns no clock; the subsystem reads its
  stage each frame and rebuilds a creature's pose — the clip swap in
  `sync_creature_animation` — only when the stage *kind* flips (Special ↔ Idle),
  never when a plain tick advances the frame counter inside a running special
  (`the_stage_snapshot_rebuilds_only_on_a_state_kind_flip`,
  `counted_run_shows_one_rebuild_per_state_change_and_none_per_frame`). The
  bound is measured over the real shipped population rather than eyeballed:
  across every `Game.0`–`Game.2` of both editions — all 404 state-4 records
  (249 Skeleton, 152 Eechetik, 3 Ion) — a full map load plus the ticks that
  finish every sequence count exactly two rebuilds per creature (entering the
  special, and its Special → idle edge), and a thousand further idle ticks
  count zero (`pose_rebuilds_count_one_per_state_four_creature_and_no_more`).
  The comparison that matters is against the per-frame path: a creature sitting
  in its idle pose pays no rebuild no matter how long it stands, so the cost is
  O(state changes) per creature, not O(ticks).
Creature nodes hang on the subsystem's `CreatureRoot`,
  born with the visibility the frontend asked for through
  `CreatureNodesVisible`, and the viewer's `#id` labels on its own
  `LabelRoot`; the viewer's `sync_creature_visibility` drives both roots
  from one query, and both are born with the Characters overlay's
  visibility — a fixed `Visible` showed the creatures for the frame
  before `sync_creature_visibility` caught up after a map change
  (`roots_are_born_hidden_when_characters_are_off`); while the overlay is off the roots
  are `Hidden`, so turning creatures off also hides their names.

## Layer-switch cost

Recorded on ENG map 17, release, Apple M4 Pro: 12501 pieces with all layers,
7180 without overlays. This measures ECS/visibility propagation, without renderer.

| Operation | Time |
|---|---|
| Draw-list parse, all layers / clean map | 26 ms / 18 ms |
| Despawn root + spawn 12501 sprites, excluding parse | 6.4 ms, worst 6.5 ms |
| Change one visibility node for 5131 sprites | 0.15 ms, worst 0.20 ms |
| Panel count over 12501 InheritedVisibility values | 0.5 µs/frame |

The old overlay switch parsed and rebuilt, about 30 ms in one update
(38–43 ms with rendering in #30). The current switch does neither; opening map
17 instead pays 8 ms extra and retains 12501 rather than 7180 entities. Hidden
groups have false ViewVisibility and are not extracted, but remain in ECS.
The panel scans sprites rather than summing group children: it must exclude
empty animation frames, and the 0.5 µs count is cheaper than a 150 µs switch.
