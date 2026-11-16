# /// script
# requires-python = ">=3.14"
# dependencies = ["pefile==2024.8.26", "unicorn==2.1.4"]
# ///
"""Follow inventory input with one original group tick using synthetic runtime data.

Run: uv run tools/audit_k1_inventory_tick.py
Exit 1 means the original input/tick chain is incomplete, with each blocked case
reported separately. No protected game helper is replaced to obtain a passing run.
"""

from dataclasses import dataclass

from audit_k1_inventory import ACTOR, BUILDS, CURSOR, ITEMS, ROOT
from audit_k1_inventory_input import InputProbe
from unicorn import UcError
from unicorn.x86_const import UC_X86_REG_EIP

CHARACTERS = 0x78A250
GROUP = 0x1002000
IMAGE = 0x1003000
HUMAN_MODEL = 0x6F348C
GROUP_TICK = 0x4123BC
PROTECTED_STEP = 0x43DB6A


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


class TickProbe(InputProbe):
    def __init__(self, build):
        self.random_calls = 0
        super().__init__(build)

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
        self.reset(state=state)
        self.random_calls = 0
        self.machine.mem_write(CHARACTERS, bytes(512))
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
        self.write(ACTOR + 0x6A, 1, "H")
        self.write(ACTOR + 0x12, 2, "B")
        self.write(ACTOR + 3, 4, "B")
        self.write(ACTOR + 2, 1 if state == 5 else 8 if state == 1 else 0, "B")
        self.write(ACTOR + 1, 7 if state == 5 else 0, "B")
        self.write(ACTOR + 0x10, frame, "B")
        self.write(ACTOR + 0xD7, 12, "B")
        self.write(ACTOR + 0x13, 0, "B")
        self.write(ACTOR + 0x14, -1, "b")
        self.write(ACTOR + 6, 2, "h")
        self.write(ACTOR + 8, 100, "h")
        self.write(ACTOR + 10, 100, "h")
        self.write(ACTOR + 12, 101, "h")
        self.write(ACTOR + 14, 100, "h")
        self.write(ACTOR + 0x5C, 1600, "h")
        self.machine.mem_write(CHARACTERS + 256, bytes(self.machine.mem_read(ACTOR, 256)))
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

    def change(self, action):
        before = self.phase(ACTOR)
        if action == "remove":
            self.click(9)
            assert self.call(self.build.land, ACTOR) == 1
            assert self.read(ACTOR + 0x6A, "H") == 0
            assert self.inventory()[:2] == [1, 2]
        elif action == "replace":
            self.take(2)
            self.click(9)
            assert self.read(ACTOR + 0x6A, "H") == 2
            assert self.inventory()[0] == 1
        assert self.phase(ACTOR) == before
        self.machine.mem_write(CHARACTERS, bytes(self.machine.mem_read(ACTOR, 256)))


def main():
    for build in BUILDS:
        if not (ROOT / "original/k1" / build.path).is_file():
            raise FileNotFoundError(f"Both owned editions are required: {build.path}")
    blocked = 0
    for build in BUILDS:
        edition = build.path.split("/")[0]
        probe = TickProbe(build)
        completed = 0
        edition_blocked = 0
        for state in (0, 1, 5):
            for frame in (0, 5, 11):
                results = []
                control_phase = None
                for action in ("control", "remove", "replace"):
                    probe.prepare(state, frame)
                    probe.change(action)
                    try:
                        probe.call(GROUP_TICK, GROUP)
                    except UcError:
                        instruction = probe.machine.reg_read(UC_X86_REG_EIP)
                        if edition != "eng" or instruction != PROTECTED_STEP:
                            raise
                        edition_blocked += 1
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
                        results.append(f"{action}={phase}")
                print(edition, "input", (state, frame), *results, sep=" | ")
        blocked += edition_blocked
        print(edition, "completed ticks:", completed, "blocked ticks:", edition_blocked)
    print("Synthetic inputs: hit target, one group, adjacent enemy, 12-frame model, 16x24 image.")
    print("CRT memcpy/memset modeled, rand supplied as zero; sound/UI presentation skipped.")
    print("Original game recalculation, mass, input dispatch and attempted group tick execute.")
    print("No renderer, protected coordinate-helper emulation or combat-formula claim.")
    if blocked:
        print("INCOMPLETE: ENG protected game coordinate helper prevents the listed next ticks.")
    return int(bool(blocked))


if __name__ == "__main__":
    raise SystemExit(main())
