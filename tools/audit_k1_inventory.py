# /// script
# requires-python = ">=3.14"
# dependencies = ["pefile==2024.8.26", "unicorn==2.1.4"]
# ///
"""Probe original inventory routines with synthetic records in both owned editions.

Run: uv run tools/audit_k1_inventory.py
The original matrices retain supplied attribute and mass boundaries. A second probe
executes recalculation and both mass procedure bodies intact. CRT memory operations,
presentation, sound and transition-to-idle remain controlled boundaries.
No game data is emitted.
"""

import struct
from dataclasses import dataclass
from pathlib import Path

import pefile
from unicorn import UC_ARCH_X86, UC_HOOK_CODE, UC_MODE_32, Uc
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EIP, UC_X86_REG_ESP

ROOT = Path(__file__).resolve().parents[1]
ACTOR = 0x1001000
STACK = 0x1008000
RETURN = 0x100F000
ITEMS = 0x659B0C
SELECTED = 0x81D560
CURSOR = 0x81D660
HELD = 0x81D400
BONUS = 0x458530
EQUIPMENT_REQUIREMENT = 0x459B80


@dataclass(frozen=True)
class Build:
    path: str
    requirement: int
    equip: int
    accessory: int
    extract: int
    prepend: int
    land: int
    restore: int
    cursor: int
    attribute: int
    recalculate: int
    character_weight: int
    item_weight: int
    copy: int
    fill: int
    sound: int
    hint: int
    screen: int
    refresh: int
    table_delta: int
    weapon: int
    weapon_ui: int


BUILDS = (
    Build(
        "rus/game/konung.res",
        0x4182D4,
        0x41E450,
        0x41E6BC,
        0x41E160,
        0x42462C,
        0x424410,
        0x428E98,
        0x424E0C,
        0x41B1A4,
        0x41C69C,
        0x41B7E4,
        0x41B0C4,
        0x43EA79,
        0x43EAD0,
        0x42A13C,
        0x42E9CC,
        0x42CBD0,
        0x42E520,
        0,
        0x4216DC,
        0x428DB0,
    ),
    Build(
        "eng/game/KONUNG.EXE",
        0x418324,
        0x41E4C0,
        0x41E738,
        0x41E1D0,
        0x424750,
        0x424530,
        0x428FBC,
        0x424F30,
        0x41B204,
        0x41C710,
        0x41B858,
        0x41B124,
        0x43ECCB,
        0x43ED20,
        0x42A2AC,
        0x42EBF4,
        0x42CDB0,
        0x42E748,
        0x38,
        0x4217A0,
        0x428ED4,
    ),
)


