"""Карта целиком в одну картинку — эталон для сверки с Tiled и с игрой."""

from collections.abc import Sequence
from functools import cache

from PIL import Image, ImageDraw

from konung import images, rle
from konung.k1 import flames_res, game_state, lights_res, map_file
from konung.k1.characters import CharacterSprite
from konung.k1.flames_res import FlamesRes
from konung.k1.graph_res import GraphRes, Sprites
from konung.k1.lights_res import LightsRes
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
    characters: Sequence[CharacterSprite] = (),
    flames: tuple[FlamesRes, list[tuple[int, int]]] | None = None,
) -> Image.Image:
    """Карта целиком; flames — кадры огней и их анимации из exe: огни объектов поверх них."""

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
        _draw_objects(canvas, konung_map, graph, objects, characters, flames)
    return canvas


def sprite_sheet(sprites: Sprites, graph: GraphRes, palette_offset: int) -> Image.Image | None:
    """Первый кадр каждой анимации по 8 направлениям: строка — анимация, столбец — направление."""
    cells: list[list[Image.Image | None]] = []
    for animation in sprites.sequences:
        row = []
        for sequence in animation:
            image = sprites.image(sequence[0].image_id) if sequence else None
            row.append(
                None
                if image is None
                else images.indexed_to_rgba(image, graph.palette(palette_offset))
            )
        cells.append(row)
    drawn = [cell for row in cells for cell in row if cell is not None]
    if not drawn:
        return None
    width = max(cell.width for cell in drawn)
    height = max(cell.height for cell in drawn)
    sheet = Image.new("RGBA", (width * len(cells[0]), height * len(cells)))
    for y, row in enumerate(cells):
        for x, cell in enumerate(row):
            if cell is not None:
                sheet.alpha_composite(cell, (x * width, y * height + height - cell.height))
    return sheet


def blit(canvas: Image.Image, sprite: Image.Image, x: int, y: int) -> None:
    """alpha_composite, которому можно выйти за край холста (у объектов бывают x, y < 0)."""
    left, top = max(0, -x), max(0, -y)
    if left >= sprite.width or top >= sprite.height:
        return
    if left or top:
        sprite = sprite.crop((left, top, sprite.width, sprite.height))
    canvas.alpha_composite(sprite, (x + left, y + top))


# Кусок картинки: картинка (с палитрой — индексы и начало палитры, иначе готовая
# RGBA), левый верхний угол на карте
Piece = tuple[rle.IndexedImage | Image.Image, int, int, int]


def _draw_objects(
    canvas: Image.Image,
    konung_map: map_file.KonungMap,
    graph: GraphRes,
    objects: ObjectsRes,
    characters: Sequence[CharacterSprite],
    flames: tuple[FlamesRes, list[tuple[int, int]]] | None = None,
) -> None:
    @cache
    def flame_image(index: int) -> Image.Image:
        return flames_res.to_rgba(flames[0].frames[index])

    # (ключ сортировки, картинки по порядку, тень)
    drawn: list[tuple[tuple[int, int, int], list[Piece], Piece | None]] = []
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
        image_x, image_y = x + frame.offset[0], y + frame.offset[1]
        shadow = None
        if frame.shadow is not None:
            shadow = (frame.shadow, 0, x + frame.shadow_offset[0], y + frame.shadow_offset[1])
        key = (image_y + frame.image.height, 0, placed.slot)
        pieces: list[Piece] = [(frame.image, simple.palette_offset, image_x, image_y)]
        # Огни объекта — сразу поверх него, первый кадр анимации своего вида,
        # от позиции объекта (Slavik game.cpp:1587-1597)
        if flames is not None:
            for flame in simple.flames:
                first, _ = flames[1][flame.flame_id]
                pieces.append((flame_image(first), 0, x + flame.x, y + flame.y))
        drawn.append((key, pieces, shadow))
    for sprite in characters:
        pieces = [(layer.image, layer.palette_offset, layer.x, layer.y) for layer in sprite.layers]
        shadow = None
        if sprite.shadow is not None:
            shadow = (sprite.shadow.image, 0, sprite.shadow.x, sprite.shadow.y)
        drawn.append(((sprite.bottom, 1, sprite.character.slot), pieces, shadow))

    shadow_layer = Image.new("RGBA", canvas.size)
    for _, _, shadow in drawn:
        if shadow is None:
            continue
        mask_image, _, x, y = shadow
        mask = Image.frombytes("L", (mask_image.width, mask_image.height), bytes(mask_image.alpha))
        black = Image.new("RGBA", mask.size, (0, 0, 0, SHADOW_ALPHA))
        layer = Image.new("RGBA", mask.size)
        layer.paste(black, (0, 0), mask)
        blit(shadow_layer, layer, x, y)
    canvas.alpha_composite(shadow_layer)

    # Ближе к зрителю то, что ниже на экране: сортировка по нижнему краю, как в игре;
    # при равенстве — объекты раньше персонажей, дальше по слоту
    drawn.sort(key=lambda item: item[0])
    for _, pieces, _ in drawn:
        for image, palette_offset, x, y in pieces:
            if isinstance(image, rle.IndexedImage):
                image = images.indexed_to_rgba(image, graph.palette(palette_offset))
            blit(canvas, image, x, y)


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

