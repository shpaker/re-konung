"""Game.N — начальное состояние новой игры: персонажи, отряды, клады, переходы
и деревни на всех картах (docs/k1/formats/game-state.md).

Места — клетки мелкой сетки 160×320. В файле у них сначала y, потом x,
в dataclass — как принято у нас: x, y.
"""

import struct
from dataclasses import dataclass
from pathlib import Path

from konung.k1 import map_file

ITEM_SLOTS = 8192
GROUP_SLOTS = 200
LOOT_SLOTS = 500
EXIT_SLOTS = 250
CHARACTER_SLOTS = 2000
VILLAGE_SLOTS = 20

ITEM = struct.Struct("<bbbb4sfhBB")
GROUP = struct.Struct("<hih8hBBBBBBBB")
LOOT = struct.Struct("<8shhhhhh22h")
EXIT = struct.Struct("<BBBbBhhhhhh")
CHARACTER = struct.Struct("<BBBBBBhhhhhBBB32sBiii9h2s9h6h32H5H10hBBBBBBBBi36s")
VILLAGE_HEAD = struct.Struct("<BBBB5i")
BUILDING = struct.Struct("<bbhhh")
JOB = struct.Struct("<BBhhh")
VILLAGE_TAIL = struct.Struct("<5i3h25h32h23hBBb")

BUILDING_SLOTS = 119
JOBS = 5
# Рабочие места деревни по порядку (Slavik game_locale.cpp, Locale::Jobs)
JOB_ROLES = ("elder", "healer", "merchant", "smith", "voevoda")

VILLAGE_SIZE = (
    VILLAGE_HEAD.size + BUILDING_SLOTS * BUILDING.size + JOBS * JOB.size + VILLAGE_TAIL.size
)
GAME_STATE_SIZE = (
    ITEM_SLOTS * ITEM.size
    + GROUP_SLOTS * GROUP.size
    + LOOT_SLOTS * LOOT.size
    + EXIT_SLOTS * EXIT.size
    + CHARACTER_SLOTS * CHARACTER.size
    + VILLAGE_SLOTS * VILLAGE_SIZE
)

# Колчан со стрелами: концентрация у него — целое число стрел, у остальных — float
# (Slavik game_item.cpp:185); название — запись 30–33 таблицы снаряжения
QUIVER_ITEM_TYPE = 12

# Виды отрядов (байт 0x1E); бродячих игра ставит в случайную клетку
# прямоугольника появления, остальных — на место из записи персонажа
PLAYER_GROUP = 0x20
FIRST_FIXED_GROUP = 0x51
VILLAGE_GROUP_BIT = 0x80


@dataclass
class Rect:
    """Прямоугольник в мелкой сетке; обе границы входят."""

    left: int
    top: int
    right: int
    bottom: int


@dataclass
class Item:
    slot: int
    type_id: int
    bonus_id: int
    special_id: int
    info_id: int
    # 4 байта как есть: s32 у колчана, f32 у остальных (value — для чтения)
    concentration: bytes
    weight: float
    poison: int
    flags: int
    unknown: int

    @property
    def concentration_value(self) -> int | float:
        fmt = "<i" if self.type_id == QUIVER_ITEM_TYPE else "<f"
        return struct.unpack(fmt, self.concentration)[0]


@dataclass
class Group:
    """Отряд: персонажи first_character … first_character + size − 1 на карте map_id."""

    slot: int
    first_character: int
    respawn_timer: int
    growth_timer: int
    spawn: Rect
    leash: Rect
    max_size: int
    growth_limit: int
    target_group: int
    map_id: int
    size: int
    ai_state: int
    kind: int
    unknown: int

    @property
    def is_roaming(self) -> bool:
        """Бродячий отряд: игра расставляет его членов случайно в spawn."""
        return self.kind < FIRST_FIXED_GROUP and not self.kind & VILLAGE_GROUP_BIT

    @property
    def members(self) -> range:
        return range(self.first_character, self.first_character + self.size)


@dataclass
class Loot:
    slot: int
    x: int
    y: int
    gold: int
    map_id: int
    items: tuple[int, ...]
    # Поля, которые игра считает при загрузке: пиксели, картинка, номер на карте
    pixels: bytes
    image: int
    loot_id: int


