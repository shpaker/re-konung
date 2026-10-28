"""Построчное RLE-сжатие спрайтов (docs/k1/formats/rle.md).

Заголовок: ширина s16, высота s16, длины строк s16 × высота.
Строка — последовательность команд, байт n:
- 0 — конец строки;
- n & 0x80 — пропустить n & 0x7F прозрачных пикселей;
- иначе — n пикселей следом (байт индекса палитры или u16 RGB555 — зависит от вида).
"""

from dataclasses import dataclass

from konung.binary import Reader


@dataclass
class IndexedImage:
    """Картинка с индексами палитры; alpha — 0 (прозрачно) или 255 на пиксель."""

    width: int
    height: int
    indices: bytearray
    alpha: bytearray


@dataclass
class RgbImage:
    """Картинка с цветами RGB555 на пиксель; alpha — 0 или 255."""

    width: int
    height: int
    colors: list[int]
    alpha: bytearray


def _header(reader: Reader) -> tuple[int, int]:
    width = reader.s16()
    height = reader.s16()
    if width < 0 or height < 0:
        raise ValueError(f"размер {width}×{height} по смещению {reader.pos - 4}")
    reader.skip(height * 2)
    return width, height


def _rows(reader: Reader, width: int, height: int, pixel_size: int):
    """Перебирает (y, x, count, смещение данных) для каждой команды с пикселями."""
    for y in range(height):
        x = 0
        while True:
            n = reader.u8()
            if n == 0:
                break
            if n & 0x80:
                x += n & 0x7F
                continue
            if x + n > width:
                raise ValueError(f"строка {y}: {x}+{n} пикселей больше ширины {width}")
            yield y, x, n, reader.pos
            reader.skip(n * pixel_size)
            x += n


def decode_indexed(reader: Reader) -> IndexedImage:
    """RL8: пиксели — индексы палитры."""
    width, height = _header(reader)
    indices = bytearray(width * height)
    alpha = bytearray(width * height)
    for y, x, n, pos in _rows(reader, width, height, 1):
        start = y * width + x
        indices[start : start + n] = reader.data[pos : pos + n]
        alpha[start : start + n] = b"\xff" * n
    return IndexedImage(width, height, indices, alpha)


def decode_rgb555(reader: Reader) -> RgbImage:
    """RL16: пиксели — цвета RGB555 (курсоры, огни)."""
    width, height = _header(reader)
    colors = [0] * (width * height)
    alpha = bytearray(width * height)
    for y, x, n, pos in _rows(reader, width, height, 2):
        start = y * width + x
        for i in range(n):
            colors[start + i] = int.from_bytes(reader.data[pos + i * 2 : pos + i * 2 + 2], "little")
        alpha[start : start + n] = b"\xff" * n
    return RgbImage(width, height, colors, alpha)


def decode_shadow(reader: Reader) -> IndexedImage:
    """Тень: пиксели без данных, важна только маска."""
    width, height = _header(reader)
    indices = bytearray(width * height)
    alpha = bytearray(width * height)
    for y, x, n, _ in _rows(reader, width, height, 0):
        start = y * width + x
        alpha[start : start + n] = b"\xff" * n
    return IndexedImage(width, height, indices, alpha)
