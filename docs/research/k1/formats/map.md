# Konung.N and LIGHTS.N — location maps

43 maps numbered within 1–50, identical in RUS and ENG. ENG tutorial maps
Tutorial.4/5 and their Tlights.4/5 use the same formats. Code:
`konung/k1/map_file.py` (`parse`, `serialize`), `konung/k1/render.py`,
`tiled/konung/lib/k1.mjs` (`parseMap`, `serializeMap`).

```sh
uv run konung k1 map original/k1/rus/game out/k1/maps -n 1
uv run konung k1 map original/k1/eng/game out/k1/maps --tutorial
```

**Confirmed:** each Konung.N is exactly 128800 bytes, equal to the sum of its
blocks; all tile/object references resolve; serialization returns identical
bytes (`tests/k1/test_maps.py`); renders show recognizable locations without
tile gaps. Source: Slavik `game.cpp` (`LoadGameMap`, drawing), checked on our files.

## Geometry

Staggered isometry:

- **Tile grid:** 80 columns × 160 rows. Tile rhombus 114×64 px, top-left corner
  `(116·x + 58·(y & 1), 32·y)`. Full rendered extent: 9338×5184.
- **Fine grid:** 160×320 for walkability, light and objects. Object position
  `(58·x − 29·(y & 1), 16·y)`, as in Slavik's post-load placement.

## Konung.N

| Offset | Size | Content |
|---|---|---|
| 0 | 160 × 80 × 2 = 25600 | Two bytes per tile: ground and upper layer; GRAPH.RES slot + 1, zero empty |
| 25600 | 320 × 160 = 51200 | Fine-grid walkability bytes |
| 76800 | 1000 × 16 = 16000 | Decorations |
| 92800 | 1000 × 36 = 36000 | Objects |

### Walkability

Rows of 160 bytes. **Confirmed** on all 43 maps of both editions: only 0x00,
0x0F, 0x40 and 0x4F occur (`tests/k1/test_maps.py`). Code:
`map_file.is_blocked`, `FOOT_STOPS_ARROWS`.

| Bits | Meaning | Confidence |
|---|---|---|
| Low nibble = 0xF | Impassable | **By Slavik** `game.cpp:1682-1690`; visually **confirmed** on water, fences, trunks and map edges |
| 0x40 | Stops arrows | **Hypothesis:** Slavik transfers the high nibble into cell flags and stops a flying arrow at flag 0x4 (`game_play.cpp:440`). Marked cells lie at trees/rocks, fences and houses, not water. |

Slavik also treats 0xFF as blocked; it is absent from these files. Its C++
expression `tmpReadMap(i, j) & 0xF == 0xF` tests only the low bit because of
operator precedence, which coincides with the intended nibble test on these
four values. Runtime flags 0x1 (exit) and 0x2 (loot) are added during loading
(`game.cpp:863`, `893`) and do not occur in the file.

Objects have no stored footprint. Our analysis collected blocked cells around
each object kind: isolated trees have a stable 1–2 cells at the trunk, but dense
forests/fences have hand-painted areas independent of object kind. Adding an
object therefore does not automatically add blocked cells.

`uv run konung k1 map … --foot` shows blocked cells red and arrow cells blue.

### Decoration — 16 bytes

| Offset | Type | Meaning | Confidence |
|---|---|---|---|
| 0 | s32 | Zero-based GRAPH.RES slot; negative empty; ≥512 raw | **Confirmed** |
| 4 | 8 bytes | First four `00 XX YY 00` (XX 0x10–0x30, 0xB6, 0xB8; YY 0/1); last four 0 or 0xFFFFFFFF | **Unknown** |
| 12 | s16, s16 | Top-left x, y in map pixels | **Confirmed** |

Decorations draw after both tile layers, in slot order.

### Object — 36 bytes

