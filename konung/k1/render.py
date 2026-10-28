"""Карта целиком в одну картинку — эталон для сверки с Tiled и с игрой."""

from functools import cache

from PIL import Image

from konung import images
from konung.k1 import map_file
from konung.k1.graph_res import GraphRes
from konung.k1.objects_res import ObjectsRes

# Тени — чёрная маска с прозрачностью 0.4 (как слой shadows в Tiled)
SHADOW_ALPHA = 102


def object_position(x: int, y: int) -> tuple[int, int]:
    """Позиция объекта (клетка мелкой сетки) в пикселях карты."""
    return x * map_file.TILE_STEP_X // 2 - (y & 1) * map_file.TILE_STEP_X // 4, y * 16


def map_size(konung_map: map_file.KonungMap) -> tuple[int, int]:
    width = map_file.WIDTH * map_file.TILE_STEP_X + map_file.ODD_ROW_SHIFT
    height = map_file.HEIGHT * map_file.TILE_STEP_Y + 64
    return width, height


def render(
    konung_map: map_file.KonungMap,
    graph: GraphRes,
    objects: ObjectsRes | None = None,
) -> Image.Image:
    @cache
    def tile_image(index: int) -> Image.Image:
        tile = graph.tiles[index]
        return images.indexed_to_rgba(tile.image, graph.palette(tile.palette_offset))

    canvas = Image.new("RGBA", map_size(konung_map), (0, 0, 0, 255))

    for layer in (konung_map.ground, konung_map.overlay):
        for row, cells in enumerate(layer):
            for column, value in enumerate(cells):
                if value:
                    canvas.alpha_composite(
                        tile_image(value - 1), map_file.tile_position(column, row)
                    )

    for decoration in konung_map.decorations:
        if decoration.tile in graph.tiles:
            blit(canvas, tile_image(decoration.tile), decoration.x, decoration.y)

    if objects is not None:
        _draw_objects(canvas, konung_map, graph, objects)
    return canvas


def blit(canvas: Image.Image, sprite: Image.Image, x: int, y: int) -> None:
    """alpha_composite, которому можно выйти за край холста (у объектов бывают x, y < 0)."""
    left, top = max(0, -x), max(0, -y)
    if left >= sprite.width or top >= sprite.height:
        return
    if left or top:
        sprite = sprite.crop((left, top, sprite.width, sprite.height))
    canvas.alpha_composite(sprite, (x + left, y + top))


def _draw_objects(
    canvas: Image.Image,
    konung_map: map_file.KonungMap,
    graph: GraphRes,
    objects: ObjectsRes,
) -> None:
    drawn = []
    for placed in konung_map.objects:
        simple = objects.simple.get(placed.object_id)
        if simple is None:
            continue
        frame = simple.frames[placed.frame] if placed.frame < len(simple.frames) else None
        if frame is None or frame.image is None:
            frame = simple.frames[0]
        if frame.image is None:
            continue
        x, y = object_position(placed.x, placed.y)
        drawn.append((x, y, simple, frame, placed.slot))

    shadow_layer = Image.new("RGBA", canvas.size)
    for x, y, _, frame, _ in drawn:
        if frame.shadow is None:
            continue
        mask = Image.frombytes(
            "L", (frame.shadow.width, frame.shadow.height), bytes(frame.shadow.alpha)
        )
        black = Image.new("RGBA", mask.size, (0, 0, 0, SHADOW_ALPHA))
        shadow = Image.new("RGBA", mask.size)
        shadow.paste(black, (0, 0), mask)
        blit(shadow_layer, shadow, x + frame.shadow_offset[0], y + frame.shadow_offset[1])
    canvas.alpha_composite(shadow_layer)

    # Ближе к зрителю то, что ниже на экране: сортировка по нижнему краю, как в игре;
    # при равенстве — по слоту в файле карты
    drawn.sort(key=lambda item: (item[1] + item[3].offset[1] + item[3].image.height, item[4]))
    for x, y, simple, frame, _ in drawn:
        sprite = images.indexed_to_rgba(frame.image, graph.palette(simple.palette_offset))
        blit(canvas, sprite, x + frame.offset[0], y + frame.offset[1])
