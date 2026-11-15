# Game.N — starting state

Characters, squads, caches, transitions and villages placed over Konung.N.
Game.0/1/2 are 709822 bytes in RUS and ENG; ENG also has tutorial TGame.1.
Code: `konung/k1/game_state.py` (`parse`, `serialize`).

```sh
uv run konung k1 game original/k1/rus/game out/k1/game -n 0
uv run konung k1 map original/k1/rus/game out/k1/maps -n 1 --game 0
```

**Confirmed:** block sizes sum to 709822, every cross-reference resolves and all
seven files round-trip byte for byte (`tests/k1/test_game_state.py`). Positions
were checked by rendering. Other field meanings retain their source-only status.
Sources: Slavik `game_state.cpp` (`GameState::Load`, `LoadGameState`), `game.h`
(array sizes), checked against our files.

## File selection

**By Slavik**, a new game reads GAME.<hero>: 0 Wolf, 1 Dragomir, 2 Mikhail
(`game_newgame.cpp:306,329,351`, `game.cpp:2436`) and starts on `3 · hero + 1`,
map 1/4/7 (`game.cpp:2458`). The player group's map field is not used for this:
it is 1 in every Game.N, 4 in TGame.1. Whether real saves use this format is
**unknown**; Slavik does not implement save writing.

**Our experiment:** TUTORIAL.EXE constructs TGAME., TLIGHTS. and TUTORIAL.
filenames. TGame.1's groups, loot, exits and villages occupy Tutorial.4/5 only;
its start is a walkable Tutorial.4 cell (test). The filename's 1 being the hero
index, giving `3 · 1 + 1 = 4`, is a **hypothesis**: consistent with the group's
map field, but tutorial code was not disassembled. APIs:
`game_state.numbers(…, map_file.TUTORIAL_FILES)`,
`load(…, files=map_file.TUTORIAL_FILES)`; render with `--tutorial --game 1`.

The three hero files differ in squads, caches and exits. **Confirmed across
editions:** for the same hero, groups, exits, villages and locations agree;
items, equipment, inventory, character attributes and cache contents differ.

## Layout

No header; consecutive little-endian blocks.

| Offset | Records × bytes | Block | Empty slot |
|---|---|---|---|
| 0x00000 | 8192 × 16 | Items | 16 × 0xFF |
| 0x20000 | 200 × 32 | Groups | Zero |
| 0x21900 | 500 × 64 | Loot | Zero; occupied slots dense from zero |
| 0x29600 | 250 × 17 | Exits | Zero; occupied slots dense from zero |
| 0x2A69A | 2000 × 256 | Characters | Zero |
| 0xA769A | 20 × 1205 | Villages | Zero |

**Confirmed:** every free slot in each block has the same representation.
Loot ends at the first map=0 record; exits at the first target-map=0
(Slavik `game_state.cpp:411-428`).

## Coordinates

All positions are fine-grid cells, 160×320 ([map](map.md)); stored **y before x**.
Rectangles include both boundaries (`game.cpp:887-892`, `game_play.cpp:620-623`).
Character point (Slavik `game.h:1054`, `FUN_00439ba0`):
`(58·x + 29·(2 − (y & 1)), 16·y + 16)`, offset (58,16) from a map object in
the same cell. Pixel fields in character/loot records are zero in every file;
the game derives them at load.

**Confirmed by rendering:** map 1's exit zones sit on portals between stone
idols, arrival cells below them; village workplaces align with doors and the
forge. Starts on maps 1/4/7 fall on walkable village paths (test). Swapping x/y
moves some points outside the map.

## Group — 32 bytes

A character's map comes from its group.

| Offset | Type | Meaning | Confidence |
|---|---|---|---|
| 0x00 | s16 | First member; `size` consecutive characters | **Confirmed:** references agree, no overlapping groups, every member names its group |
| 0x02 | s32 | Respawn countdown | **By Slavik** `game_play.cpp:1326`, `game.cpp:714` |
| 0x06 | s16 | Growth countdown | **By Slavik** `game_play.cpp:1315` |
| 0x08 | 8 × s16 | Interleaved spawn/leash top, bottom, left, right | **By Slavik** |
| 0x18 | u8 | Maximum size | **By Slavik** |
| 0x19 | u8 | Growth limit | **By Slavik** |
| 0x1A | u8 | Target-group index | **By Slavik** |
| 0x1B | u8 | Map number | **Confirmed:** every map exists |
| 0x1C | u8 | Member count | **Confirmed** |
| 0x1D | u8 | AI state: 0 calm, 1 player member inside spawn rect, 0x20 own member outside; 0x80 unexplained | **By Slavik** `game_play.cpp:1024-1062`, `1623-1638`, `1533-1561`; our files only 0/0x80; [behavior](../creatures/behavior.md) |
| 0x1E | u8 | Group kind; zero empty | Empty meaning **confirmed** |
| 0x1F | u8 | Unknown | **Unknown** |

