"""Таблицы из исполняемого файла игры (docs/k1/formats/executable.md).

Снаряжение персонажа рисуется наборами спрайтов GRAPH.RES, а какой набор
у какого предмета, игра берёт из таблицы в exe. В RUS это `konung.res`
(переименованный установщиком PE), в ENG — `KONUNG.EXE`. Таблицы ищутся
по строению, а не по смещению: в изданиях они лежат в разных местах.
"""

import struct
from dataclasses import dataclass
from pathlib import Path

EXECUTABLE_NAMES = ("konung.exe", "konung.res")

# Описание доспеха или оружия: 44 байта названия, 10 × s16; последнее — набор спрайтов
EQUIPMENT_INFO = struct.Struct("<44s10h")
EQUIPMENT_COUNT = 70
EQUIPMENT_SETS = 24
# Порядок слотов снаряжения при отрисовке: 72 строки-перестановки 0–5 по 6 × s32;
# последние 8 — по направлениям для людей без оружия в руках (гипотеза, см. docs)
SLOTS = 6
DRAW_ORDER_ROW = struct.Struct(f"<{SLOTS}i")
DRAW_ORDER_ROWS = 72
DIRECTIONS = 8

# Анимации огней: 7 пар (первый, последний кадр FLAMES.RES) по 2 × s32,
# подряд от 0 до последнего кадра; номер пары — вид огня у простого объекта
FLAME_ANIMATION = struct.Struct("<2i")
FLAME_KINDS = 7
FLAME_FRAMES = 74

# Тексты — записи фиксированной длины: текст, дальше нули. RUS — CP866, ENG —
# латиница (в ней почти нет старших байт, CP866 читает и её)
TEXT_ENCODING = "cp866"
# Монстры, имена людей и прозвища — сплошной блок 30 + 171 + 21 записей по 32 байта;
# имя 0 и прозвище 0 пустые, у монстров заполнены первые 20
NAME_SIZE = 32
MONSTER_NAMES = 30
PEOPLE_NAMES = 171
NICKNAMES = 21
NAMES_BLOCK = MONSTER_NAMES + PEOPLE_NAMES + NICKNAMES
# Названия карт: 56 записей по 40 байт, номер записи — номер карты − 1
MAP_NAME_SIZE = 40
MAP_NAMES = 56
# Украшения, зелья, прочие предметы: 120 байт названия, 4 × s16 (бонус, вес,
# картинка, картинка клада); три серии таких записей, у всех обе картинки > 0
ITEM_INFO = struct.Struct("<120s4h")
ITEM_TABLES = {"accessories": 9, "alchemies": 14, "misc": 54}
# Вид предмета (Item.type_id) → таблица названий; 10 — таблицы нет
EQUIPMENT_TYPES = (0, 1, 2, 3, 4, 5, 12)
ACCESSORY_TYPES = (6, 7, 8)
ALCHEMY_TYPE = 9
MISC_TYPE = 11
# Классы 0x41–0x50 (бит 0x40, виды 0–15) называются по виду из таблицы монстров;
# с 0x51 — женщины и дети деревень, они по имени, как люди
MONSTER_CLASS = 0x40
FIRST_VILLAGER_CLASS = 0x51

# Палитра каждого набора снаряжения (номер палитры в банке цветов GRAPH.RES).
# В данных exe её нет — по Slavik resources.cpp:15 (CharEquipPal)
EQUIPMENT_PALETTES = (2, 2, 2, 2, 4, 4, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 4, 4, 2, 2, 5, 5, 5, 5)


@dataclass
class EquipmentInfo:
    """Описание доспеха или оружия; номер — Item.info_id предмета."""

    index: int
    name: bytes
    values: tuple[int, ...]
    # Набор спрайтов снаряжения GRAPH.RES; −1 — не рисуется
    sprite_set: int


@dataclass
class ItemInfo:
    """Описание украшения, зелья или прочего предмета; номер — Item.info_id."""

    index: int
    name: bytes
    # Бонус, вес, картинка, картинка клада — по Slavik
    values: tuple[int, ...]


@dataclass
class Texts:
    """Названия из exe: тексты игры, в репозиторий и тесты не попадают."""

    # Монстры по виду (класс & 0x7F) − 0x41; имена и прозвища по номеру, 0 — пусто
    monsters: list[str]
    names: list[str]
    nicknames: list[str]
    # [номер карты − 1]
    maps: list[str]


@dataclass
class Executable:
    equipment: list[EquipmentInfo]
    # [направление] → слоты Character.equipment в порядке отрисовки
    draw_order: list[tuple[int, ...]]
    # [вид огня] → (первый, последний) кадр FLAMES.RES, оба включены
    flame_animations: list[tuple[int, int]]
    # Таблицы ITEM_TABLES по имени
    items: dict[str, list[ItemInfo]]
    texts: Texts


