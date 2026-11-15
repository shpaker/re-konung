# Konung 1 research

[Research](../README.md) · [Edition comparison](editions.md) · [File inventory](inventory.md)

## Rules and behavior

- [Movement](movement.md): grid, directions, walking, occupancy and original limits.
- [Creatures](creatures/README.md): animation, decisions, placement and combat.
- [New-game rules](new-game.md): attributes, allocation, starting equipment and preview.
- [Game time](time.md) and [lighting](lighting.md): counters, intervals, colour arithmetic and gates.

## Format status

Writing is byte-exact for maps, lights and starting states on both available
editions. Graphics are read-only. Detailed confidence and sources live in each
format page; the table is a navigation summary.

| File | Known / remaining | Implementation | Description |
|---|---|---|---|
| `Konung.N`, `Tutorial.N` | Layout and writing confirmed; some object fields unknown, foot bit 0x40 hypothetical | `konung/k1/map_file.py`; Rust `k1-data::map`; Tiled | [Maps](formats/map.md) |
| `LIGHTS.N`, `TLIGHTS.N` | Fine values derived from tile masks; byte-exact writing | Same map implementations | [Maps and lights](formats/map.md#lightsn) |
| `Game.N`, `TGame.N` | Layout, links, placement and writing checked; some field meanings reference-only | `konung/k1/game_state.py`, `game_edit.py`; Rust `k1-data`; Tiled | [Starting state](formats/game-state.md) |
| `GRAPH.RES` | Parsed to EOF; character and equipment sprites composed | `konung/k1/graph_res.py`, `characters.py`; Rust `k1-data`; Tiled | [Graphics](formats/graph-res.md) |
| `OBJECTS.RES` | Simple and dynamic objects parsed | `konung/k1/objects_res.py`; Rust `k1-data`; Tiled | [Objects](formats/objects-res.md) |
| `LIGHTS.RES` | Tile masks parsed; the first two masks' purpose unknown | `konung/k1/lights_res.py`; Rust `k1-data`; Tiled | [Light masks](formats/lights-res.md) |
| `FLAMES.RES` | Parsed to EOF; flames rendered | `konung/k1/flames_res.py`; Rust `k1-data`; Tiled | [Flames](formats/flames-res.md) |
| `KONUNG.EXE`, RUS `konung.res` | Equipment, draw-order, flame and text tables recovered; coverage differs by implementation | `konung/k1/executable.py`; Rust `k1-data::executable`; Tiled | [Executable tables](formats/executable.md) |
| `NEWH*.RES` | All four files of both editions parsed to EOF; portraits match across editions | Rust `k1-data::newh_res` | [New-game images](formats/newh-res.md) |
| RLE images | Codec recovered | `konung/rle.py`; Rust `k1-data::rle`; Tiled | [RLE](formats/rle.md) |
| Sound, other interface resources | Reference-only knowledge | See inventory | [Inventory](inventory.md) |
| `Maps.res`, `Game.res`, `SAVITEMS.DAT`, `QUEST*.RES`, `*.flm` | Unstudied; `Maps.res` layout hypothetical | See inventory | [Inventory](inventory.md) |