Kinds from Slavik `game.cpp:637-697`, `game_state.cpp:441`, all observed:

- 0x20: player, always slot 0. Spawn top/left give start y/x, bottom gives
  direction (`game_play.cpp:208-231`). **Our census:** all Game.0–2 of both
  editions contain exactly one member, character 0, class 0x5, name_id 124.
  The game assigns hero name indices 10/124/151 by hero number in code
  (`game_newgame.cpp:305-351`); the raw name does not distinguish heroes.
- 0x41–0x50: roaming squads; load scatters members in the spawn rect, most
  having file cell (0,0).
- 0x53: lone NPC groups.
- 0x80–0x82: villagers; low two bits describe the village side.
- All other groups retain member record positions. **Confirmed:** those cells
  are never (0,0), including the player.

Roaming uses the exact range 0x41–0x50 in Python `Group.is_roaming`, Rust
`Group::is_roaming` and Tiled `roaming`. Slavik's `unk5 < 0x51 && !(unk5 & 0x80)`
loop (`game.cpp:651`) skips `_mainMapChar` separately (`638`); its respawn/growth
path instead uses `unk5 - 0x41 < 0x10` (`708`). Our helper has no player exception,
so it uses the range. **Confirmed on all seven files:** the only kinds are 0x20,
0x41–0x50, 0x53, 0x80–0x82. The predicates differ only for three player groups
per edition; no roaming group changes classification (issue #53).

## Character — 256 bytes

| Offset | Type | Meaning | Confidence |
|---|---|---|---|
| 0x00 | u8 | Animation state 0–9: 0/6 idle, 1/7 walk, 2/8 hit, 3/9 death, 4 human shot or dynamic appearance/transformation, 5 melee | **Confirmed:** [animation states](../creatures/animation.md); paired variants differ by byte 0x03 bit 0x04 |
| 0x01 | u8 | Direction 0–7 | **By Slavik** |
| 0x02 | u8 | Orders: 0x01 attack, 0x02 follow, 0x04 shooting, 0x08 move, 0x20 player follower, 0x40 needs decision | **By Slavik**; shooting use confirmed; file census only 0x40/0x60; [orders](../creatures/behavior.md) |
| 0x03 | u8 | 0x04 weapons in hands; 0x40 pure move without attack/pickup; 0x80 dynamic sprite not yet loaded | **Confirmed:** [flags](../creatures/behavior.md) |
| 0x04 | u8 | Class: 0x40 dynamic OBJECTS.RES sprite, kind `(class & 0x3F) − 1`; 0x80 dead. Human low 3 bits occupation, bits 3–5 village job | **By Slavik** `game.h:296-309`, `game_play.cpp:2323` |
| 0x05 | u8 | Group index | **Confirmed** |
| 0x06 | s16 | Enemy reference | **By Slavik** |
| 0x08 | s16, s16 | Cell y,x; ignored at roaming load | **Confirmed** by rendering |
| 0x0C | s16, s16 | Movement goal y,x | **By Slavik** |
| 0x10 | u8 | Frame | **By Slavik** |
| 0x11 | u8 | Walk skip mask: process only if `mask == 0 \|\| (counter & mask) == mask` | **By Slavik** `game_play.cpp:3503`; **confirmed** zero in every checked character |
| 0x12 | u8 | Weapon slot in hands | **By Slavik** |
| 0x13 | 32 bytes | Route, 0xFF terminator | **By Slavik** |
| 0x33 | u8 | Level | **By Slavik** |
| 0x34 | 3 × s32 | Gold, free points, experience | **By Slavik** |
| 0x40 | 9 × s16 | Fencing, trading, accuracy, carpentry, medicine, identification, smithing, armor, poison (`SKILL_NAMES`) | **By Slavik** |
| 0x52 | 2 bytes | Unknown | **Unknown** |
| 0x54 | 9 × s16 | Agility current/base, charisma current/base, health, strength current/base, endurance current/base (`ATTRIBUTE_NAMES`) | **Confirmed** by source and both-edition census |
| 0x66 | 6 × s16 | Equipment item indices | **Confirmed** by rendering |
| 0x72 | 32 × u16 | Inventory item indices | **By Slavik** |
| 0xB2 | 5 × u16 | Accessories | **By Slavik** |
| 0xBC | 10 × s16 | Miscellaneous; ninth at 0xCC is quiver (item type 12) | **By Slavik** `game.h:320`, `game_state.cpp:70`; remainder **unknown**; quiver **confirmed** zero in all 3179 records per edition |
| 0xD0 | u8 | See dialog observations below | **Unknown** beyond source reading |
| 0xD1 | u8 | Flags | **By Slavik** |
| 0xD2 | 2 bytes | Unknown | **Unknown** |
| 0xD4 | u8, u8 | Name and nickname indices into EXE text tables | **Confirmed:** references resolve; [texts](executable.md#texts) |
| 0xD6 | u8 | Human model 0–2 in GRAPH.RES, or dynamic OBJECTS.RES sprite 0–29 for class bit 0x40 | **Confirmed** by rendering |
| 0xD7 | u8 | Frame count | **By Slavik**; **confirmed** zero in all 6358 filled records across both editions; resources supply duration |
| 0xD8 | s32 | Palette byte offset in GRAPH.RES color bank | **Confirmed** by rendering |
| 0xDC | 36 bytes | Pixel positions and image size, derived by the game; zero in files | **By Slavik**, zero census confirmed |

Health is the fifth attribute, s16 at 0x5C
([`game_state.cpp:47`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_state.cpp#L47)),
scale 0…1600; displayed points are `(HP + 15) / 16`. Every filled record is at
1600 except 116 at 1; see [combat measurements](../creatures/combat.md#our-measurements).

### Menu values versus record values

The original new-game menu initializes its own values rather than displaying
the record (`game.cpp:2301-2327`): strength 15, agility 3, charisma 3, endurance
10; fencing 100, trading 10, accuracy 0, carpentry 1, medicine 50,
identification 20, smithing 10, free points 0, equal for all three heroes.
The chosen menu values overwrite loaded fields at start (`game.cpp:2453-2472`).
The raw player record instead has agility 10, trading 64, medicine 101,
identification 13 in every Game.N of both editions; those values are not used
as the menu defaults. **Confirmed** by both-edition census plus source.

Heroes differ in assigned name, base sprite/palette, start map `3 · hero + 1`
and the selected world's Game.N. Menu demonstration equipment
(`game_newgame.cpp:276-365`) must be distinguished from the actual starting
loadout: [new-game research](../new-game.md) records the transfer and our
intentional truthful-preview difference.

## Loot — 64 bytes

| Offset | Type | Meaning | Confidence |
|---|---|---|---|
| 0x00 | 2 × s32 | Derived pixel position | **By Slavik** |
| 0x08 | s16, s16 | Cell y,x | **Confirmed** by rendering |
| 0x0C | s16 | Derived image (`game_loot.cpp:7-25`) | **By Slavik** |
| 0x0E | s16 | Derived index on the map | **By Slavik** |
| 0x10 | s16 | Gold; negative hides the cache | **By Slavik** `game.cpp:1197` |
| 0x12 | s16 | Map; zero ends list | **Confirmed** |
| 0x14 | 22 × s16 | Item references | **By Slavik** |

Chests use the same records, not a separate block.

## Exit — 17 bytes

| Offset | Type | Meaning | Confidence |
|---|---|---|---|
| 0x00 | 2 bytes | Unknown | **Unknown** |
| 0x02 | u8 | Arrival direction | **By Slavik** |
| 0x03 | s8 | Source map | **Confirmed:** all maps exist |
| 0x04 | u8 | Target map; zero ends list | **Confirmed** |
| 0x05 | s16, s16 | Arrival y,x on target | **Confirmed** by rendering |
| 0x09 | 4 × s16 | Zone top,left,bottom,right | **Confirmed** by rendering |

**By Slavik**, entering any zone cell triggers it (`game_play.cpp:613-641`).

## Village — 1205 bytes

| Offset | Type | Meaning | Confidence |
|---|---|---|---|
| 0x000 | 4 × u8 | Counts of two building kinds, elder group, map (zero empty) | **By Slavik**; map **confirmed** |
| 0x004 | 5 × s32 | Healer, smith, commander timers; tribute amount/deadline | **By Slavik** |
| 0x018 | 119 × 8 | Buildings: s8 kind, s8 state, s16 map object, s16 unknown, s16 countdown | **By Slavik**; unnamed component **unknown** |
| 0x3D0 | 5 × 8 | Elder/healer/trader/smith/commander workplace: u8 direction, u8 unknown, s16 character, s16 y,x | **By Slavik** `game_play.cpp:2323-2370`, `game_locale.cpp:202` |
| 0x3F8 | 5 × s32 | Timers | **By Slavik** |
| 0x40C | 3 × s16 | Healer potions | **By Slavik** |
| 0x412 | 25 × s16 | Healer goods | **By Slavik** |
| 0x444 | 32 × s16 | Trader goods | **By Slavik** |
| 0x484 | 23 × s16 | Smith goods | **By Slavik** |
| 0x4B2 | u8,u8,s8 | Flags, unknown, smith-goods count | **By Slavik**; unnamed byte **unknown** |

Buildings refer to Konung.N objects of the same map; their state changes the
image, not placement (`game.cpp:764-780`).

## Item — 16 bytes

**By Slavik** `game_state.cpp:196-210`: s8 type, s8 bonus, s8 special property,
s8 info index; concentration (s32 arrow count for quiver type 12, otherwise
f32); f32 weight; s16 poison; u8 flags (bit 0 unidentified); one **unknown** byte.

Quiver sources: [game.h:320](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.h#L320),
[game_state.cpp:70](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_state.cpp#L70).

## Dense layout and references

**Confirmed on all seven files** (`tests/k1/test_game_edit.py`,
`test_every_state_file_has_the_dense_layout`):

- Groups occupy consecutive slots from zero; each reserves `growth_limit`
  character slots. Group i starts at the sum of preceding growth limits.
- Its first `size` slots are filled, the remainder zero. No characters lie
  outside groups; `size ≤ max_size ≤ growth_limit`; the last reserve ends by 2000.
- Each item has exactly one owner: character equipment/inventory/accessory or
  loot; zero means absent. Additional references are quiver 0xCC
  (`game.h:498`, `game_character.cpp:694-696`) and village goods
  (`game_village.cpp:24,36`), all zero in checked files.

Character references: group's first member; enemy 0x06 (zero none, positive
index + 1, negative map object; all zero in files); workplace character (zero
vacant). Group references: character 0x05, village elder, target group 0x1A
(all zero for targets). Every workplace references an elder-group member.

The original grows into `first + size`: hiring, respawn and growth up to the
limit (`game_quest.cpp:744,1269-1272`, `game.cpp:714`, `game_play.cpp:1316-1322`).
Reserves must therefore exist and be zero. Quests are loaded from QUEST*.RES,
not Game.N (`game_state.cpp:446-490`). Dialog selection uses byte 0xD0
(`game_quest.cpp:207`): most records 0xFF, workers a dialog number; vacant
workplaces have byte 0x01 in 100–103 (**source reading**, not recovered quest rules).

## Editing the roster

`konung/k1/game_edit.py` (`add_character`, `remove_character`) and the Tiled
mirror preserve the layout and rewrite the references above; other bytes stay.

- Removal shifts later group members down, zeroes the freed slot, decreases
  size and releases owned items to 0xFF. An empty 0x53 group is removed;
  following groups and their character reserves shift, as do group/elder/target
  references. Player members and village workers cannot be removed.
- Addition copies a character and its items, including the quiver, with enemy
  zero and dialog 0xFF. A 0x53 NPC with reserve 1 gets a copied group in the
  first free group slot after the last character reserve, reserve 1. Other
  copies append to the existing group. Some shipped 0x53 groups already hold
  2, 14 or 21 members. If needed, the reserve grows by one (byte limit 255)
  and everything after it shifts. All character/group/item capacity checks
  precede mutation; failure changes nothing. Player-group additions are refused.

This editing policy is inferred from the layout and Slavik: **hypothesis**
about compatibility with the original game, where edited files have not been
checked. No shipped group array has an interior hole; edits create none.

## Writing

Serialize in the same block order. Loot/exits are dense from zero, with zero
padding. Empty groups, characters and villages are zero; empty items 0xFF.
Every checked file round-trips byte for byte. Edited-state behavior in the
original remains unverified.
