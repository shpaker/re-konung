# Data loading and scene assembly

[Architecture](../../../ARCHITECTURE.md) owns the crate boundaries. Binary
layouts and their confidence live in [K1 formats](../../research/k1/README.md#format-status).
Python remains the reference for parser and rendering parity.

## Loading and parsing

`k1-data::repo::GameDir` locates files case-insensitively, enumerates maps and
states, and reads bytes. `FileSet` selects the main game or tutorial. Parsers
and serializers take bytes/values without touching the filesystem. Map and
starting-state serialization is byte-exact; graphics are read-only.

`game_edit` takes record indices rather than borrowed records, so a caller does
not hold a borrow while mutating the state. It preserves dense reserves and
rewrites references; [Game.N](../../research/k1/formats/game-state.md#editing-the-roster)
owns the documented editing policy and its unverified in-game compatibility.

Format tests substitute bytes, not provider implementations. Traits for mocks
are declared by a consumer when needed (accept traits, return structs); k1-data
has no speculative provider traits. GameDir's concrete return types and &self
methods leave the boundary usable without forcing an abstraction now.

Malformed input is rejected before unsafe sizes reach subtraction/allocation:
RLE bounds dimensions by MAX_DIMENSION and checks skips against row width;
GRAPH.RES checks tile palette ranges; OBJECTS.RES limits flames to 20 slots.
The RLE skip guard is mirrored in Python and Tiled.

GRAPH.RES and OBJECTS.RES borrow their buffers. Resources owns the bytes and
parses them inside with_graphics; owning EXE tables, masks and flames are parsed
once. `k1_game::scene::Graphics` holds borrowed tables throughout a pose batch.

## Draw list

The scene follows `konung/k1/render.py`: ground → upper layer → drawable
decorations → shadows → objects/flames and creatures, sorted by image bottom
with stable `(bottom, kind, slot)` ties → light, walkability and Game.N markers.
Flames stay above their parent object. Raw decorations are retained in data but
cannot draw. The scene has no text; viewer UI supplies marker labels. Marker
rings approximate PIL ellipses of width 3.

## Piece groups

Every piece carries PieceGroup, exposed through kn-game: ground, decorations,
map shadows, objects, creatures, creature shadows, light masks, walkability and
markers. Grouping controls visibility only; draw-list position still defines
z-order and groups' pieces can interleave. PieceGroup::ALL defines node creation
order. `PieceGroup::is_shown_by` and facade `group_is_shown` own the
SceneOptions→group mapping. Creature shadows follow the Characters overlay.

A full scene filtered by groups equals a scene built for only those layers.
`group_filter_matches_a_scene_without_the_layer` checks all maps/editions;
`pieces_carry_their_group` checks facade preservation; renderer
`every_group_of_a_real_map_gets_its_own_node` checks nodes/z/inherited visibility.
The headless check report keeps its own layer selections and statistics.

## Frames and animation

Piece.animation holds one layer's texture, corner delta and duration in ticks;
Piece.texture is the base frame. K1's tick is 60 ms
(`k1_data::objects_res::TICK_MS`, Slavik game.h:30). The format study did not
verify animation timing in the original game; retain that confidence distinction
from later [creature-state research](../../research/k1/creatures/animation.md).

- Flames use the inclusive EXE range, one tick per frame. Simple OBJECTS.RES
  objects use frame_count and frame_time; multiple frames without frame_time
  do not animate. Creatures compose every frame of the state/direction sequence.
- The base is the record's frame. Draw order is calculated from that frame once;
  turning animation off reproduces Python's static image.
- A missing image in a layer's frame remains an empty frame, hiding the sprite;
  dropping it would shift timing. LayerKey (Body, Shadow, Gear { slot }) joins
  layers despite different sequence ordering or missing parts.
- Identical durations/base and one map clock keep composite layers synchronized.
- Scene::texture_ids includes every animation frame, so missing textures fail
  headless checks before mid-animation. The recorded ENG check used about 450
  unique textures per map versus about 300 static and took about 5 s in release.

The selected map source also chooses starting state: Game.N for game, TGame.N
for tutorial. Sharing one state would place main-game characters on tutorials
(recorded ENG counts 17/5 instead of 8/2). A map error stays in that map's report
and does not stop other checks. Empty slots differ from failed texture decodes,
which retain their cause (corrupt encoding or palette outside the bank).

## Pose cost

Release build, Apple M4 Pro, `konung.1` of both editions (25 creatures), the spread
of three runs, measured by the `k1-game/tests/pose_cost.rs` test under `#[ignore]`:

```bash
cargo test --release --manifest-path crates/Cargo.toml -p k1-game \
    --test pose_cost -- --ignored --nocapture
```

| Operation | Time |
|---|---|
| parsing `GRAPH.RES` and `Objects.res` | 16–25 ms |
| one pose: the layers and frames of one state and direction | 57–61 µs |
| rebuilding a pose when state or direction changes | 66–72 µs |
| every creature of the map in its resting pose (25 of them) | 0.95–1.06 ms |

**Decision (issue #36, whether lazily built poses are affordable):** a pose is built
when it is needed and kept until the creature changes its state or direction. One
assembly is a hundredth of the 60 ms tick, and a whole map of creatures costs 38–42 µs
per creature, so preloading every state and every direction for a map would multiply
the work by dozens to show what nobody asked for. The part worth caching is the
parsed resources, not the poses: one parse is worth 250–450 poses, which is why
`Resources::with_graphics` hands the tables to a closure instead of returning them,
and `k1_game::scene::Graphics` holds them while poses are built.
