"""Konung.N — карта локации N; LIGHTS.N — её освещение (docs/k1/formats/map.md)."""

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