def _is_equipment_table(data: bytes, start: int) -> bool:
    for index in range(EQUIPMENT_COUNT):
        name, *values = EQUIPMENT_INFO.unpack_from(data, start + index * EQUIPMENT_INFO.size)
        text, _, rest = name.partition(b"\0")
        if not text or text[0] < 0x20 or any(rest):
            return False
        if not -1 <= values[-1] < EQUIPMENT_SETS:
            return False
    return True


def _find_equipment(data: bytes) -> int:
    end = len(data) - EQUIPMENT_COUNT * EQUIPMENT_INFO.size
    found = [start for start in range(0, end, 4) if _is_equipment_table(data, start)]
    if len(found) != 1:
        raise ValueError(f"таблица снаряжения не найдена однозначно: {[hex(s) for s in found]}")
    return found[0]


def _find_draw_order(data: bytes) -> int:
    """Начало блока из 72 подряд идущих строк-перестановок 0–5."""
    slots = set(range(SLOTS))
    found = []
    # Строки идут с шагом 24 байта; каждое выравнивание по 4 проверяется отдельно
    for alignment in range(0, DRAW_ORDER_ROW.size, 4):
        run_start, run = 0, 0
        for start in range(alignment, len(data) - DRAW_ORDER_ROW.size, DRAW_ORDER_ROW.size):
            if set(DRAW_ORDER_ROW.unpack_from(data, start)) == slots:
                if run == 0:
                    run_start = start
                run += 1
                continue
            if run == DRAW_ORDER_ROWS:
                found.append(run_start)
            run = 0
    if len(found) != 1:
        raise ValueError(f"порядок отрисовки не найден однозначно: {[hex(s) for s in found]}")
    return found[0]


def _flame_animations_at(data: bytes, start: int) -> list[tuple[int, int]] | None:
    pairs = [
        FLAME_ANIMATION.unpack_from(data, start + kind * FLAME_ANIMATION.size)
        for kind in range(FLAME_KINDS)
    ]
    if pairs[0][0] != 0 or pairs[-1][1] != FLAME_FRAMES - 1:
        return None
    if any(first > last for first, last in pairs):
        return None
    if any(pairs[kind + 1][0] != pairs[kind][1] + 1 for kind in range(FLAME_KINDS - 1)):
        return None
    return pairs


def _find_flame_animations(data: bytes) -> list[tuple[int, int]]:
    """7 пар подряд, без пропусков, от кадра 0 до последнего кадра FLAMES.RES."""
    end = len(data) - FLAME_KINDS * FLAME_ANIMATION.size
    found = []
    for start in range(0, end, 4):
        if data[start : start + 4] != b"\0\0\0\0":
            continue
        pairs = _flame_animations_at(data, start)
        if pairs is not None:
            found.append(pairs)
    if len(found) != 1:
        raise ValueError(f"таблица огней не найдена однозначно: {len(found)}")
    return found[0]


def _text(field: bytes) -> bytes | None:
    """Текст записи «текст + нули»; None — запись не такая (пустая тоже None)."""
    text, _, rest = field.partition(b"\0")
    if not text or min(text) < 0x20 or any(rest):
        return None
    return text


def _decode(text: bytes | None) -> str:
    return (text or b"").decode(TEXT_ENCODING)


def _is_names_block(data: bytes, start: int) -> bool:
    """30 монстров (первые заполнены, хвост пуст), 171 имя и 21 прозвище.

    Имя 0 и прозвище 0 пусты.
    """
    empty = bytes(NAME_SIZE)
    monsters_end = MONSTER_NAMES
    nicknames_start = MONSTER_NAMES + PEOPLE_NAMES
    seen_empty = False
    for index in range(NAMES_BLOCK):
        field = data[start + index * NAME_SIZE : start + (index + 1) * NAME_SIZE]
        if index in (monsters_end, nicknames_start):
            if field != empty:
                return False
            continue
        if index < monsters_end:
            if field == empty and index:
                seen_empty = True
                continue
            if seen_empty:
                return False
        if _text(field) is None:
            return False
    return True


def _find_names(data: bytes) -> int:
    end = len(data) - NAMES_BLOCK * NAME_SIZE
    found = [
        start
        for start in range(0, end, 4)
        if _text(data[start : start + NAME_SIZE]) is not None and _is_names_block(data, start)
    ]
    if len(found) != 1:
        raise ValueError(f"имена не найдены однозначно: {[hex(s) for s in found]}")
    return found[0]


