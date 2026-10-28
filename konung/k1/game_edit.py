"""Добавление и удаление персонажей в начальном состоянии Game.N (docs/k1/formats/game-state.md).

Отряды уложены плотно: за отрядом зарезервировано growth_limit слотов персонажей,
его члены — первые size из них, следующий отряд начинается сразу за резервом.
Ссылки на номер персонажа — первый член отряда, враг (номер + 1, 0 — нет) и
персонаж рабочего места деревни (0 — место свободно); на номер отряда — отряд
персонажа, староста деревни и отряд-цель. Правки ниже сохраняют эту укладку и
переписывают все ссылки, поэтому остальной файл не меняется.
"""

import copy
from collections.abc import Callable

from konung.k1.game_state import (
    CHARACTER_SLOTS,
    GROUP_SLOTS,
    ITEM_SLOTS,
    PLAYER_GROUP,
    Character,
    GameState,
    Group,
)

# NPC на своих местах: обычно отряд из одного персонажа без резерва, но в каждом
# Game.N есть такие отряды из 2, 14 и 21 персонажа
NPC_GROUP = 0x53
# Байт диалога 0xD0 у персонажа без своего диалога — так у большинства в файлах
NO_DIALOG = 0xFF
# Номер записи колчана со стрелами — восьмое из «разного» (0xCC, Slavik game.h:498,
# game_character.cpp:694); во всех файлах 0, но это тоже ссылка на предмет
ARROWS = 8
# Резерв, размер и наибольший размер отряда — по байту
GROUP_RESERVE_MAX = 0xFF


class EditError(ValueError):
    """Правку нельзя сделать, не сломав укладку или ссылки."""


def _workers(state: GameState) -> set[int]:
    return {job.character for v in state.villages if v for job in v.jobs if job.character}


def _renumber(state: GameState, moved: Callable[[int], int | None]) -> None:
    """Переложить персонажей: moved(номер) — новый номер, None — персонаж удалён.

    Ссылки «враг» и «рабочее место» следуют за персонажем; first_character
    отрядов правит вызывающий, он знает новую укладку.
    """
    characters: list[Character | None] = [None] * CHARACTER_SLOTS
    for character in state.characters:
        if character is None:
            continue
        slot = moved(character.slot)
        if slot is None:
            continue
        if not 0 <= slot < CHARACTER_SLOTS:
            raise EditError(f"персонажей больше {CHARACTER_SLOTS}: не хватает слотов")
        character.slot = slot
        characters[slot] = character
    for character in characters:
        if character is not None and character.enemy > 0:
            enemy = moved(character.enemy - 1)
            character.enemy = 0 if enemy is None else enemy + 1
    for village in state.villages:
        for job in village.jobs if village else ():
            if job.character:
                slot = moved(job.character)
                job.character = 0 if slot is None else slot
    state.characters = characters


def _item_refs(character: Character) -> list[int]:
    refs = [*character.equipment, *character.inventory, *character.accessories]
    return [slot for slot in (*refs, character.misc[ARROWS]) if slot > 0]


def _free_items(state: GameState, character: Character) -> None:
    for slot in _item_refs(character):
        state.items[slot] = None


def _clone_items(state: GameState, character: Character) -> None:
    """У копии свои предметы: у каждой записи предмета ровно один владелец.

    Места вызывающий проверяет заранее (_check_room).
    """
    free = (slot for slot in range(1, ITEM_SLOTS) if state.items[slot] is None)

    def clone(slot: int) -> int:
        if slot <= 0:
            return slot
        new = next(free)
        item = copy.copy(state.items[slot])
        item.slot = new
        state.items[new] = item
        return new

    character.equipment = tuple(map(clone, character.equipment))
    character.inventory = tuple(map(clone, character.inventory))
    character.accessories = tuple(map(clone, character.accessories))
    misc = list(character.misc)
    misc[ARROWS] = clone(misc[ARROWS])
    character.misc = tuple(misc)


def _reserves_end(state: GameState) -> int:
    """Слот сразу за резервом последнего отряда."""
    last = [g for g in state.groups if g is not None][-1]
    return last.first_character + last.growth_limit


