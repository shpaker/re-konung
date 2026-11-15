# KONUNG.EXE — embedded tables

Some data lives in the executable: RUS konung.res (PE, renamed by the installer)
and ENG KONUNG.EXE. Code: `konung/k1/executable.py`, Rust
`crates/game/k1/k1-data/src/executable.rs`; Rust does not parse the item-name
tables. Tables are located by structure rather than fixed offsets, which vary
by edition. Equipment needs two tables, flames a third; text tables supply
names and menu descriptions. See [characters](../creatures/README.md).

**Confirmed:** the three graphics tables are found uniquely in both editions
(`tests/k1/test_graphics.py`), equipment aligns with models and flames with
objects. We searched independently using Slavik's `WeapArmorItemInfo`
(`game.h:735-748`) and `EqLookUp1` (`game_data.cpp:115`) as structural references;
values come from the EXE, not copied reference tables.

## Armor and weapon descriptions

70 consecutive 64-byte records, indexed by Item.info_id ([Game.N](game-state.md)).

| Offset | Type | Meaning | Confidence |
|---|---|---|---|
| 0 | 44 bytes | Zero-padded name, ENG Latin / RUS CP866 | **Confirmed** |
| 44 | 9 × s16 | Damage and other numeric properties | **By Slavik**; most meanings **unknown** |
| 62 | s16 | GRAPH.RES equipment set 0–23; −1 means not drawn | **Confirmed** by rendering |

Slavik uses 32-bit numbers and a leading index; the file instead uses 16-bit
numbers after the name, without an index. Search for 70 records with nonempty
names followed by zeros and a final set in −1…23. Starts: ENG 0x40F88, RUS
0x3FD50. Records 42/43 exchange sets between editions: 13,16 versus 16,13.

## Equipment slot order

72 rows of six s32 values, each a permutation of equipment slots 0–5. Search
for all 72 consecutively: ENG 0x4A034, RUS 0x48DFC. The last eight rows give
direction order without weapons in hands (`EqLookUp1`); reference composition
uses them. The first 64 are two 4×8 armed sets (`EqLookUp2`, `EqLookUp3`), not
yet used by that composition path.

## Flame animation ranges

Seven pairs of s32: inclusive first/last [FLAMES.RES](flames-res.md) frames.
Consecutive ranges cover 0–73 without gaps: 0–10, 11–21, 22–32, 33–43, 44–54,
55–59, 60–73. That structure uniquely locates ENG 0x422A0 / RUS 0x41068.
**Confirmed** by map 17's burning-house render.

Slavik stores these in `game_data.cpp:479` but wraps at the last index
(`game.cpp:1596`), skipping the final frame. We include it because the table
assigns all 74 frames to a kind; fidelity of that choice remains a **hypothesis**.

## Texts

Fixed-size text fields followed by zeros, no pointer arrays: base + index ×
record size. RUS is CP866; ENG is Latin except one name byte interpreted as
CP866. Text is copyrighted content: display it locally, never embed it in
fixtures, documentation or reports.

Code: Python `parse` (`Texts`, `items`), `character_name`, `item_name`; Rust
`Texts`, `parse_texts`, `character_name`. Graphics tables remain usable without
texts. Rust's `Executable::texts_error` preserves the ambiguity reason so
not searched, missing and ambiguous are distinguishable from `texts == None`.

### Recognition and parity

A text record in Python, Tiled and Rust must have at least one byte before the
first zero, every text byte ≥0x20, and only zeros afterwards. A former Rust
check inspected only the first byte; it was corrected to match Python. No
checked ENG name contains an interior control byte, so it did not affect those files.

Equipment-name recognition is deliberately looser: Python `_is_equipment_table`
and Tiled `isEquipmentTable` inspect the first byte only, so an interior control
byte must not invalidate an equipment table. Rust uses `equipment_text_start`.
Python/Rust accept a full unterminated 44-byte name; Tiled rejects it. It does
not occur in our editions; synthetic test
`equipment_names_are_checked_looser_than_texts` records that distinction.

