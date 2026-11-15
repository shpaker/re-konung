# K1 editions

**Confirmed by comparing our unpacked copies:** RUS, 1C CD 1999
(`original/k1/rus`) and ENG, Strategy First CD 2000 (`original/k1/eng`).
[Game-file preparation](../../guides/game-files.md) owns extraction commands;
[disc images](../disc-images.md) describes the container.

## Disc contents

Both discs carry the installer, GRAPH.RES, LIGHTS.RES and Video/*.flm; the
original reads these from disc. The rest of the installed game is in
`inst/setup.dat`, a ZIP with 18 extra trailing bytes reported by 7-Zip.

The image-size difference comes from disc extras:

- RUS has a 130 MB poster.flm (ENG: 20 MB) and Allods 3/WAR demos in Demo/ (48 MB).
- ENG has prolog.flm (69 MB), tutor.flm and DirectX 7 in inst/dx/.
- ENG's data track is CD-XA and its image includes subchannels.

## Installed files

| Property | RUS | ENG |
|---|---|---|
| Files | 119 | 134 |
| Konung.N | 43 | 43, byte-identical |
| LIGHTS.N | 40 | 40, byte-identical |
| Objects.res, FLAMES.RES, LIGHTS.RES | Same in both editions | Same |
| GRAPH.RES | One cursor differs, +22 bytes; the RUS disc carries ENG's version | Reference comparison |
| Game.0/1/2 | Different | See below |
| SOUNDS.RES, GAME.RES, MAPS.RES, Sounds_.res, playavi.dll, Uninstall.exe | Different | Different |
| Interface | *R.RES, Russian | *E.RES, English; *T.RES, unknown third language (MENU, INTR, QUEST) |
| ENG-only | — | TUTORIAL.EXE, tutorial.4/5, TLIGHTS.4/5, TGame.1; LAN_KONG.EXE, Lanusers.dat, SERVER, Servers.cfg; SAVITEMS.DAT; KONUNG.EXE under that name |
| RUS-only | CLIENT.EXE; konung.res, a PE file apparently renamed from konung.exe | — |

The maps are interchangeable. ENG adds the tutorial and network content;
RUS is the original 1999 edition. Full file sizes: [inventory](inventory.md).

## Differences in Game.0/1/2

**Confirmed over both unpacked editions:** files have the same length and differ
in about 10–11.5 thousand bytes, only in characters (attributes/equipment) and
items/loot. Groups, exits and villages match byte for byte. ENG has more filled
item slots: 1448 versus 1405 in Game.1. State content is compared per edition.

Without creatures, draw lists match on all 43 maps. With creatures they differ
on 16 maps. On 14 the check counters also differ (pieces, visible pieces,
textures, animations, frames); maps 2 and 12 preserve every counter but differ
in equipment selection and one pixel of placement. The difference is worn
equipment, so scene baselines are per edition and counters alone miss two maps.

## Not examined

The collector's edition, GOG and RUS patches 01/02 are not available in the
checked set. OGG music in MUSICS.RES and their HD-patch compatibility remain
external reports rather than conclusions from our files.