def _check_room(state: GameState, template: Character, characters: int, groups: int) -> None:
    """Хватит ли места на копию: characters слотов персонажей за резервами,
    groups слотов отрядов и записи предметов — до любой правки состояния."""
    if _reserves_end(state) + characters > CHARACTER_SLOTS:
        raise EditError(f"персонажей больше {CHARACTER_SLOTS}: не хватает слотов")
    if sum(1 for g in state.groups if g is not None) + groups > GROUP_SLOTS:
        raise EditError(f"отрядов больше {GROUP_SLOTS}: не хватает слотов")
    free = sum(1 for item in state.items[1:] if item is None)
    if len(_item_refs(template)) > free:
        raise EditError(f"предметов больше {ITEM_SLOTS}: не хватает слотов")


def _remove_group(state: GameState, group: Group) -> None:
    """Убрать пустой отряд: следующие отряды сдвигаются на слот вниз, их персонажи —
    на его резерв, ссылки на номера отрядов переписываются."""
    removed, reserve = group.slot, group.growth_limit
    end = group.first_character + reserve
    _renumber(state, lambda slot: slot - reserve if slot >= end else slot)
    groups: list[Group | None] = [None] * GROUP_SLOTS
    for other in state.groups:
        if other is None or other is group:
            continue
        if other.slot > removed:
            other.slot -= 1
            other.first_character -= reserve
        if other.target_group > removed:
            other.target_group -= 1
        groups[other.slot] = other
    state.groups = groups
    for character in state.characters:
        if character is not None and character.group > removed:
            character.group -= 1
    for village in state.villages:
        if village is not None and village.elder_group > removed:
            village.elder_group -= 1


def remove_character(state: GameState, slot: int) -> None:
    """Удалить персонажа: следующие члены его отряда сдвигаются на слот вниз.

    Отряд одиночного NPC, оставшийся пустым, убирается целиком. Игрока и
    работников деревни удалять нельзя: на них держится игра и деревня.
    """
    character = state.characters[slot]
    if character is None:
        raise EditError(f"персонажа {slot} нет")
    group = state.groups[character.group]
    if group.kind == PLAYER_GROUP:
        raise EditError("персонажей отряда игрока удалять нельзя")
    if slot in _workers(state):
        raise EditError(f"персонаж {slot} занимает рабочее место деревни — его удалять нельзя")
    _free_items(state, character)
    last = group.first_character + group.size - 1
    _renumber(
        state,
        lambda s: None if s == slot else s - 1 if slot < s <= last else s,
    )
    group.size -= 1
    if group.size == 0 and group.kind == NPC_GROUP:
        _remove_group(state, group)


def _grow(state: GameState, group: Group) -> None:
    """Увеличить резерв отряда на слот: всё за ним сдвигается на слот вверх."""
    end = group.first_character + group.growth_limit
    _renumber(state, lambda slot: slot + 1 if slot >= end else slot)
    for other in state.groups:
        if other is not None and other.slot > group.slot:
            other.first_character += 1
    group.growth_limit += 1


def add_character(state: GameState, template: Character, group: Group, x: int, y: int) -> int:
    """Новый персонаж — копия template на месте (x, y); возвращает его номер.

    Копия одиночного NPC (отряд 0x53 с резервом 1) получает свой отряд — копию
    отряда template в первом свободном слоте, за резервом последнего отряда.
    Остальные встают последним членом отряда group; если резерв занят, он растёт
    на слот. Предметы копии свои, врага и диалога у неё нет. Если места нет,
    состояние не меняется.
    """
    if group.kind == PLAYER_GROUP:
        raise EditError("добавлять персонажей в отряд игрока нельзя")
    own_group = group.kind == NPC_GROUP and group.growth_limit == 1
    grows = not own_group and group.size == group.growth_limit
    if grows and group.growth_limit == GROUP_RESERVE_MAX:
        raise EditError(
            f"в отряде {group.slot} уже {GROUP_RESERVE_MAX} персонажей — больше не помещается"
        )
    _check_room(state, template, 1 if own_group or grows else 0, 1 if own_group else 0)
    character = copy.deepcopy(template)
    character.x, character.y = x, y
    character.enemy = 0
    character.unknown_d0 = NO_DIALOG
    _clone_items(state, character)
    if own_group:
        last = [g for g in state.groups if g is not None][-1]
        new_group = copy.deepcopy(group)
        new_group.slot = last.slot + 1
        new_group.first_character = _reserves_end(state)
        new_group.size = 0
        new_group.max_size = new_group.growth_limit = 1
        state.groups[new_group.slot] = new_group
        group = new_group
    elif grows:
        _grow(state, group)
    slot = group.first_character + group.size
    character.slot, character.group = slot, group.slot
    state.characters[slot] = character
    group.size += 1
    group.max_size = max(group.max_size, group.size)
    return slot
