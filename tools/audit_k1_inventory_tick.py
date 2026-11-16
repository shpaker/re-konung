# /// script
# requires-python = ">=3.14"
# dependencies = ["pefile==2024.8.26", "unicorn==2.1.4"]
# ///
"""Follow inventory input with one original group tick using synthetic runtime data.

Run: uv run tools/audit_k1_inventory_tick.py
Exit 1 means the original input/tick chain is incomplete, with each blocked case
reported separately. No unavailable game helper is replaced to obtain a passing run.
"""

from dataclasses import dataclass

import pefile
from audit_k1_inventory import BUILDS, CURSOR, ITEMS, ROOT, Probe
from audit_k1_inventory_input import InputProbe
from audit_k1_inventory_ui import UIProbe
from k1_inventory_evidence import pair_recalculations
from unicorn import UcError
from unicorn.x86_const import UC_X86_REG_EIP

CHARACTERS = 0x78A250
GROUP = 0x1002000
IMAGE = 0x1003000
HUMAN_MODEL = 0x6F348C
GROUP_TICK = 0x4123BC
UNAVAILABLE_STEP = 0x43DB6A


@dataclass(frozen=True)
class Phase:
    state: int
    direction: int
    orders: int
    armed: int
    frame: int
    active_slot: int
    tile: tuple[int, int]
    goal: tuple[int, int]


class TickFixture:
    def __init__(self, build):
        self.random_calls = 0
        executable = pefile.PE(str(ROOT / "original/k1" / build.path))
        self.step_address = 0x43D922 if build.path.startswith("rus/") else UNAVAILABLE_STEP
        self.step_rva = self.step_address - executable.OPTIONAL_HEADER.ImageBase
        self.step_offset = executable.get_offset_from_rva(self.step_rva)
        self.step_section = executable.get_section_by_rva(self.step_rva).Name.rstrip(b"\0").decode()
        self.entry_address = (
            executable.OPTIONAL_HEADER.ImageBase + executable.OPTIONAL_HEADER.AddressOfEntryPoint
        )
        raw_data = (ROOT / "original/k1" / build.path).read_bytes()
        self.step_bytes = raw_data[self.step_offset : self.step_offset + 64]
        mapped = executable.get_memory_mapped_image()
        assert mapped[self.step_rva : self.step_rva + 64] == self.step_bytes
        super().__init__(build)
        self.check_step_bytes()

    def check_step_bytes(self):
        assert bytes(self.machine.mem_read(self.step_address, 64)) == self.step_bytes

    def boundary(self, machine, address, size, data):
        if address in (0x43EA46, 0x43EC98):
            self.random_calls += 1
            self.return_value(0)
        elif address == 0x414A34:
            # Unlike the isolated transfer probe, execute the original idle transition.
            self.idle_requests += 1
        else:
            super().boundary(machine, address, size, data)

    def phase(self, actor):
        return Phase(
            *(self.read(actor + offset, "B") for offset in (0, 1, 2)),
            self.read(actor + 3, "B") & 4,
            self.read(actor + 0x10, "B"),
            self.read(actor + 0x12, "B"),
            (self.read(actor + 8, "h"), self.read(actor + 10, "h")),
            (self.read(actor + 12, "h"), self.read(actor + 14, "h")),
        )

    def prepare(self, state, frame):
        self.machine.mem_write(CHARACTERS, bytes(512))
        self.reset(state=state)
        self.random_calls = 0
        self.machine.mem_write(GROUP, bytes(32))
        self.machine.mem_write(HUMAN_MODEL, bytes(0x4D28))
        self.write(HUMAN_MODEL + 4, IMAGE)
        self.write(IMAGE, 16, "H")
        self.write(IMAGE + 2, 24, "H")
        for animation in range(10):
            for direction in range(8):
                self.write(HUMAN_MODEL + animation * 0xD0 + direction * 0x1A + 8, 12, "h")
        self.item(1, 2)
        self.item(2, 2)
        for item in (1, 2):
            self.write(ITEMS + item * 16 + 4, 100.0, "f")
            self.write(ITEMS + item * 16 + 8, 100.0, "f")
        self.mass(2, 1)
        self.inventory([2])
        self.write(self.actor + 0x6A, 1, "H")
        self.write(self.actor + 0x12, 2, "B")
        self.write(self.actor + 3, 4, "B")
        self.write(self.actor + 2, 1 if state == 5 else 8 if state == 1 else 0, "B")
        self.write(self.actor + 1, 7 if state == 5 else 0, "B")
        self.write(self.actor + 0x10, frame, "B")
        self.write(self.actor + 0xD7, 12, "B")
        self.write(self.actor + 0x13, 0, "B")
        self.write(self.actor + 0x14, -1, "b")
        self.write(self.actor + 6, 2, "h")
        self.write(self.actor + 8, 100, "h")
        self.write(self.actor + 10, 100, "h")
        self.write(self.actor + 12, 101, "h")
        self.write(self.actor + 14, 100, "h")
        self.write(self.actor + 0x5C, 1600, "h")
        self.machine.mem_write(CHARACTERS + 256, bytes(self.machine.mem_read(self.actor, 256)))
        self.machine.mem_write(CHARACTERS + 256 + 0x66, bytes(0x66))
        self.write(CHARACTERS + 256, 0, "B")
        self.write(CHARACTERS + 256 + 2, 0, "B")
        self.write(CHARACTERS + 256 + 5, 1, "B")
        self.write(CHARACTERS + 256 + 8, 101, "h")
        foot_map = 0x4EE6B0 + (0x38 if self.build.path.startswith("eng/") else 0)
        self.machine.mem_write(foot_map, bytes(320 * 320))
        if state == 5:
            self.write(foot_map + 101 * 320 + 100 * 2, 2, "H")
        self.write(GROUP + 0x1C, 1, "B")
        self.write(0x81D56C, CHARACTERS)
        self.write(CURSOR, 4)
        self.check_step_bytes()

    def change(self, action):
        before = self.phase(self.actor)
        if action == "remove":
            self.click(9)
            assert self.call(self.build.land, self.actor) == 1
            assert self.read(self.actor + 0x6A, "H") == 0
            assert self.inventory()[:2] == [1, 2]
        elif action == "replace":
            self.take(2)
            self.click(9)
            assert self.read(self.actor + 0x6A, "H") == 2
            assert self.inventory()[0] == 1
        assert self.phase(self.actor) == before
        self.machine.mem_write(CHARACTERS, bytes(self.machine.mem_read(self.actor, 256)))

    def tick(self):
        # Keep raw Unicorn failures available to this diagnostic's explicit classifier.
        return Probe.call(self, GROUP_TICK, GROUP)


