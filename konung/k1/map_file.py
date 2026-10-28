"""Konung.N — карта локации N; LIGHTS.N — её освещение (docs/k1/formats/map.md)."""

import struct
from dataclasses import dataclass
from pathlib import Path

from konung.binary import Reader
from konung.k1.graph_res import TILE_SLOTS

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


@dataclass(frozen=True)
class FileSet:
    """Префиксы имён файлов одного набора карт: карта, её свет, начальные состояния.

    Номер у карты и света — номер локации, у состояния — номер героя.
    """

    map: str
    lights: str
    state: str


GAME_FILES = FileSet("konung", "lights", "game")
# Обучение, только в ENG: TUTORIAL.EXE собирает имена TUTORIAL.N, TLIGHTS.N
# и TGAME.N, а GRAPH.RES, Objects.res, LIGHTS.RES и FLAMES.RES у него общие с игрой.
# Источник — строки имён файлов в TUTORIAL.EXE (наш эксперимент)
TUTORIAL_FILES = FileSet("tutorial", "tlights", "tgame")


def file_set(map_name: str) -> FileSet:
    """Набор, к которому относится файл карты: tutorial.N — обучение, остальное — игра."""
    stem = map_name.partition(".")[0].lower()
    return TUTORIAL_FILES if stem == TUTORIAL_FILES.map else GAME_FILES


@dataclass
class Decoration:
    """Тайл из GRAPH.RES, поставленный в произвольную точку карты (в пикселях)."""

    slot: int
    tile: int
    x: int
    y: int
    unknown: bytes

    @property
    def is_raw(self) -> bool:
        """Запись с тайлом за пределами GRAPH.RES: её не нарисовать, она хранится как есть.

        Такая одна в Tutorial.4 и Tutorial.5 (слот 994). Запись пишется обратно
        в свой слот, а не подряд с обычными (docs/k1/formats/map.md, «Запись»).
        """
        return self.tile >= TILE_SLOTS


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
    # Номер маски LIGHTS.RES + 1 на клетку тайловой сетки, 0 — света нет [строка][столбец]
    tiles: list[bytes]
    # Тот же номер на 4 клетки мелкой сетки в ромбе тайла (fine_cells), у края — 0
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


def decoration_slots(decorations: list[Decoration]) -> list[Decoration | None]:
    """Декорации по слотам файла: сырые (is_raw) — в свой slot, обычные — по порядку
    списка в свободные слоты с нулевого; None — пустой слот."""
    if len(decorations) > MAX_DECORATIONS:
        raise ValueError(f"декораций {len(decorations)}, максимум {MAX_DECORATIONS}")
    slots: list[Decoration | None] = [None] * MAX_DECORATIONS
    for decoration in decorations:
        if not decoration.is_raw:
            continue
        if not 0 <= decoration.slot < MAX_DECORATIONS or slots[decoration.slot] is not None:
            raise ValueError(f"сырая декорация: слот {decoration.slot} занят или вне 0–999")
        slots[decoration.slot] = decoration
    free = (slot for slot, placed in enumerate(slots) if placed is None)
    for decoration in decorations:
        if not decoration.is_raw:
            slots[next(free)] = decoration
    return slots


def serialize(konung_map: KonungMap) -> bytes:
    """Карта обратно в формат Konung.N.

    Объекты и обычные декорации пишутся подряд с нулевого слота в порядке
    списков, сырые декорации — в свой слот (decoration_slots), остальные
    слоты — байты 0xFF: так устроены все карты дистрибутива и обучения,
    и parse → serialize возвращает их байт в байт.
    """
    decorations = decoration_slots(konung_map.decorations)
    if len(konung_map.objects) > MAX_OBJECTS:
        raise ValueError(f"объектов {len(konung_map.objects)}, максимум {MAX_OBJECTS}")

    out = bytearray()
    for ground_row, overlay_row in zip(konung_map.ground, konung_map.overlay, strict=True):
        for ground, overlay in zip(ground_row, overlay_row, strict=True):
            out += bytes((ground, overlay))
    for row in konung_map.foot:
        out += row

    for decoration in decorations:
        if decoration is None:
            out += b"\xff" * DECORATION_SIZE
            continue
        out += struct.pack("<i", decoration.tile) + decoration.unknown
        out += struct.pack("<hh", decoration.x, decoration.y)

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


def lights_fine(tiles: list[bytes]) -> list[bytes]:
    """Мелкий блок LIGHTS.N из тайлового: так он устроен во всех файлах дистрибутива."""
    fine = [bytearray(FINE_WIDTH) for _ in range(FINE_HEIGHT)]
    for row, values in enumerate(tiles):
        for column, value in enumerate(values):
            for x, y in fine_cells(column, row):
                if x < FINE_WIDTH and y < FINE_HEIGHT:
                    fine[y][x] = value
    return [bytes(row) for row in fine]


def serialize_lights(lights: Lights) -> bytes:
    """Свет обратно в LIGHTS.N; мелкий блок пересчитывается из тайлового."""
    out = b"".join(lights.tiles) + b"".join(lights_fine(lights.tiles))
    if len(out) != LIGHTS_SIZE:
        raise ValueError(f"получилось {len(out)} байт вместо {LIGHTS_SIZE}")
    return out


def lights_path(game_dir: Path, number: int, files: FileSet = GAME_FILES) -> Path | None:
    """LIGHTS.N карты (TLights.N у обучения); у карт 6, 8 и 46 его нет."""
    try:
        return find(game_dir, files.lights, number)
    except FileNotFoundError:
        return None


def find(game_dir: Path, prefix: str, number: int) -> Path:
    """Файл вида Konung.N/KONUNG.N без учёта регистра (в дистрибутиве он разный)."""
    name = f"{prefix}.{number}".lower()
    for path in game_dir.iterdir():
        if path.name.lower() == name:
            return path
    raise FileNotFoundError(f"{game_dir}: нет {prefix}.{number}")


def numbered(game_dir: Path, prefix: str) -> list[int]:
    """Номера файлов вида prefix.N в папке, без учёта регистра."""
    numbers = []
    for path in game_dir.iterdir():
        stem, _, suffix = path.name.partition(".")
        if stem.lower() == prefix and suffix.isdigit():
            numbers.append(int(suffix))
    return sorted(numbers)


def map_numbers(game_dir: Path, files: FileSet = GAME_FILES) -> list[int]:
    """Номера карт Konung.N (или Tutorial.N обучения) в папке игры."""
    return numbered(game_dir, files.map)


def load(game_dir: Path, number: int, files: FileSet = GAME_FILES) -> KonungMap:
    return parse(find(game_dir, files.map, number).read_bytes())
