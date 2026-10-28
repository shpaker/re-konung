"""Konung.N — карта локации N; LIGHTS.N — её освещение (docs/k1/formats/map.md)."""

import struct
from dataclasses import dataclass
from pathlib import Path

from konung.binary import Reader

WIDTH = 80
HEIGHT = 160
# Сетка проходимости и света вдвое мельче тайловой
FINE_WIDTH = 160
FINE_HEIGHT = 320
MAX_DECORATIONS = 1000
MAX_OBJECTS = 1000
DECORATION_SIZE = 16
OBJECT_SIZE = 36
FLAME_SLOTS = 20

MAP_SIZE = (
    WIDTH * HEIGHT * 2
    + FINE_WIDTH * FINE_HEIGHT
    + MAX_DECORATIONS * DECORATION_SIZE
    + MAX_OBJECTS * OBJECT_SIZE
)
LIGHTS_SIZE = WIDTH * HEIGHT + FINE_WIDTH * FINE_HEIGHT

# Геометрия сетки: ромб 114×64, шаг по x — 116, шаг строки — 32,
# нечётные строки сдвинуты вправо на половину шага
TILE_STEP_X = 116
TILE_STEP_Y = 32
ODD_ROW_SHIFT = TILE_STEP_X // 2

# Байт проходимости: младшие 4 бита 0xF — непроходимо, старшие — флаги.
# В картах дистрибутива только 0x00, 0x0F, 0x40, 0x4F;
# флаг 0x40 — здесь застревают стрелы (гипотеза)
FOOT_BLOCKED = 0x0F
FOOT_STOPS_ARROWS = 0x40


@dataclass
class Decoration:
    """Тайл из GRAPH.RES, поставленный в произвольную точку карты (в пикселях)."""

    slot: int
    tile: int
    x: int
    y: int
    unknown: bytes


@dataclass
class MapObject:
    """Объект из OBJECTS.RES (деревья, дома, анимации) в пикселях карты."""

    slot: int
    object_id: int
    x: int
    y: int
    flames: bytes
    frame_time: int
    flags: int
    frame: int
    unknown: bytes


@dataclass
class KonungMap:
    # Номера тайлов GRAPH.RES с единицы, 0 — пусто; [строка][столбец]
    ground: list[list[int]]
    overlay: list[list[int]]
    # Байт проходимости на клетку мелкой сетки [строка][столбец]
    foot: list[bytes]
    decorations: list[Decoration]
    objects: list[MapObject]


@dataclass
class Lights:
    tiles: list[bytes]
    fine: list[bytes]


def tile_position(column: int, row: int) -> tuple[int, int]:
    """Левый верхний угол тайла клетки на картинке карты."""
    return column * TILE_STEP_X + (row & 1) * ODD_ROW_SHIFT, row * TILE_STEP_Y


def is_blocked(foot: int) -> bool:
    return foot & FOOT_BLOCKED == FOOT_BLOCKED


def fine_cells(column: int, row: int) -> tuple[tuple[int, int], ...]:
    """Четыре клетки мелкой сетки в ромбе тайла: верхняя, левая, правая, нижняя.

    Ромб клетки мелкой сетки — 58 × 32, вчетверо меньше ромба тайла, и каждая
    клетка лежит ровно в одном тайле. Клетки за краем мелкой сетки тоже
    возвращаются — их отбрасывает вызывающий.
    """
    x, y = 2 * column + (row & 1), 2 * row
    return (x, y), (x, y + 1), (x + 1, y + 1), (x, y + 2)


