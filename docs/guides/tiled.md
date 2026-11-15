# Edit K1 maps in Tiled

Tiled 1.12 with `tiled/konung/` opens original Konung.N and ENG Tutorial.4/5,
then saves the same formats. It edits map tiles/decorations/objects/walkability,
LIGHTS.N and that map's characters, groups, caches and exits in Game.N.
Graphics resources and EXE tables are read-only; new graphics cannot be added.

[Prepare the game folder](game-files.md). Before editing, make a working copy
of the entire folder under out/: Game.N is shared by every map. Development
agents keep original/ unchanged.

## Installation from a checkout

```sh
just tiled-install
```

The extension is copied into Tiled's extensions directory and is reloaded by
a running editor. Choose File → Open → the working copy's Konung.N.

| Platform | Extension directory |
|---|---|
| Windows | `%LOCALAPPDATA%/Tiled/extensions/konung` |
| macOS | `~/Library/Preferences/Tiled/extensions/konung` |
| Linux | `${XDG_CONFIG_HOME:-~/.config}/tiled/extensions/konung` |

GRAPH.RES and Objects.res must be beside the map. Game.0–2 add game-object
groups; the EXE (RUS konung.res, ENG KONUNG.EXE) supplies equipment and flame
tables; FLAMES.RES supplies flames; LIGHTS.RES plus LIGHTS.N supplies light.
Missing optional files omit the corresponding visual layer. Resource names are
case-insensitive. The first map takes seconds to decode; later maps use caches.

Tiled selects formats by suffix. Numeric suffixes register 50 formats, one
per map number. LIGHTS.N and Game.N are not maps: opening them directly is a
size/format error. See [extension internals](../development/tiled.md) for the
full layer/property reference and tutorial source selection.

## Installation through an agent

The repository is also a plugin for Claude Code, Codex and Qwen Code. This
route needs Tiled 1.12 and the user's game copy, not a checkout, just or uv.

Claude Code:

```text
/plugin marketplace add shpaker/re-konung
/plugin install konung@re-konung
```

Codex: `codex plugin marketplace add shpaker/re-konung`, then select konung
in `/plugins`. Qwen Code:

```sh
qwen extensions install https://github.com/shpaker/re-konung
```

Ask the agent to install the Konung extension and open a map. The shipped
`konung-tiled-install` skill owns installation/validation details. For a release
ZIP, extract it so extensions/konung/konung.mjs exists.

## Editing and saving

File → Save (Ctrl+S) writes the map in its original format. Save As with the
Konung map format writes another file. Do not resize the map, delete/rename
layers or tilesets, or place tiles from another layer's tileset; save rejects
these with a reason.

| Layer | Supported changes and limits |
|---|---|
| ground, overlay | Tiles from graph only, indices up to 254 (one-byte map values) |
| decorations | Add/remove/move/reorder graph tile objects. Top-left in map pixels. New unknown field 0016000000000000, the most common observed value, meaning unknown. At most 1000 including raw records. |
| objects | Add/remove/move/change object tiles; at most 1000. Save snaps to the 58×16 staggered fine grid. Existing slot order is retained; new objects append. New service fields flags/frame/frameTime are zero, flames off. |
| lights | Paint/change/erase mask tiles from lights. Saves alongside the target map with its number and existing filename case; fine values are derived. An originally absent LIGHTS.N is not created. |
| foot, foot-arrows | Paint their own tiles; erasing clears all four fine cells. Only changed cells and the layer's bits are replaced: low nibble 0xF/0 for foot, 0x40 for foot-arrows. The 160 unmapped edge cells retain their bytes. Object edits do not repaint walkability. |
| characters | Move, change direction, duplicate or delete, subject to the rules below |
| groups | Move/resize spawn and leash rectangles; deletion is refused |
| loot, exits | Add/delete/move/edit; resize exit zones. Existing entries retain list order, new entries append; duplicate slot means a new entry. New exits require targetMap; new loot defaults to zero gold and empty items. Limits: 500 loot, 250 exits across all maps. |
| arrivals, villages, start | Move and change direction. An arrivals point edits the other map's exit and overrides arrivalX/arrivalY in exits. |

Objects' shadows/flames, rotated character images and names refresh on reopen.
Tiled places characters above all objects; unlike the game/Python render, a
house cannot occlude a character in that layer arrangement.

### Character copies and fields

Duplicate a character object with Ctrl+D. The first occurrence of a character
index is the original; later occurrences create copies with independent
properties/items, no enemy and no dialog. A lone NPC with reserve one gets its
own group; other copies append to the existing group. The tile must come from
that game.K group's characters.K tileset. Deletion shifts group members and
removes an emptied lone-NPC group. Village workers (elder, healer, trader,
smith, commander) cannot be deleted. mobs is derived/locked and ignored on save.

After a roster addition/deletion, reopen before saving again: record indices
have shifted. Both Save and Save As otherwise reject the stale map.

| Property | Accepted value |
|---|---|
| classId | Human 0x00–0x3F with occupation bits 0–5; monsters 0x41–0x50; villagers 0x51–0x57; dead bit 0x80 forbidden |
| level | 0–255 |
| nameId, nickId | 0–170 and 0–20; nickname zero means absent |
| spriteSet | Humans 0–2; dynamic classes 0–29 |
| gold, freePoints, experience | 32-bit integers |
| Skills and attributes | 16-bit integers |

These class restrictions follow recognized data/source tables; behavior outside
them in the original is a **hypothesis**. A class does not uniquely determine
spriteSet: some classes use two/three sprites; objects 21 and 24–29 are absent
from character records, so their game presentation is **unknown**. Changing
class does not change palette 0xD8. Equipment/inventory references cannot be
edited here: each item record has exactly one owner. Out-of-range fields fail
save with a reason; mobs does not expose these editable properties.

### Shared state and tutorials

Save loads Game.K beside the target map, falling back to the opened map's
directory, applies only this map's changes and writes beside the target.
If Game.K changed since opening (for example another map was saved), reopen
rather than overwrite shifted loot/exit indices. Re-saving your own unchanged
record layout remains valid.

Tutorial.N uses Tlights.N and TGame.1, with only game.1 visible. Main maps use
LIGHTS.N and Game.0–2. Saving a tutorial as Konung.N or the reverse is rejected
so state from the two sources cannot mix. Technical behavior and recorded
byte/pixel checks are in [extension internals](../development/tiled.md).