@dataclass
class Exit:
    slot: int
    map_id: int
    target_map: int
    zone: Rect
    arrival_x: int
    arrival_y: int
    direction: int
    unknown: bytes


@dataclass
class Character:
    slot: int
    state: int
    direction: int
    animation_flags: bytes
    class_id: int
    group: int
    enemy: int
    x: int
    y: int
    move_x: int
    move_y: int
    frame: int
    unknown_11: int
    weapon_slot: int
    path: bytes
    level: int
    gold: int
    free_points: int
    experience: int
    skills: tuple[int, ...]
    unknown_52: bytes
    attributes: tuple[int, ...]
    equipment: tuple[int, ...]
    inventory: tuple[int, ...]
    accessories: tuple[int, ...]
    misc: tuple[int, ...]
    unknown_d0: int
    flags: int
    unknown_d2: bytes
    name_id: int
    nick_id: int
    sprite_set: int
    frame_count: int
    palette_offset: int
    # Пиксельные позиции и размер картинки: их считает игра, в файлах нули
    runtime: bytes

    @property
    def has_dynamic_sprite(self) -> bool:
        """Бит 0x40 класса: спрайт — динамический объект OBJECTS.RES, а не модель GRAPH.RES.

        Так рисуются монстры, а ещё женщины и дети в деревнях.
        """
        return bool(self.class_id & 0x40)


@dataclass
class Building:
    info_id: int
    state: int
    # Номер объекта Konung.N той же карты
    object_index: int
    unknown: int
    countdown: int


@dataclass
class Job:
    role: str
    direction: int
    unknown: int
    character: int
    x: int
    y: int


@dataclass
class Village:
    slot: int
    building_counts: tuple[int, int]
    elder_group: int
    map_id: int
    timers: tuple[int, ...]
    buildings: list[Building]
    jobs: list[Job]
    # Всё после рабочих мест: таймеры, товары знахаря, купца и кузнеца, флаги
    tail: tuple[int, ...]


@dataclass
class GameState:
    # Пустые слоты — None: у предметов это 16 × 0xFF, у остальных — нули
    items: list[Item | None]
    groups: list[Group | None]
    loot: list[Loot]
    exits: list[Exit]
    characters: list[Character | None]
    villages: list[Village | None]


def _records(data: bytes, pos: int, count: int, size: int) -> list[bytes]:
    return [data[pos + i * size : pos + (i + 1) * size] for i in range(count)]


def _item(slot: int, raw: bytes) -> Item | None:
    if raw == b"\xff" * ITEM.size:
        return None
    return Item(slot, *ITEM.unpack(raw))


def _group(slot: int, raw: bytes) -> Group | None:
    if raw == bytes(GROUP.size):
        return None
    first, respawn, growth, *rects, max_size, limit, target, map_id, size, ai, kind, unknown = (
        GROUP.unpack(raw)
    )
    # Прямоугольники в файле вперемешку: top, top, bottom, bottom, left, left, right, right
    spawn = Rect(rects[4], rects[0], rects[6], rects[2])
    leash = Rect(rects[5], rects[1], rects[7], rects[3])
    return Group(
        slot, first, respawn, growth, spawn, leash, max_size, limit, target, map_id, size, ai,
        kind, unknown,
    )  # fmt: skip


def _loot(slot: int, raw: bytes) -> Loot:
    pixels, y, x, image, loot_id, gold, map_id, *items = LOOT.unpack(raw)
    return Loot(slot, x, y, gold, map_id, tuple(items), pixels, image, loot_id)


def _exit(slot: int, raw: bytes) -> Exit:
    first, second, direction, map_id, target, arrival_y, arrival_x, top, left, bottom, right = (
        EXIT.unpack(raw)
    )
    zone = Rect(left, top, right, bottom)
    return Exit(slot, map_id, target, zone, arrival_x, arrival_y, direction, bytes((first, second)))


