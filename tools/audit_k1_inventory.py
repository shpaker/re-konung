# /// script
# requires-python = ">=3.14"
# dependencies = ["pefile==2024.8.26", "unicorn==2.1.4"]
# ///
"""Probe original inventory routines with synthetic records in both owned editions.

Run: uv run tools/audit_k1_inventory.py
Only operation bodies, requirement comparisons and inventory movement execute intact.
Attribute recalculation, weight calculation, presentation, sound and CRT memcpy are
controlled boundaries; their effects are not verified here. No game data is emitted.
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
    def __init__(self, build):
        self.build = build
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
        elif address == build.character_weight:
            self.return_value(self.carried_weight)
        elif address == build.item_weight:
            self.return_value(self.held_weight)
        elif address == 0x414A34:
            self.idle_requests += 1
            self.return_value()
        elif address in (
            build.recalculate,
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
        print(build.path.split("/")[0], results, "original calls:", probe.calls)
    print("Boundaries: recalculation and weights supplied; memcpy modeled; sound/UI skipped.")
    print("Shooting cancellation checked through idle request; idle transition itself skipped.")
    print("No runtime UI, combat effects or ownership validation claim.")


if __name__ == "__main__":
    main()
