"""Synthetic parsing contracts and local cross-references; no embedded game records."""

import pytest

from konung.k1 import bonus_table, executable, game_state, map_file


def synthetic_table():
    record = bonus_table.RECORD.pack(0xFEFE, 5, 20, 5, 1, -7, 6, 0, 30, 0, 0, 0)
    return record * bonus_table.RECORD_COUNT


def test_preserves_unknown_bytes_and_signed_values():
    data = synthetic_table()
    records = bonus_table.parse(data)
    assert b"".join(record.raw for record in records) == data
    assert records[0].unknown_prefix == b"\xfe\xfe"
    assert (records[0].required_attribute, records[0].required_value) == (5, 20)
    assert records[0].bonuses == (
        bonus_table.Bonus(5, True, -7),
        bonus_table.Bonus(6, False, 30),
        bonus_table.Bonus(0, False, 0),
    )


@pytest.mark.parametrize("length_change", [-1, 1, -bonus_table.TABLE_SIZE])
def test_rejects_wrong_table_size(length_change):
    data = synthetic_table()
    data = data[: len(data) + length_change] if length_change < 0 else data + b"\0"
    with pytest.raises(ValueError, match="bytes"):
        bonus_table.parse(data)


@pytest.mark.parametrize("offset,value", [(2, 255), (3, 255), (4, 0), (5, 2), (8, 10)])
def test_rejects_invalid_record_fields(offset, value):
    data = bytearray(synthetic_table())
    data[offset] = value
    with pytest.raises(ValueError, match="record"):
        bonus_table.parse(bytes(data))


def test_finds_table_at_end_and_refuses_missing_or_ambiguous_tables():
    table = synthetic_table()
    assert bonus_table.find(b"\xff" * 12 + table) == 12
    with pytest.raises(ValueError, match="not unique"):
        bonus_table.find(b"\0" * bonus_table.TABLE_SIZE)
    with pytest.raises(ValueError, match="not unique"):
        bonus_table.find(table + b"\xff" * 16 + table)


def test_original_tables_and_item_references(k1_game):
    data = executable.find(k1_game).read_bytes()
    offset = bonus_table.find(data)
    assert offset == (0x3E730 if k1_game.parent.name == "rus" else 0x3F968)
    records = bonus_table.load(k1_game)
    assert len(records) == 74
    reconstructed = b"".join(record.raw for record in records)
    assert reconstructed == data[offset : offset + bonus_table.TABLE_SIZE]
    for files in (map_file.GAME_FILES, map_file.TUTORIAL_FILES):
        for number in game_state.numbers(k1_game, files):
            state = game_state.load(k1_game, number, files)
            for item in filter(None, state.items):
                assert item.bonus_id == -1 or 0 <= item.bonus_id < len(records)
