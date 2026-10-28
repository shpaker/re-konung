"""Цвета RGB555: 0RRRRRGG GGGBBBBB, младший байт первым."""


def expand5(value: int) -> int:
    """5 бит → 8 бит: старшие биты повторяются в младших, 31 → 255."""
    return (value << 3) | (value >> 2)


def rgb555(color: int) -> tuple[int, int, int]:
    return expand5((color >> 10) & 0x1F), expand5((color >> 5) & 0x1F), expand5(color & 0x1F)
