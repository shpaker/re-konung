# LIGHTS.RES — tile-light masks

145472 bytes, identical in RUS and ENG. Code: `konung/k1/lights_res.py`
(`parse`, `to_rgba`). Each map's [LIGHTS.N](map.md#lightsn) selects the masks.

```sh
uv run konung k1 map original/k1/rus/game out/k1/maps -n 1 --lights
```

**Confirmed:** the mask sizes described by Slavik sum to the file size
(`tests/k1/test_maps.py`); the render shows smooth light patches without gaps.
Sources: Slavik `resources.cpp:350-377` (`LoadLightsRes`), `game.cpp:996-1003`,
checked on our files.

## Structure

No header; one brightness byte per pixel.

| Offset | Size | Meaning | Confidence |
|---|---|---|---|
| 0 | 64 × 64 | Mask | **Unknown** purpose: Slavik reads but never uses it |
| 4096 | 64 × 43 | Mask | **Unknown**, same limitation |
| 6848 | 19 × 114 × 64 | Tile masks | **Confirmed** |

Values range 0–43, or 0–31 in tile masks. Slavik converts them by
`255 / 100 · v`; `to_rgba` does the same. This intensity mapping is a **hypothesis**.

## Tile masks

A mask fills the 114 × 64 rectangle. The game selects it before drawing the
ground and upper tile, so only their rhombus receives its light (Slavik
`game.cpp:996-1003`). Decorations and objects draw later; their exclusion from
this tile light was initially **hypothetical**, without an in-game check.
The later byte-level [lighting research](../lighting.md) distinguishes tile
light from the ambient fade that reaches other surfaces. `to_rgba` clears alpha
outside the rhombus.

Mask 0 is almost uniform (29–31 across the rhombus); other masks fall to zero
at their patch's edge, giving smooth transitions between tiles.

**By Slavik:** ground light is enabled in evening windows (`CalculateLight`,
`game.cpp:1764-1880`); the mask adds the light color (`game.cpp:975-983`). Python
and Tiled draw a warm-yellow indication immediately above the ground and below
decorations/objects. It indicates light placement rather than a final night image.
