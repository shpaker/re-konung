# /// script
# requires-python = ">=3.14"
# dependencies = ["pefile==2024.8.26", "unicorn==2.1.4"]
# ///
"""Execute original inventory refresh and character-panel paths on synthetic data.

Run: UV_PYTHON=3.14 uv run tools/audit_k1_inventory_ui.py
Only draw sinks, sound and explicitly listed CRT byte/text helpers are modeled.
Original requirements, recalculation, item masses and transfer procedures execute.
"""

import argparse
import json
import struct
from dataclasses import asdict

from audit_k1_inventory import BUILDS, CURSOR, RETURN, ROOT, SELECTED, STACK, Probe
from k1_inventory_evidence import RecalculationTrace, pair_recalculations
from unicorn import UcError
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EIP, UC_X86_REG_ESP

HERO = 0x78A250
IMAGE = 0x1003000
PALETTE = 0x1004000
SCREEN = 0x81D610


class UIProbe(Probe):
    """Canonical-character fixture reusable by the next-tick integration audit."""

    def __init__(self, build):
        self.trace = []
        self.pending_recalculations = []
        self.draw_calls = 0
        self.recalculation_calls = 0
        self.trace_output = False
        self.hit = 9
        if build.path.startswith("rus/"):
            self.dispatcher, self.hit_test = 0x42260C, 0x4381C8
            self.draw_sinks = (0x430430, 0x42770C, 0x427780, 0x427698)
            self.font_width = 0x43E46F
            self.tint, self.palette = 0x43E979, 0x43E3B1
            self.text_copy, self.text_append, self.integer_text, self.format_text = (
                0x43EB6E,
                0x43EC59,
                0x43EC24,
                0x43F549,
            )
        else:
            self.dispatcher, self.hit_test = 0x4226D8, 0x438410
            self.draw_sinks = (0x430658, 0x42783C, 0x4278B0, 0x4277C8)
            self.font_width = 0x43E6C1
            self.tint, self.palette = 0x43EBCB, 0x43E603
            self.text_copy, self.text_append, self.integer_text, self.format_text = (
                0x43EDBE,
                0x43EEA9,
                0x43EE74,
                0x43F8ED,
            )
        super().__init__(build, intact=True, actor=HERO)

    def reset(self, *, state=0, class_id=5):
        super().reset(state=state, class_id=class_id)
        self.trace.clear()
        self.pending_recalculations.clear()
        self.draw_calls = 0
        for address, value in (
            (0x81D724, 32),
            (0x81D744, 63),
            (0x81D738, 0),
            (0x81D728, 0),
            (0x81D708, 0),
            (0x81D590, IMAGE),
            (0x5076F4 + self.build.table_delta, PALETTE),
            (0x81D670, 0),
            (0x81D600, 0),
            (0x81D630, 0),
            (0x81D610, 0),
            (0x81D67C, 0),
            (0x81D734, 0),
            (0x81D5C0, 0),
            (CURSOR, 4),
        ):
            self.write(address, value)
        self.machine.mem_write(0x61F3D8, bytes(512 * 8))
        self.machine.mem_write(IMAGE, struct.pack("<HH", 16, 16))
        self.write(SELECTED, self.actor)

    def record(self, phase, return_address, actor):
        self.trace.append(
            RecalculationTrace(
                phase,
                return_address,
                actor,
                self.read(actor + 0x5C, "h"),
                self.read(actor + 0x62, "h"),
                tuple(struct.unpack("<6H", self.machine.mem_read(actor + 0x66, 12))),
                tuple(struct.unpack("<32H", self.machine.mem_read(actor + 0x72, 64))),
                self.read(CURSOR),
                self.read(0x81D400),
                self.read(SCREEN),
            )
        )
        if phase == "entry":
            self.recalculation_calls += 1
        if self.trace_output:
            print(json.dumps(asdict(self.trace[-1]), sort_keys=True))

    def boundary(self, machine, address, size, data):
        if self.pending_recalculations and address == self.pending_recalculations[-1][0]:
            return_address, actor = self.pending_recalculations.pop()
            self.record("return", return_address, actor)
        if address == self.build.recalculate:
            stack = machine.reg_read(UC_X86_REG_ESP)
            return_address, actor = struct.unpack("<II", machine.mem_read(stack, 8))
            self.record("entry", return_address, actor)
            self.pending_recalculations.append((return_address, actor))
        if address in self.draw_sinks:
            self.draw_calls += 1
            self.return_value()
        elif address == self.font_width:
            self.return_value(16)
        elif address == self.tint:
            self.return_value()
        elif address == self.palette:
            self.return_value(PALETTE)
        elif address in (self.text_copy, self.text_append):
            stack = machine.reg_read(UC_X86_REG_ESP)
            destination, source = struct.unpack("<II", machine.mem_read(stack + 4, 8))
            offset = len(self.text_at(destination)) if address == self.text_append else 0
            machine.mem_write(destination + offset, self.text_at(source) + b"\0")
            self.return_value(destination)
        elif address == self.integer_text:
            stack = machine.reg_read(UC_X86_REG_ESP)
            value, destination, base = struct.unpack("<iII", machine.mem_read(stack + 4, 12))
            assert base == 10, base
            machine.mem_write(destination, str(value).encode("ascii") + b"\0")
            self.return_value(destination)
        elif address == self.format_text:
            destination = self.read(machine.reg_read(UC_X86_REG_ESP) + 4)
            machine.mem_write(destination, b"0\0")
            self.return_value(1)
        elif address == self.hit_test:
            self.return_value(self.hit)
        elif address not in (self.build.refresh, self.build.screen, self.build.weapon_ui, 0x414A34):
            super().boundary(machine, address, size, data)

    def text_at(self, address):
        result = bytearray()
        for offset in range(4096):
            value = self.read(address + offset, "B")
            if value == 0:
                return bytes(result)
            result.append(value)
        raise ValueError("Synthetic UI string exceeded the fixture bound")

    def call(self, address, *arguments):
        self.machine.mem_write(
            STACK, struct.pack("<" + "I" * (1 + len(arguments)), RETURN, *arguments)
        )
        self.machine.reg_write(UC_X86_REG_ESP, STACK)
        try:
            self.machine.emu_start(address, RETURN, count=2000000)
        except UcError as error:
            raise RuntimeError(
                f"{self.build.path}: original path blocked at "
                f"{self.machine.reg_read(UC_X86_REG_EIP):#x}: {error}"
            ) from error
        assert self.machine.reg_read(UC_X86_REG_EIP) == RETURN, hex(address)
        if self.pending_recalculations and self.pending_recalculations[-1][0] == RETURN:
            return_address, actor = self.pending_recalculations.pop()
            self.record("return", return_address, actor)
        self.calls += 1
        return self.machine.reg_read(UC_X86_REG_EAX)

    def click(self, hit):
        self.hit = hit
        self.write(0x81D5B0, 1, "B")
        return self.call(self.dispatcher)

    def pick_inventory(self, index):
        self.write(0x81D5C0, self.read(0x81D734) + index * self.read(0x81D724))
        return self.click(50)

    def refresh_inventory(self):
        return self.call(self.build.refresh, self.actor, 0)

    def show_character(self):
        return self.call(self.build.screen, 2)


