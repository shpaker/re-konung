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


def test_lights_files_match_their_size(k1_game):
    for path in k1_game.iterdir():
        stem, _, suffix = path.name.partition(".")
        if stem.lower() == "lights" and suffix.isdigit():
            lights = map_file.parse_lights(path.read_bytes())
            assert len(lights.tiles) == map_file.HEIGHT
            assert len(lights.fine) == map_file.FINE_HEIGHT
