from konung.k1 import game_state, map_file


def _state_files(k1_game):
    for path in k1_game.iterdir():
        stem, _, suffix = path.name.partition(".")
        if stem.lower() in ("game", "tgame") and suffix.isdigit():
            yield path


def test_serialize_returns_every_state_byte_for_byte(k1_game):
    paths = list(_state_files(k1_game))
    assert len(paths) >= 3
    for path in paths:
        data = path.read_bytes()
        assert game_state.serialize(game_state.parse(data)) == data, path.name


def test_references_between_blocks_match(k1_game):
    maps = set(map_file.map_numbers(k1_game))
    for path in _state_files(k1_game):
        state = game_state.parse(path.read_bytes())
        owners = {}
        for group in state.groups:
            if group is None:
                continue
            assert group.map_id in maps, (path.name, group.slot)
            for index in group.members:
                assert index not in owners, (path.name, index)
                owners[index] = group.slot
                assert state.characters[index].group == group.slot, (path.name, index)
                # Всех, кроме бродячих, игра ставит на место из файла
                if not group.is_roaming:
                    character = state.characters[index]
                    assert (character.x, character.y) != (0, 0), (path.name, index)
        for loot in state.loot:
            assert loot.map_id in maps, (path.name, loot.slot)
            assert all(i == 0 or state.items[i] is not None for i in loot.items), loot.slot
        for exit_ in state.exits:
            assert {exit_.map_id, exit_.target_map} <= maps, (path.name, exit_.slot)
        for village in state.villages:
            if village is not None:
                assert village.map_id in maps, (path.name, village.slot)


def test_player_starts_on_walkable_cell_of_hero_map(k1_game):
    for number in game_state.numbers(k1_game):
        state = game_state.load(k1_game, number)
        player = state.groups[0]
        assert player.kind == game_state.PLAYER_GROUP
        # Карту в отряде игрока игра не читает: она считается по номеру героя
        assert player.map_id == 1
        start = game_state.on_map(state, game_state.start_map(number), number).start
        assert start is player
        foot = map_file.load(k1_game, game_state.start_map(number)).foot
        assert foot[player.spawn.top][player.spawn.left] == 0, number


def _empty_state() -> game_state.GameState:
    data = b"\xff" * game_state.ITEM_SLOTS * game_state.ITEM.size
    data += bytes(game_state.GAME_STATE_SIZE - len(data))
    return game_state.parse(data)


def test_serialize_keeps_edits():
    state = _empty_state()
    assert not state.loot and not state.exits and state.groups[0] is None
    state.loot.append(game_state.Loot(0, 17, 40, 25, 3, (0,) * 22, bytes(8), 0, 0))
    zone = game_state.Rect(4, 158, 5, 159)
    state.exits.append(game_state.Exit(0, 3, 11, zone, 8, 164, 7, bytes(2)))
    edited = game_state.parse(game_state.serialize(state))
    assert [(loot.x, loot.y, loot.gold, loot.map_id) for loot in edited.loot] == [(17, 40, 25, 3)]
    assert edited.exits[0].zone == zone
    assert (edited.exits[0].arrival_x, edited.exits[0].arrival_y) == (8, 164)


def test_character_position_round_trips():
    for x, y in [(0, 0), (33, 86), (17, 219), (159, 319)]:
        assert game_state.character_cell(*game_state.character_position(x, y)) == (x, y)
    rect = game_state.Rect(4, 150, 9, 161)
    assert game_state.rect_cells(*game_state.rect_pixels(rect)) == rect