def _character(slot: int, raw: bytes) -> Character | None:
    if raw == bytes(CHARACTER.size):
        return None
    v = CHARACTER.unpack(raw)
    return Character(
        slot=slot,
        state=v[0],
        direction=v[1],
        animation_flags=bytes(v[2:4]),
        class_id=v[4],
        group=v[5],
        enemy=v[6],
        y=v[7],
        x=v[8],
        move_y=v[9],
        move_x=v[10],
        frame=v[11],
        unknown_11=v[12],
        weapon_slot=v[13],
        path=v[14],
        level=v[15],
        gold=v[16],
        free_points=v[17],
        experience=v[18],
        skills=v[19:28],
        unknown_52=v[28],
        attributes=v[29:38],
        equipment=v[38:44],
        inventory=v[44:76],
        accessories=v[76:81],
        misc=v[81:91],
        unknown_d0=v[91],
        flags=v[92],
        unknown_d2=bytes(v[93:95]),
        name_id=v[95],
        nick_id=v[96],
        sprite_set=v[97],
        frame_count=v[98],
        palette_offset=v[99],
        runtime=v[100],
    )


def _village(slot: int, raw: bytes) -> Village | None:
    if raw == bytes(VILLAGE_SIZE):
        return None
    pos = 0
    first, second, elder, map_id, *timers = VILLAGE_HEAD.unpack_from(raw, pos)
    pos += VILLAGE_HEAD.size
    buildings = []
    for _ in range(BUILDING_SLOTS):
        buildings.append(Building(*BUILDING.unpack_from(raw, pos)))
        pos += BUILDING.size
    jobs = []
    for role in JOB_ROLES:
        direction, unknown, character, y, x = JOB.unpack_from(raw, pos)
        jobs.append(Job(role, direction, unknown, character, x, y))
        pos += JOB.size
    tail = VILLAGE_TAIL.unpack_from(raw, pos)
    return Village(slot, (first, second), elder, map_id, tuple(timers), buildings, jobs, tail)


def parse(data: bytes) -> GameState:
    if len(data) != GAME_STATE_SIZE:
        raise ValueError(f"Game.N {len(data)} байт, ожидалось {GAME_STATE_SIZE}")
    pos = 0

    def block(count: int, size: int) -> list[bytes]:
        nonlocal pos
        records = _records(data, pos, count, size)
        pos += count * size
        return records

    items = [_item(i, raw) for i, raw in enumerate(block(ITEM_SLOTS, ITEM.size))]
    groups = [_group(i, raw) for i, raw in enumerate(block(GROUP_SLOTS, GROUP.size))]
    # Клады и переходы идут подряд с нулевого слота; список обрывает первая
    # запись с картой 0, как в игре (хвост в файлах дистрибутива — нули)
    loot = []
    for i, raw in enumerate(block(LOOT_SLOTS, LOOT.size)):
        entry = _loot(i, raw)
        if entry.map_id == 0:
            break
        loot.append(entry)
    exits = []
    for i, raw in enumerate(block(EXIT_SLOTS, EXIT.size)):
        entry = _exit(i, raw)
        if entry.target_map == 0:
            break
        exits.append(entry)
    characters = [
        _character(i, raw) for i, raw in enumerate(block(CHARACTER_SLOTS, CHARACTER.size))
    ]
    villages = [_village(i, raw) for i, raw in enumerate(block(VILLAGE_SLOTS, VILLAGE_SIZE))]
    return GameState(items, groups, loot, exits, characters, villages)


def _pack_item(item: Item | None) -> bytes:
    if item is None:
        return b"\xff" * ITEM.size
    return ITEM.pack(
        item.type_id, item.bonus_id, item.special_id, item.info_id, item.concentration,
        item.weight, item.poison, item.flags, item.unknown,
    )  # fmt: skip


def _pack_group(group: Group | None) -> bytes:
    if group is None:
        return bytes(GROUP.size)
    spawn, leash = group.spawn, group.leash
    return GROUP.pack(
        group.first_character, group.respawn_timer, group.growth_timer,
        spawn.top, leash.top, spawn.bottom, leash.bottom,
        spawn.left, leash.left, spawn.right, leash.right,
        group.max_size, group.growth_limit, group.target_group, group.map_id, group.size,
        group.ai_state, group.kind, group.unknown,
    )  # fmt: skip


