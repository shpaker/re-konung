# Tiled extension internals

[User guide](../guides/tiled.md) · [K1 formats](../research/k1/README.md#format-status)

`tiled/konung/lib/k1.mjs` parses without the Tiled API; konung.mjs constructs
the map. Python wins disagreements. Map/state writing retains all unknown bytes
and is checked byte for byte across every available edition.

## Map representation

| Tiled element | Contract |
|---|---|
| Orientation | Staggered Y, odd rows shifted |
| Dimensions | 80×160; Tiled tile 116×64, pitch 116×32. The game's 114×64 rhombus aligns from the cell's bottom-left. |
| Map foot | Original 51200 walkability bytes in hex, overlaid by edits; do not edit the property |
| mapNumber | Opened map number used for Game.N objects even when Save As uses another number |
| tutorial | Tutorial source flag selecting Tlights.N/TGame.N; do not edit |
| rawDecorations | Space-separated slot:record, 16-byte hex records for tiles ≥512. Not drawable; retained in their own slots. Empty on game maps, one slot-994 record on both tutorials. Deleting this property data deletes those records. |
| mapName | EXE map name, display-only. Absent without EXE and on tutorials; not read on save. |
| graph tileset | Image collection indexed by GRAPH.RES slot, top-left object alignment |
| ground / overlay | Ground and upper tile layer |
| decorations | graph tile objects, in draw/slot order, with slot and unknown. Four non-114×64 tile-layer images lead the layer via cellLayer/cellX/cellY, preserving the game's top rather than Tiled's bottom alignment. |
| objects tileset/layer | All 463 simple objects. Tile objectId, offsetX/Y; placed slot, flags, frame, frameTime, flames, unknown. Bottom-sorted. Objects 341,342,343,355,451,452 animate; appended frame tiles keep objectId. |
| shadows | Derived black object masks, opacity 0.4; locked, ignored on save |
| flames tileset/layer | All 74 FLAMES.RES frames (frame), seven kinds with animation on their first tile (kind). Above objects, derived/locked, linked by object slot; refreshes on reopen. Absent without FLAMES.RES or EXE. |
| game.K | Starting state for hero K: 0 Wolf, 1 Dragomir, 2 Mikhail. game.0 shown by default; others hidden. checksum records the opened Game.K, not editable. |
| foot / foot-arrows | Hidden layers above everything: red blocked cells / blue arrow-stop cells (**hypothesis** for 0x40). Fifteen mask tiles encode the four fine cells per rhombus: top 1, left 2, right 4, bottom 8. Empty means none. |
| characters.K / character-shadows.K | Per-character composite images/animations. Every tile/frame carries character and the shared frame envelope's offsetX/Y. |

## State-group layers

Inside game.K, bottom to top. Positions are fine-grid points; rectangles follow
cell edges and include both boundaries.

| Layer | Representation | Properties |
|---|---|---|
| shadows | Derived character shadows, locked/ignored on save | — |
| groups | Spawn/leash rectangles | group, area, kind, size, maxSize |
| exits | Transition zones | slot, targetMap, arrivalX/Y, direction, unknown |
| loot | Cache points | slot, gold (negative hidden), items (22 indices / 44-byte hex), record (whole record hex), itemNames (EXE names separated by semicolon; read-only/absent without EXE) |
| arrivals | Entry points from other maps | slot (exit index), fromMap, direction |
| villages | Elder/healer/trader/smith/commander workplaces | village, job, direction |
| start | Hero start on 1/4/7, tutorial on Tutorial.4 | direction |
| mobs | Roaming group display, first member at spawn center; actual game scatters them; locked/ignored on save | — |
| characters | NPCs/villagers at file cells, EXE names or c<index> | character, group, classId, level, direction, nameId, nickId, spriteSet, gold, freePoints, experience; fencing/trading/accuracy/carpentry/medicine/identification/smithing/armor/poison; agility/agilityBase/charisma/charismaBase/health/strength/strengthBase/stamina/staminaBase |

LIGHTS.N is the hidden lights tile layer immediately above ground/overlay and
below decorations/objects. Its tileset displays 19 LIGHTS.RES masks in warm
yellow, brightness mapped to alpha. Tile index is mask index, file value one
higher. This is a placement hint for tile light, not the full ambient fade.
Absent LIGHTS.N (maps 6/8/46) or LIGHTS.RES means no layer.

## Tutorial source selection

**Confirmed by our executable-string inspection:** TUTORIAL.EXE shares GRAPH.RES,
Objects.res, LIGHTS.RES and FLAMES.RES with the main game. Equipment/draw-order/
flame tables parsed from KONUNG.EXE and TUTORIAL.EXE compare equal. The extension
reads them from KONUNG.EXE. k1.fileSet (Python map_file.file_set) selects by the
opened map's case-insensitive name:

| Map | Light | State |
|---|---|---|
| Tutorial.N | Tlights.N | TGame.K; ENG has TGame.1 only |
| Other / Konung.N | LIGHTS.N | Game.0–2, never TGame.K |

Save uses the same source, retaining existing filename case; new tutorial light
is named Tlights.N. Cross-source Save As is refused. Tutorial hero/start
interpretation remains the [Game.N hypothesis](../research/k1/formats/game-state.md#file-selection).

## Animation

Tiled's shared animation clock runs six animated map objects at
frame_time × 60 ms (180–540 ms), seven flame kinds at 5–14 frames of 60 ms,
and character state/direction sequences at 60 ms. The tick/frame sources are
Slavik game.h:30, game_play.cpp:1003-1017 and 3729; direct original-game timing
was not checked in this extension study and remains **hypothesis**.

Each sequence occupies the union of its frame rectangles because Tiled draws
frames from one point. Tile offsetX/Y holds that envelope's offset; serialization
is unaffected. Matching objects animate in sync; stored initial frame indices
are not shown. Rotating a character refreshes its image on reopen.

## Recorded verification

These are historical local checks, not new runs made by the documentation move.

- All 43 maps of both editions open/save byte-identically, including all Game.N;
  this remained true after animation/flames, walkability, light, character
  fields and roster editing were added. Raw decorations are empty on game maps.
- Map 1 tile/object/decor edits are read by Python as exactly the intended edits.
  The animated map opened in about 2 s; warrior, ant and object 451 frame PNGs
  were visually checked for stable envelopes; map 17 flames align with houses.
- Map 1 RUS game.0: moved/turned NPC, expanded exit, removed/added loot and moved
  arrival changed only the expected state. Konung.1, items, Game.1/2 stayed.
  Stale external state and forbidden worker deletion fail with a reason;
  repeated own saves without roster edits succeed.
- Walkability: map 1's foot tile count matches Python. Filling one cell,
  erasing another and setting an arrow mark changes exactly nine foot bytes,
  leaving other map/state bytes alone. Renders differ only at one-pixel
  rhombus borders and edge cells versus `konung k1 map --foot`.
- Lighting: all 40 LIGHTS.N preserve bytes and filename case; absent light files
  for 6/8/46 stay absent. Moving one mask changes two tile bytes and eight fine
  bytes, not Konung.N. Tiled versus Python `--lights` differs by at most one
  intensity unit where the underlying background matches.
- Tiled 1.12 `tiled.exe --evaluate`, map 17: moving the mask from column 34,
  row 25 to its right neighbor produces exactly Python serialize_lights bytes
  (two/eight), leaves Konung.17/Game.N unchanged, reopens with 103 lit tiles.
  Asset.save()/map.save() are editor-only and did nothing under --evaluate in
  that check; the recorded CLI format API was mapFormatForFile → read → write.
- Tutorial.4/5 each expose game.1, Tlights.N and one raw slot 994. Their map,
  light and TGame.1 round-trip byte-identically with filename case retained;
  Python tests/k1/test_maps.py and test_game_state.py cover the same files.
- Name checks on all 43 maps/editions matched Python character_name/item_name:
  1271 character names, 1132 loot item-name lists and map names.
- Character-field edit on map 1 changed only the selected NPC's level, nickname,
  gold, fencing and base stamina in Game.0. Invalid sprite/name indices and
  fractional gold were rejected without writes; Konung.1/Game.1/2 stayed.
- Map 1 game.0 roster copies/removals (villager and NPC, including separate NPC
  copy) matched Python game_edit byte for byte and passed _check_layout.
  Worker removal and another save after roster-index changes were refused.
- Edited files have not been launched in the original game.

## Reference image comparison

Map 1's Tiled render differs from Python by at most two intensity units from
shadow blending rounding. Characters/equipment/monsters match `--game 0`,
except Tiled's fixed layer stack cannot place map objects over creatures.
[Agent workflow](agents.md#tiled-inspection) explains full-size captures.
