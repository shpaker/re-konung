# /// script
# requires-python = ">=3.14"
# dependencies = ["pefile==2024.8.26", "unicorn==2.1.4"]
# ///
"""Check original dispatch and transfer boundaries with a supplied hit-test result.

Run: uv run tools/audit_k1_inventory_input.py
Game recalculation and weight routines execute; CRT byte operations are modeled.
No next-tick, full input eligibility or renderer equivalence is claimed.
"""

from audit_k1_inventory import ACTOR, BUILDS, CURSOR, ITEMS, ROOT, Probe

FOREIGN_ACTOR = ACTOR + 0x200


class InputProbe(Probe):
    def __init__(self, build, *, actor=ACTOR):
        self.dispatcher, self.hit_test = (
            (0x42260C, 0x4381C8) if build.path.startswith("rus/") else (0x4226D8, 0x438410)
        )
        self.hit = 9
        super().__init__(build, intact=True, actor=actor)

    def boundary(self, machine, address, size, data):
        if address == self.hit_test:
            self.return_value(self.hit)
        else:
            super().boundary(machine, address, size, data)

    def click(self, hit):
        self.hit = hit
        self.write(0x81D5B0, 1, "B")
        self.call(self.dispatcher)


def dispatched_removal(probe):
    """Supply the hit target, but execute the original left-click dispatcher."""
    cases = 0
    for slot, hit in ((0, 9), (1, 12), (2, 9), (3, 11), (4, 8), (5, 10)):
        for state in range(10):
            for frame in (0, 5, 11):
                probe.reset(state=state)
                probe.item(1, slot)
                probe.mass(slot, 1)
                probe.write(ACTOR + 0x66 + slot * 2, 1, "H")
                probe.write(ACTOR + 0x12, slot if slot < 3 else 2, "B")
                probe.write(ACTOR + 3, 4, "B")
                probe.write(ACTOR + 0x10, frame, "B")
                probe.write(CURSOR, 4)
                before = bytes(probe.machine.mem_read(ACTOR, 0x54))
                probe.click(hit)
                assert probe.read(ACTOR + 0x66 + slot * 2, "H") == 0
                assert bytes(probe.machine.mem_read(ACTOR, 0x54)) == before
                assert probe.call(probe.build.land, ACTOR) == 1
                assert probe.inventory()[0] == 1
                cases += 1
    return cases


def dispatched_replacement(probe):
    cases = 0
    for slot, hit in ((0, 9), (1, 12), (2, 9), (3, 11), (4, 8), (5, 10)):
        for state in range(10):
            for frame in (0, 5, 11):
                for full in (False, True):
                    probe.reset(state=state)
                    probe.item(1, slot)
                    probe.item(2, slot)
                    probe.mass(slot, 1)
                    bag = [1, *range(3, 34)] if full else [1]
                    probe.inventory(bag)
                    probe.write(ACTOR + 0x66 + slot * 2, 2, "H")
                    probe.write(ACTOR + 0x12, slot if slot < 3 else 2, "B")
                    probe.write(ACTOR + 3, 4, "B")
                    probe.write(ACTOR + 0x10, frame, "B")
                    phase = bytes(probe.machine.mem_read(ACTOR, 0x54))
                    probe.take(1)
                    probe.click(hit)
                    assert probe.read(ACTOR + 0x66 + slot * 2, "H") == 1
                    assert probe.inventory() == [2, *bag[1:], *([0] * (32 - len(bag)))]
                    assert bytes(probe.machine.mem_read(ACTOR, 0x54)) == phase
                    cases += 1
    return cases


