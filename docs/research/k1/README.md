<a id="konung-1-research"></a>

# Исследования «Князя 1»

[Исследования](../README.md) · [Сравнение изданий](editions.md) · [Перечень файлов](inventory.md)

<a id="rules-and-behavior"></a>

## Правила и поведение

- [Перемещение](movement.md): сетка, направления, ходьба, занятость и ограничения оригинала.
- [Существа](creatures/README.md): анимация, решения, размещение и бой.
- [Новая игра](new-game.md): характеристики, очки, стартовое снаряжение и предпросмотр.
- [Время](time.md) и [освещение](lighting.md): счётчики, интервалы, арифметика цвета и условия включения.
- [Lighting audit](lighting-audit.md): original-binary evidence, corrections, render checks and remaining unknowns.

<a id="format-status"></a>

## Состояние форматов

Запись карт, света и стартовых состояний побайтно точна на обоих доступных
изданиях. Графика доступна только для чтения. Подробные источники и уверенность
находятся на страницах форматов; таблица служит навигацией.

| Файл | Известно и что осталось | Реализация | Описание |
|---|---|---|---|
| `Konung.N`, `Tutorial.N` | Структура и запись подтверждены; часть полей объектов неизвестна, foot 0x40 — гипотеза | `konung/k1/map_file.py`; Rust `k1-data::map`; Tiled | [Карты](formats/map.md) |
| `LIGHTS.N`, `TLIGHTS.N` | Мелкие значения выводятся из масок тайлов; запись побайтно точна | Те же реализации карт | [Карты и свет](formats/map.md#lightsn) |
| `Game.N`, `TGame.N` | Проверены структура, ссылки, размещение и запись; смысл части полей только по источнику | `konung/k1/game_state.py`, `game_edit.py`; Rust `k1-data`; Tiled | [Стартовое состояние](formats/game-state.md) |
| `GRAPH.RES` | Разбор до EOF; сборка спрайтов персонажей и снаряжения | `konung/k1/graph_res.py`, `characters.py`; Rust `k1-data`; Tiled | [Графика](formats/graph-res.md) |
| `OBJECTS.RES` | Разобраны простые и динамические объекты | `konung/k1/objects_res.py`; Rust `k1-data`; Tiled | [Объекты](formats/objects-res.md) |
| `LIGHTS.RES` | Маски тайлов разобраны; назначение первых двух неизвестно | `konung/k1/lights_res.py`; Rust `k1-data`; Tiled | [Маски света](formats/lights-res.md) |
| `FLAMES.RES` | Разбор до EOF, огонь отображается | `konung/k1/flames_res.py`; Rust `k1-data`; Tiled | [Огонь](formats/flames-res.md) |
| `KONUNG.EXE`, RUS `konung.res` | Таблицы снаряжения, порядка отрисовки, огня и текстов; полнота различается по реализациям | `konung/k1/executable.py`; Rust `k1-data::executable`; Tiled | [Таблицы EXE](formats/executable.md) |
| `NEWH*.RES` | Все четыре файла обоих изданий разобраны до EOF; портреты совпадают | Rust `k1-data::newh_res` | [Изображения новой игры](formats/newh-res.md) |
| RLE | Кодек восстановлен | `konung/rle.py`; Rust `k1-data::rle`; Tiled | [RLE](formats/rle.md) |
| Звук, прочие ресурсы интерфейса | Сведения только по источникам | См. перечень | [Перечень](inventory.md) |
| `Maps.res`, `Game.res`, `SAVITEMS.DAT`, `QUEST*.RES`, `*.flm` | Не исследованы; структура `Maps.res` — гипотеза | См. перечень | [Перечень](inventory.md) |
