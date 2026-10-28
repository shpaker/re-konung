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
class Executable:
    equipment: list[EquipmentInfo]
    # [направление] → слоты Character.equipment в порядке отрисовки
    draw_order: list[tuple[int, ...]]
    # [вид огня] → (первый, последний) кадр FLAMES.RES, оба включены
    flame_animations: list[tuple[int, int]]


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
    return Executable(equipment, draw_order, _find_flame_animations(data))


def find(game_dir: Path) -> Path:
    for path in game_dir.iterdir():
        if path.name.lower() in EXECUTABLE_NAMES:
            return path
    raise FileNotFoundError(f"{game_dir}: нет ни одного из {EXECUTABLE_NAMES}")


def load(game_dir: Path) -> Executable:
    return parse(find(game_dir).read_bytes())