def fixture(probe, occupied=2, *, panel=False):
    probe.reset()
    probe.bonus(0, (4, 1, 1), (6, 1, 3))
    probe.item(1, 5, bonus=0)
    probe.write(probe.actor + 0x70, 1, "H")
    probe.write(probe.actor + 0x62, 23, "h")
    probe.write(probe.actor + 0x5C, 100, "h")
    for reference in range(2, occupied + 2):
        probe.item(reference, 2)
    probe.mass(2, 0)
    probe.inventory(list(range(2, occupied + 2)))
    if panel:
        probe.show_character()
        assert probe.read(SCREEN) == 3
        validate_trace(probe)
        probe.trace.clear()
        probe.write(probe.actor + 0x5C, 100, "h")


def validate_trace(probe):
    effects = pair_recalculations(probe.trace)
    for effect in effects:
        assert effect.references_unchanged
        assert effect.before.cursor == effect.after.cursor
        assert effect.before.screen == effect.after.screen
        active = effect.before.equipment[5] == 1
        expected_health = min(1600, effect.before.health + 16) if active else effect.before.health
        assert effect.after.health == expected_health
        assert effect.after.endurance == (23 if active else 20)
        assert effect.before.actor == probe.actor
    return effects


def assert_final_health(probe):
    expected = 100
    for event in probe.trace:
        if event.phase == "entry" and event.equipment[5] == 1:
            expected = min(1600, expected + 16)
    assert probe.read(probe.actor + 0x5C, "h") == expected


