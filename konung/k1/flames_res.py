"""FLAMES.RES — кадры огней: факелы, костры, горны (docs/k1/formats/flames-res.md).

74 блока подряд: u32 метка, s32 размер, картинка RL16. Какие кадры образуют
анимацию огня какого вида, задаёт таблица в exe (konung/k1/executable.py).
"""

from dataclasses import dataclass
from pathlib import Path

from PIL import Image

from konung import rle
from konung.binary import Reader
from konung.palette import rgb555


@dataclass
class FlamesRes:
    frames: list[rle.RgbImage]
    # Метки блоков: у всех 0x05000000, смысл неизвестен
    tags: list[int]


def parse(data: bytes) -> FlamesRes:
    reader = Reader(data)
    frames, tags = [], []
    while reader.pos < len(data):
        tags.append(reader.u32())
        size = reader.s32()
        start = reader.pos
        frames.append(rle.decode_rgb555(reader))
        if reader.pos != start + size:
            raise ValueError(
                f"огонь {len(frames) - 1}: картинка {reader.pos - start} байт, в блоке {size}"
            )
    return FlamesRes(frames, tags)


def load(path: Path) -> FlamesRes:
    return parse(path.read_bytes())


def to_rgba(frame: rle.RgbImage) -> Image.Image:
    """Огонь светится: прозрачность — из яркости, a = (2r + g + b) / 4 (Slavik resources.cpp:632).

    Игра ещё умножает цвет на a; здесь цвет остаётся как есть, а a идёт в альфу —
    так огонь смешивается обычной прозрачностью (гипотеза, в игре не сверено).
    """
    pixels = bytearray()
    for color, alpha in zip(frame.colors, frame.alpha, strict=True):
        if not alpha:
            pixels += b"\0\0\0\0"
            continue
        r, g, b = rgb555(color)
        pixels += bytes((r, g, b, (2 * r + g + b) // 4))
    return Image.frombytes("RGBA", (frame.width, frame.height), bytes(pixels))
