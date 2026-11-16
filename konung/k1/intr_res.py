"""Indexed interface images; format contract: docs/research/k1/formats/intr-res.md."""

import struct
from dataclasses import dataclass, field
from pathlib import Path

from konung import rle
from konung.binary import Reader

DIRECTORY_COUNT = 512
DATA_START = 8 + DIRECTORY_COUNT * 8
PALETTE_BANK_BYTES = 256 * 256 * 2


@dataclass(frozen=True)
class InterfaceImage:
    offset: int
    palette_offset: int
    image: rle.IndexedImage
    end: int


@dataclass(frozen=True)
class IntrRes:
    tag: int
    data_size: int
    directory: tuple[tuple[int, int], ...]
    images: tuple[InterfaceImage, ...]
    # Retain unused directory entries, row-length words, gaps and trailing bytes verbatim.
    data: bytes = field(repr=False)

    @property
    def size(self) -> int:
        return DATA_START + self.data_size


class _BoundedReader(Reader):
    def skip(self, count: int) -> None:
        if count < 0 or self.pos + count > len(self.data):
            raise ValueError("interface image extends past its encoded region")
        super().skip(count)


def parse(data: bytes) -> IntrRes:
    if len(data) < DATA_START:
        raise ValueError("truncated INTR header or directory")
    tag, data_size = struct.unpack_from("<II", data)
    end = DATA_START + data_size
    if end > len(data):
        raise ValueError("truncated INTR image block")
    directory = tuple(struct.iter_unpack("<ii", data[8:DATA_START]))
    active = []
    for offset, palette in directory:
        if offset == -1:
            break
        if not 0 <= offset < data_size:
            raise ValueError("interface image offset is outside the image block")
        if palette < 0 or palette % 2 or palette + 512 > PALETTE_BANK_BYTES:
            raise ValueError("interface palette is outside the GRAPH.RES color bank")
        active.append((offset, palette))
    boundaries = sorted({offset for offset, _ in active} | {data_size})
    limits = dict(zip(boundaries, boundaries[1:], strict=False))
    decoded = {}
    images = []
    for offset, palette in active:
        if offset not in decoded:
            reader = _BoundedReader(data[DATA_START + offset : DATA_START + limits[offset]])
            try:
                image = rle.decode_indexed(reader)
            except (struct.error, EOFError, ValueError) as error:
                raise ValueError(f"invalid INTR image at offset {offset}: {error}") from error
            decoded[offset] = image, DATA_START + offset + reader.pos
        image, image_end = decoded[offset]
        images.append(InterfaceImage(offset, palette, image, image_end))
    return IntrRes(tag, data_size, directory, tuple(images), data)


def load(path: Path) -> IntrRes:
    return parse(path.read_bytes())
