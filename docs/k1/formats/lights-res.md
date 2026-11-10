# LIGHTS.RES — маски света

Masks the game lights the map tiles with. 145472 bytes, identical in RUS and ENG. Code — `konung/k1/lights_res.py` (`parse`, `to_rgba`). Which tile is lit by which mask is set by the map's `LIGHTS.N` ([map.md](map.md)). The light in the render:

```bash
uv run konung k1 map original/k1/rus/game out/k1/maps -n 1 --lights
```

**Статус:** раскладка **подтверждена**: сумма размеров масок по Slavik даёт ровно размер файла (`tests/k1/test_maps.py`), а свет на рендере ложится плавными пятнами без щелей. Источник: Slavik `resources.cpp:350-377` (`LoadLightsRes`) и `game.cpp:996-1003`, проверено на наших файлах.

## Структура

Заголовка нет, байт на пиксель — яркость:

| Смещение | Размер | Смысл | Уверенность |
|---|---|---|---|
| 0 | 64 × 64 | маска | назначение **неизвестно**: Slavik её читает, но не использует |
| 4096 | 64 × 43 | маска | то же |
| 6848 | 19 × 114 × 64 | маски тайлов | подтверждено |

Яркость в файле — от 0 до 43, у масок тайлов — до 31. Slavik растягивает её в 0–255 как `255 / 100 · v`; так же делает `to_rgba`, это **гипотеза**.

## Маски тайлов

Маска заполняет весь прямоугольник 114 × 64, а не только ромб тайла. Игра задаёт маску перед тем, как нарисовать тайлы земли и верхнего слоя клетки, поэтому свет достаётся только пикселям этих тайлов, то есть ромбу (Slavik `game.cpp:996-1003`). Декорации и объекты рисуются позже и, по Slavik, не светятся — **гипотеза**, в игре не сверено. `to_rgba` делает маску прозрачной вне ромба.

Маска 0 почти равномерна (яркость 29–31 по всему ромбу), у остальных яркость спадает до 0 к краю пятна. Поэтому соседние тайлы с разными масками дают плавное пятно.

By Slavik, the ground light turns on with the evening phases of the day cycle (`CalculateLight`, `game.cpp:1764-1880`): the mask adds the light color to the tile (`game.cpp:975-983`). In the render and in Tiled the light is drawn warm yellow right above the ground tiles, under decorations and objects — it shows where the light is, not what a night scene looks like.