**Confirmed:** table searches are unique in both editions and all Game.0–2
references reach filled records (`tests/k1/test_graphics.py`, Rust
`k1-data/tests/original_graphics.rs`). Our structure/code search agrees with
Slavik `game_locale.cpp:31-224` counts, without using its strings. The detailed
ENG census found the name block at 0x45794, 20/30 monster names, all nonzero
human/nickname records filled, all 56 map names. That census session did not
repeat RUS measurements; cross-edition tests remain separate evidence. Runtime
Rust/Python digests compare table ordering/content and every character name
without writing the strings anywhere.

| Table | Record | Count | RUS | ENG | Index |
|---|---|---|---|---|---|
| Monsters | 32 bytes | 30, first 20 filled | 0x4455C | 0x45794 | `(class & 0x7F) − 0x41` |
| Human names | 32 bytes | 171, record 0 empty | Immediately after monsters | Same block | Character byte 0xD4 |
| Nicknames | 32 bytes | 21, record 0 empty | Immediately after names | Same block | Byte 0xD5; zero omits nickname |
| Map names | 40 bytes | 56 | 0x41594 | 0x427CC | Map − 1 |
| Accessories | 120-byte name + 4 × s16 | 9 | 0x3E2B0 | 0x3F4E8 | Item.info_id |
| Potions | Same | 14 | 0x3F650 | 0x40888 | Item.info_id |
| Other items | Same | 54 | 0x46B1C | 0x47D54 | Item.info_id |
| New-game menu lines | 512 bytes | 20 | 0x4951C | 0x4A754 | Line index |

Monsters/names/nicknames form a 222-record block recognized by filled/empty
positions. Map names form the unique 56×40 run, including names for absent
maps. Accessory/potion/other records are 128 bytes with both image indices
(third/fourth numbers) positive; lengths distinguish the three runs. The four
numbers are bonus, weight, image and loot image **by Slavik**.

New-game menu texts are the unique 20×512 run (`game_newgame.cpp:276-365`,
`Locale::NewGameMenu`): 0–2 hero descriptions in screen-box order Dragomir,
Mikhail, Wolf (Game.N 1,2,0), 3–6 classes, 7–19 hints for attributes, carrying
capacity, party and skills. A neighboring 19×512 message table starts at RUS
0x41F1C / ENG 0x43154; it is a separate measured table, not parsed here.
Rust reads the 20-record series as `Texts::newgame`.

### Character names

**Confirmed by EXE-code inspection:** classes 0x41–0x50 use the monster-species
name; all others, including village women/children from 0x51, use name plus
space plus a nonzero nickname. Our Game.N census finds one unnamed class-0x51
villager per file: name/nickname zero, group 31, map 37. Rust tests reproduce
that count.

For invalid indices, Python can join an empty part with a space (`name ` or
` name`); Rust omits empty parts. There is no difference on checked files:
placed characters' indices resolve, except the villager with both parts empty.
This distinction matters when interpreting name digests.

Python parse loses all texts if accessory/potion/other item tables are missing.
Rust does not parse those tables and does not require them for character/map
names. Both editions contain all required tables, so this does not change their
results.

### Item names

Item.type_id 0–5 and 12 use armor/weapons; 6–8 accessories; 9 potions; 11 other
items. Type 10 has no table and does not occur. Type 12 is a quiver: weapon
records 30–33 name it; concentration stores arrow count (Slavik
`game_item.cpp:185`). Every owned/loot item of every Game.N resolves a name.

## Not found in EXE data

The 24 equipment palette indices were not found as bytes, u16 or u32 arrays;
embedded instructions are a **hypothesis**. Our indices come **by Slavik**
`resources.cpp:15` (`CharEquipPal`), visually checked for natural equipment colors.
