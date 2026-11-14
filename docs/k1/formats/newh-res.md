# NEWH.RES — the new-game menu images

`NEWH<width><language>.RES` (`NEWH640R.RES`, `NEWH800E.RES`, …) holds the 13
images of the new-game menu: the background, the tile underlay, the selection
dot, the three heroes' portrait pairs and the page buttons. A file per screen
resolution (640, 800) and per edition language (R, E) lives in the game
directory; the player opens one of them the same way the original does.

**Status:** confirmed — every file of both editions (4 files) parses record
by record to the last byte, each record consumed to its declared size
(`k1-data/tests/original_newh_res.rs`; Python has no mirror — the format is
needed only by the player window).
Source: found by ourselves over the structure from Slavik
`game_newgame.cpp:96-124` (`LoadNEWH`) and `resources.cpp:582-630`
(`LoadRL16BitImage`); the decoder follows the byte stream, nothing is copied.

## The file

A plain series of 13 records; the last one ends on the last byte of the file.

| Offset | Type | Meaning | Confidence |
|---|---|---|---|
| +0 | 4 bytes | unknown, never read (zeros in our files) | unknown |
| +4 | u32 | the byte size of the image record | confirmed |
| +8 | image record | RL16 image, exactly `size` bytes | confirmed |

## The RL16 image record

| Offset | Type | Meaning | Confidence |
|---|---|---|---|
| 0 | s16 | width in pixels | confirmed |
| 2 | s16 | height in pixels | confirmed |
| 4 | h × s16 | per-line byte lengths; the decoder skips them | confirmed (never read, per Slavik) |
| 4+2h | stream | the rows, one after another | confirmed |

The row stream: one byte `n` at a time — `0` ends the row; a byte with the
high bit set skips `n & 0x7F` pixels (they stay transparent); any other byte
copies `n` pixels, each a little-endian u16 in RGB555
(R = bits 10–14, G = 5–9, B = 0–4). 5-bit channels scale to 8 by
`(v × 255 + 15) / 31` — the original's `Lkp5To8` table (Slavik `gfx.h:62-65`).

## The 13 records

| Record | Content | Size at 800 | Confidence |
|---|---|---|---|
| 0 | the menu background (800×600 or 640×480), the title is baked into it per language | 481956 (R) / 462341 (E) | confirmed |
| 1 | the tile underlay (100×100) | 20404 | confirmed |
| 2 | the selection dot (13×13) | 344 | confirmed |
| 3–4 | hero portrait, box 0: selected / plain (76×87; 56×65 at 640) | 12185 / 12179 | confirmed |
| 5–6 | hero portrait, box 1: selected / plain | 12287 / 12233 | confirmed |
| 7–8 | hero portrait, box 2: selected / plain | 12243 / 12221 | confirmed |
| 9–12 | the page buttons (105×46, 102×46), four states | 7098…5734 | confirmed |

The three portrait pairs sit in the screen-box order of the original: box 0 is
Драгомir (`Game.N` 1), box 1 is Михаил (`Game.N` 2), box 2 is Волк (`Game.N`
0) — `game_newgame.cpp:276-365`. In `Game.N` numbering the pairs are 7/8,
3/4 and 5/6 (`k1_data::newh_res::PORTRAIT_RECORDS`). The six portraits are
byte-identical between the editions; the backgrounds differ (the baked-in
title), so a tool that needs only the portraits may open either language.

The original also draws the chosen hero's live model over the background
(`DrawNewGameChar`, `game_newgame.cpp:188-280`) — the base sprites and the
equipment layers come from `GRAPH.RES` ([graph-res.md](graph-res.md)), not
from this file.

## Rust

`k1_data::newh_res::parse(&bytes) -> NewhRes` — a pure function; each image
decodes into straight-alpha RGBA. Python has no mirror: the format is needed
only by the player window, which is Rust-only.