class Probe:
    def __init__(self, build, *, intact=False):
        self.build = build
        self.intact = intact
        executable = pefile.PE(str(ROOT / "original/k1" / build.path))
        image = executable.get_memory_mapped_image()
        self.machine = Uc(UC_ARCH_X86, UC_MODE_32)
        self.machine.mem_map(executable.OPTIONAL_HEADER.ImageBase, (len(image) + 4095) & ~4095)
        self.machine.mem_write(executable.OPTIONAL_HEADER.ImageBase, image)
        self.machine.mem_map(0x1000000, 0x10000)
        self.machine.hook_add(UC_HOOK_CODE, self.boundary)
        self.carried_weight = self.held_weight = 0
        self.calls = 0
        self.idle_requests = 0
        self.reset()

    def write(self, address, value, format="I"):
        self.machine.mem_write(address, struct.pack("<" + format, value))

    def read(self, address, format="I"):
        return struct.unpack("<" + format, self.machine.mem_read(address, struct.calcsize(format)))[
            0
        ]

    def return_value(self, value=0):
        stack = self.machine.reg_read(UC_X86_REG_ESP)
        destination = self.read(stack)
        self.machine.reg_write(UC_X86_REG_EAX, value & 0xFFFFFFFF)
        self.machine.reg_write(UC_X86_REG_ESP, stack + 4)
        self.machine.reg_write(UC_X86_REG_EIP, destination)

    def boundary(self, machine, address, size, data):
        build = self.build
        if address == build.copy:
            stack = machine.reg_read(UC_X86_REG_ESP)
            destination, source, length = struct.unpack("<III", machine.mem_read(stack + 4, 12))
            machine.mem_write(destination, bytes(machine.mem_read(source, length)))
            self.return_value(destination)
        elif address == build.fill:
            stack = machine.reg_read(UC_X86_REG_ESP)
            destination, value, length = struct.unpack("<III", machine.mem_read(stack + 4, 12))
            machine.mem_write(destination, bytes([value & 255]) * length)
            self.return_value(destination)
        elif not self.intact and address == build.character_weight:
            self.return_value(self.carried_weight)
        elif not self.intact and address == build.item_weight:
            self.return_value(self.held_weight)
        elif address == 0x414A34:
            self.idle_requests += 1
            self.return_value()
        elif (not self.intact and address == build.recalculate) or address in (
            build.sound,
            build.hint,
            build.screen,
            build.refresh,
            build.weapon_ui,
        ):
            self.return_value()

    def call(self, address, *arguments):
        self.machine.mem_write(
            STACK, struct.pack("<" + "I" * (1 + len(arguments)), RETURN, *arguments)
        )
        self.machine.reg_write(UC_X86_REG_ESP, STACK)
        self.machine.emu_start(address, RETURN, count=10000)
        assert self.machine.reg_read(UC_X86_REG_EIP) == RETURN, hex(address)
        self.calls += 1
        return self.machine.reg_read(UC_X86_REG_EAX)

    def reset(self, *, state=0, class_id=5):
        self.machine.mem_write(ACTOR, bytes(256))
        self.write(ACTOR, state, "B")
        self.write(ACTOR + 4, class_id, "B")
        for offset in (0x54, 0x58, 0x5E, 0x62):
            self.write(ACTOR + offset, 20, "h")
        self.write(SELECTED, ACTOR)
        for address in (CURSOR, HELD, 0x81D638, 0x81D610, 0x81D600, 0x81D5AC):
            self.write(address, 0)
        self.machine.mem_write(BONUS + self.build.table_delta, bytes(16))
        self.machine.mem_write(EQUIPMENT_REQUIREMENT + self.build.table_delta, bytes(4))
        for index in range(40):
            self.item(index, 0)
        self.write(0x81D624, 0)
        self.write(0x81A814, ACTOR)
        self.idle_requests = 0
        self.carried_weight = self.held_weight = 0
        if self.intact:
            for offset in (0x56, 0x5A, 0x60, 0x64):
                self.write(ACTOR + offset, 20, "h")
            self.write(ACTOR + 0xD2, -1, "b")
            self.write(ACTOR + 0xD3, -1, "b")
            self.machine.mem_write(BONUS + self.build.table_delta, bytes(74 * 16))

    def item(self, index, kind, *, bonus=-1, info=0):
        record = bytearray(16)
        struct.pack_into("<bbbb", record, 0, kind, bonus, -1, info)
        self.machine.mem_write(ITEMS + 16 * index, bytes(record))
        return ITEMS + 16 * index

    def inventory(self, values=None):
        if values is not None:
            self.machine.mem_write(
                ACTOR + 0x72, struct.pack("<32H", *(values + [0] * (32 - len(values))))
            )
        return list(struct.unpack("<32H", self.machine.mem_read(ACTOR + 0x72, 64)))

    def take(self, item_id, kind=32):
        self.call(self.build.cursor, ITEMS + 16 * item_id)
        self.call(self.build.extract, ACTOR, kind, item_id)

    def bonus(self, index, *effects, requirement=(0, 0)):
        record = bytearray(16)
        struct.pack_into("<bb", record, 2, *requirement)
        for position, effect in enumerate(effects):
            struct.pack_into("<BBh", record, 4 + position * 4, *effect)
        self.machine.mem_write(BONUS + self.build.table_delta + index * 16, bytes(record))

    def mass(self, kind, value, *, info=0):
        address, stride = (
            (EQUIPMENT_REQUIREMENT + 8, 64)
            if kind < 6 or kind == 12
            else (0x45812A, 128)
            if kind < 9
            else (0x4594CA, 128)
            if kind == 9
            else (0x460996, 128)
        )
        self.write(address + self.build.table_delta + info * stride, value, "h")


