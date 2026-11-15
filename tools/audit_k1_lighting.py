# /// script
# requires-python = ">=3.14"
# dependencies = ["pefile==2024.8.26", "unicorn==2.1.4"]
# ///
"""Execute the owner's K1 light routines and compare every tick with our Rust model.

Run from the repository root: uv run tools/audit_k1_lighting.py
Only numeric summaries are printed. No binary, trace or game asset is distributed.
"""

import hashlib
import struct
import subprocess
from pathlib import Path

import pefile
from unicorn import UC_ARCH_X86, UC_HOOK_CODE, UC_MODE_32, Uc
from unicorn.x86_const import (
    UC_X86_REG_EAX,
    UC_X86_REG_EBP,
    UC_X86_REG_EIP,
    UC_X86_REG_ESP,
    UC_X86_REG_FPCW,
)

BUILDS = (
    ("rus/game/konung.res", 0x428FEC, 0x42952A, 0x45AE0E, 0x45ACE0),
    ("eng/game/KONUNG.EXE", 0x429164, 0x4296A2, 0x45AE46, 0x45AD18),
    ("eng/game/TUTORIAL.EXE", 0x429164, 0x4296A2, 0x45AE52, 0x45AD24),
)
FRAME = 0x1008000


def write_integer(machine, address, value):
    machine.mem_write(address, struct.pack("<I", value))


def load_machine(path):
    executable = pefile.PE(str(path))
    machine = Uc(UC_ARCH_X86, UC_MODE_32)
    machine.mem_map(0x400000, 0x500000)
    machine.mem_write(executable.OPTIONAL_HEADER.ImageBase, executable.get_memory_mapped_image())
    machine.mem_map(0x1000000, 0x10000)
    return machine


def evaluate(machine, start, end, tick, palette):
    machine.reg_write(UC_X86_REG_EBP, FRAME)
    machine.reg_write(UC_X86_REG_ESP, FRAME - 0x1000)
    machine.reg_write(UC_X86_REG_FPCW, 0x37F)
    write_integer(machine, 0x81D640, tick)
    write_integer(machine, 0x81D608, palette)
    machine.emu_start(start, end, count=2000)
    if machine.reg_read(UC_X86_REG_EIP) != end:
        raise RuntimeError("Original routine did not reach its return")
    b, g, r, _ = struct.unpack("<bbbB", machine.mem_read(FRAME - 8, 4))
    ground = struct.unpack("<I", machine.mem_read(0x82D168, 4))[0]
    return r, g, b, ground


def check_rus_fade_tables(machine):
    machine.reg_write(UC_X86_REG_EBP, FRAME)
    machine.reg_write(UC_X86_REG_ESP, FRAME - 0x1000)
    machine.reg_write(UC_X86_REG_FPCW, 0x37F)
    machine.emu_start(0x439B08, 0x439DDE, count=2000000)
    assert machine.reg_read(UC_X86_REG_EIP) == 0x439DDE
    factor = 0.0
    step = struct.unpack("<f", struct.pack("<f", 0.01))[0]
    checked = 0
    for level in range(100):
        for negative, address, bits, shift in [
            (True, 0x47620C, 5, 0),
            (False, 0x477B0C, 5, 0),
            (True, 0x479D0C, 6, 5),
            (False, 0x47CF0C, 6, 5),
            (True, 0x4A02D0, 5, 11),
            (False, 0x4A1BD0, 5, 11),
        ]:
            maximum = (1 << bits) - 1
            for component in range(maximum + 1):
                actual = (
                    struct.unpack(
                        "<H", machine.mem_read(address + 2 * (level * (maximum + 1) + component), 2)
                    )[0]
                    >> shift
                )
                expected = (
                    int(component * (1 - factor))
                    if negative
                    else component + int((maximum - component) * factor)
                )
                assert actual == expected, (level, component, bits, negative, actual, expected)
                checked += 1
        factor = struct.unpack("<f", struct.pack("<f", factor + step))[0]
    print(f"  original 565 table builder: {checked} entries passed")