def refresh_matrix(probe):
    cases = 0
    for occupied in (0, 1, 2, 8, 32):
        for visible in (0, 1, 2, 8, 32):
            for scroll in (0, 1, 31):
                for excluded in (0, 2):
                    fixture(probe, occupied)
                    probe.write(0x81D744, visible * 32 - 2, "i")
                    probe.write(0x81D708, scroll)
                    probe.write(0x81D670, excluded)
                    before = probe.inventory()
                    eligible = sum(
                        before[index & 31] not in (0, excluded)
                        for index in range(scroll, scroll + visible)
                    )
                    for _ in range(3):
                        probe.refresh_inventory()
                    assert probe.inventory() == before
                    effects = validate_trace(probe)
                    assert len(effects) == eligible * 3
                    assert probe.read(probe.actor + 0x5C, "h") == min(1600, 100 + eligible * 48)
                    assert probe.read(probe.actor + 0x62, "h") == 23
                    assert probe.read(SCREEN) == 0
                    cases += 1
    return cases


def character_panel(probe):
    cases = 0
    for occupied in (0, 1, 2, 8):
        for weapon in (0, 1, 2):
            for armed in (0, 4):
                fixture(probe, occupied)
                probe.write(probe.actor + 0x12, weapon, "B")
                probe.write(probe.actor + 3, armed, "B")
                before = probe.inventory()
                probe.show_character()
                effects = validate_trace(probe)
                assert len(effects) == 3, (
                    probe.build.path,
                    occupied,
                    weapon,
                    armed,
                    [
                        (hex(effect.before.return_address), effect.health_delta)
                        for effect in effects
                    ],
                )
                expected_callers = (
                    (0x42D08D, 0x41AB91, 0x41AEFB)
                    if probe.build.path.startswith("rus/")
                    else (0x42D26D, 0x41ABF1, 0x41AF5B)
                )
                assert tuple(effect.before.return_address for effect in effects) == expected_callers
                assert probe.read(SCREEN) == 3
                assert probe.read(probe.actor + 0x5C, "h") == 100 + len(effects) * 16
                assert probe.inventory() == before
                assert probe.read(probe.actor + 0x12, "B") == weapon
                assert probe.read(probe.actor + 3, "B") == armed
                cases += 1
    return cases