def _pack_loot(loot: Loot) -> bytes:
    return LOOT.pack(
        loot.pixels, loot.y, loot.x, loot.image, loot.loot_id, loot.gold, loot.map_id, *loot.items
    )


def _pack_exit(exit_: Exit) -> bytes:
    zone = exit_.zone
    return EXIT.pack(
        *exit_.unknown, exit_.direction, exit_.map_id, exit_.target_map,
        exit_.arrival_y, exit_.arrival_x, zone.top, zone.left, zone.bottom, zone.right,
    )  # fmt: skip


def _pack_character(c: Character | None) -> bytes:
    if c is None:
        return bytes(CHARACTER.size)
    return CHARACTER.pack(
        c.state, c.direction, *c.animation_flags, c.class_id, c.group, c.enemy,
        c.y, c.x, c.move_y, c.move_x, c.frame, c.unknown_11, c.weapon_slot, c.path,
        c.level, c.gold, c.free_points, c.experience, *c.skills, c.unknown_52, *c.attributes,
        *c.equipment, *c.inventory, *c.accessories, *c.misc, c.unknown_d0, c.flags,
        *c.unknown_d2, c.name_id, c.nick_id, c.sprite_set, c.frame_count, c.palette_offset,
        c.runtime,
    )  # fmt: skip


def _pack_village(village: Village | None) -> bytes:
    if village is None:
        return bytes(VILLAGE_SIZE)
    out = bytearray(
        VILLAGE_HEAD.pack(*village.building_counts, village.elder_group, village.map_id,
                          *village.timers)
    )  # fmt: skip
    for b in village.buildings:
        out += BUILDING.pack(b.info_id, b.state, b.object_index, b.unknown, b.countdown)
    for job in village.jobs:
        out += JOB.pack(job.direction, job.unknown, job.character, job.y, job.x)
    out += VILLAGE_TAIL.pack(*village.tail)
    return bytes(out)


def serialize(state: GameState) -> bytes:
    """Состояние обратно в формат Game.N.

    Клады и переходы пишутся подряд с нулевого слота в порядке списков,
    остальные слоты — нули: так устроены все файлы дистрибутива.
    """
    if len(state.loot) > LOOT_SLOTS:
        raise ValueError(f"кладов {len(state.loot)}, максимум {LOOT_SLOTS}")
    if len(state.exits) > EXIT_SLOTS:
        raise ValueError(f"переходов {len(state.exits)}, максимум {EXIT_SLOTS}")
    for loot in state.loot:
        if loot.map_id == 0:
            raise ValueError(f"клад {loot.slot}: карта 0 обрывает список")
    for exit_ in state.exits:
        if exit_.target_map == 0:
            raise ValueError(f"переход {exit_.slot}: целевая карта 0 обрывает список")

    out = bytearray()
    out += b"".join(_pack_item(item) for item in state.items)
    out += b"".join(_pack_group(group) for group in state.groups)
    out += b"".join(_pack_loot(loot) for loot in state.loot)
    out += bytes(LOOT.size * (LOOT_SLOTS - len(state.loot)))
    out += b"".join(_pack_exit(exit_) for exit_ in state.exits)
    out += bytes(EXIT.size * (EXIT_SLOTS - len(state.exits)))
    out += b"".join(_pack_character(c) for c in state.characters)
    out += b"".join(_pack_village(village) for village in state.villages)
    if len(out) != GAME_STATE_SIZE:
        raise ValueError(f"получилось {len(out)} байт вместо {GAME_STATE_SIZE}")
    return bytes(out)


@dataclass
class MapEntities:
    """Всё из Game.N, что стоит на одной карте."""

    # Отряды этой карты, кроме отряда игрока; у бродячих важен прямоугольник
    # появления, у остальных — места персонажей
    groups: list[Group]
    # Персонажи с местом из файла: NPC и жители
    characters: list[Character]
    # Отряд игрока, если эта карта — стартовая
    start: Group | None
    loot: list[Loot]
    exits: list[Exit]
    # Переходы с других карт, которые ведут сюда: важно их место прибытия
    arrivals: list[Exit]
    jobs: list[tuple[Village, Job]]


