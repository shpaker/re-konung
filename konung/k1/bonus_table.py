"""Read the embedded K1 bonus records without applying their gameplay effects.

Format contract: docs/research/k1/formats/executable.md#equipment-bonuses.
"""

import struct
from dataclasses import dataclass
from pathlib import Path

from konung.k1 import executable

RECORD_COUNT = 74
RECORD = struct.Struct("<HbbBBhBBhBBh")
TABLE_SIZE = RECORD_COUNT * RECORD.size


@dataclass(frozen=True)
class Bonus:
    attribute: int
    additive: bool
    value: int


@dataclass(frozen=True)
class BonusSet:
    unknown_prefix: bytes
    required_attribute: int
    required_value: int
    bonuses: tuple[Bonus, ...]
    raw: bytes


def _matches(data: bytes, offset: int) -> bool:
    fields = RECORD.unpack_from(data, offset)
    # These are recognition constraints of the two owned tables, not gameplay limits.
    return (
        0 <= fields[1] <= 9
        and fields[2] >= 0
        and 1 <= fields[3] <= 9
        and all(0 <= fields[index] <= 9 and fields[index + 1] in (0, 1) for index in (3, 6, 9))
    )


def parse(data: bytes) -> tuple[BonusSet, ...]:
    """Parse exactly one table; preserve the uninterpreted prefix and original bytes."""
    if len(data) != TABLE_SIZE:
        raise ValueError(f"bonus table must contain {TABLE_SIZE} bytes, got {len(data)}")
    records = []
    for offset in range(0, TABLE_SIZE, RECORD.size):
        if not _matches(data, offset):
            raise ValueError(f"invalid bonus record at {offset}")
        fields = RECORD.unpack_from(data, offset)
        records.append(
            BonusSet(
                data[offset : offset + 2],
                fields[1],
                fields[2],
                tuple(Bonus(fields[i], bool(fields[i + 1]), fields[i + 2]) for i in (3, 6, 9)),
                data[offset : offset + RECORD.size],
            )
        )
    return tuple(records)


def find(data: bytes) -> int:
    """Locate the unique structurally matching table in executable bytes."""
    candidates = [
        offset
        for offset in range(0, len(data) - TABLE_SIZE + 1, 4)
        if all(_matches(data, offset + index * RECORD.size) for index in range(RECORD_COUNT))
    ]
    if len(candidates) != 1:
        raise ValueError(f"bonus table is not unique: {candidates}")
    return candidates[0]


def load(game_directory: Path) -> tuple[BonusSet, ...]:
    data = executable.find(game_directory).read_bytes()
    offset = find(data)
    return parse(data[offset : offset + TABLE_SIZE])