def transfer_matrix(probe):
    cases = 0
    for panel in (False, True):
        for operation in ("take", "equip", "unequip", "replace", "requirements", "full", "cancel"):
            fixture(probe, 32 if operation == "full" else 2, panel=panel)
            if operation == "replace":
                probe.item(3, 5)
                probe.pick_inventory(1)
                probe.click(10)
                assert probe.read(probe.actor + 0x70, "H") == 3
                assert probe.inventory()[:3] == [1, 2, 0]
            elif operation == "unequip":
                probe.click(10)
                assert probe.read(probe.actor + 0x70, "H") == 0
                probe.pick_inventory(0)
                assert probe.read(probe.actor + 0x70, "H") == 0
                assert probe.inventory()[:4] == [1, 2, 3, 0]
            elif operation == "full":
                before = probe.inventory()
                probe.click(10)
                assert probe.read(probe.actor + 0x70, "H") == 0
                probe.pick_inventory(0)
                assert probe.inventory() == before
                assert probe.read(probe.actor + 0x70, "H") == 1
            else:
                if operation == "requirements":
                    probe.item(2, 2, info=1)
                    probe.mass(2, 0, info=1)
                    probe.write(0x459B80 + probe.build.table_delta + 64, 5, "h")
                    probe.write(0x459B82 + probe.build.table_delta + 64, 21, "h")
                probe.pick_inventory(0)
                assert probe.inventory()[:2] == [3, 0]
                assert probe.read(0x81D400) == 2
                if operation in ("equip", "requirements"):
                    probe.click(9)
                    assert probe.read(probe.actor + 0x6A, "H") == (2 if operation == "equip" else 0)
                    if operation == "equip":
                        assert probe.inventory()[:2] == [3, 0]
                elif operation == "cancel":
                    probe.call(probe.build.restore)
                if operation in ("cancel", "requirements"):
                    expected = [3, 2] if probe.build.path.startswith("rus/") else [2, 3]
                    assert probe.inventory()[:3] == [*expected, 0]
            effects = validate_trace(probe)
            assert effects
            assert_final_health(probe)
            assert probe.read(probe.actor + 0x62, "h") == (
                20 if operation in ("unequip", "replace") else 23
            )
            assert probe.read(CURSOR) == (10 if operation == "take" else 4), (
                probe.build.path,
                panel,
                operation,
                probe.read(CURSOR),
            )
            assert probe.read(SCREEN) == (3 if panel else 0)
            assert not probe.pending_recalculations
            cases += 1
    return cases


def mass_timing(probe):
    cases = 0
    for panel in (False, True):
        for visible in (0, 1, 2):
            for excluded in (0, 2):
                fixture(probe, 1, panel=panel)
                probe.item(2, 2, info=1)
                probe.mass(2, 16667, info=1)
                probe.write(0x81D744, visible * 32 - 2, "i")
                probe.write(0x81D670, excluded)
                probe.click(10)
                refreshed = panel or (visible > 0 and excluded != 2)
                assert probe.read(probe.actor + 0x62, "h") == (20 if refreshed else 23)
                probe.pick_inventory(0)
                assert probe.read(probe.actor + 0x70, "H") == int(refreshed)
                assert probe.inventory()[:3] == ([2, 0, 0] if refreshed else [1, 2, 0])
                final_endurance = 23 if refreshed or visible == 0 else 20
                assert probe.read(probe.actor + 0x62, "h") == final_endurance
                validate_trace(probe)
                assert_final_health(probe)
                assert probe.read(CURSOR) == 4
                cases += 1
    return cases


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--trace", action="store_true", help="Print synthetic entry/return JSON before the summary"
    )
    arguments = parser.parse_args()
    for build in BUILDS:
        if not (ROOT / "original/k1" / build.path).is_file():
            raise FileNotFoundError(f"Both owned editions are required: {build.path}")
        probe = UIProbe(build)
        probe.trace_output = arguments.trace
        results = {
            "refresh": refresh_matrix(probe),
            "character_panel": character_panel(probe),
            "transfers": transfer_matrix(probe),
            "mass_timing": mass_timing(probe),
        }
        print(
            build.path,
            results,
            "entry calls:",
            probe.calls,
            "recalculations:",
            probe.recalculation_calls,
        )
    print("Original inventory eligibility, full character panel and transfers execute intact.")
    print(
        "Boundaries: supplied hit target; draw/text width/palette sinks; sound and hints skipped;"
    )
    print("CRT memcpy/memset/strcpy/strcat/itoa modeled; display sprintf supplies inert text.")
    print("No palette colors, font layout, next tick or full window scheduling claim.")


if __name__ == "__main__":
    main()
