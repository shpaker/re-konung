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
CHARACTER_BLOCKS = 3 + 24


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
    character_blocks = []
    for _ in range(CHARACTER_BLOCKS):
        offset, tag, size = reader.pos, reader.u32(), reader.u32()
        block = Block(offset, tag, size, offset + size)
        character_blocks.append(block)
        reader.seek(block.end)

    return GraphRes(colors, cursors, tiles, tiles_block, character_blocks, size=reader.pos)


def load(path: Path) -> GraphRes:
    return parse(path.read_bytes())
