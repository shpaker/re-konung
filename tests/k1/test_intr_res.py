"""Synthetic INTR bounds and local-data decoding, without resource excerpts."""

import struct

import pytest

from konung.k1 import executable, graph_res, intr_res


def _resource(*images, entries=None, trailing=b""):
    offsets = []
    position = 0
    for image in images:
        offsets.append((position, 0))
        position += len(image)
    directory = offsets if entries is None else entries
    directory = [*directory, *[(-1, -1)] * (intr_res.DIRECTORY_COUNT - len(directory))]
    return (
        struct.pack("<II", 0x12345678, position)
        + b"".join(struct.pack("<ii", *entry) for entry in directory)
        + b"".join(images)
        + trailing
    )


def _image(row=b"\x81\x02\x07\x09\x00", *, width=3):
    return struct.pack("<hhh", width, 1, len(row)) + row


def test_decode_preserves_metadata_unused_directory_and_trailing_bytes():
    data = _resource(_image(), trailing=b"unknown")
    resource = intr_res.parse(data)
    assert resource.data == data
    assert resource.tag == 0x12345678
    assert resource.directory[1:] == ((-1, -1),) * 511
    assert resource.size == len(data) - 7
    image = resource.images[0]
    assert image.end == resource.size
    assert list(image.image.indices) == [0, 7, 9]
    assert list(image.image.alpha) == [0, 255, 255]


def test_offsets_may_be_reordered_or_shared_with_separate_palettes():
    first, second = _image(), _image(b"\x03\x01\x02\x03\x00")
    resource = intr_res.parse(_resource(first, second, entries=[(len(first), 512), (0, 0), (0, 2)]))
    assert list(resource.images[0].image.indices) == [1, 2, 3]
    assert resource.images[1].image is resource.images[2].image
    assert [image.palette_offset for image in resource.images] == [512, 0, 2]


@pytest.mark.parametrize("cut", [0, 7, intr_res.DATA_START - 1, intr_res.DATA_START + 3])
def test_truncated_header_directory_and_body_are_rejected(cut):
    with pytest.raises(ValueError, match="truncated"):
        intr_res.parse(_resource(_image())[:cut])


@pytest.mark.parametrize("offset", [-2, 1000000])
def test_out_of_block_image_offset_is_rejected(offset):
    with pytest.raises(ValueError, match="offset"):
        intr_res.parse(_resource(_image(), entries=[(offset, 0)]))


@pytest.mark.parametrize("palette", [-2, 1, intr_res.PALETTE_BANK_BYTES - 510])
def test_invalid_palette_is_rejected(palette):
    with pytest.raises(ValueError, match="palette"):
        intr_res.parse(_resource(_image(), entries=[(0, palette)]))


@pytest.mark.parametrize(
    "image",
    [
        struct.pack("<hh", 3, 1),
        _image(b"\x03\x01\x02"),
        _image(b"\x03\x01\x02\x03"),
        _image(b"\x84\x00"),
        _image(b"\x04\x01\x02\x03\x04\x00"),
        _image(width=-1),
    ],
)
def test_malformed_rle_is_rejected(image):
    with pytest.raises(ValueError):
        intr_res.parse(_resource(image))


def test_rle_cannot_consume_the_next_image():
    first = _image(b"\x03\x01\x02\x03")
    with pytest.raises(ValueError, match="invalid INTR image"):
        intr_res.parse(_resource(first, _image()))


def test_all_local_interface_resources_decode_to_eof(k1_game, k1_file):
    tables = executable.load(k1_game)
    icons = {row.values[7] for row in tables.equipment}
    icons.update(row.values[2] for rows in tables.items.values() for row in rows)
    resources = sorted(path for path in k1_game.iterdir() if path.name.lower().startswith("intr"))
    assert len(resources) == (4 if k1_game.parent.name == "eng" else 2)
    graph = graph_res.load(k1_file("graph.res"))
    for path in resources:
        data = path.read_bytes()
        resource = intr_res.parse(data)
        assert resource.data == data
        assert resource.size == len(data)
        assert len(resource.images) == 275
        assert resource.images[-1].end == len(data)
        assert resource.directory[275:] == ((-1, -1),) * (512 - 275)
        assert resource.images[0].offset == 0
        for current, following in zip(resource.images, resource.images[1:], strict=False):
            assert current.end == intr_res.DATA_START + following.offset
        for image in resource.images:
            assert image.image.width > 0 and image.image.height > 0
            assert len(image.image.indices) == image.image.width * image.image.height
            assert len(image.image.alpha) == len(image.image.indices)
            assert len(graph.palette(image.palette_offset // 2)) == 256
        assert icons <= set(range(len(resource.images)))
        assert all(any(resource.images[index].image.alpha) for index in icons)