def requirements(probe):
    """Exercise both independent requirement gates with synthetic table entries."""
    count = 0
    for kind in (*range(9), 12):
        for current in (19, 20, 21):
            for base_required in (0, 20, 21):
                for bonus_required in (0, 20, 21):
                    probe.reset()
                    # Strength is field 5 in the original characteristic selector.
                    probe.write(ACTOR + 0x5E, current, "h")
                    probe.write(EQUIPMENT_REQUIREMENT + probe.build.table_delta, 5, "h")
                    probe.write(
                        EQUIPMENT_REQUIREMENT + probe.build.table_delta + 2, base_required, "h"
                    )
                    probe.write(BONUS + probe.build.table_delta + 2, 5, "b")
                    probe.write(BONUS + probe.build.table_delta + 3, bonus_required, "b")
                    item = probe.item(1, kind, bonus=0)
                    expected = current >= bonus_required
                    if kind < 6 or kind == 12:
                        expected &= current >= base_required
                    actual = probe.call(probe.build.requirement, ACTOR, item)
                    assert actual == expected, (
                        probe.build.path,
                        kind,
                        current,
                        base_required,
                        bonus_required,
                    )
                    count += 1
    return count


def compatibility(probe):
    count = 0
    for kind in range(13):
        for slot in (*range(6), 12, *range(14, 19)):
            probe.reset()
            # Accessory records need a valid bonus entry, even with no requirement.
            probe.item(1, kind, bonus=0 if kind in (6, 7, 8) else -1)
            probe.inventory([1])
            probe.take(1)
            accessory = slot >= 14
            expected = (
                kind == slot
                if not accessory
                else (kind == 6 and slot == 14)
                or (kind == 7 and slot in (15, 16))
                or (kind == 8 and slot in (17, 18))
            )
            actual = probe.call(probe.build.accessory if accessory else probe.build.equip, slot)
            assert actual == expected, (probe.build.path, kind, slot, actual, expected)
            offset = (
                0xB2 + 2 * (slot - 14) if accessory else 0xCC if slot == 12 else 0x66 + 2 * slot
            )
            assert probe.read(ACTOR + offset, "H") == expected
            assert probe.inventory()[0] == (0 if expected else 1)
            count += 1
    return count


def operations(probe):
    count = 0
    for state in range(10):
        for slot in (*range(6), 12, *range(14, 19)):
            accessory = slot >= 14
            kind = 6 if slot == 14 else 7 if slot in (15, 16) else 8 if accessory else slot
            offset = (
                0xB2 + 2 * (slot - 14) if accessory else 0xCC if slot == 12 else 0x66 + 2 * slot
            )
            function = probe.build.accessory if accessory else probe.build.equip
            # Replacing from a full inventory frees the held item's former cell first.
            probe.reset(state=state)
            probe.item(1, kind, bonus=0 if accessory else -1)
            probe.item(2, kind, bonus=0 if accessory else -1)
            probe.write(ACTOR + offset, 2, "H")
            probe.inventory([1, *range(3, 34)])
            probe.take(1)
            assert probe.inventory()[-1] == 0
            assert probe.call(function, slot) == 1
            assert probe.read(ACTOR + offset, "H") == 1
            assert probe.inventory() == [2, *range(3, 34)]
            assert probe.read(ACTOR, "B") == state
            count += 1
            # Unequip succeeds into the cursor; full-inventory landing restores the slot.
            probe.write(CURSOR, 4)
            assert probe.call(function, slot) == 1
            assert probe.read(ACTOR + offset, "H") == 0
            before = probe.inventory()
            assert probe.call(probe.build.land, ACTOR) == 0
            assert probe.read(ACTOR + offset, "H") == 1
            assert probe.inventory() == before
            count += 1
            # With a free cell the unequipped item lands at inventory index zero.
            probe.inventory([3])
            probe.write(CURSOR, 4)
            assert probe.call(function, slot) == 1
            assert probe.call(probe.build.land, ACTOR) == 1
            assert probe.read(ACTOR + offset, "H") == 0
            assert probe.inventory()[:3] == [1, 3, 0]
            assert probe.read(ACTOR, "B") == state
            count += 1
            # Empty equipment slots cannot create an item in the cursor.
            probe.write(CURSOR, 4)
            assert probe.call(function, slot) == 0
            count += 1
    # Dynamic-sprite characters reject ordinary equipment, before requirements.
    probe.reset(class_id=0x41)
    probe.item(1, 2)
    probe.inventory([1])
    probe.take(1)
    assert probe.call(probe.build.equip, 2) == 0
    assert probe.inventory()[0] == 1
    count += 1
    return count


