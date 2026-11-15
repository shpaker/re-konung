# FLAMES.RES — flame frames

1533070 bytes, identical in RUS and ENG: torches, forges and burning houses.
Code: `konung/k1/flames_res.py` (`parse`, `to_rgba`).

```sh
uv run konung k1 flames original/k1/rus/game out/k1/flames
```

**Confirmed:** both editions parse to EOF (`tests/k1/test_graphics.py`); frames
align with burning houses on map 17 and the forge on map 1. Sources: Slavik
`resources.cpp:424-441` (`LoadFlames`), `632-680` (`LoadFlameImage`), checked
against our files and renders.

## Structure

74 consecutive blocks, no file header.

| Offset | Type | Meaning | Confidence |
|---|---|---|---|
| 0 | u32 | Tag, always `0x05000000` | Meaning **unknown** |
| 4 | s32 | Image byte length | **Confirmed** for all 74 blocks |
| 8 | RL16 | [RLE image](rle.md), RGB555 | **Confirmed** |

Frames range from 38 × 31 to 256 × 195 pixels.

## Transparency

The file has no alpha. Slavik derives `a = (2r + g + b) / 4` and multiplies color
by it (`resources.cpp:660-670`). Dark pixels become almost transparent.
`to_rgba` keeps the color and puts `a` into ordinary alpha. The original blend
remains a **hypothesis**: Slavik's hardware path also uses `GL_SRC_ALPHA` with
premultiplied color (`gfx.cpp:337-370`); this was not checked in the game.

## Animation

The [EXE table](executable.md) assigns inclusive frame ranges to seven flame
kinds. A simple OBJECTS.RES object has up to 20 points, each carrying kind 0–6
and the frame's top-left offset from the object ([objects](objects-res.md)).
**By Slavik:** advance once per tick (`game.cpp:1587-1597`). The reference render
places the first frame immediately above the object.

## Open questions

- **By Slavik**, map loading randomizes each initial frame (`game.cpp:815-823`).
  The object's 20 flame bytes in Konung.N may preserve current frames; verify on data.
- Objects with at least two flames omit shadows and darkening in Slavik
  (`game.cpp:1333`, `1564-1584`). The reference render does not implement that case.
