# /// script
# requires-python = ">=3.14"
# dependencies = ["pefile", "unicorn"]
# ///
"""Execute the original upgrade check on synthetic records, without running the game."""

import struct
from pathlib import Path

import pefile
from unicorn import UC_ARCH_X86, UC_MODE_32, Uc
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EIP, UC_X86_REG_ESP

root = Path(__file__).resolve().parents[1]
english_executable = pefile.PE(str(root / "original/k1/eng/game/KONUNG.EXE"))
signature = english_executable.get_data(0x10B08, 25)
rows = [
    (13, 9, 2),
    (19, 14, 5),
    (26, 20, 11),
    (34, 28, 14),
    (43, 36, 18),
    (53, 44, 23),
    (63, 52, 29),
    (74, 60, 35),
    (80, 65, 72),
    (90, 70, 100),
    (95, 75, 150),
    (100, 80, 200),
    (999999, 999999, 999),
]
for edition, name in [("rus", "konung.res"), ("eng", "KONUNG.EXE")]:
    path = root / "original/k1" / edition / "game" / name
    executable = pefile.PE(str(path))
    data = path.read_bytes()
    offset = data.index(signature)
    entry = executable.OPTIONAL_HEADER.ImageBase + executable.get_rva_from_offset(offset)
    image = executable.get_memory_mapped_image()
    machine = Uc(UC_ARCH_X86, UC_MODE_32)
    machine.mem_map(executable.OPTIONAL_HEADER.ImageBase, (len(image) + 4095) & ~4095)
    machine.mem_write(executable.OPTIONAL_HEADER.ImageBase, image)
    machine.mem_map(0x20000000, 0x20000)
    machine.mem_map(0x30000000, 0x20000)
    count = 0
    for class_id, primary in [(5, 2), (3, 0), (2, 3), (4, 1)]:
        for attribute, field in enumerate([0x56, 0x5A, 0x60, 0x64]):
            for value in range(101):
                for available in [0, 2, 5, 11, 1000]:
                    record = bytearray(256)
                    record[4] = class_id
                    record[0x33] = 10
                    struct.pack_into("<h", record, field, value)
                    struct.pack_into("<i", record, 0x38, available)
                    machine.mem_write(0x20000000, bytes(record))
                    machine.mem_write(
                        0x30010000,
                        struct.pack("<III", 0x30001000, 0x20000000, [2, 3, 5, 6][attribute]),
                    )
                    machine.reg_write(UC_X86_REG_ESP, 0x30010000)
                    machine.emu_start(entry, 0x30001000, count=1000)
                    assert machine.reg_read(UC_X86_REG_EIP) == 0x30001000
                    actual = machine.reg_read(UC_X86_REG_EAX)
                    cost = next(
                        row[2] for row in rows if value < row[0 if attribute == primary else 1]
                    )
                    expected = (
                        cost
                        if value < (95 if attribute == primary else 37) and available >= cost
                        else 0
                    )
                    assert actual == expected, (
                        edition,
                        class_id,
                        attribute,
                        value,
                        available,
                        actual,
                        expected,
                    )
                    count += 1
    print(edition, hex(entry), count, "original calls agree")
