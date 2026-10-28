import copy

import pytest

from konung.k1 import game_edit, game_state


def _check_layout(state: game_state.GameState) -> None:
    """Укладка, как во всех файлах дистрибутива: отряды подряд, резервы плотно,
    персонажи только внутри отрядов, ссылки сходятся, у предмета один владелец."""
    groups = [g for g in state.groups if g is not None]
    assert [g.slot for g in groups] == list(range(len(groups)))
    first = 0
    members = set()
    for group in groups:
        assert group.first_character == first
        assert group.size <= group.max_size <= group.growth_limit
        for slot in group.members:
            character = state.characters[slot]
            assert character is not None and character.group == group.slot
            members.add(slot)
        first += group.growth_limit
    assert {c.slot for c in state.characters if c} == members
    for village in filter(None, state.villages):
        elder = state.groups[village.elder_group]
        for job in village.jobs:
            assert job.character == 0 or job.character in elder.members
    owners = {}
    for character in filter(None, state.characters):
        for slot in (*character.equipment, *character.inventory, *character.accessories):
            if slot > 0:
                assert slot not in owners and state.items[slot] is not None
                owners[slot] = character.slot
    for loot in state.loot:
        for slot in loot.items:
            if slot > 0:
                assert slot not in owners
                owners[slot] = loot.slot
    data = game_state.serialize(state)
    assert game_state.serialize(game_state.parse(data)) == data


def _state(k1_game) -> game_state.GameState:
    return game_state.load(k1_game, 0)


def _workers(state):
    return {j.character for v in state.villages if v for j in v.jobs if j.character}


def test_remove_npc_drops_its_group(k1_game):
    state = _state(k1_game)
    npc_group = next(g for g in state.groups if g and g.kind == game_edit.NPC_GROUP)
    groups_before = sum(1 for g in state.groups if g)
    people_before = sum(1 for c in state.characters if c)
    game_edit.remove_character(state, npc_group.first_character)
    assert sum(1 for g in state.groups if g) == groups_before - 1
    assert sum(1 for c in state.characters if c) == people_before - 1
    _check_layout(state)


def test_remove_villager_shifts_the_rest_of_the_group(k1_game):
    state = _state(k1_game)
    workers = _workers(state)
    village = next(
        g
        for g in state.groups
        if g and g.kind & game_state.VILLAGE_GROUP_BIT and set(g.members) - workers
    )
    slot = next(s for s in village.members if s not in workers)
    size, after = village.size, [id(state.characters[s]) for s in village.members if s > slot]
    game_edit.remove_character(state, slot)
    assert village.size == size - 1
    assert [id(state.characters[s]) for s in village.members if s >= slot] == after
    _check_layout(state)


def test_add_copy_to_full_group_grows_its_reserve(k1_game):
    # Деревни в данных никогда не заполнены до резерва, бродячие отряды — бывают;
    # рост резерва один и тот же
    state = _state(k1_game)
    full = next(g for g in state.groups if g and g.is_roaming and g.size == g.growth_limit)
    template = state.characters[full.first_character]
    later = [g for g in state.groups if g and g.slot > full.slot]
    firsts = [g.first_character for g in later]
    slot = game_edit.add_character(state, template, full, 10, 20)
    assert slot == full.first_character + full.size - 1
    assert (state.characters[slot].x, state.characters[slot].y) == (10, 20)
    assert [g.first_character for g in later] == [f + 1 for f in firsts]
    _check_layout(state)


def test_add_copy_of_npc_gets_its_own_group(k1_game):
    state = _state(k1_game)
    npc_group = next(g for g in state.groups if g and g.kind == game_edit.NPC_GROUP)
    template = state.characters[npc_group.first_character]
    groups_before = sum(1 for g in state.groups if g)
    slot = game_edit.add_character(state, copy.deepcopy(template), npc_group, 5, 6)
    assert sum(1 for g in state.groups if g) == groups_before + 1
    new_group = state.groups[state.characters[slot].group]
    assert new_group.kind == game_edit.NPC_GROUP and new_group.map_id == npc_group.map_id
    assert state.characters[slot].unknown_d0 == game_edit.NO_DIALOG
    _check_layout(state)


def test_copy_then_remove_original_keeps_layout(k1_game):
    state = _state(k1_game)
    npc_group = next(g for g in state.groups if g and g.kind == game_edit.NPC_GROUP)
    original = state.characters[npc_group.first_character]
    game_edit.add_character(state, original, npc_group, 1, 2)
    game_edit.remove_character(state, original.slot)
    _check_layout(state)


def test_player_and_workers_cannot_be_removed(k1_game):
    state = _state(k1_game)
    with pytest.raises(game_edit.EditError):
        game_edit.remove_character(state, state.groups[0].first_character)
    with pytest.raises(game_edit.EditError):
        game_edit.remove_character(state, next(iter(_workers(state))))