def capacity(probe):
    count = 0
    for occupied in range(33):
        probe.reset()
        before = list(range(1, occupied + 1))
        probe.inventory(before)
        assert probe.call(probe.build.prepend, ACTOR, 39) == (occupied < 32)
        assert probe.inventory() == (
            [39, *before, *([0] * (31 - occupied))] if occupied < 32 else before
        )
        count += 1
    # Equality is allowed; one unit over the original capacity is rejected.
    for weight, expected in ((16665, 1), (16666, 1), (16667, 0)):
        probe.reset()
        probe.item(1, 2)
        probe.inventory([1])
        probe.take(1)
        probe.carried_weight = weight
        assert probe.call(probe.build.land, ACTOR) == expected
        assert probe.inventory()[0] == 1
        count += 1
    return count


def active_weapon(probe):
    count = 0
    for mask in range(8):
        for current in range(3):
            for requested in (0, 1, 2, 3):
                for state in range(10):
                    for shooting in (0, 4):
                        probe.reset(state=state)
                        probe.write(CURSOR, 4)
                        probe.write(ACTOR + 0x12, current, "B")
                        probe.write(ACTOR + 2, shooting, "B")
                        for slot in range(3):
                            probe.write(
                                ACTOR + 0x66 + 2 * slot,
                                (slot + 1) if mask & (1 << slot) else 0,
                                "H",
                            )
                        expected = current
                        if state not in (3, 9):
                            if requested < 3:
                                if mask & (1 << requested):
                                    expected = requested
                            else:
                                expected = next(
                                    (
                                        slot
                                        for step in (1, 2)
                                        if mask & (1 << (slot := (current + step) % 3))
                                    ),
                                    current,
                                )
                        probe.call(probe.build.weapon, requested)
                        assert probe.read(ACTOR + 0x12, "B") == expected
                        cancelled = bool(shooting and expected != 1 and state not in (3, 9))
                        assert probe.idle_requests == cancelled
                        assert probe.read(ACTOR + 2, "B") == (0 if cancelled else shooting)
                        count += 1
    probe.reset(class_id=0x85)
    probe.write(CURSOR, 4)
    probe.write(ACTOR + 0x66, 1, "H")
    probe.write(ACTOR + 0x12, 2, "B")
    probe.call(probe.build.weapon, 0)
    assert probe.read(ACTOR + 0x12, "B") == 2
    return count + 1