def parse(data: bytes) -> KonungMap:
    if len(data) != MAP_SIZE:
        raise ValueError(f"карта {len(data)} байт, ожидалось {MAP_SIZE}")
    reader = Reader(data)

    ground, overlay = [], []
    for _ in range(HEIGHT):
        row = reader.bytes(WIDTH * 2)
        ground.append(list(row[0::2]))
        overlay.append(list(row[1::2]))

    foot = [reader.bytes(FINE_WIDTH) for _ in range(FINE_HEIGHT)]

    decorations = []
    for slot in range(MAX_DECORATIONS):
        tile = reader.s32()
        unknown = reader.bytes(8)
        x, y = reader.s16(), reader.s16()
        if tile >= 0:
            decorations.append(Decoration(slot, tile, x, y, unknown))

    objects = []
    for slot in range(MAX_OBJECTS):
        start = reader.pos
        object_id = reader.s32()
        if object_id < 0:
            reader.seek(start + OBJECT_SIZE)
            continue
        unknown = reader.bytes(4)
        flames = reader.bytes(FLAME_SLOTS)
        frame_time = reader.s16()
        x, y = reader.s16(), reader.s16()
        flags, frame = reader.u8(), reader.u8()
        objects.append(MapObject(slot, object_id, x, y, flames, frame_time, flags, frame, unknown))

    return KonungMap(ground, overlay, foot, decorations, objects)


def serialize(konung_map: KonungMap) -> bytes:
    """Карта обратно в формат Konung.N.

    Декорации и объекты пишутся подряд с нулевого слота в порядке списков,
    остальные слоты — байты 0xFF: так устроены все карты дистрибутива,
    и parse → serialize возвращает их байт в байт.
    """
    if len(konung_map.decorations) > MAX_DECORATIONS:
        raise ValueError(f"декораций {len(konung_map.decorations)}, максимум {MAX_DECORATIONS}")
    if len(konung_map.objects) > MAX_OBJECTS:
        raise ValueError(f"объектов {len(konung_map.objects)}, максимум {MAX_OBJECTS}")

    out = bytearray()
    for ground_row, overlay_row in zip(konung_map.ground, konung_map.overlay, strict=True):
        for ground, overlay in zip(ground_row, overlay_row, strict=True):
            out += bytes((ground, overlay))
    for row in konung_map.foot:
        out += row

    for decoration in konung_map.decorations:
        out += struct.pack("<i", decoration.tile) + decoration.unknown
        out += struct.pack("<hh", decoration.x, decoration.y)
    out += b"\xff" * DECORATION_SIZE * (MAX_DECORATIONS - len(konung_map.decorations))

    for placed in konung_map.objects:
        out += struct.pack("<i", placed.object_id) + placed.unknown + placed.flames
        out += struct.pack(
            "<hhhBB", placed.frame_time, placed.x, placed.y, placed.flags, placed.frame
        )
    out += b"\xff" * OBJECT_SIZE * (MAX_OBJECTS - len(konung_map.objects))

    if len(out) != MAP_SIZE:
        raise ValueError(f"получилось {len(out)} байт вместо {MAP_SIZE}: неверный размер слоёв")
    return bytes(out)


def parse_lights(data: bytes) -> Lights:
    if len(data) != LIGHTS_SIZE:
        raise ValueError(f"свет {len(data)} байт, ожидалось {LIGHTS_SIZE}")
    reader = Reader(data)
    tiles = [reader.bytes(WIDTH) for _ in range(HEIGHT)]
    fine = [reader.bytes(FINE_WIDTH) for _ in range(FINE_HEIGHT)]
    return Lights(tiles, fine)


def find(game_dir: Path, prefix: str, number: int) -> Path:
    """Файл вида Konung.N/KONUNG.N без учёта регистра (в дистрибутиве он разный)."""
    name = f"{prefix}.{number}".lower()
    for path in game_dir.iterdir():
        if path.name.lower() == name:
            return path
    raise FileNotFoundError(f"{game_dir}: нет {prefix}.{number}")


def map_numbers(game_dir: Path) -> list[int]:
    numbers = []
    for path in game_dir.iterdir():
        stem, _, suffix = path.name.partition(".")
        if stem.lower() == "konung" and suffix.isdigit():
            numbers.append(int(suffix))
    return sorted(numbers)


def load(game_dir: Path, number: int) -> KonungMap:
    return parse(find(game_dir, "konung", number).read_bytes())