def main():
    output = subprocess.check_output(
        [
            "cargo",
            "run",
            "--quiet",
            "--manifest-path",
            "crates/Cargo.toml",
            "-p",
            "k1-game",
            "--example",
            "lighting_trace",
        ],
        text=True,
    )
    model = [tuple(map(int, line.split(","))) for line in output.splitlines()]
    assert len(model) == 43200
    map_output = subprocess.check_output(
        [
            "cargo",
            "run",
            "--quiet",
            "--manifest-path",
            "crates/Cargo.toml",
            "-p",
            "k1-game",
            "--example",
            "lighting_trace",
            "--",
            "maps",
        ],
        text=True,
    )
    map_model = [tuple(map(int, line.split(","))) for line in map_output.splitlines()]
    failed = False
    for relative, start, end, thresholds, colors in BUILDS:
        path = Path("original/k1") / relative
        if not path.exists():
            raise FileNotFoundError(path)
        machine = load_machine(path)
        # Resolve the actual rand call from this build, instead of trusting drifted addresses.
        call = start + 0x28
        assert machine.mem_read(call, 1) == b"\xe8"
        random_address = call + 5 + struct.unpack("<i", machine.mem_read(call + 1, 4))[0]

        def random_return(emulator, _address, _size, _data):
            palette_flag = struct.unpack("<I", emulator.mem_read(0x81D608, 4))[0]
            emulator.reg_write(UC_X86_REG_EAX, 0 if palette_flag else 99)
            stack = emulator.reg_read(UC_X86_REG_ESP)
            return_address = struct.unpack("<I", emulator.mem_read(stack, 4))[0]
            emulator.reg_write(UC_X86_REG_ESP, stack + 4)
            emulator.reg_write(UC_X86_REG_EIP, return_address)

        machine.hook_add(UC_HOOK_CODE, random_return, begin=random_address, end=random_address)
        if not relative.startswith("rus"):
            # These builds have a protected rounding helper on disk. Exercise the
            # intact routine with the RUS-confirmed chop semantics at its FISTP.
            rounding_call = start + 0x1D3
            assert machine.mem_read(rounding_call, 1) == b"\xe8"
            rounding_address = (
                rounding_call + 5 + struct.unpack("<i", machine.mem_read(rounding_call + 1, 4))[0]
            )

            def rounding_return(emulator, _address, _size, _data):
                emulator.reg_write(UC_X86_REG_FPCW, 0xF7F)
                stack = emulator.reg_read(UC_X86_REG_ESP)
                return_address = struct.unpack("<I", emulator.mem_read(stack, 4))[0]
                emulator.reg_write(UC_X86_REG_ESP, stack + 4)
                emulator.reg_write(UC_X86_REG_EIP, return_address)

            machine.hook_add(
                UC_HOOK_CODE, rounding_return, begin=rounding_address, end=rounding_address
            )
        assert struct.unpack("<13h", machine.mem_read(thresholds, 26)) == (
            0,
            1800,
            1830,
            3570,
            3600,
            5400,
            8100,
            11700,
            11880,
            11910,
            16155,
            16200,
            18900,
        )
        if relative.startswith("rus"):
            check_rus_fade_tables(machine)
        write_integer(machine, 0x81D5EC, 1)
        mismatches = []
        trace = bytearray()
        for palette in range(2):
            for tick in range(21600):
                actual = evaluate(machine, start, end, tick, palette)
                trace.extend(struct.pack("<bbbB", *actual))
                expected = model[palette * 21600 + tick]
                if actual != expected:
                    mismatches.append((palette, tick, actual, expected))
        print(
            f"{relative}: {len(mismatches)} mismatches / 43200; "
            f"sha256={hashlib.sha256(trace).hexdigest()}"
        )
        for mismatch in mismatches[:8]:
            print("  palette, tick, original, model:", mismatch)
        failed |= bool(mismatches)
        # Constant-light bypass precedes the option and leaves tile light off.
        write_integer(machine, 0x81D5EC, 0)
        for tick in (0, 8100, 12000, 18900, 21600):
            assert evaluate(machine, start, end, tick, 0) == (0, 0, 0, 0)
        write_integer(machine, 0x81D5DC, 1)
        for map_id in range(56):
            write_integer(machine, 0x81D6C4, map_id)
            b, g, r, _ = struct.unpack("<bbbB", machine.mem_read(colors + map_id * 4, 4))
            assert evaluate(machine, start, end, 12000, 0) == (r, g, b, 0)
        executable = pefile.PE(str(path)).get_memory_mapped_image()
        signature = b"\xa3" + struct.pack("<I", 0x81D5DC)
        stores = [i for i in range(len(executable)) if executable.startswith(signature, i)]
        assert len(stores) == 1, stores
        store = 0x400000 + stores[0]
        assert machine.mem_read(store - 3, 3) == b"\xc1\xf8\x18"
        assert machine.mem_read(store - 9, 2) == b"\x8b\x80"
        flag_table = struct.unpack("<I", machine.mem_read(store - 7, 4))[0] + 3
        for map_id in range(56):
            write_integer(machine, 0x81D6C4, map_id)
            machine.emu_start(store - 14, store + 5, count=4)
            flag = struct.unpack("<I", machine.mem_read(0x81D5DC, 4))[0]
            actual = evaluate(machine, start, end, 12000, 0)[:3] + (flag,)
            assert actual == map_model[map_id], (relative, map_id, actual, map_model[map_id])
        hashes = []
        for half in (trace[:86400], trace[86400:]):
            value = 0xCBF29CE484222325
            for byte in half:
                value = ((value ^ byte) * 0x100000001B3) & ((1 << 64) - 1)
            hashes.append(hex(value))
        print(f"  full-cycle FNV-1a per palette: {hashes}")
        print(f"  flag writer {store:#x}, table {flag_table:#x}; all 56 map rules passed")
    raise SystemExit(int(failed))


if __name__ == "__main__":
    main()
