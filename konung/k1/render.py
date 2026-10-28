"""Карта целиком в одну картинку — эталон для сверки с Tiled и с игрой."""

from functools import cache

from PIL import Image, ImageDraw

from konung import images
from konung.k1 import game_state, map_file
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


# Маркеры Game.N: цвет на каждый вид, чтобы проверить места картинкой
MARKER_COLORS = {
    "spawn": (255, 64, 64, 255),
    "leash": (255, 160, 0, 160),
    "character": (255, 255, 0, 255),
    "start": (0, 255, 0, 255),
    "loot": (255, 215, 0, 255),
    "exit": (64, 160, 255, 255),
    "arrival": (0, 255, 255, 255),
    "job": (255, 0, 255, 255),
}
MARKER_RADIUS = 8


def draw_game(canvas: Image.Image, entities: game_state.MapEntities) -> None:
    """Поверх карты — маркеры персонажей, отрядов, кладов и переходов."""
    draw = ImageDraw.Draw(canvas)

    def point(kind: str, x: int, y: int, label: str) -> None:
        px, py = game_state.character_position(x, y)
        r = MARKER_RADIUS
        draw.ellipse((px - r, py - r, px + r, py + r), outline=MARKER_COLORS[kind], width=3)
        draw.text((px + r + 2, py - r), label, fill=MARKER_COLORS[kind])

    def rect(kind: str, area: game_state.Rect, label: str) -> None:
        box = game_state.rect_pixels(area)
        draw.rectangle(box, outline=MARKER_COLORS[kind], width=3)
        draw.text((box[0] + 4, box[1] + 2), label, fill=MARKER_COLORS[kind])

    if entities.start is not None:
        # У отряда игрока прямоугольник — точка старта: top = y, left = x
        point("start", entities.start.spawn.left, entities.start.spawn.top, "start")
    for group in entities.groups:
        if group.is_roaming:
            rect("leash", group.leash, "")
            rect("spawn", group.spawn, f"g{group.slot} {group.kind:#x} ×{group.size}")
    for character in entities.characters:
        point("character", character.x, character.y, f"c{character.slot}")
    for _, job in entities.jobs:
        point("job", job.x, job.y, job.role)
    for loot in entities.loot:
        point("loot", loot.x, loot.y, f"$ {loot.gold}")
    for exit_ in entities.exits:
        rect("exit", exit_.zone, f"→ {exit_.target_map}")
    for exit_ in entities.arrivals:
        point("arrival", exit_.arrival_x, exit_.arrival_y, f"← {exit_.map_id}")
