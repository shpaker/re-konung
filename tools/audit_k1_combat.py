# /// script
# requires-python = ">=3.14"
# dependencies = ["pefile==2024.8.26", "unicorn==2.1.4", "capstone==5.0.6"]
# ///
"""Check the owner's original melee routines with synthetic inputs, without game dumps.

Run: uv run tools/audit_k1_combat.py
ENG nearest-target probes replace its protected CRT abs helper only; this limitation
is printed separately from intact original execution. Randomness is a controlled input.
"""

import struct
from pathlib import Path

import pefile
from capstone import CS_ARCH_X86, CS_MODE_32, Cs
from unicorn import UC_ARCH_X86, UC_HOOK_CODE, UC_MODE_32, Uc
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EIP, UC_X86_REG_ESP

ROOT = Path(__file__).resolve().parents[1]
CHARACTERS = 0x78A250
STACK = 0x1008000
RETURN = 0x100F000
ACTOR = 0x1001000
TARGET_GROUP = 0x1002000
OWN_GROUP = 0x1002100
BUILDS = (
    ("rus/game/konung.res", 0x41FF68, 0x420098, 0x420190, 0x43881C, 0x43EA46, 0x4EE6B0, 0),
    ("eng/game/KONUNG.EXE", 0x41FFEC, 0x42011C, 0x420214, 0x438A64, 0x43EC98, 0x4EE6E8, 0x130),
)


def write_integer(machine, address, value):
    machine.mem_write(address, struct.pack("<I", value))


def read_integer(machine, address):
    return struct.unpack("<i", machine.mem_read(address, 4))[0]


def call(machine, address, *arguments):
    machine.mem_write(STACK, struct.pack("<" + "I" * (len(arguments) + 1), RETURN, *arguments))
    machine.reg_write(UC_X86_REG_ESP, STACK)
    machine.emu_start(address, RETURN, count=100000)
    assert machine.reg_read(UC_X86_REG_EIP) == RETURN, "routine did not return"
    return machine.reg_read(UC_X86_REG_EAX)


def return_value(machine, value):
    stack = machine.reg_read(UC_X86_REG_ESP)
    destination = struct.unpack("<I", machine.mem_read(stack, 4))[0]
    machine.reg_write(UC_X86_REG_EAX, value & 0xFFFFFFFF)
    machine.reg_write(UC_X86_REG_ESP, stack + 4)
    machine.reg_write(UC_X86_REG_EIP, destination)


def character(machine, slot, x, y, *, state=0, dead=False, target=0):
    record = bytearray(256)
    record[0] = state
    record[2] = int(target != 0)
    record[4] = 0x80 if dead else 5
    struct.pack_into("<hhh", record, 6, target, y, x)
    machine.mem_write(CHARACTERS + slot * 256, bytes(record))


def group(machine, address, first, count):
    record = bytearray(32)
    struct.pack_into("<h", record, 0, first)
    record[0x1C] = count
    machine.mem_write(address, bytes(record))


def selectors(machine, shared, nearest, unassigned):
    group(machine, TARGET_GROUP, 0, 3)
    group(machine, OWN_GROUP, 3, 2)
    machine.mem_write(ACTOR, bytes(256))
    machine.mem_write(ACTOR + 8, struct.pack("<hh", 10, 10))
    for slot, x in enumerate([12, 11, 9, 0, 0]):
        character(machine, slot, x, 10)
    assert call(machine, nearest, TARGET_GROUP, 0, ACTOR) == CHARACTERS + 256
    character(machine, 1, 11, 10, dead=True)
    assert call(machine, nearest, TARGET_GROUP, 0, ACTOR) == CHARACTERS + 512
    character(machine, 2, 9, 10, state=3)
    assert call(machine, nearest, TARGET_GROUP, 0, ACTOR) == CHARACTERS
    character(machine, 0, 12, 10, state=9)
    assert call(machine, nearest, TARGET_GROUP, 0, ACTOR) == 0
    for slot, x in enumerate([12, 11, 9]):
        character(machine, slot, x, 10)
    character(machine, 3, 0, 0, target=2)
    assert call(machine, unassigned, TARGET_GROUP, OWN_GROUP, ACTOR) == CHARACTERS + 512
    character(machine, 4, 0, 0, target=3)
    assert call(machine, unassigned, TARGET_GROUP, OWN_GROUP, ACTOR) == CHARACTERS
    character(machine, 3, 0, 0, target=1)
    assert call(machine, shared, 0, OWN_GROUP) == CHARACTERS
    character(machine, 0, 12, 10, state=3)
    assert call(machine, shared, 0, OWN_GROUP) == CHARACTERS + 512