def recalculation(probe):
    """Check original ordering, identification, clamps and repeated health changes."""
    count = 0
    for characteristic, current, base, minimum in (
        (2, 0x54, 0x56, 0),
        (3, 0x58, 0x5A, 0),
        (5, 0x5E, 0x60, 0),
        (6, 0x62, 0x64, 1),
    ):
        for additive in (0, 1):
            for value in (-30, -20, -1, 0, 7, 20):
                for unidentified in (0, 1):
                    probe.reset()
                    probe.bonus(0, (characteristic, additive, value))
                    item = probe.item(1, 2, bonus=0)
                    probe.write(item + 14, unidentified, "B")
                    probe.write(ACTOR + 0x6A, 1, "H")
                    expected = 20 if unidentified or value == 0 else value + 20 * additive
                    expected = max(minimum, expected)
                    probe.call(probe.build.recalculate, ACTOR)
                    assert probe.read(ACTOR + current, "h") == expected
                    assert probe.read(ACTOR + base, "h") == 20
                    snapshot = bytes(probe.machine.mem_read(ACTOR, 256))
                    probe.call(probe.build.recalculate, ACTOR)
                    assert bytes(probe.machine.mem_read(ACTOR, 256)) == snapshot
                    probe.write(ACTOR + 0x6A, 0, "H")
                    probe.call(probe.build.recalculate, ACTOR)
                    assert probe.read(ACTOR + current, "h") == 20
                    count += 1

    # Equipment precedes accessories, then the two character-local effect sets.
    locations = (0x66, 0x6A, 0xB2, 0xD2, 0xD3)
    for replacement in range(len(locations)):
        for replace_value in (0, 31):
            probe.reset()
            for index, offset in enumerate(locations):
                probe.bonus(
                    index,
                    (5, int(index != replacement), replace_value if index == replacement else 3),
                )
                if offset < 0xD2:
                    probe.item(index + 1, 6 if offset == 0xB2 else 2, bonus=index)
                    probe.write(ACTOR + offset, index + 1, "H")
                else:
                    probe.write(ACTOR + offset, index, "b")
            probe.call(probe.build.recalculate, ACTOR)
            assert probe.read(ACTOR + 0x5E, "h") == (replace_value or 20)
            count += 1
    probe.reset()
    probe.bonus(0, (5, 1, 3), (5, 0, 31), (5, 1, 7))
    probe.item(1, 2, bonus=0)
    probe.write(ACTOR + 0x6A, 1, "H")
    probe.call(probe.build.recalculate, ACTOR)
    assert probe.read(ACTOR + 0x5E, "h") == 31
    count += 1

    for characteristic, offset, flag in ((7, 0xC4, 1), (8, 0xC6, 2), (9, 0xC8, 4)):
        for additive in (0, 1):
            for value in (-3, 0, 7):
                probe.reset()
                probe.bonus(0, (characteristic, additive, value))
                probe.item(1, 2, bonus=0)
                probe.write(ACTOR + 0x6A, 1, "H")
                probe.call(probe.build.recalculate, ACTOR)
                assert probe.read(ACTOR + offset, "h") == value
                assert probe.read(ACTOR + 0xD1, "B") == (7 if additive else 7 ^ flag)
                count += 1

    for value, additive, initial, first, second in (
        (1, 1, 100, 116, 132),
        (-1, 1, 100, 84, 68),
        (-10, 1, 100, -60, -220),
        (100, 1, 100, 1600, 1600),
        (7, 0, 100, 112, 112),
        (0, 0, 100, 100, 100),
    ):
        probe.reset()
        probe.bonus(0, (4, additive, value))
        probe.item(1, 2, bonus=0)
        probe.write(ACTOR + 0x6A, 1, "H")
        probe.write(ACTOR + 0x5C, initial, "h")
        probe.call(probe.build.recalculate, ACTOR)
        assert probe.read(ACTOR + 0x5C, "h") == first
        probe.call(probe.build.recalculate, ACTOR)
        assert probe.read(ACTOR + 0x5C, "h") == second
        probe.write(ACTOR + 0x6A, 0, "H")
        probe.call(probe.build.recalculate, ACTOR)
        assert probe.read(ACTOR + 0x5C, "h") == second
        count += 1
    return count


