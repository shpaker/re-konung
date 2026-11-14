# Документация re-konung

Здесь собирается всё, что известно об играх серии «Князь» и их файлах. Документы пишут агенты по ходу работы. Правила — [AGENTS.md](../AGENTS.md), цикл разбора формата — навык `konung-reverse-format`.

## Общее

- [macos-testing.md](macos-testing.md) — local application packaging and interactive game testing on macOS.

- [games.md](games.md) — игры серии, издания, какие части на одном движке.
- [design.md](design.md) — product principles: the original is the source of the game, the modern CRPGs of Larian Studios are the reference for controls, the camera and the interfaces.
- [sources.md](sources.md) — внешние работы и полезные ресурсы.
- [disc-images.md](disc-images.md) — образы дисков MDS/MDF: как из них достать данные и музыку.
- [agents.md](agents.md) — как устроена работа агентов: навыки, проверки, Tiled.
- [rust.md](rust.md) — Rust-реализация: workspace `crates/`, слои, статус переноса, проверки.
- [releases.md](releases.md) — версии и релизы: автоматический расчёт версии, ручной запуск.

## «Князь: Легенды Лесной страны» (1999)

- [k1/creatures.md](k1/creatures.md) — поведение существ: состояния, приказы, движение; что подтверждено, что гипотеза и что осталось неизвестным.
- [k1/editions.md](k1/editions.md) — RUS и ENG: состав, отличия, как распаковать.
- [k1/inventory.md](k1/inventory.md) — все файлы игры и что о них известно.
- [k1/movement.md](k1/movement.md) — movement on the map: the fine grid, eight directions, step duration, the tick, the route limit.
- [k1/time-of-day.md](k1/time-of-day.md) — the day counter, the 21600-tick cycle, the light phase boundaries, what a phase does to the picture, and what the two option switches gate.
- [k1/tiled.md](k1/tiled.md) — как карта раскладывается в Tiled, как её править и сохранять.

### Статус форматов

| Файл | Что внутри | Статус | Код | Описание |
|---|---|---|---|---|
| `Konung.N` | карта локации N | разобран, кроме части полей объектов; у проходимости смысл флага 0x40 — гипотеза; запись побайтно точная | `konung/k1/map_file.py` | [map.md](k1/formats/map.md) |
| `LIGHTS.N` | освещение карты | разобран: номер маски на тайл, мелкий блок выводится из него; запись побайтно точная | `konung/k1/map_file.py` | [map.md](k1/formats/map.md) |
| `LIGHTS.RES` | маски света тайлов | разобран, кроме назначения двух первых масок | `konung/k1/lights_res.py` | [lights-res.md](k1/formats/lights-res.md) |
| `GRAPH.RES` | палитры, курсоры, тайлы, спрайты персонажей | разобран до последнего байта, спрайты персонажей и снаряжения рисуются на карте | `konung/k1/graph_res.py`, `konung/k1/characters.py` | [graph-res.md](k1/formats/graph-res.md) |
| `Game.N` | начальное состояние новой игры: персонажи, отряды, клады, переходы, деревни | раскладка разобрана, места проверены рендером; часть полей — по Slavik; запись побайтно точная | `konung/k1/game_state.py`, `konung/k1/game_edit.py` | [game-state.md](k1/formats/game-state.md) |
| `OBJECTS.RES` | объекты карты и динамические спрайты (монстры, женщины, дети) | разобран: простые и динамические объекты | `konung/k1/objects_res.py` | [objects-res.md](k1/formats/objects-res.md) |
| `KONUNG.EXE` (`konung.res` в RUS) | таблицы снаряжения, порядка отрисовки и анимаций огней; имена и названия | найдены таблицы снаряжения, отрисовки, огней, имён, прозвищ, монстров, карт и предметов; в Rust — всё кроме предметов | `konung/k1/executable.py`, `crates/game/k1/k1-data/src/executable.rs` | [executable.md](k1/formats/executable.md) |
| `NEWH*.RES` | картинки меню новой игры: фон, портреты героев (три пары), кнопки | разобран до последнего байта, все 4 файла обоих изданий; портреты одинаковы между изданиями | — (только Rust: `crates/game/k1/k1-data/src/newh_res.rs`) | [newh-res.md](k1/formats/newh-res.md) |
| RLE-картинки | сжатие всех спрайтов | разобрано | `konung/rle.py` | [rle.md](k1/formats/rle.md) |
| `FLAMES.RES` | кадры огней | разобран до последнего байта, огни рисуются на карте | `konung/k1/flames_res.py` | [flames-res.md](k1/formats/flames-res.md) |
| `SOUNDS.RES`, `*.RES` интерфейса | — | известно только по Slavik | — | [inventory.md](k1/inventory.md) |
| `Maps.res`, `Game.res`, `SAVITEMS.DAT`, `QUEST*.RES`, `*.flm` | — | не изучены; у `Maps.res` есть гипотеза о раскладке | — | [inventory.md](k1/inventory.md) |

## «Князь 2» и «Князь 2.5»

Пока ничего: нужны копии игр. План и гипотезы — в [games.md](games.md).

The new-game constructor repair, its runtime contract and pending acceptance checks:
[k1/new-game.md](k1/new-game.md).
