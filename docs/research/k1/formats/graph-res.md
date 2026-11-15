# GRAPH.RES

Palettes, cursors, ground/decorative tiles and character sprites; about 14.5 MB.
Code: `konung/k1/graph_res.py`, `tiled/konung/lib/k1.mjs` (`parseGraph`).

```sh
uv run konung k1 tiles original/k1/rus/game/GRAPH.RES out/k1/tiles
uv run konung k1 sprites original/k1/rus/game out/k1/sprites
```

The second command exports each model, equipment and OBJECTS.RES dynamic
object's first frame per animation and all eight directions.

**Confirmed:** parsing reaches EOF in RUS game/, ENG game/ and the disc copy;
tiles were inspected visually. Source: Slavik `resources.cpp` (`LoadGraphRes`),
checked on our files.

## Structure

Little-endian; consecutive blocks.

| Block | Size | Content |
|---|---|---|
| Color bank | 65536 × u16 = 131072 bytes | RGB555; an image palette is 256 consecutive colors |
| Cursors | 11 blocks | Block header plus 32×32 RL16 image |
| Tiles | 1 block | Header, 512 directory entries, RL8 images |
| Characters | 3 + 24 blocks | Three base models and 24 equipment sets |

### Block header

| Offset | Type | Meaning |
|---|---|---|
| 0 | u32 | Tag: cursors 0x05000000, tiles 0x01000000, characters 0; purpose **unknown** |
| 4 | u32 | Size |

**Confirmed size origins:** after the header for cursors; after the directory
for tiles (image data only); from block start for characters (including header).

### Tile directory

512 × 8 bytes: s32 image offset from directory end, s32 palette **byte offset**
from color-bank start (color index = value / 2). Either negative means empty.
305 slots are filled (0–304): 221 tiles of 114×64 are ground rhombi (grass,
sand, rock, flooring, water); the rest are logs, bushes, skeletons, cliffs and
walls of varying sizes. Maps store the slot, plus one in their tile layers.

### Character block

Three human models (`Character.sprite_set` in [Game.N](game-state.md)) and
24 equipment sets. Code: `graph_res.read_sprites`, `konung/k1/characters.py`.
**Confirmed on all 27 blocks of both editions:** every image decodes, the last
ends at the block boundary, every animation frame has an image. Source:
Slavik `resources.cpp:20-102` (`CharacterSprites::Load`).

| Block offset | Size | Content |
|---|---|---|
| 0 | 8 | Block header |
| 8 | 80 × (s16 + 12 × s16) | Ten animations × eight directions: frame count and image indices |
| 2088 | 80 × 12 × (s16, s16) | Equipment offset from character point per frame |
| 5928 | 768 × (s32, s32) | Shadow/image offsets from data start; negative means absent, zero also means no shadow |
| 12072 | 80 × 12 × (2 × s16, 2 × s16) | Per-frame shadow/image offsets from character point |
| 19752 | | RL8 images and shadows |

Animations 2 and 8 have seven frames; all others twelve. **Confirmed:** pairs
0/6, 1/7, 2/8, 3/9 have matching counts in every block/direction
(`animations_six_to_nine_mirror_zero_to_three`). States 6–9 are the variants
without weapons in hands. Slavik selects by byte 0x03 bit 0x04
(`game_play.cpp:305-308`, `2465-2468`, `1964-1967`, `3944-3947`), and maps
6–9 to 0–3 for dynamic objects (`game_character.cpp:144-150`). Our file census
finds the bit set exactly for states below 6; see [animation](../creatures/animation.md).

Frame composition (Slavik `game.cpp:1087-1110`, `game_character.cpp:131-241`),
**confirmed by rendering**:

1. Draw the base with the character's palette: field 0xD8 / 2 indexes the color
   bank. Position comes from [Game.N coordinates](game-state.md#coordinates)
   plus the frame's image offset.
2. Iterate Character.equipment in direction order. Item.info_id selects the
   equipment set through the [EXE table](executable.md).
3. Use the same animation, direction and frame. Add the base frame's equipment
   offset and the equipment frame's image offset to the character point.
4. Equipment uses its own palette. Its indices were not found in EXE data;
   they come from Slavik `resources.cpp:15`. Index × 256 gives the bank position.

**By Slavik**, `game_play.cpp:3729` advances one frame per 60 ms tick
(`game.h:30`); direct in-game timing was not checked in this format study.
Code: `characters.animation`, `characters.frame_count`. Later behavioral
measurements and play modes are in [creature research](../creatures/animation.md).

Class bit 0x40 selects an OBJECTS.RES dynamic sprite (monsters, women, children),
without equipment. Slavik's armed path uses different slot-order tables and
sometimes equipment set + 1 (`game_character.cpp:167-169`). The reference
composition currently uses the basic order; that implementation limitation is
separate from the recorded weapon-bit census. An earlier claim that only
dynamic sprites carry the bit contradicted that census and is not retained.

## Open questions

- Some decorations (skeletons, pots, columns) appear purple: copies of the same
  images with another palette. Whether the game changes the palette offset or
  substitutes colors dynamically is **unknown**.
- RUS game/GRAPH.RES is 22 bytes larger: one cursor differs; the rest matches.
