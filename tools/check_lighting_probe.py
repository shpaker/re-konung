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

# The left patch crossed light boundaries 240 times before settling on neutral.
# It must match the standard sprite on the right, including tint and alpha.
neutral = image.getpixel((128, 208))
standard = image.getpixel((256, 208))
assert neutral == standard, (neutral, standard)
assert neutral[:3] != image.getpixel((0, 208))[:3], "probe patches must be visible"
print(f"neutral after 240 switches: {neutral}, standard sprite: {standard}")
