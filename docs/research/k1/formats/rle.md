# RLE images

All K1 sprites (tiles, objects, cursors, flames and shadows) use row-based
compression with transparent skips. Code: `konung/rle.py`,
`tiled/konung/lib/k1.mjs` (`decodeRle`); tests: `tests/test_rle.py`.

**Confirmed:** all GRAPH.RES tiles and OBJECTS.RES objects of both editions decode
without exceeding row width; GRAPH.RES parses to EOF. Source: Slavik
`resources.cpp` (`LoadRL8BitImage` and neighboring functions), checked on our files.

## Header

| Offset | Type | Meaning |
|---|---|---|
| 0 | s16 | Width |
| 2 | s16 | Height |
| 4 | s16 × height | Each row's byte length; not needed by the decoder |

## Row

Each command starts with a byte `n`:

| `n` | Meaning |
|---|---|
| 0 | End of row |
| `n & 0x80` | Skip `n & 0x7F` transparent pixels |
| 1–127 | Followed by `n` pixels |

Neither skips nor pixel runs may exceed row width. Python, JS and Rust reject
both cases before writing the output buffer; none occurs in the checked files.

| Kind | Pixel | Used by |
|---|---|---|
| RL8 | u8 index into a 256-color palette | Tiles, objects, interface screens |
| RL16 | u16 RGB555 | Cursors, FLAMES.RES |
| Shadow | No pixel data; only the mask matters | Object and character shadows |

## Colors

RGB555: `0RRRRRGG GGGBBBBB`, little-endian. Expand five bits to eight by
`(v << 3) | (v >> 2)`, as Slavik's `Lkp5To8` does. Its fidelity to the original
appearance is a **hypothesis**. Slavik's additive flame treatment derives
alpha as `(2r + g + b) / 4`; this too remains a **hypothesis**.