def removal_boundary(probe):
    """Observe the active slot and phase immediately after a completed transfer."""
    cases = 0
    for slot in range(3):
        for mask in range(8):
            if not mask & (1 << slot):
                continue
            for state in range(10):
                for armed in (0, 4):
                    for frame in (0, 5, 11):
                        probe.reset(state=state)
                        for weapon in range(3):
                            probe.item(weapon + 1, weapon)
                            probe.mass(weapon, 1)
                            probe.write(
                                ACTOR + 0x66 + weapon * 2,
                                weapon + 1 if mask & (1 << weapon) else 0,
                                "H",
                            )
                        probe.write(ACTOR + 0x12, slot, "B")
                        probe.write(ACTOR + 3, armed, "B")
                        probe.write(ACTOR + 0x10, frame, "B")
                        probe.write(ACTOR + 2, 4 if state == 4 else 8, "B")
                        probe.write(ACTOR + 8, 10, "h")
                        probe.write(ACTOR + 10, 12, "h")
                        probe.write(ACTOR + 12, 14, "h")
                        probe.write(ACTOR + 14, 16, "h")
                        before = bytes(probe.machine.mem_read(ACTOR, 0x54))
                        probe.write(CURSOR, 4)
                        assert probe.call(probe.build.equip, slot) == 1
                        assert probe.call(probe.build.land, ACTOR) == 1
                        assert probe.read(ACTOR + 0x66 + slot * 2, "H") == 0
                        assert probe.inventory() == [slot + 1, *([0] * 31)]
                        assert bytes(probe.machine.mem_read(ACTOR, 0x54)) == before
                        assert probe.idle_requests == 0
                        cases += 1
    return cases


def unchecked_sources(probe):
    """Invalid UI requests demonstrate why helpers cannot be a public mutation API."""
    cases = 0
    for scenario in ("absent", "foreign", "repeated"):
        probe.reset()
        probe.machine.mem_write(FOREIGN_ACTOR, bytes(256))
        probe.item(1, 2)
        probe.mass(2, 1)
        if scenario == "foreign":
            probe.write(FOREIGN_ACTOR + 0x72, 1, "H")
        elif scenario == "repeated":
            probe.inventory([1])
            probe.take(1)
            assert probe.call(probe.build.equip, 2) == 1
        original_item = bytes(probe.machine.mem_read(ITEMS + 16, 16))
        probe.take(1)
        assert probe.call(probe.build.equip, 2) == 1
        assert probe.read(ACTOR + 0x6A, "H") == 1
        assert bytes(probe.machine.mem_read(ITEMS + 16, 16)) == original_item
        if scenario == "foreign":
            assert probe.read(FOREIGN_ACTOR + 0x72, "H") == 1
        if scenario == "repeated":
            assert probe.inventory()[0] == 1
        cases += 1
    return cases


def cancellation_order(probe):
    probe.reset()
    for item in (1, 2, 3):
        probe.item(item, 2)
    probe.inventory([2, 1, 3])
    probe.take(1)
    assert probe.call(probe.build.equip, 0) == 0
    expected = [2, 3, 1] if probe.build.path.startswith("rus/") else [1, 2, 3]
    assert probe.inventory() == [*expected, *([0] * 29)]
    assert probe.read(ACTOR + 0x66, "H") == 0
    return 1


def main():
    for build in BUILDS:
        if not (ROOT / "original/k1" / build.path).is_file():
            raise FileNotFoundError(f"Both owned editions are required: {build.path}")
    for build in BUILDS:
        probe = InputProbe(build)
        results = {
            "dispatched_removal": dispatched_removal(probe),
            "dispatched_replacement": dispatched_replacement(probe),
            "removal_boundary": removal_boundary(probe),
            "unchecked_sources": unchecked_sources(probe),
            "cancellation_order": cancellation_order(probe),
        }
        print(build.path.split("/")[0], results, "original calls:", probe.calls)
    print("Dispatch uses a supplied hit target; actual hit testing, next tick/rendering omitted.")
    print("Absent/foreign/repeated references pass internal helpers; they are not safe UI actions.")
    print("Removal preserves active slot/armed flag/phase at return; no automatic fallback proven.")


if __name__ == "__main__":
    main()
