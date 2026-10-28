# Документация re-konung

Здесь собирается всё, что известно об играх серии «Князь» и их файлах. Документы пишут агенты по ходу работы. Правила — [AGENTS.md](../AGENTS.md), цикл разбора формата — навык `konung-reverse-format`.

## Общее

- [games.md](games.md) — игры серии, издания, какие части на одном движке.
- [sources.md](sources.md) — внешние работы и полезные ресурсы.
- [disc-images.md](disc-images.md) — образы дисков MDS/MDF: как из них достать данные и музыку.
- [agents.md](agents.md) — как устроена работа агентов: навыки, проверки, Tiled.
- [releases.md](releases.md) — версии и релизы: автоматический расчёт версии, ручной запуск.

## «Князь: Легенды Лесной страны» (1999)

- [k1/editions.md](k1/editions.md) — RUS и ENG: состав, отличия, как распаковать.
- [k1/inventory.md](k1/inventory.md) — все файлы игры и что о них известно.
- [k1/tiled.md](k1/tiled.md) — как карта раскладывается в Tiled, как её править и сохранять.

### Статус форматов

| Файл | Что внутри | Статус | Код | Описание |
|---|---|---|---|---|
| `Konung.N` | карта локации N | разобран, кроме карты проходимости и части полей объектов; запись побайтно точная | `konung/k1/map_file.py` | [map.md](k1/formats/map.md) |
| `LIGHTS.N` | освещение карты | разобрана структура, смысл значений — нет | `konung/k1/map_file.py` | [map.md](k1/formats/map.md) |
| `GRAPH.RES` | палитры, курсоры, тайлы, спрайты персонажей | разобран до последнего байта, спрайты персонажей и снаряжения рисуются на карте | `konung/k1/graph_res.py`, `konung/k1/characters.py` | [graph-res.md](k1/formats/graph-res.md) |
| `Game.N` | начальное состояние новой игры: персонажи, отряды, клады, переходы, деревни | раскладка разобрана, места проверены рендером; часть полей — по Slavik; запись побайтно точная | `konung/k1/game_state.py` | [game-state.md](k1/formats/game-state.md) |
| `OBJECTS.RES` | объекты карты и динамические спрайты (монстры, женщины, дети) | разобран: простые и динамические объекты | `konung/k1/objects_res.py` | [objects-res.md](k1/formats/objects-res.md) |
| `KONUNG.EXE` (`konung.res` в RUS) | таблицы снаряжения и порядка отрисовки | найдены две таблицы | `konung/k1/executable.py` | [executable.md](k1/formats/executable.md) |
| RLE-картинки | сжатие всех спрайтов | разобрано | `konung/rle.py` | [rle.md](k1/formats/rle.md) |
| `FLAMES.RES`, `LIGHTS.RES`, `SOUNDS.RES`, `*.RES` интерфейса | — | известно только по Slavik | — | [inventory.md](k1/inventory.md) |
| `Maps.res`, `Game.res`, `SAVITEMS.DAT`, `QUEST*.RES`, `*.flm` | — | не изучены; у `Maps.res` есть гипотеза о раскладке | — | [inventory.md](k1/inventory.md) |

## «Князь 2» и «Князь 2.5»

Пока ничего: нужны копии игр. План и гипотезы — в [games.md](games.md).