def start_map(hero: int) -> int:
    """Карта, где начинается новая игра из Game.<hero>.

    Карту в отряде игрока игра не читает (во всех файлах там 1), а считает
    по номеру героя (Slavik game.cpp:2458).
    """
    return 3 * hero + 1


def on_map(state: GameState, number: int, hero: int | None = None) -> MapEntities:
    """Что стоит на карте number; hero — номер Game.N, чтобы найти точку старта."""
    player = state.groups[0]
    start = player if hero is not None and start_map(hero) == number else None
    groups = [
        g for g in state.groups if g is not None and g.kind != PLAYER_GROUP and g.map_id == number
    ]
    characters = []
    for group in groups:
        if group.is_roaming:
            continue
        for index in group.members:
            character = state.characters[index]
            if character is not None:
                characters.append(character)
    jobs = [
        (village, job)
        for village in state.villages
        if village is not None and village.map_id == number
        for job in village.jobs
    ]
    return MapEntities(
        groups=groups,
        characters=characters,
        start=start,
        loot=[loot for loot in state.loot if loot.map_id == number],
        exits=[e for e in state.exits if e.map_id == number],
        arrivals=[e for e in state.exits if e.target_map == number],
        jobs=jobs,
    )


# Клетка мелкой сетки: шаг 58 по x, 16 по строке; нечётные строки сдвинуты
FINE_STEP_X = map_file.TILE_STEP_X // 2
FINE_STEP_Y = map_file.TILE_STEP_Y // 2


def character_position(x: int, y: int) -> tuple[int, int]:
    """Точка персонажа или клада (клетка мелкой сетки) в пикселях карты.

    По Slavik — середина клетки: на (58, 16) правее и ниже позиции объекта карты.
    """
    return x * FINE_STEP_X + (2 - (y & 1)) * (FINE_STEP_X // 2), y * FINE_STEP_Y + FINE_STEP_Y


def character_cell(x: float, y: float) -> tuple[int, int]:
    """Обратно: ближайшая клетка мелкой сетки к точке на карте."""
    row = round((y - FINE_STEP_Y) / FINE_STEP_Y)
    column = round((x - (2 - (row & 1)) * (FINE_STEP_X // 2)) / FINE_STEP_X)
    return column, row


# Клетка вокруг своей точки: ±29 по x, ±8 по y. Из-за сдвига строк
# левый край прямоугольника — у нечётных строк, правый — у чётных
CELL_HALF_X = FINE_STEP_X // 2
CELL_HALF_Y = FINE_STEP_Y // 2
RECT_RIGHT_EDGE = FINE_STEP_X + CELL_HALF_X


def rect_pixels(rect: Rect) -> tuple[int, int, int, int]:
    """Прямоугольник клеток в пикселях карты: left, top, right, bottom по краям клеток."""
    return (
        rect.left * FINE_STEP_X,
        rect.top * FINE_STEP_Y + FINE_STEP_Y - CELL_HALF_Y,
        rect.right * FINE_STEP_X + RECT_RIGHT_EDGE,
        rect.bottom * FINE_STEP_Y + FINE_STEP_Y + CELL_HALF_Y,
    )


def rect_cells(left: float, top: float, right: float, bottom: float) -> Rect:
    """Обратно: прямоугольник в пикселях — клетки, которые он накрывает."""
    return Rect(
        round(left / FINE_STEP_X),
        round((top - FINE_STEP_Y + CELL_HALF_Y) / FINE_STEP_Y),
        round((right - RECT_RIGHT_EDGE) / FINE_STEP_X),
        round((bottom - FINE_STEP_Y - CELL_HALF_Y) / FINE_STEP_Y),
    )


def numbers(game_dir: Path, files: map_file.FileSet = map_file.GAME_FILES) -> list[int]:
    """Номера Game.N в папке — героев (без TGame.N обучения; с ним — files=TUTORIAL_FILES).

    Номер у TGame.N — тоже номер героя: TGame.1 начинается на Tutorial.4 = start_map(1).
    """
    return map_file.numbered(game_dir, files.state)


def load(game_dir: Path, number: int, files: map_file.FileSet = map_file.GAME_FILES) -> GameState:
    return parse(map_file.find(game_dir, files.state, number).read_bytes())
