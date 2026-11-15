# Prepare your own game files

Use your own disc or installed licensed copy. No game files are distributed.
The checked editions are [RUS and ENG](../research/k1/editions.md); digital and
collector's editions have not been verified. Keep originals in `original/`
and all generated output in `out/`, both excluded from git.

## Installed copy

Copy the installed game directory to `original/k1/<edition>/game/`, for example
`original/k1/rus/game/`. The parsers, tests, player, viewer and Tiled extension
use this directory. Resource lookup is case-insensitive.

## Your own disc image

Use an MDS/MDF image of your disc, for example made with Alcohol 120%, in
`original/k1/<edition>/image/`. Install uv and 7-Zip, then run from the checkout:

```sh
just k1-unpack rus
just k1-unpack eng
```

Run only the recipe for the edition you own. It produces:

| Directory | Content |
|---|---|
| `image/` | The input MDS/MDF |
| `disc/` | `track01.iso` and CD-audio `track02…11.wav` |
| `files/` | Extracted disc files |
| `game/` | Installed game, extracted from `inst/setup.dat` |

7-Zip may report 18 trailing bytes in setup.dat; our checked copies' extracted
files are intact. MDS/MDF track structure is in the
[disc-image reference](../research/disc-images.md).

For individual extraction steps, `just k1-unpack` in the justfile is the
canonical recipe. The first step can also be run directly:

```sh
uv run konung disc original/k1/rus/image/konung.mds original/k1/rus/disc
```

It writes a 7-Zip-readable ISO and WAV tracks (44100 Hz, 16-bit stereo).
[Setup](setup.md) lists the tools; the [Tiled guide](tiled.md) explains editing.
Use a separate copy of the game folder when saving edits: Game.N is shared
across maps. Development agents keep `original/` unchanged and write edited
copies under `out/`.