| Offset | Type | Meaning | Confidence |
|---|---|---|---|
| 0 | s32 | Simple OBJECTS.RES index; negative empty | **Confirmed** |
| 4 | 4 bytes | Usually 0; sometimes `00 C4 01 00`, `00 C8 01 00` | **Unknown**, possibly memory residue |
| 8 | 20 bytes | Flame frames | **By Slavik** |
| 28 | s16 | Remaining frame time, 0 or −1 | **By Slavik** |
| 30 | s16, s16 | Fine-grid x, y | **Confirmed** |
| 34 | u8 | Flags, 0 or 0xFF | Observed values **confirmed**; bits below |
| 35 | u8 | Current frame, 0 or 0xFF; use frame 0 for 0xFF | **Hypothesis** |

Objects draw over decorations: all shadows first, then objects by image bottom,
breaking ties by slot.

## LIGHTS.N

64000 bytes, two row-major blocks, one byte per cell. Absent for maps 6, 8 and
46. Code: `parse_lights`, `serialize_lights`, `lights_fine`.

| Offset | Size | Meaning | Confidence |
|---|---|---|---|
| 0 | 160 × 80 | LIGHTS.RES [tile mask](lights-res.md) index + 1; zero means no light | **By Slavik** `game.cpp:996-1000`; **confirmed** visually around map 1's construction, range 0–19 |
| 12800 | 320 × 160 | Same value for each tile's four fine cells (`fine_cells`); zero on 160 edge cells outside every rhombus | **Confirmed** on all 40 files of both editions, without a mismatch |

**By Slavik:** the second block decides whether a character is lit; zero means
darkened (`game.cpp:1089-1100`, `1611-1626`). It is wholly derived from the
first, so serialization recomputes it and still round-trips byte for byte.

## Writing

`serialize` / `serializeMap` preserve these facts checked on all 43 maps of
both editions and both tutorial maps:

- Occupied decorations/objects are dense from slot zero, except the raw tutorial
  record below. Empty slots are all 0xFF: 16 or 36 bytes respectively.
- Objects have two service-field patterns: flags/frame/frameTime zero and flames
  `00 × 8, FF × 12` (8127 objects), or all 0xFF (1633 objects).
- **Confirmed flag meanings by source plus file/render check:** bit 0 (`& 1`)
  makes an object clickable (Slavik `game.cpp:1447`, `CheckTraceImage`); bit 3
  (`& 8`) subtracts `height / 4` from its sorting bottom (`game.cpp:1391-1395`),
  preventing tall objects covering nearby creatures. Both are set at flags 0xFF.
- The first four decoration-unknown bytes are most often `00 16 00 00`
  (1468 occurrences); Tiled uses that value for added decorations.

### Raw tutorial slot 994

Tutorial.4 and Tutorial.5 decoration slot 994, offset `76800 + 994 · 16`, is
occupied: `ff ff ff 00`, `00 00 00 00`, `ff ff ff ff`, `ff ff ff ff`. It resembles
an empty slot with zero bytes left in the tile number and unknown field:
editor-memory residue is a **hypothesis**. Tile 0x00FFFFFF is nonnegative, so
both our parser and Slavik read it as a decoration (`game.cpp:1696-1706`);
Slavik then indexes `Res.Tiles[0xFFFFFF]` out of bounds (`game.cpp:1017-1018`).
The original game's handling is **unknown**. No main-game map has such a
record (`tests/k1/test_maps.py`).

A tile ≥512 (`TILE_SLOTS`) is **raw** (`Decoration.is_raw`, `isRawDecoration`)
and writes to its own slot. Ordinary decorations use list order in the free
slots from zero, skipping raw ones (`decoration_slots`, `decorationSlots`).
Their order is independent of raw-record position in the list; a raw tile cannot
be drawn or placed through the tileset. With fewer than 994 ordinary records
they remain dense from zero, and adding/removing/reordering them never moves
the raw record. Both tutorials round-trip exactly. Duplicate raw slots or a
raw slot outside 0–999 are write errors.

Objects and ordinary decorations are dense; raw records retain their slots;
unused slots receive 0xFF. Every checked map round-trips byte for byte.
Edited maps have not yet been verified in the original game.
