"""GRAPH.RES — палитры, курсоры, тайлы земли и спрайты персонажей (docs/k1/formats/graph-res.md)."""

from dataclasses import dataclass, field
from pathlib import Path

from konung import rle
from konung.binary import Reader
from konung.palette import rgb555

PALETTE_BANK_COLORS = 256 * 256
CURSOR_COUNT = 11
TILE_SLOTS = 512
# Спрайты персонажей: 3 базовые модели и 24 вида снаряжения
CHARACTER_MODELS = 3
EQUIPMENT_SETS = 24
CHARACTER_BLOCKS = CHARACTER_MODELS + EQUIPMENT_SETS
# В блоке персонажа: 10 анимаций × 8 направлений × до 12 кадров, 768 картинок
CHARACTER_ANIMATIONS = 10
DIRECTIONS = 8
CHARACTER_FRAMES = 12
CHARACTER_IMAGES = 768


@dataclass
class Block:
    """Блок: u32 метка (смысл не ясен), u32 размер, данные; end — первый байт после блока.

    От чего считается размер, зависит от вида блока: у курсора — от конца
    заголовка, у тайлов — от конца таблицы, у спрайтов персонажа — от начала блока.
    """

    offset: int
    tag: int
    size: int
    end: int


@dataclass
class SpriteFrame:
    """Кадр анимации: номер картинки и смещения от точки персонажа."""

    image_id: int
    image_offset: tuple[int, int]
    shadow_offset: tuple[int, int]
    # Куда цепляется снаряжение (только у персонажей GRAPH.RES)
    weapon_offset: tuple[int, int] = (0, 0)


@dataclass
class Sprites:
    """Анимированный спрайт: [анимация][направление] → кадры; картинки — по запросу.

    Так устроены и персонажи GRAPH.RES, и динамические объекты OBJECTS.RES.
    Картинок сотни, поэтому они декодируются при первом обращении.
    """

    data: bytes = field(repr=False)
    # Начало картинок; смещения в таблице считаются от него
    images_start: int
    sequences: list[list[list[SpriteFrame]]]
    # Пары (тень, картинка) смещений от images_start; < 0 — нет
    table: list[tuple[int, int]]

    def image(self, image_id: int) -> rle.IndexedImage | None:
        offset = self.table[image_id][1]
        if offset < 0:
            return None
        return rle.decode_indexed(Reader(self.data, self.images_start + offset))

    def shadow(self, image_id: int) -> rle.IndexedImage | None:
        # Нулевое смещение тени игра тоже считает пустым (Slavik resources.cpp:91)
        offset = self.table[image_id][0]
        if offset <= 0:
            return None
        return rle.decode_shadow(Reader(self.data, self.images_start + offset))


def read_sprites(
    data: bytes, start: int, animations: int, frames: int, images: int, weapons: bool
) -> Sprites:
    """Заголовок спрайта с позиции start (сразу после метки и размера блока).

    Порядок: число кадров и номера картинок по анимациям, смещения оружия
    (если weapons), таблица картинок, смещения тени и картинки по кадрам.
    У персонажей GRAPH.RES таблица идёт после смещений оружия, у динамических
    объектов оружия нет (Slavik resources.cpp:20-102, 104-142).
    """
    reader = Reader(data, start)
    counts, ids = [], []
    for _ in range(animations * DIRECTIONS):
        counts.append(reader.s16())
        ids.append([reader.s16() for _ in range(frames)])
    weapon = [[(0, 0)] * frames for _ in range(animations * DIRECTIONS)]
    if weapons:
        weapon = [
            [(reader.s16(), reader.s16()) for _ in range(frames)]
            for _ in range(animations * DIRECTIONS)
        ]
    table = [(reader.s32(), reader.s32()) for _ in range(images)]
    sequences = []
    for animation in range(animations):
        row = []
        for direction in range(DIRECTIONS):
            index = animation * DIRECTIONS + direction
            sequence = []
            for frame in range(frames):
                shadow_offset = (reader.s16(), reader.s16())
                image_offset = (reader.s16(), reader.s16())
                if frame < counts[index]:
                    sequence.append(
                        SpriteFrame(
                            ids[index][frame], image_offset, shadow_offset, weapon[index][frame]
                        )
                    )
            row.append(sequence)
        sequences.append(row)
    return Sprites(data, reader.pos, sequences, table)


@dataclass
class Tile:
    index: int
    image: rle.IndexedImage
    palette_offset: int


@dataclass
class GraphRes:
    # Банк из 65536 цветов RGB555; палитра тайла — 256 цветов с palette_offset
    colors: list[int]
    cursors: list[rle.RgbImage]
    tiles: dict[int, Tile]
    tiles_block: Block
    character_blocks: list[Block] = field(default_factory=list)
    # Модели людей 0–2 (Character.sprite_set) и наборы снаряжения 0–23
    models: list[Sprites] = field(default_factory=list)
    equipment: list[Sprites] = field(default_factory=list)
    size: int = 0

    def palette(self, offset: int) -> list[tuple[int, int, int]]:
        return [rgb555(c) for c in self.colors[offset : offset + 256]]


def parse(data: bytes) -> GraphRes:
    reader = Reader(data)
    colors = [reader.u16() for _ in range(PALETTE_BANK_COLORS)]

    cursors = []
    for _ in range(CURSOR_COUNT):
        offset, tag, size = reader.pos, reader.u32(), reader.u32()
        block = Block(offset, tag, size, reader.pos + size)
        cursors.append(rle.decode_rgb555(reader))
        reader.seek(block.end)

    offset, tag, size = reader.pos, reader.u32(), reader.u32()
    entries = [(reader.s32(), reader.s32()) for _ in range(TILE_SLOTS)]
    data_start = reader.pos
    tiles_block = Block(offset, tag, size, data_start + size)
    tiles = {}
    for index, (offset, palette) in enumerate(entries):
        if offset < 0 or palette < 0:
            continue
        reader.seek(data_start + offset)
        # Смещение палитры записано в байтах от начала банка
        tiles[index] = Tile(index, rle.decode_indexed(reader), palette // 2)

    reader.seek(tiles_block.end)
    character_blocks, sprites = [], []
    for _ in range(CHARACTER_BLOCKS):
        offset, tag, size = reader.pos, reader.u32(), reader.u32()
        block = Block(offset, tag, size, offset + size)
        character_blocks.append(block)
        sprites.append(
            read_sprites(
                data, reader.pos, CHARACTER_ANIMATIONS, CHARACTER_FRAMES, CHARACTER_IMAGES, True
            )
        )
        reader.seek(block.end)

    return GraphRes(
        colors,
        cursors,
        tiles,
        tiles_block,
        character_blocks,
        models=sprites[:CHARACTER_MODELS],
        equipment=sprites[CHARACTER_MODELS:],
        size=reader.pos,
    )


def load(path: Path) -> GraphRes:
    return parse(path.read_bytes())
