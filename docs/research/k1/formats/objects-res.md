# OBJECTS.RES

52 MB, identical in RUS and ENG. Map objects include trees, houses, fences and
rocks. Code: `konung/k1/objects_res.py`, `tiled/konung/lib/k1.mjs`
(`readSimpleObject`).

**Confirmed:** every simple object referenced by all 43 maps of both editions
exists and renders in place; all 30 dynamic objects parse to their last byte
and render on maps. Sources: Slavik `resources.cpp` (`LoadObjectsRes`,
`SimpleObject::LoadInfo`, `DynamicObject::LoadInfo`), checked on our files.

## Structure

| Offset | Type | Meaning |
|---|---|---|
| 0 | 8 bytes | Skipped; purpose **unknown** |
| 8 | 512 × (s32, s32) | Directory: offset from directory end, size; negative offset means empty |
| 4104 | | Object data |

Slots 0–29 are dynamic, 30–511 simple. A map object number is the simple-object
index, or directory slot − 30. There are 30 + 463 occupied entries.

## Simple object

| Offset | Type | Meaning | Confidence |
|---|---|---|---|
| 0 | u32 | Flags: 0, 1, 8, 9 | Meaning **unknown**; **by Slavik**, bit 8 raises the sorting bottom by one quarter of image height |
| 4 | u32 | Block size | **Confirmed** |
| 8 | s32 | Flame count, up to 20 | **By Slavik** |
| 12 | u32 | Frame count, 1–6; 457 objects have one | **Confirmed** |
| 16 | u32 | Always `0xFFFFFFFF` | **Unknown** |
| 20 | u32 | Byte offset of the palette in GRAPH.RES | **Confirmed** |
| 24 | s32 | Frame time | **By Slavik** |
| 28 | 20 × (s8 id, s16 x, s16 y) | Flame kind 0–6 and frame corner relative to object | **Confirmed** by map 17's burning houses; [flames](flames-res.md) |
| 128 | 8 × (s32, s32) | Shadow/image offsets from data start; negative means absent | **Confirmed** |
| 192 | 8 × (2 × s16, 2 × s16) | Shadow/image offsets from object position | **Confirmed** |
| 256 | | RL8 images and shadow masks | **Confirmed** |

## Dynamic objects

Monsters, village women and children use character class bit 0x40; the index is
`Character.sprite_set` ([starting state](game-state.md)). `graph_res.read_sprites`
uses the GRAPH.RES sprite structure without equipment offsets.

| Block offset | Size | Content |
|---|---|---|
| 0 | 8 | u32 tag `0x02000000` (meaning **unknown**), u32 block size including header, equal to directory size |
| 8 | 48 × (s16 + 18 × s16) | Six animations × eight directions: count and frame image indices |
| 1832 | 512 × (s32, s32) | Shadow/image offsets from data start; negative means absent, zero also means no shadow |
| 5928 | 48 × 18 × (2 × s16, 2 × s16) | Per-frame shadow/image offsets from character point |
| 12840 | | RL8 images and shadows |

**Confirmed on both editions:** the last image ends at the block boundary and
every animation frame has an image. Palette comes from character field 0xD8.
Slavik maps states 6–9 to 0–3 (`game_character.cpp:144-150`), the variants
without a weapon in hands; see [animation states](../creatures/animation.md).
Slots 24–29 match slot 0's size and may be copies — **hypothesis**.
