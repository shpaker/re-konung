"""OBJECTS.RES — объекты карты: 30 динамических и 482 простых (docs/k1/formats/objects-res.md).

Простой объект — до 8 кадров (картинка + тень) со смещениями и точками огней.
Динамический объект — монстр: анимации по 8 направлениям, как спрайт персонажа.
"""

from dataclasses import dataclass
from pathlib import Path

from konung import rle
from konung.binary import Reader
from konung.k1.graph_res import Sprites, read_sprites

SLOTS = 512
DYNAMIC_COUNT = 30
FRAMES = 8
FLAME_SLOTS = 20
# Динамический объект: 6 анимаций × 8 направлений × до 18 кадров, 512 картинок
DYNAMIC_ANIMATIONS = 6
DYNAMIC_FRAMES = 18
DYNAMIC_IMAGES = 512


@dataclass
class Flame:
    flame_id: int
    x: int
    y: int


@dataclass
class Frame:
    image: rle.IndexedImage | None
    shadow: rle.IndexedImage | None
    # Смещение левого верхнего угла картинки и тени от позиции объекта
    offset: tuple[int, int]
    shadow_offset: tuple[int, int]


@dataclass
class SimpleObject:
    object_id: int
    flags: int
    frame_count: int
    palette_offset: int
    frame_time: int
    flames: list[Flame]
    frames: list[Frame]
    unknown: int


@dataclass
class ObjectsRes:
    simple: dict[int, SimpleObject]
    # Монстры: номер — Character.sprite_set у персонажей с битом 0x40 в классе
    dynamic: dict[int, Sprites]


def _simple(reader: Reader, object_id: int) -> SimpleObject:
    flags = reader.u32()
    reader.u32()  # размер блока
    flame_count = reader.s32()
    frame_count = reader.u32()
    unknown = reader.u32()
    palette_offset = reader.u32() // 2
    frame_time = reader.s32()

    flames = [Flame(reader.s8(), reader.s16(), reader.s16()) for _ in range(flame_count)]
    reader.skip((FLAME_SLOTS - flame_count) * 5)

    data_offsets = [(reader.s32(), reader.s32()) for _ in range(FRAMES)]
    offsets = []
    for _ in range(FRAMES):
        shadow_offset = (reader.s16(), reader.s16())
        offset = (reader.s16(), reader.s16())
        offsets.append((shadow_offset, offset))
    data_start = reader.pos

    frames = []
    for (shadow_at, image_at), (shadow_offset, offset) in zip(data_offsets, offsets, strict=True):
        image = shadow = None
        if image_at >= 0:
            reader.seek(data_start + image_at)
            image = rle.decode_indexed(reader)
        if shadow_at >= 0:
            reader.seek(data_start + shadow_at)
            shadow = rle.decode_shadow(reader)
        frames.append(Frame(image, shadow, offset, shadow_offset))

    return SimpleObject(
        object_id, flags, frame_count, palette_offset, frame_time, flames, frames, unknown
    )


def parse(data: bytes) -> ObjectsRes:
    reader = Reader(data)
    reader.skip(8)
    entries = [(reader.s32(), reader.s32()) for _ in range(SLOTS)]
    data_start = reader.pos

    dynamic = {}
    simple = {}
    for slot, (offset, _size) in enumerate(entries):
        if offset < 0:
            continue
        if slot < DYNAMIC_COUNT:
            # Пропускаем метку и размер блока
            dynamic[slot] = read_sprites(
                data,
                data_start + offset + 8,
                DYNAMIC_ANIMATIONS,
                DYNAMIC_FRAMES,
                DYNAMIC_IMAGES,
                weapons=False,
            )
            continue
        reader.seek(data_start + offset)
        object_id = slot - DYNAMIC_COUNT
        simple[object_id] = _simple(reader, object_id)
    return ObjectsRes(simple, dynamic)


def load(path: Path) -> ObjectsRes:
    return parse(path.read_bytes())
