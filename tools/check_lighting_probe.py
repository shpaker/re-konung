"""Validate a synthetic GPU capture against palette-space numeric expectations.

uv run python tools/check_lighting_probe.py out/lighting-audit/probe.png
"""

import sys

from PIL import Image

image = Image.open(sys.argv[1]).convert("RGBA")
assert image.size == (384, 256), image.size
# Display-encoded source is 128/255. Mask byte 20 maps exactly to 51/255.
expected = [
    (128, 128, 128),
    (51, 38, 38),
    (143, 136, 128),
    (83, 67, 38),
    (138, 133, 64),
    (138, 133, 128),
]
for index, wanted in enumerate(expected):
    actual = image.getpixel((index * 64 + 32, 128))
    assert all(abs(a - b) <= 1 for a, b in zip(actual[:3], wanted, strict=True)), (
        index,
        actual,
        wanted,
    )
    assert actual[3] == 255
    print(f"case {index}: {actual[:3]} (expected {wanted}, tolerance 1/255)")
