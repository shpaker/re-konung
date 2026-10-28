from konung.k1 import graph_res, map_file, objects_res


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


def test_lights_files_match_their_size(k1_game):
    for path in k1_game.iterdir():
        stem, _, suffix = path.name.partition(".")
        if stem.lower() == "lights" and suffix.isdigit():
            lights = map_file.parse_lights(path.read_bytes())
            assert len(lights.tiles) == map_file.HEIGHT
            assert len(lights.fine) == map_file.FINE_HEIGHT
