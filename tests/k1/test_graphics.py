from konung.k1 import graph_res, objects_res


def test_graph_res_is_parsed_to_the_last_byte(k1_file):
    data = k1_file("graph.res").read_bytes()
    graph = graph_res.parse(data)
    assert graph.size == len(data)
    assert len(graph.cursors) == 11
    ground = [t for t in graph.tiles.values() if (t.image.width, t.image.height) == (114, 64)]
    assert len(graph.tiles) == 305
    assert len(ground) == 221


def test_objects_res_has_all_simple_objects(k1_file):
    objects = objects_res.load(k1_file("objects.res"))
    assert len(objects.dynamic_offsets) == 30
    assert len(objects.simple) == 463
    assert all(obj.frames[0].image is not None for obj in objects.simple.values())
