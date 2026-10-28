import pytest

from konung.k1 import graph_res, lights_res, map_file, objects_res


def test_every_map_references_existing_resources(k1_game, k1_file):
    graph = graph_res.load(k1_file("graph.res"))
    objects = objects_res.load(k1_file("objects.res"))
    numbers = map_file.map_numbers(k1_game)
    assert len(numbers) == 43
    for number in numbers:
        konung_map = map_file.load(k1_game, number)
        for layer in (konung_map.ground, konung_map.overlay):
            assert all(v == 0 or v - 1 in graph.tiles for row in layer for v in row), number
        assert all(d.tile in graph.tiles for d in konung_map.decorations), number
        assert all(o.object_id in objects.simple for o in konung_map.objects), number


def test_serialize_returns_every_map_byte_for_byte(k1_game):
    for number in map_file.map_numbers(k1_game):
        data = map_file.find(k1_game, "konung", number).read_bytes()
        assert map_file.serialize(map_file.parse(data)) == data, number


def test_serialize_keeps_edits():
    empty = bytes(map_file.MAP_SIZE - 16000 - 36000) + b"\xff" * (16000 + 36000)
    konung_map = map_file.parse(empty)
    konung_map.ground[3][5] = 7
    konung_map.decorations.append(map_file.Decoration(0, 230, 100, -20, bytes(8)))
    konung_map.objects.append(map_file.MapObject(0, 117, 27, 33, b"\xff" * 20, 0, 0, 0, bytes(4)))
    edited = map_file.parse(map_file.serialize(konung_map))
    assert edited.ground[3][5] == 7
    assert [(d.tile, d.x, d.y) for d in edited.decorations] == [(230, 100, -20)]
    assert [(o.object_id, o.x, o.y) for o in edited.objects] == [(117, 27, 33)]


def test_foot_has_only_four_values(k1_game):
    # Правило Slavik «младшие 4 бита = 0xF» и его версия с ошибкой приоритета
    # («младший бит», docs/k1/formats/map.md) на этих значениях совпадают
    for number in map_file.map_numbers(k1_game):
        values = {v for row in map_file.load(k1_game, number).foot for v in row}
        assert values <= {0x00, 0x0F, 0x40, 0x4F}, number
        assert all(map_file.is_blocked(v) == bool(v & 1) for v in values)


def test_fine_cells_split_tiles_without_overlap():
    owners = {}
    for row in range(map_file.HEIGHT):
        for column in range(map_file.WIDTH):
            for x, y in map_file.fine_cells(column, row):
                if 0 <= x < map_file.FINE_WIDTH and 0 <= y < map_file.FINE_HEIGHT:
                    assert (x, y) not in owners
                    owners[(x, y)] = (column, row)
    orphans = {
        (x, y)
        for y in range(map_file.FINE_HEIGHT)
        for x in range(map_file.FINE_WIDTH)
        if (x, y) not in owners
    }
    # Без тайла — только нечётные клетки верхней строки и левый край нечётных строк тайлов
    assert orphans == {(x, 0) for x in range(1, map_file.FINE_WIDTH, 2)} | {
        (0, y) for y in range(3, map_file.FINE_HEIGHT, 4)
    }


def test_lights_files_match_their_size(k1_game):
    for path in k1_game.iterdir():
        stem, _, suffix = path.name.partition(".")
        if stem.lower() == "lights" and suffix.isdigit():
            lights = map_file.parse_lights(path.read_bytes())
            assert len(lights.tiles) == map_file.HEIGHT
            assert len(lights.fine) == map_file.FINE_HEIGHT


def test_lights_fine_block_follows_tiles(k1_game, k1_file):
    # Мелкий блок — номер маски тайла на 4 его клетки: запись его пересчитывает
    numbers = map_file.map_numbers(k1_game)
    found = [p for n in numbers if (p := map_file.lights_path(k1_game, n))]
    assert len(found) == 40
    assert {n for n in numbers if not map_file.lights_path(k1_game, n)} == {6, 8, 46}
    masks = len(lights_res.load(k1_file("lights.res")).tiles)
    for path in found:
        data = path.read_bytes()
        lights = map_file.parse_lights(data)
        assert lights.fine == map_file.lights_fine(lights.tiles), path.name
        assert map_file.serialize_lights(lights) == data, path.name
        assert all(v <= masks for row in lights.tiles for v in row), path.name