def shared_targets(machine, shared):
    group(machine, OWN_GROUP, 3, 2)
    character(machine, 0, 12, 10)
    character(machine, 1, 11, 10)
    character(machine, 3, 0, 0, target=1)
    character(machine, 4, 0, 0, target=2)
    assert call(machine, shared, 0, OWN_GROUP) == CHARACTERS
    character(machine, 0, 12, 10, state=3)
    assert call(machine, shared, 0, OWN_GROUP) == CHARACTERS + 256
    character(machine, 1, 11, 10, dead=True)
    assert call(machine, shared, 0, OWN_GROUP) == 0
    character(machine, 0, 12, 10)
    character(machine, 1, 11, 10)
    character(machine, 3, 0, 0, dead=True, target=1)
    assert call(machine, shared, 0, OWN_GROUP) == CHARACTERS + 256


def viewport_and_retreat(machine, shift, random_address, foot_grid):
    for start, end, expected in [
        (0x42784E, 0x42790F, (528, 445, 7, 17)),
        (0x427914, 0x4279D5, (660, 556, 9, 20)),
    ]:
        write_integer(machine, 0x81D754, 0)
        machine.reg_write(UC_X86_REG_ESP, STACK)
        machine.emu_start(start + shift, end + shift, count=1000)
        assert machine.reg_read(UC_X86_REG_EIP) == end + shift
        actual = tuple(
            read_integer(machine, address) for address in (0x81D720, 0x81D71C, 0x81D6A4, 0x81D6E0)
        )
        assert actual == expected, actual

    side = 0

    def controlled_random(emulator, _address, _size, _data):
        return_value(emulator, side)

    hook = machine.hook_add(
        UC_HOOK_CODE, controlled_random, begin=random_address, end=random_address
    )
    machine.mem_write(ACTOR, bytes(256))
    machine.mem_write(ACTOR + 8, struct.pack("<hh", 160, 80))
    offsets = [(-9, -40), (-9, 40), (-18, -20), (18, -20)]
    for side, (x, y) in enumerate(offsets):
        for blocked in range(3):
            machine.mem_write(foot_grid, b"\xff\x0f" * 51200 if blocked == 2 else bytes(102400))
            if blocked == 1:
                machine.mem_write(foot_grid + (160 + y) * 320 + (80 + x) * 2, b"\xff\x0f")
            result = call(machine, 0x4101E4, ACTOR, 0x1003000, 0x1003004)
            assert bool(result) == (blocked != 2)
            if result:
                actual = (read_integer(machine, 0x1003004), read_integer(machine, 0x1003000))
                expected = (x + int(blocked == 1 and side < 2), y + int(blocked == 1 and side >= 2))
                assert actual == expected, (side, blocked, actual, expected)
    machine.hook_del(hook)


def verify_control_flow(machine, english):
    disassembler = Cs(CS_ARCH_X86, CS_MODE_32)
    # Compare original branches without copying any code bytes into the repository.
    checks = [
        (0x4120A3, "cmp"),
        (0x4126AC, "cmp"),
        (0x4126BD, "call"),
        (0x421D7F if english else 0x421CB7, "cmp"),
        (0x421DD8 if english else 0x421D10, "call"),
        (0x41EBCC + 0xF if english else 0x41EB57, "test"),
    ]
    for address, mnemonic in checks:
        instruction = next(disassembler.disasm(bytes(machine.mem_read(address, 16)), address))
        assert instruction.mnemonic == mnemonic, (hex(address), instruction.mnemonic)


def main():
    for relative, shared, nearest, unassigned, distance, random_address, foot_grid, shift in BUILDS:
        path = ROOT / "original/k1" / relative
        executable = pefile.PE(str(path))
        machine = Uc(UC_ARCH_X86, UC_MODE_32)
        machine.mem_map(0x400000, 0x500000)
        machine.mem_write(0x400000, executable.get_memory_mapped_image())
        machine.mem_map(0x1000000, 0x10000)
        english = relative.startswith("eng")
        verify_control_flow(machine, english)
        shared_targets(machine, shared)
        viewport_and_retreat(machine, shift, random_address, foot_grid)
        print(f"{relative}: intact shared-target 4, viewport 2, retreat 12, control-flow 6 passed")
        if english:

            def absolute_value(emulator, _address, _size, _data):
                stack = emulator.reg_read(UC_X86_REG_ESP)
                value = read_integer(emulator, stack + 4)
                return_value(emulator, abs(value))

            machine.hook_add(UC_HOOK_CODE, absolute_value, begin=0x43FADB, end=0x43FADB)
        selectors(machine, shared, nearest, unassigned)
        for x in range(25):
            for y in range(25):
                horizontal, vertical = abs(x - 12), abs(y - 12)
                expected = max(horizontal, vertical) + int(min(horizontal, vertical) > 1)
                assert call(machine, distance, x, y, 12, 12) == expected
        qualifier = "conditional: protected CRT abs replaced" if english else "intact"
        print(f"{relative}: {qualifier}; target 8, distance 625 passed")


if __name__ == "__main__":
    main()
