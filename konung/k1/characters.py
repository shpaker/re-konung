"""Кадр персонажа на карте: модель или монстр, снаряжение, тень (docs/k1/formats/graph-res.md).

Воин — модель GRAPH.RES по Character.sprite_set с его палитрой, поверх неё —
наборы снаряжения надетых предметов в порядке слотов для его направления.
Монстры, женщины и дети — динамический объект OBJECTS.RES с тем же номером,
без снаряжения.
"""

from dataclasses import dataclass

from konung import rle
from konung.k1 import executable, game_state
from konung.k1.executable import Executable
from konung.k1.graph_res import GraphRes, SpriteFrame, Sprites
from konung.k1.objects_res import ObjectsRes

COLORS_PER_PALETTE = 256
# У динамических объектов 6 анимаций; состояния 6–9 игра сводит к 0–3
# (Slavik game_character.cpp:147)
DYNAMIC_ANIMATIONS = 6


@dataclass
class Layer:
    image: rle.IndexedImage
    # Начало палитры в банке цветов GRAPH.RES (номер цвета)
    palette_offset: int
    # Левый верхний угол на карте
    x: int
    y: int


@dataclass
class CharacterSprite:
    character: game_state.Character
    # Сначала модель, потом снаряжение
    layers: list[Layer]
    shadow: Layer | None

    @property
    def bottom(self) -> int:
        """Нижний край модели: по нему игра сортирует, что рисовать раньше."""
        body = self.layers[0]
        return body.y + body.image.height


def _frame(sprites: Sprites, animation: int, direction: int, frame: int) -> SpriteFrame | None:
    sequence = sprites.sequences[animation][direction]
    if not sequence:
        return None
    return sequence[frame] if frame < len(sequence) else sequence[0]


def _sprites(
    character: game_state.Character, graph: GraphRes, objects: ObjectsRes
) -> tuple[Sprites | None, int]:
    """Спрайт персонажа и номер анимации его состояния."""
    if character.has_dynamic_sprite:
        return objects.dynamic.get(character.sprite_set), character.state % DYNAMIC_ANIMATIONS
    return graph.models[character.sprite_set], character.state


def frame_count(character: game_state.Character, graph: GraphRes, objects: ObjectsRes) -> int:
    """Сколько кадров в анимации персонажа: его состояние и направление."""
    sprites, animation = _sprites(character, graph, objects)
    if sprites is None:
        return 0
    return len(sprites.sequences[animation][character.direction])


def compose(
    character: game_state.Character,
    x: int,
    y: int,
    state: game_state.GameState,
    graph: GraphRes,
    objects: ObjectsRes,
    tables: Executable,
    frame_index: int | None = None,
) -> CharacterSprite | None:
    """Кадр персонажа, стоящего в точке (x, y) карты; None — если кадра нет.

    frame_index — номер кадра анимации; без него — кадр из записи персонажа.
    """
    sprites, animation = _sprites(character, graph, objects)
    if sprites is None:
        return None
    if frame_index is None:
        frame_index = character.frame
    frame = _frame(sprites, animation, character.direction, frame_index)
    if frame is None:
        return None
    image = sprites.image(frame.image_id)
    if image is None:
        return None

    palette = character.palette_offset // 2
    layers = [Layer(image, palette, x + frame.image_offset[0], y + frame.image_offset[1])]
    shadow_image = sprites.shadow(frame.image_id)
    shadow = None
    if shadow_image is not None:
        shadow = Layer(shadow_image, 0, x + frame.shadow_offset[0], y + frame.shadow_offset[1])

    if not character.has_dynamic_sprite:
        for slot in tables.draw_order[character.direction]:
            item_index = character.equipment[slot]
            item = state.items[item_index] if item_index > 0 else None
            if item is None or item.info_id >= len(tables.equipment):
                continue
            sprite_set = tables.equipment[item.info_id].sprite_set
            if sprite_set < 0:
                continue
            gear = graph.equipment[sprite_set]
            gear_frame = _frame(gear, animation, character.direction, frame_index)
            gear_image = gear.image(gear_frame.image_id) if gear_frame else None
            if gear_image is None:
                continue
            layers.append(
                Layer(
                    gear_image,
                    executable.EQUIPMENT_PALETTES[sprite_set] * COLORS_PER_PALETTE,
                    x + frame.weapon_offset[0] + gear_frame.image_offset[0],
                    y + frame.weapon_offset[1] + gear_frame.image_offset[1],
                )
            )
    return CharacterSprite(character, layers, shadow)


def animation(
    character: game_state.Character,
    x: int,
    y: int,
    state: game_state.GameState,
    graph: GraphRes,
    objects: ObjectsRes,
    tables: Executable,
) -> list[CharacterSprite]:
    """Все кадры анимации персонажа; кадр меняется каждый тик (Slavik game_play.cpp:3729)."""
    sprites = []
    for index in range(frame_count(character, graph, objects)):
        sprite = compose(character, x, y, state, graph, objects, tables, index)
        if sprite is not None:
            sprites.append(sprite)
    return sprites


def placed(
    entities: game_state.MapEntities, state: game_state.GameState
) -> list[tuple[game_state.Character, int, int]]:
    """Персонажи карты и клетки, где их рисовать.

    NPC и жители — на своих местах. Бродячий отряд — его первый член в середине
    прямоугольника появления: туда игра целится, расставляя отряд (Slavik PlaceMob).
    """
    result = [(c, c.x, c.y) for c in entities.characters]
    for group in entities.groups:
        if not group.is_roaming or group.size == 0:
            continue
        first = state.characters[group.first_character]
        if first is None:
            continue
        spawn = group.spawn
        x = (spawn.left + spawn.right + 1) // 2
        y = (spawn.top + spawn.bottom + 1) // 2
        result.append((first, x, y))
    return result