def intact_requirements(probe):
    """Requirements use recalculated equipped effects, not the candidate's effects."""
    count = 0
    for characteristic, offset in ((2, 0x54), (3, 0x58), (5, 0x5E), (6, 0x62)):
        for base in (19, 20, 21):
            for required in (19, 20, 21, 25):
                for supporting in (False, True):
                    probe.reset()
                    probe.write(ACTOR + offset + 2, base, "h")
                    probe.write(ACTOR + offset, 99, "h")
                    probe.bonus(0, (characteristic, 1, 5), requirement=(characteristic, required))
                    probe.write(
                        EQUIPMENT_REQUIREMENT + probe.build.table_delta, characteristic, "h"
                    )
                    probe.write(EQUIPMENT_REQUIREMENT + probe.build.table_delta + 2, required, "h")
                    candidate = probe.item(1, 2, bonus=0)
                    if supporting:
                        probe.item(2, 0, bonus=0)
                        probe.write(ACTOR + 0x66, 2, "H")
                    assert probe.call(probe.build.requirement, ACTOR, candidate) == (
                        base + 5 * supporting >= required
                    )
                    assert probe.read(ACTOR + offset, "h") == base + 5 * supporting
                    if supporting:
                        probe.write(ACTOR + 0x66, 0, "H")
                        assert probe.call(probe.build.requirement, ACTOR, candidate) == (
                            base >= required
                        )
                    count += 1
    # A check on an already-equipped item sees that item's own bonus.
    probe.reset()
    probe.bonus(0, (5, 1, 5), requirement=(5, 25))
    candidate = probe.item(1, 2, bonus=0)
    probe.write(ACTOR + 0x6A, 1, "H")
    assert probe.call(probe.build.requirement, ACTOR, candidate) == 1
    probe.write(ACTOR + 0x6A, 0, "H")
    assert probe.call(probe.build.requirement, ACTOR, candidate) == 0
    count += 1
    # Even a failed predicate can change HP through its recalculation call.
    probe.reset()
    probe.bonus(0, (4, 1, 1))
    probe.item(2, 0, bonus=0)
    probe.write(ACTOR + 0x66, 2, "H")
    probe.write(ACTOR + 0x5C, 100, "h")
    candidate = probe.item(1, 2)
    probe.write(EQUIPMENT_REQUIREMENT + probe.build.table_delta, 5, "h")
    probe.write(EQUIPMENT_REQUIREMENT + probe.build.table_delta + 2, 21, "h")
    for health in (116, 132):
        assert probe.call(probe.build.requirement, ACTOR, candidate) == 0
        assert probe.read(ACTOR + 0x5C, "h") == health
    return count + 1


def intact_mass(probe):
    """Read masses from synthetic EXE tables; arrows use the raw integer quantity."""
    count = 0
    for kind in range(13):
        for mass in (0, 1, 1234, 32767):
            for quantity in (0, 1, 3):
                probe.reset()
                item = probe.item(1, kind)
                probe.mass(kind, mass)
                probe.write(item + 4, quantity, "i")
                probe.write(item + 8, 999.0, "f")
                expected = 0 if kind == 10 else mass * (quantity if kind == 12 else 1)
                assert probe.call(probe.build.item_weight, item) == expected
                probe.inventory([1])
                assert probe.call(probe.build.character_weight, ACTOR) == expected
                count += 1

    # All stores count; gaps in the bag do not terminate the mass scan.
    probe.reset()
    for index, kind, info, mass in (
        (1, 2, 0, 123),
        (2, 6, 0, 234),
        (3, 9, 0, 345),
        (4, 11, 0, 456),
        (5, 12, 1, 7),
    ):
        item = probe.item(index, kind, info=info)
        probe.mass(kind, mass, info=info)
        if kind == 12:
            probe.write(item + 4, 3, "i")
    probe.write(ACTOR + 0x6A, 1, "H")
    probe.write(ACTOR + 0xB2, 2, "H")
    probe.inventory([3, 0, 4])
    probe.write(ACTOR + 0xCC, 5, "H")
    assert probe.call(probe.build.character_weight, ACTOR) == 123 + 234 + 345 + 456 + 21
    count += 1

    for endurance in (1, 2, 20, 37, 95):
        limit = endurance * 1000 // 3 + 10000
        for difference in (-1, 0, 1):
            probe.reset()
            probe.write(ACTOR + 0x64, endurance, "h")
            probe.call(probe.build.recalculate, ACTOR)
            probe.item(1, 2, info=0)
            probe.item(2, 2, info=1)
            probe.item(3, 2, info=2)
            probe.mass(2, 500)
            probe.mass(2, 9000, info=1)
            probe.mass(2, limit + difference - 9500, info=2)
            probe.inventory([1, 2, 3])
            probe.take(1)
            assert probe.call(probe.build.land, ACTOR) == (difference <= 0)
            restored = [2, 3, 1] if probe.build.table_delta == 0 else [1, 2, 3]
            assert probe.inventory()[:3] == ([1, 2, 3] if difference <= 0 else restored), (
                probe.build.path,
                endurance,
                difference,
                probe.inventory()[:3],
            )
            count += 1
    return count