def test_lights_res_has_nineteen_tile_masks(k1_file):
    res = lights_res.load(k1_file("lights.res"))
    assert len(res.tiles) == lights_res.TILE_MASKS
    assert max(max(mask) for mask in res.tiles) < lights_res.BRIGHTNESS_SCALE


def _tutorial_numbers(k1_game):
    # Обучение есть только в ENG: Tutorial.4 и Tutorial.5
    numbers = map_file.map_numbers(k1_game, map_file.TUTORIAL_FILES)
    assert numbers == ([4, 5] if k1_game.parent.name == "eng" else [])
    return numbers


def test_tutorial_maps_return_byte_for_byte(k1_game, k1_file):
    graph = graph_res.load(k1_file("graph.res"))
    for number in _tutorial_numbers(k1_game):
        data = map_file.find(k1_game, "tutorial", number).read_bytes()
        konung_map = map_file.parse(data)
        assert map_file.serialize(konung_map) == data, number
        # Единственная запись не из GRAPH.RES — сырая, в слоте 994
        assert [
            (d.slot, d.is_raw) for d in konung_map.decorations if d.tile not in graph.tiles
        ] == [(994, True)]
        lights = map_file.lights_path(k1_game, number, map_file.TUTORIAL_FILES)
        assert lights is not None and lights.name.lower() == f"tlights.{number}"
        assert map_file.serialize_lights(map_file.parse_lights(lights.read_bytes())) == (
            lights.read_bytes()
        )


def test_file_set_follows_map_name():
    assert map_file.file_set("Tutorial.4") is map_file.TUTORIAL_FILES
    assert map_file.file_set("TUTORIAL.5") is map_file.TUTORIAL_FILES
    for name in ("Konung.4", "KONUNG.12", "konung.1"):
        assert map_file.file_set(name) is map_file.GAME_FILES


def _decoration(slot: int, tile: int, x: int = 0) -> map_file.Decoration:
    return map_file.Decoration(slot, tile, x, -1, bytes(8))


def test_raw_decoration_keeps_its_slot():
    raw = _decoration(994, 0x00FFFFFF)
    decorations = [_decoration(0, 5, 10), raw, _decoration(1, 7, 20)]
    slots = map_file.decoration_slots(decorations)
    assert slots[:2] == [decorations[0], decorations[2]] and slots[994] is raw
    assert sum(placed is not None for placed in slots) == 3

    # Удаление, добавление и перестановка обычных декораций сырую не сдвигают
    empty = bytes(map_file.MAP_SIZE - 16000 - 36000) + b"\xff" * (16000 + 36000)
    konung_map = map_file.parse(empty)
    konung_map.decorations = [_decoration(0, 3, 1), raw, _decoration(0, 4, 2)]
    edited = map_file.parse(map_file.serialize(konung_map))
    assert [(d.slot, d.tile) for d in edited.decorations] == [(0, 3), (1, 4), (994, 0x00FFFFFF)]
    konung_map.decorations = [_decoration(0, 4, 2), raw]
    edited = map_file.parse(map_file.serialize(konung_map))
    assert [(d.slot, d.tile) for d in edited.decorations] == [(0, 4), (994, 0x00FFFFFF)]

    # Обычные обходят занятый слот, а два сырых в одном слоте — ошибка
    many = [_decoration(0, 1) for _ in range(map_file.MAX_DECORATIONS - 1)] + [raw]
    assert map_file.decoration_slots(many)[995].tile == 1
    with pytest.raises(ValueError):
        map_file.decoration_slots([raw, _decoration(994, 0x01000000)])
    with pytest.raises(ValueError):
        map_file.decoration_slots(many + [_decoration(0, 1)])
