"""LIGHTS.RES — маски света (docs/k1/formats/lights-res.md).

Заголовка нет: маска 64 × 64, маска 64 × 43 и 19 масок тайлов 114 × 64, байт на
пиксель — яркость. Номер маски тайла хранит первый блок LIGHTS.N (map_file.Lights).
"""

from dataclasses import dataclass
from pathlib import Path

from PIL import Image

TILE_MASKS = 19
TILE_MASK_WIDTH = 114
TILE_MASK_HEIGHT = 64
# Две маски в начале файла: Slavik их читает, но не использует, назначение неизвестно
FIRST_SIZE = (64, 64)
SECOND_SIZE = (64, 43)
SIZE = (
    FIRST_SIZE[0] * FIRST_SIZE[1]
    + SECOND_SIZE[0] * SECOND_SIZE[1]
    + TILE_MASKS * TILE_MASK_WIDTH * TILE_MASK_HEIGHT
)
# Slavik переводит яркость в 0–255 как 255 / 100 · v; в файле она не больше 43
BRIGHTNESS_SCALE = 100


@dataclass
class LightsRes:
    first: bytes
    second: bytes
    # Маски тайлов: в LIGHTS.N маска i записана как i + 1, 0 — света нет
    tiles: list[bytes]


def parse(data: bytes) -> LightsRes:
    if len(data) != SIZE:
        raise ValueError(f"LIGHTS.RES {len(data)} байт, ожидалось {SIZE}")
    second_at = FIRST_SIZE[0] * FIRST_SIZE[1]
    tiles_at = second_at + SECOND_SIZE[0] * SECOND_SIZE[1]
    mask = TILE_MASK_WIDTH * TILE_MASK_HEIGHT
    tiles = [data[tiles_at + i * mask : tiles_at + (i + 1) * mask] for i in range(TILE_MASKS)]
    return LightsRes(data[:second_at], data[second_at:tiles_at], tiles)


def load(path: Path) -> LightsRes:
    return parse(path.read_bytes())


def in_diamond(x: int, y: int) -> bool:
    """Пиксель маски внутри ромба тайла 114 × 64."""
    half_x, half_y = TILE_MASK_WIDTH / 2, TILE_MASK_HEIGHT / 2
    return abs(x + 0.5 - half_x) / half_x + abs(y + 0.5 - half_y) / half_y <= 1


def to_rgba(mask: bytes, color: tuple[int, int, int]) -> Image.Image:
    """Маска тайла цветом color, прозрачность — яркость, как её растягивает Slavik.

    Маска заполняет весь прямоугольник, а игра светит ею только пиксели своего
    тайла (Slavik `game.cpp:996-1003`), поэтому вне ромба она прозрачна.
    """
    alpha = bytes(
        value * 255 // BRIGHTNESS_SCALE
        if in_diamond(i % TILE_MASK_WIDTH, i // TILE_MASK_WIDTH)
        else 0
        for i, value in enumerate(mask)
    )
    image = Image.new("RGBA", (TILE_MASK_WIDTH, TILE_MASK_HEIGHT), color)
    image.putalpha(Image.frombytes("L", (TILE_MASK_WIDTH, TILE_MASK_HEIGHT), alpha))
    return image