# Проходимость поверх карты — те же цвета, что у слоёв foot и foot-arrows в Tiled
FOOT_BLOCKED_COLOR = (255, 40, 40, 110)
FOOT_ARROWS_COLOR = (40, 120, 255, 110)


def draw_foot(canvas: Image.Image, konung_map: map_file.KonungMap) -> None:
    """Поверх карты — ромбы мелкой сетки: непроходимые красным, где застревают стрелы — синим.

    Ромбы не перекрываются, поэтому каждый слой рисуется сразу с прозрачностью.
    """
    half_x, half_y = game_state.FINE_STEP_X // 2, game_state.FINE_STEP_Y
    for color, marked in (
        (FOOT_BLOCKED_COLOR, map_file.is_blocked),
        (FOOT_ARROWS_COLOR, lambda value: bool(value & map_file.FOOT_STOPS_ARROWS)),
    ):
        overlay = Image.new("RGBA", canvas.size)
        draw = ImageDraw.Draw(overlay)
        for y, row in enumerate(konung_map.foot):
            for x, value in enumerate(row):
                if marked(value):
                    cx, cy = game_state.character_position(x, y)
                    top, bottom = (cx, cy - half_y), (cx, cy + half_y)
                    draw.polygon((top, (cx + half_x, cy), bottom, (cx - half_x, cy)), fill=color)
        canvas.alpha_composite(overlay)


# Свет — тот же цвет, что у тайлсета lights в Tiled
LIGHT_COLOR = (255, 210, 120)


def draw_lights(canvas: Image.Image, lights: map_file.Lights, masks: LightsRes) -> None:
    """Поверх карты — маски света тайлов из LIGHTS.N: яркость маски — прозрачность.

    Это подсказка, где свет: в игре он ложится только на тайлы земли и верхнего
    слоя, а декорации и объекты рисуются поверх (Slavik `game.cpp:996-1003`).
    """
    images = [lights_res.to_rgba(mask, LIGHT_COLOR) for mask in masks.tiles]
    for row, values in enumerate(lights.tiles):
        for column, value in enumerate(values):
            if value:
                canvas.alpha_composite(images[value - 1], map_file.tile_position(column, row))


def draw_game(
    canvas: Image.Image, entities: game_state.MapEntities, characters: bool = True
) -> None:
    """Поверх карты — маркеры отрядов, кладов и переходов; characters — и персонажей."""
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
    for character in entities.characters if characters else ():
        point("character", character.x, character.y, f"c{character.slot}")
    for _, job in entities.jobs:
        point("job", job.x, job.y, job.role)
    for loot in entities.loot:
        point("loot", loot.x, loot.y, f"$ {loot.gold}")
    for exit_ in entities.exits:
        rect("exit", exit_.zone, f"→ {exit_.target_map}")
    for exit_ in entities.arrivals:
        point("arrival", exit_.arrival_x, exit_.arrival_y, f"← {exit_.map_id}")
