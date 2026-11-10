"""Render pipeline: the layer order that a picture cannot pin down."""

from konung.k1 import graph_res, lights_res, map_file, objects_res, render

# Map 17 — the burned village: the houses stand on lit tiles, so part of every
# light mask is covered by an object sprite.
MAP = 17


def test_lights_are_composited_under_objects(k1_game, k1_file):
    graph = graph_res.load(k1_file("graph.res"))
    objects = objects_res.load(k1_file("objects.res"))
    konung_map = map_file.load(k1_game, MAP, map_file.GAME_FILES)
    lights_path = map_file.lights_path(k1_game, MAP, map_file.GAME_FILES)
    assert lights_path is not None, f"map {MAP} must have a LIGHTS file"
    lights = (
        map_file.parse_lights(lights_path.read_bytes()),
        lights_res.load(k1_file("lights.res")),
    )

    under = render.render(konung_map, graph, objects, lights=lights)
    on_top = render.render(konung_map, graph, objects)
    render.draw_lights(on_top, *lights)

    assert under.tobytes() != on_top.tobytes(), (
        "the light must lie under decorations and objects, not on top of the finished picture"
    )