def _text_runs(data: bytes, size: int, is_record) -> list[tuple[int, int]]:
    """Серии подряд идущих записей size байт, для которых is_record верно: (начало, длина)."""
    runs = []
    for alignment in range(0, size, 4):
        run_start, run = 0, 0
        for start in range(alignment, len(data) - size + 1, size):
            if is_record(data[start : start + size]):
                if run == 0:
                    run_start = start
                run += 1
                continue
            if run:
                runs.append((run_start, run))
            run = 0
        if run:
            runs.append((run_start, run))
    return runs


def _find_map_names(data: bytes) -> int:
    found = [s for s, n in _text_runs(data, MAP_NAME_SIZE, _text) if n == MAP_NAMES]
    if len(found) != 1:
        raise ValueError(f"названия карт не найдены однозначно: {[hex(s) for s in found]}")
    return found[0]


def _is_item_info(record: bytes) -> bool:
    name, *values = ITEM_INFO.unpack(record)
    return _text(name) is not None and values[2] > 0 and values[3] > 0


def _find_items(data: bytes) -> dict[str, list[ItemInfo]]:
    """Три серии описаний, по одной на каждую длину из ITEM_TABLES."""
    runs = _text_runs(data, ITEM_INFO.size, _is_item_info)
    tables = {}
    for table, count in ITEM_TABLES.items():
        found = [s for s, n in runs if n == count]
        if len(found) != 1:
            raise ValueError(f"таблица {table} не найдена однозначно: {[hex(s) for s in found]}")
        tables[table] = [
            ItemInfo(index, name.partition(b"\0")[0], tuple(values))
            for index in range(count)
            for name, *values in [ITEM_INFO.unpack_from(data, found[0] + index * ITEM_INFO.size)]
        ]
    return tables


def _parse_texts(data: bytes) -> Texts:
    start = _find_names(data)
    names = [
        _decode(_text(data[start + i * NAME_SIZE : start + (i + 1) * NAME_SIZE]))
        for i in range(NAMES_BLOCK)
    ]
    maps_start = _find_map_names(data)
    maps = [
        _decode(_text(data[maps_start + i * MAP_NAME_SIZE : maps_start + (i + 1) * MAP_NAME_SIZE]))
        for i in range(MAP_NAMES)
    ]
    return Texts(
        monsters=names[:MONSTER_NAMES],
        names=names[MONSTER_NAMES : MONSTER_NAMES + PEOPLE_NAMES],
        nicknames=names[MONSTER_NAMES + PEOPLE_NAMES :],
        maps=maps,
    )


def parse(data: bytes) -> Executable:
    start = _find_equipment(data)
    equipment = []
    for index in range(EQUIPMENT_COUNT):
        name, *values = EQUIPMENT_INFO.unpack_from(data, start + index * EQUIPMENT_INFO.size)
        equipment.append(EquipmentInfo(index, name.partition(b"\0")[0], tuple(values), values[-1]))
    order_start = _find_draw_order(data)
    first = order_start + (DRAW_ORDER_ROWS - DIRECTIONS) * DRAW_ORDER_ROW.size
    draw_order = [
        DRAW_ORDER_ROW.unpack_from(data, first + direction * DRAW_ORDER_ROW.size)
        for direction in range(DIRECTIONS)
    ]
    return Executable(
        equipment, draw_order, _find_flame_animations(data), _find_items(data), _parse_texts(data)
    )


def item_name(tables: Executable, type_id: int, info_id: int) -> str | None:
    """Название предмета по виду и номеру описания; None — у вида нет таблицы."""
    if type_id in EQUIPMENT_TYPES:
        name = tables.equipment[info_id].name
    elif type_id in ACCESSORY_TYPES:
        name = tables.items["accessories"][info_id].name
    elif type_id == ALCHEMY_TYPE:
        name = tables.items["alchemies"][info_id].name
    elif type_id == MISC_TYPE:
        name = tables.items["misc"][info_id].name
    else:
        return None
    return _decode(name)


def character_name(texts: Texts, class_id: int, name_id: int, nick_id: int) -> str:
    """Имя персонажа, как его собирает игра: у монстров — название вида,
    у людей, женщин и детей — имя и прозвище через пробел (прозвище 0 — без него)."""
    kind = class_id & 0x7F
    if class_id & MONSTER_CLASS and kind < FIRST_VILLAGER_CLASS:
        return texts.monsters[kind - MONSTER_CLASS - 1]
    name = texts.names[name_id]
    return f"{name} {texts.nicknames[nick_id]}" if nick_id else name


def find(game_dir: Path) -> Path:
    for path in game_dir.iterdir():
        if path.name.lower() in EXECUTABLE_NAMES:
            return path
    raise FileNotFoundError(f"{game_dir}: нет ни одного из {EXECUTABLE_NAMES}")


def load(game_dir: Path) -> Executable:
    return parse(find(game_dir).read_bytes())
