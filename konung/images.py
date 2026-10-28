"""Перевод декодированных картинок в Pillow."""

from PIL import Image

from konung import rle
from konung.palette import rgb555


def indexed_to_rgba(image: rle.IndexedImage, palette: list[tuple[int, int, int]]) -> Image.Image:
    rgba = bytearray(image.width * image.height * 4)
    for i, (index, alpha) in enumerate(zip(image.indices, image.alpha, strict=True)):
        if alpha:
            rgba[i * 4 : i * 4 + 3] = bytes(palette[index])
            rgba[i * 4 + 3] = alpha
    return Image.frombytes("RGBA", (image.width, image.height), bytes(rgba))


def rgb555_to_rgba(image: rle.RgbImage) -> Image.Image:
    rgba = bytearray(image.width * image.height * 4)
    for i, (color, alpha) in enumerate(zip(image.colors, image.alpha, strict=True)):
        if alpha:
            rgba[i * 4 : i * 4 + 3] = bytes(rgb555(color))
            rgba[i * 4 + 3] = alpha
    return Image.frombytes("RGBA", (image.width, image.height), bytes(rgba))
