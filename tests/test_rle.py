import pytest

from konung import rle
from konung.binary import Reader


def _encode(width: int, height: int, rows: list[bytes]) -> bytes:
    header = width.to_bytes(2, "little") + height.to_bytes(2, "little")
    lengths = b"".join(len(row).to_bytes(2, "little") for row in rows)
    return header + lengths + b"".join(rows)


def test_indexed_skip_and_literal():
    # Строка 0: пропуск 1, два пикселя 7 и 9; строка 1: три пикселя
    data = _encode(3, 2, [bytes([0x81, 2, 7, 9, 0]), bytes([3, 1, 2, 3, 0])])
    image = rle.decode_indexed(Reader(data))
    assert (image.width, image.height) == (3, 2)
    assert list(image.indices) == [0, 7, 9, 1, 2, 3]
    assert list(image.alpha) == [0, 255, 255, 255, 255, 255]


def test_rgb555_reads_two_bytes_per_pixel():
    red = (31 << 10).to_bytes(2, "little")
    data = _encode(2, 1, [bytes([0x81, 1]) + red + bytes([0])])
    image = rle.decode_rgb555(Reader(data))
    assert image.colors == [0, 31 << 10]
    assert list(image.alpha) == [0, 255]


def test_shadow_has_no_pixel_data():
    data = _encode(4, 1, [bytes([2, 0x81, 1, 0])])
    image = rle.decode_shadow(Reader(data))
    assert list(image.alpha) == [255, 255, 0, 255]


def test_reader_stops_after_image():
    data = _encode(1, 1, [bytes([1, 5, 0])]) + b"tail"
    reader = Reader(data)
    rle.decode_indexed(reader)
    assert data[reader.pos :] == b"tail"


def test_skip_past_row_width_is_an_error():
    # Ширина 2, пропуск на 3 пикселя
    data = _encode(2, 1, [bytes([0x83, 0])])
    with pytest.raises(ValueError, match="пропуск"):
        rle.decode_indexed(Reader(data))


def test_skip_exactly_to_row_width_is_allowed():
    # Пропуск ровно до конца строки, затем terminator
    data = _encode(2, 1, [bytes([0x82, 0])])
    image = rle.decode_indexed(Reader(data))
    assert list(image.alpha) == [0, 0]