def intact_operations(probe):
    """Repeat transfer scenarios with genuine recalculation and mass bodies."""
    for kind in (0, 6, 9, 11):
        probe.mass(kind, 0)
    count = operations(probe)
    for bonus_value in (0, 3):
        for difference in (-1, 0, 1):
            for recalculate_before_landing in (False, True):
                probe.reset()
                probe.bonus(0, (6, 1, bonus_value))
                probe.item(1, 5, bonus=0)
                probe.item(2, 2, info=1)
                probe.mass(5, 0)
                probe.mass(2, 16666 + difference, info=1)
                probe.inventory([2])
                probe.write(ACTOR + 0x70, 1, "H")
                probe.call(probe.build.recalculate, ACTOR)
                assert probe.read(ACTOR + 0x62, "h") == 20 + bonus_value
                probe.write(CURSOR, 4)
                assert probe.call(probe.build.equip, 5) == 1
                # The handler does not recalculate on extraction; caller ordering matters.
                assert probe.read(ACTOR + 0x62, "h") == 20 + bonus_value
                if recalculate_before_landing:
                    probe.call(probe.build.recalculate, ACTOR)
                current = 20 if recalculate_before_landing else 20 + bonus_value
                accepted = 16666 + difference <= current * 1000 // 3 + 10000
                assert probe.call(probe.build.land, ACTOR) == accepted
                assert probe.read(ACTOR + 0x70, "H") == (0 if accepted else 1)
                assert probe.read(ACTOR + 0x62, "h") == current
                assert probe.inventory()[:2] == ([1, 2] if accepted else [2, 0])
                count += 1
    return count


def main():
    for build in BUILDS:
        if not (ROOT / "original/k1" / build.path).is_file():
            raise FileNotFoundError(f"Both owned editions are required: {build.path}")
    for build in BUILDS:
        probe = Probe(build)
        results = {
            "requirements": requirements(probe),
            "compatibility": compatibility(probe),
            "operations": operations(probe),
            "capacity": capacity(probe),
            "active_weapon": active_weapon(probe),
        }
        intact = Probe(build, intact=True)
        results.update(
            recalculation=recalculation(intact),
            intact_requirements=intact_requirements(intact),
            intact_mass=intact_mass(intact),
            intact_operations=intact_operations(intact),
        )
        print(build.path.split("/")[0], results, "original calls:", probe.calls + intact.calls)
    print("Legacy matrices: recalculation and weights supplied. New intact_* and recalculation")
    print("matrices execute game recalculation/mass bodies; CRT memcpy/memset modeled;")
    print("sound/UI skipped. ENG protected memset is not executed.")
    print("Shooting cancellation checked through idle request; idle transition itself skipped.")
    print("No runtime UI, combat damage or ownership validation claim.")


if __name__ == "__main__":
    main()
