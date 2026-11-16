"""Read-only evidence for inventory research; no game rules or resource excerpts."""

import struct

from konung.k1 import executable, game_state, map_file


def test_existing_equipment_and_inventory_references(k1_game):
    mismatches = []
    counts = []
    for files in (map_file.GAME_FILES, map_file.TUTORIAL_FILES):
        for number in game_state.numbers(k1_game, files):
            state = game_state.load(k1_game, number, files)
            characters = list(filter(None, state.characters))
            counts.append(
                (
                    len(characters),
                    sum(item is not None for item in state.items),
                    sum(
                        bool(reference)
                        for character in characters
                        for reference in character.equipment
                    ),
                    sum(
                        bool(reference)
                        for character in characters
                        for reference in character.inventory
                    ),
                )
            )
            owners = set()
            for character in characters:
                occupied = tuple(reference for reference in character.inventory if reference)
                assert len(occupied) < 32
                assert character.inventory == occupied + (0,) * (32 - len(occupied))
                assert not any(character.accessories)
                assert character.misc[8] == 0
                for slot, reference in enumerate(character.equipment):
                    if reference and state.items[reference].type_id != slot:
                        mismatches.append(
                            (
                                files.state,
                                number,
                                character.slot,
                                slot,
                                reference,
                                state.items[reference].type_id,
                                state.items[reference].info_id,
                            )
                        )
                for reference in (*character.equipment, *character.inventory):
                    if reference:
                        assert state.items[reference] is not None
                        assert reference not in owners
                        owners.add(reference)
            for loot in state.loot:
                for reference in loot.items:
                    if reference:
                        assert state.items[reference] is not None
                        assert reference not in owners
                        owners.add(reference)
    # Loaded records are evidence, not a promise that every UI operation could create them.
    english = k1_game.parent.name == "eng"
    assert mismatches == ([("game", 2, 215, 0, 205, 2, 0)] if english else [])
    assert counts == (
        [(987, 1535, 749, 300), (915, 1448, 702, 286), (1277, 1469, 727, 264), (13, 27, 17, 6)]
        if english
        else [(987, 1492, 706, 300), (915, 1405, 659, 286), (1277, 1440, 698, 264)]
    )


def test_six_starting_loadouts_use_file_items(k1_game):
    assert game_state.numbers(k1_game) == [0, 1, 2]
    for number in game_state.numbers(k1_game):
        state = game_state.load(k1_game, number)
        character = state.characters[state.groups[0].first_character]
        assert character.equipment == (0, 0, 1, 0, 0, 0)
        assert character.weapon_slot == 2
        assert sum(bool(reference) for reference in character.inventory) == 6
        carried = [state.items[reference] for reference in character.inventory if reference]
        assert [item.type_id for item in carried] == [11, 9, 9, 9, 9, 9]
        assert all((item.bonus_id, item.special_id, item.flags) == (-1, -1, 0) for item in carried)
        assert character.accessories == (0,) * 5
        item = state.items[1]
        assert (item.type_id, item.info_id, item.bonus_id, item.special_id) == (2, 18, -1, -1)


def test_item_names_and_inventory_icon_references(k1_game):
    tables = executable.load(k1_game)
    assert len(tables.equipment) == 70
    assert {name: len(rows) for name, rows in tables.items.items()} == executable.ITEM_TABLES
    icons = {row.values[7] for row in tables.equipment}
    icons.update(row.values[2] for rows in tables.items.values() for row in rows)
    assert len(icons) == 130
    assert (min(icons), max(icons)) == (22, 152)
    resources = sorted(path for path in k1_game.iterdir() if path.name.lower().startswith("intr"))
    assert len(resources) == (4 if k1_game.parent.name == "eng" else 2)
    for path in resources:
        data = path.read_bytes()
        entries = [struct.unpack_from("<ii", data, 8 + index * 8) for index in range(512)]
        assert next(index for index, (offset, _) in enumerate(entries) if offset == -1) == 275
        for icon in icons:
            offset, palette = entries[icon]
            assert 0 <= offset < len(data) - 4104 - 4, (path.name, icon)
            assert palette >= 0 and palette % 512 == 0
            width, height = struct.unpack_from("<HH", data, 4104 + offset)
            assert width > 0 and height > 0, (path.name, icon)
    for files in (map_file.GAME_FILES, map_file.TUTORIAL_FILES):
        for number in game_state.numbers(k1_game, files):
            state = game_state.load(k1_game, number, files)
            for item in filter(None, state.items):
                assert executable.item_name(tables, item.type_id, item.info_id)
