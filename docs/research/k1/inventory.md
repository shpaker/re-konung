# K1 file inventory

Installed contents of `inst/setup.dat` (`original/k1/<edition>/game`). RUS and ENG were compared by SHA-256: `=` identical, `≠` different. An uncited purpose is a **hypothesis** from the name or Slavik, not a verified fact.

| File | RUS, bytes | ENG, bytes | | Purpose |
|---|---:|---:|---|---|
| `CLIENT.EXE` | 939008 | — |  | Network client? |
| `FLAMES.RES` | 1533070 | 1533070 | = | Flames — [flames-res](formats/flames-res.md) |
| `Game.res` | 365509 | 365509 | ≠ | ? Starts with 16-byte records resembling Game.N items, but its size is not a multiple; Slavik does not read it |
| `GRAPH.RES` | 14505371 | 14505349 | ≠ | Palettes, cursors, ground tiles, decorations, character sprites — [graph-res](formats/graph-res.md) |
| `KONUNG.EXE` | — | 1884291 |  | Game; equipment tables — [executable](formats/executable.md) |
| `konung.ico` | 2238 | 2238 | = | Icon |
| `konung.res` | 1586688 | — |  | PE file (MZ), probably konung.exe renamed by the installer; same tables as ENG KONUNG.EXE — [executable](formats/executable.md) |
| `LAN_KONG.EXE` | — | 4118147 |  | Network play |
| `Lanusers.dat` | — | 1288192 |  | ? |
| `LIGHTS.RES` | 145472 | 145472 | = | Light masks — [lights-res](formats/lights-res.md) |
| `ListFile.ini` | 1337 | — |  | Installer file list |
| `Maps.res` | 3856000 | 3856000 | ≠ | ? **Hypothesis:** 20 × 192800-byte records = Konung.N + LIGHTS.N; none matches a shipped map; Slavik does not read it |
| `Objects.res` | 52522058 | 52522058 | = | Map objects (trees, houses) — [objects-res](formats/objects-res.md) |
| `playavi.dll` | 316416 | 328704 | ≠ | Video player for `*.flm` |
| `readme.txt` | — | 8545 |  | readme |
| `SAVITEMS.DAT` | — | 5120000 |  | ? 10000 × 512-byte records, all 0xFF except zero at byte 42: empty table; Slavik does not read it |
| `SERVER` | — | 91 |  | Network play |
| `Servers.cfg` | — | 21 |  | Network play |
| `SOUNDS.RES` | 21981240 | 21961483 | ≠ | 22050 Hz PCM sounds (**by Slavik**) |
| `Sounds_.res` | 13314264 | 13294507 | ≠ | Sounds, second bank? |
| `TUTORIAL.EXE` | — | 1880707 |  | Tutorial |
| `uninstall.dat` | — | 98402 |  | Uninstaller |
| `Uninstall.exe` | 432128 | 535552 | ≠ | Uninstaller |
| `Uninstall.ini` | 679 | — |  | Uninstaller |

- `game.N`: RUS 3, ENG 3, identical 0; sizes: [709822]; files: game.0, game.1, game.2
- `into…{r,e,t}.res`: RUS 2, ENG 2, identical 0; sizes: [1066080, 1072233, 1302491]…; files: into640e.res, into640r.res, into800e.res, into800r.res
- `intr…{r,e,t}.res`: RUS 2, ENG 4, identical 0; sizes: [1003654, 1009735, 1240065]…; files: intr640e.res, intr640r.res, intr640t.res, intr800e.res, intr800r.res, intr800t.res
- `konung.N`: RUS 43, ENG 43, identical 43; sizes: [128800]; files: 43
- `lights.N`: RUS 40, ENG 40, identical 40; sizes: [64000]; files: 40
- `me…{r,e,t}.res`: RUS 2, ENG 2, identical 0; sizes: [1375748, 1398061, 2152854]…; files: me640e.res, me640r.res, me800e.res, me800r.res
- `menu…{r,e,t}.res`: RUS 2, ENG 4, identical 0; sizes: [1312566, 1350410, 1381112]…; files: menu640e.res, menu640r.res, menu640t.res, menu800e.res, menu800r.res, menu800t.res
- `new…{r,e,t}.res`: RUS 2, ENG 2, identical 0; sizes: [365223, 376113, 510531]…; files: new640e.res, new640r.res, new800e.res, new800r.res
- `newh…{r,e,t}.res`: RUS 2, ENG 2, identical 0; sizes: [410382, 410492, 582193]…; files: newh640e.res, newh640r.res, newh800e.res, newh800r.res
- `play…{r,e,t}.res`: RUS 2, ENG 2, identical 0; sizes: [1303963, 1505501, 2031104]…; files: play640e.res, play640r.res, play800e.res, play800r.res
- `quest…{r,e,t}.res`: RUS 2, ENG 3, identical 0; sizes: [337800]; files: quest_e.res, quest_r.res, quest_t.res, queste.res, questr.res
- `save…{r,e,t}.res`: RUS 2, ENG 2, identical 0; sizes: [1237464, 1932024]; files: save640e.res, save640r.res, save800e.res, save800r.res
- `tgame.N`: RUS 0, ENG 1, identical 0; sizes: [709822]; files: tgame.1
- `tlights.N`: RUS 0, ENG 2, identical 0; sizes: [64000]; files: tlights.4, tlights.5
- `tutorial.N`: RUS 0, ENG 2, identical 0; sizes: [128800]; files: tutorial.4, tutorial.5

## File groups
- `Konung.N` — maps, [formats/map.md](formats/map.md). Numbers 1–50 excluding 9, 13, 18, 26, 31, 34, 41.
- `LIGHTS.N` — map lighting, [formats/map.md](formats/map.md). Absent for maps 6, 8, 46. Masks: `LIGHTS.RES`, [formats/lights-res.md](formats/lights-res.md).
- `Game.N` — starting state for hero N (0 Wolf, 1 Dragomir, 2 Mikhail), [formats/game-state.md](formats/game-state.md). `TGame.1` is the tutorial equivalent.
- `Tutorial.N`, `Tlights.N` (ENG only, N = 4, 5) — tutorial maps/lights, same format as Konung.N/LIGHTS.N ([formats/map.md](formats/map.md)). TUTORIAL.EXE reads them and shares GRAPH.RES, Objects.res, LIGHTS.RES and FLAMES.RES with the game: our executable inspection found the same filename strings.
- `<screen><resolution><language>.RES` — interface: MENU (menu), INTR/INTO (intro?), NEW/NEWH (creation/hero selection), PLAY (game), SAVE (saving), ME (?). Resolution 640 or 800; language R/E/T. **By Slavik:** starts with an RLE image table using GRAPH.RES palettes.
- `QUEST*.RES` — 337800 bytes in every language, probably fixed-length quest texts (**hypothesis**).

On disc, outside the installed archive: GRAPH.RES, LIGHTS.RES, Video/*.flm (videos played by playavi.dll).