class TickProbe(TickFixture, InputProbe):
    """Retain the original transfer-only UI boundaries for comparison."""


class UITickProbe(TickFixture, UIProbe):
    """Execute inventory refresh and character-screen logic before the group tick."""

    def change(self, action):
        before = self.phase(self.actor)
        self.write(0x81D5C0, self.read(0x81D734))
        if action == "remove":
            self.click(9)
            self.click(50)
            assert self.read(self.actor + 0x6A, "H") == 0
            assert self.inventory()[:2] == [1, 2]
        elif action == "replace":
            self.click(50)
            self.click(9)
            assert self.read(self.actor + 0x6A, "H") == 2
            assert self.inventory()[0] == 1
        assert self.phase(self.actor) == before


def matrix(probe, edition, mode):
    completed = 0
    blocked = 0
    if mode != "isolated":
        assert probe.actor == CHARACTERS
    for state in (0, 1, 5):
        for frame in (0, 5, 11):
            results = []
            control_phase = None
            for action in ("control", "remove", "replace"):
                probe.prepare(state, frame)
                if mode == "inventory":
                    probe.refresh_inventory()
                elif mode == "character":
                    probe.show_character()
                if mode != "isolated":
                    assert probe.read(0x81D610) == (3 if mode == "character" else 0)
                probe.change(action)
                probe.check_step_bytes()
                if isinstance(probe, UIProbe):
                    pair_recalculations(probe.trace)
                try:
                    probe.tick()
                except UcError:
                    instruction = probe.machine.reg_read(UC_X86_REG_EIP)
                    if edition != "eng" or instruction != UNAVAILABLE_STEP:
                        raise
                    probe.check_step_bytes()
                    blocked += 1
                    results.append(f"{action}=BLOCKED@{instruction:#x}")
                else:
                    completed += 1
                    phase = probe.phase(CHARACTERS)
                    assert phase.active_slot == 2
                    assert phase.armed == 4
                    if action == "control":
                        control_phase = phase
                    else:
                        assert phase == control_phase
                    expected_equipment = {"control": 1, "remove": 0, "replace": 2}
                    assert probe.read(CHARACTERS + 0x6A, "H") == expected_equipment[action]
                    if isinstance(probe, UIProbe):
                        pair_recalculations(probe.trace)
                    results.append(f"{action}={phase}")
            print(edition, mode, "input", (state, frame), *results, sep=" | ")
    print(edition, mode, "completed ticks:", completed, "blocked ticks:", blocked)
    return blocked


def main():
    for build in BUILDS:
        if not (ROOT / "original/k1" / build.path).is_file():
            raise FileNotFoundError(f"Both owned editions are required: {build.path}")
    blocked = 0
    for build in BUILDS:
        edition = build.path.split("/")[0]
        for mode, probe_type in (
            ("isolated", TickProbe),
            ("inventory", UITickProbe),
            ("character", UITickProbe),
        ):
            probe = probe_type(build)
            print(
                edition,
                f"step helper VA={probe.step_address:#x} RVA={probe.step_rva:#x}",
                f"raw offset={probe.step_offset:#x} section={probe.step_section}",
                f"PE entry VA={probe.entry_address:#x} raw/mapped equal=True",
            )
            blocked += matrix(probe, edition, mode)
    print("Synthetic inputs: hit target, one group, adjacent enemy, 12-frame model, 16x24 image.")
    print("CRT memory/text helpers modeled, rand supplied as zero; sound/draw sinks skipped.")
    print("Original game recalculation, mass, input dispatch and attempted group tick execute.")
    print("No renderer, coordinate-helper emulation or combat-formula claim.")
    if blocked:
        print("INCOMPLETE: ENG raw-PE game coordinate helper fails at its first instruction.")
        print("Runtime initialization/protection is a hypothesis; fixture preserves helper bytes.")
    return int(bool(blocked))


if __name__ == "__main__":
    raise SystemExit(main())
