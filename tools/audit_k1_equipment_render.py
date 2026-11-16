# /// script
# requires-python = ">=3.14"
# dependencies = ["pefile==2024.8.26", "unicorn==2.1.4"]
# ///
"""Execute original equipment-layer selection on synthetic loaded sprite descriptors.

Run: UV_PYTHON=3.14 uv run tools/audit_k1_equipment_render.py
Only final shadow/image drawing calls are intercepted. Original equipment tables,
slot selection, frame lookup, palette selection and coordinate arithmetic execute.
No real images, original loader, body renderer, animation tick or input path runs.
Exit 1 reports an incomplete path; missing initialization/protection is only a hypothesis.
"""

import struct
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ACTOR = 0x1001000
STACK = 0x1008000
RETURN = 0x100F000
PIXELS = 0x1100000
PALETTES = 0x1300000
TARGET = 0x1400000
ITEMS = 0x659B0C
EQUIPMENT = 0x67F8CC
MODELS = 0x6F348C
DESCRIPTOR_SIZE = 0x4D28
IMAGE_DIRECTORY = 0x1728
FRAME_OFFSETS = 0x2F28
SETS = (0, 2, 4, 6, 8, 10)
STATES = (0, 1, 4, 5, 6, 7)
FRAMES = (0, 5, 11)


@dataclass(frozen=True)
class Build:
    path: str
    shadow: int
    image: int
    draw_shadow: int
    draw_image: int
    table_base: int
    descriptions: int
    palette_base: int
    palette_helper: int


BUILDS = (
    Build(
        "rus/game/konung.res",
        0x424ED0,
        0x427E9C,
        0x43C040,
        0x43CEAA,
        0x462BFC,
        0x459B50,
        0x5076F4,
        0x43D971,
    ),
    Build(
        "eng/game/KONUNG.EXE",
        0x425000,
        0x427FCC,
        0x43C288,
        0x43D0F2,
        0x462C34,
        0x459B88,
        0x50772C,
        0x43DBB9,
    ),
)


@dataclass(frozen=True)
class SelectionPolicy:
    family: str
    row: int
    raised_slots: tuple[int, ...]


def selection_policy(state: int, direction: int, armed: int, active: int) -> SelectionPolicy:
    """Describe verified selection inputs; tables themselves come from the owned EXE."""
    if not armed & 4:
        return SelectionPolicy("unarmed", direction, ())
    phase = {1: 3, 4: 1, 5: 2}.get(state, 0)
    if active == 1 and phase != 2:
        return SelectionPolicy("bow", phase * 8 + direction, (1,))
    raised = (5,) if active == 1 else (active, 5)
    return SelectionPolicy("melee", phase * 8 + direction, raised)


@dataclass(frozen=True)
class DrawTrace:
    slot: int
    sprite_set: int
    frame_id: int
    pointer: int
    palette: int | None
    model_offset: tuple[int, int]
    gear_offset: tuple[int, int]
    x: int
    y: int


def frame_id(state: int, direction: int, frame: int) -> int:
    return state * 64 + direction * 4 + frame // 4


def model_offset(direction: int, frame: int) -> tuple[int, int]:
    return -7 - direction, 11 + frame


def gear_offsets(sprite_set: int, state: int, direction: int, frame: int):
    return (-30 - sprite_set, 40 + direction, 50 + state, -60 - frame)


class RenderProbe:
    def __init__(self, build: Build):
        import pefile
        from unicorn import UC_ARCH_X86, UC_HOOK_CODE, UC_HOOK_MEM_READ, UC_MODE_32, Uc
        from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EBP, UC_X86_REG_EIP, UC_X86_REG_ESP

        self.registers = UC_X86_REG_EAX, UC_X86_REG_EBP, UC_X86_REG_EIP, UC_X86_REG_ESP
        self.build = build
        executable = pefile.PE(str(ROOT / "original/k1" / build.path))
        image = executable.get_memory_mapped_image()
        self.helper_rva = build.palette_helper - executable.OPTIONAL_HEADER.ImageBase
        self.helper_offset = executable.get_offset_from_rva(self.helper_rva)
        self.helper_section = (
            executable.get_section_by_rva(self.helper_rva).Name.rstrip(b"\0").decode()
        )
        self.entry_address = (
            executable.OPTIONAL_HEADER.ImageBase + executable.OPTIONAL_HEADER.AddressOfEntryPoint
        )
        raw_data = (ROOT / "original/k1" / build.path).read_bytes()
        self.helper_bytes = raw_data[self.helper_offset : self.helper_offset + 64]
        assert len(self.helper_bytes) == 64
        assert image[self.helper_rva : self.helper_rva + 64] == self.helper_bytes
        self.machine = Uc(UC_ARCH_X86, UC_MODE_32)
        self.machine.mem_map(executable.OPTIONAL_HEADER.ImageBase, (len(image) + 4095) & ~4095)
        self.machine.mem_write(executable.OPTIONAL_HEADER.ImageBase, image)
        self.machine.mem_map(0x1000000, 0x10000)
        self.machine.mem_map(PIXELS, 0x200000)
        self.machine.hook_add(UC_HOOK_CODE, self.boundary)
        self.machine.hook_add(UC_HOOK_MEM_READ, self.observe_read)
        self.tables = {
            "melee": self.rows(build.table_base, 32),
            "bow": self.rows(build.table_base + 0x300, 32),
            "unarmed": self.rows(build.table_base + 0x600, 8),
        }
        assert all(sorted(row) == list(range(6)) for rows in self.tables.values() for row in rows)
        self.calls = 0
        self.draw_calls = 0
        self.initialize_descriptors()
        self.check_helper_bytes()

    def check_helper_bytes(self):
        assert bytes(self.machine.mem_read(self.build.palette_helper, 64)) == self.helper_bytes

    def read(self, address, format="I"):
        return struct.unpack("<" + format, self.machine.mem_read(address, struct.calcsize(format)))[
            0
        ]

    def write(self, address, value, format="I"):
        self.machine.mem_write(address, struct.pack("<" + format, value))

    def rows(self, address, count):
        return tuple(struct.iter_unpack("<6i", self.machine.mem_read(address, count * 24)))

    def initialize_descriptors(self):
        for sprite_set in range(24):
            descriptor = EQUIPMENT + sprite_set * DESCRIPTOR_SIZE
            self.machine.mem_write(descriptor, bytes(DESCRIPTOR_SIZE))
            self.write(descriptor, sprite_set * 512)
            self.write(descriptor + 4, PIXELS + sprite_set * 0x10000)
            for state in STATES:
                for direction in range(8):
                    self.write(descriptor + 8 + state * 208 + direction * 26, 12, "h")
                    for frame in FRAMES:
                        identifier = frame_id(state, direction, frame)
                        self.write(
                            descriptor + 10 + state * 208 + direction * 26 + frame * 2,
                            identifier,
                            "h",
                        )
                        self.write(
                            descriptor + IMAGE_DIRECTORY + identifier * 8, identifier * 32 + 8, "i"
                        )
                        self.write(
                            descriptor + IMAGE_DIRECTORY + identifier * 8 + 4,
                            identifier * 32 + 16,
                            "i",
                        )
                        offset = (
                            descriptor + FRAME_OFFSETS + state * 768 + direction * 96 + frame * 8
                        )
                        self.machine.mem_write(
                            offset,
                            struct.pack("<4h", *gear_offsets(sprite_set, state, direction, frame)),
                        )
        for state in STATES:
            for direction in range(8):
                for frame in FRAMES:
                    offset = MODELS + 0x828 + state * 384 + direction * 48 + frame * 4
                    self.machine.mem_write(
                        offset, struct.pack("<2h", *model_offset(direction, frame))
                    )
        self.write(0x81D5CC, 101)
        self.write(0x81D5D0, -203, "i")
        self.write(self.build.palette_base, PALETTES)
        # Zero palette-lighting mode lets the original palette helper return base + offset.
        self.write(self.build.palette_base - 0x38, 0)

    def configure(self, state, direction, frame, armed, active, shield=True, weapons=True):
        self.machine.mem_write(ACTOR, bytes(256))
        for offset, value in (
            (0, state),
            (1, direction),
            (3, armed),
            (0x10, frame),
            (0x12, active),
        ):
            self.write(ACTOR + offset, value, "B")
        self.write(ACTOR + 0xE8, -307, "i")
        self.write(ACTOR + 0xF0, 409)
        for slot, sprite_set in enumerate(SETS):
            self.write(ITEMS + (slot + 1) * 16 + 3, slot, "b")
            self.write(self.build.descriptions + slot * 64 + 62, sprite_set, "h")
            reference = slot + 1 if (slot != 5 or shield) and (slot > 2 or weapons) else 0
            self.write(ACTOR + 0x66 + slot * 2, reference, "h")
        self.state, self.direction, self.frame = state, direction, frame

    def observe_read(self, machine, access, address, size, value, data):
        if size == 4 and ACTOR + 0x64 <= address <= ACTOR + 0x6E and address % 2 == 0:
            self.slot = (address - ACTOR - 0x64) // 2
            self.visited_slots.append(self.slot)
        if size == 4 and self.build.table_base <= address < self.build.table_base + 72 * 24:
            self.table_reads.add((address - self.build.table_base) // 24)

    def boundary(self, machine, address, size, data):
        if address == self.build.palette_helper:
            self.palette_helper_entered = True
        if address not in (self.build.draw_shadow, self.build.draw_image):
            return
        accumulator, base_register, instruction, stack_register = self.registers
        stack = machine.reg_read(stack_register)
        base = machine.reg_read(base_register)
        shadow = address == self.build.draw_shadow
        descriptor = self.read(base - (0x24 if shadow else 0x2C))
        sprite_set = (descriptor - EQUIPMENT) // DESCRIPTOR_SIZE
        identifier = self.read(
            descriptor + 10 + self.state * 208 + self.direction * 26 + self.frame * 2, "h"
        )
        if shadow:
            pointer, x, y = struct.unpack("<Iii", machine.mem_read(stack + 4, 12))
            palette = None
        else:
            target, pointer, palette, x, y = struct.unpack(
                "<IIIii", machine.mem_read(stack + 4, 20)
            )
            assert target == TARGET
        model_address = MODELS + 0x828 + self.state * 384 + self.direction * 48 + self.frame * 4
        weapon_offset = struct.unpack("<2h", machine.mem_read(model_address, 4))
        gear_address = (
            descriptor
            + FRAME_OFFSETS
            + self.state * 768
            + self.direction * 96
            + self.frame * 8
            + (0 if shadow else 4)
        )
        gear_offset = struct.unpack("<2h", machine.mem_read(gear_address, 4))
        self.traces.append(
            DrawTrace(
                self.slot,
                sprite_set,
                identifier,
                pointer,
                palette,
                weapon_offset,
                gear_offset,
                x,
                y,
            )
        )
        self.draw_calls += 1
        destination = self.read(stack)
        machine.reg_write(accumulator, 0)
        machine.reg_write(stack_register, stack + 4)
        machine.reg_write(instruction, destination)

    def render(self, shadow, *, direct_palette=True):
        self.traces, self.visited_slots, self.table_reads = [], [], set()
        self.slot = -1
        self.palette_helper_entered = False
        before = bytes(self.machine.mem_read(ACTOR, 256))
        arguments = (ACTOR,) if shadow else (TARGET, ACTOR, int(direct_palette))
        self.machine.mem_write(
            STACK, struct.pack("<" + "I" * (len(arguments) + 1), RETURN, *arguments)
        )
        _, _, instruction, stack_register = self.registers
        self.machine.reg_write(stack_register, STACK)
        self.machine.emu_start(
            self.build.shadow if shadow else self.build.image, RETURN, count=10000
        )
        assert self.machine.reg_read(instruction) == RETURN
        assert bytes(self.machine.mem_read(ACTOR, 256)) == before
        self.calls += 1
        return tuple(self.traces)

    def expected(self, policy, shadow):
        result = []
        weapon_x, weapon_y = model_offset(self.direction, self.frame)
        for slot in self.tables[policy.family][policy.row]:
            if not self.read(ACTOR + 0x66 + slot * 2, "h"):
                continue
            original_set = self.read(self.build.descriptions + slot * 64 + 62, "h")
            if original_set < 0:
                continue
            sprite_set = original_set + (slot in policy.raised_slots)
            descriptor = EQUIPMENT + sprite_set * DESCRIPTOR_SIZE
            identifier = self.read(
                descriptor + 10 + self.state * 208 + self.direction * 26 + self.frame * 2, "h"
            )
            offset = self.read(
                descriptor + IMAGE_DIRECTORY + identifier * 8 + (0 if shadow else 4), "i"
            )
            if shadow and offset < 0:
                continue
            offsets = gear_offsets(sprite_set, self.state, self.direction, self.frame)
            x, y = offsets[:2] if shadow else offsets[2:]
            result.append(
                DrawTrace(
                    slot,
                    sprite_set,
                    identifier,
                    (PIXELS + sprite_set * 0x10000 + offset) & 0xFFFFFFFF,
                    None if shadow else PALETTES + sprite_set * 512,
                    (weapon_x, weapon_y),
                    (x, y),
                    101 - 307 + weapon_x + x,
                    -203 + 409 + weapon_y + y,
                )
            )
        return tuple(result)

    def check(self, policy, *, direct_palette=True):
        for shadow in (False, True):
            actual = self.render(shadow, direct_palette=direct_palette)
            assert actual == self.expected(policy, shadow), (
                self.build.path,
                policy,
                shadow,
                actual,
            )
            assert self.visited_slots == list(self.tables[policy.family][policy.row])
            start = {"melee": 0, "bow": 32, "unarmed": 64}[policy.family]
            assert self.table_reads == {start + policy.row}


def selection_matrix(probe):
    cases = 0
    for state in STATES:
        for direction in range(8):
            for frame in FRAMES:
                for armed in (0, 4):
                    for active in range(3):
                        for shield in (False, True):
                            probe.configure(state, direction, frame, armed, active, shield)
                            probe.check(selection_policy(state, direction, armed, active))
                            cases += 1
    return cases


def last_weapon_removed(probe):
    cases = 0
    for state in STATES:
        for direction in range(8):
            for frame in FRAMES:
                for armed in (0, 4):
                    for active in range(3):
                        probe.configure(state, direction, frame, armed, active, weapons=False)
                        probe.check(selection_policy(state, direction, armed, active))
                        cases += 1
    return cases


def empty_layers(probe):
    cases = 0
    for armed in (0, 4):
        for active in range(3):
            for slot in range(6):
                for scenario in (
                    "no_set",
                    "no_shadow",
                    "zero_shadow",
                    "no_image",
                    "no_frame",
                    "zero_count",
                ):
                    probe.configure(5, 3, 5, armed, active)
                    policy = selection_policy(5, 3, armed, active)
                    description = probe.build.descriptions + slot * 64 + 62
                    sprite_set = SETS[slot] + (slot in policy.raised_slots)
                    entry = (
                        EQUIPMENT
                        + sprite_set * DESCRIPTOR_SIZE
                        + IMAGE_DIRECTORY
                        + frame_id(5, 3, 5) * 8
                    )
                    address = (
                        description
                        if scenario == "no_set"
                        else entry + (4 if scenario == "no_image" else 0)
                    )
                    if scenario == "no_frame":
                        address = (
                            EQUIPMENT + sprite_set * DESCRIPTOR_SIZE + 10 + 5 * 208 + 3 * 26 + 5 * 2
                        )
                    elif scenario == "zero_count":
                        address = EQUIPMENT + sprite_set * DESCRIPTOR_SIZE + 8 + 5 * 208 + 3 * 26
                    format = "h" if scenario in ("no_set", "no_frame", "zero_count") else "i"
                    before = probe.read(address, format)
                    probe.write(
                        address, 0 if scenario in ("zero_shadow", "zero_count") else -1, format
                    )
                    probe.check(policy)
                    probe.write(address, before, format)
                    cases += 1
    return cases


def palette_helper_branch(probe):
    from unicorn import UcError

    completed = 0
    for active in range(3):
        probe.configure(4, 7, 11, 4, active)
        probe.check_helper_bytes()
        try:
            probe.check(selection_policy(4, 7, 4, active), direct_palette=False)
        except UcError as error:
            assert probe.palette_helper_entered
            assert not probe.traces
            probe.check_helper_bytes()
            return {
                "completed": completed,
                "blocked": 1,
                "reason": f"standalone PE helper {probe.build.palette_helper:#x}: {error}",
            }
        completed += 1
    return {"completed": completed, "blocked": 0}


def main():
    for build in BUILDS:
        if not (ROOT / "original/k1" / build.path).is_file():
            raise FileNotFoundError(f"Both owned editions are required: {build.path}")
    baseline = None
    blocked = 0
    for build in BUILDS:
        probe = RenderProbe(build)
        if baseline is None:
            baseline = probe.tables
        else:
            assert probe.tables == baseline
        results = {
            "selection": selection_matrix(probe),
            "last_weapon_removed": last_weapon_removed(probe),
            "empty_layers": empty_layers(probe),
            "palette_helper_branch": palette_helper_branch(probe),
        }
        blocked += results["palette_helper_branch"]["blocked"]
        print(
            build.path.split("/")[0],
            f"palette helper VA={build.palette_helper:#x} raw offset={probe.helper_offset:#x}",
            f"section={probe.helper_section} PE entry VA={probe.entry_address:#x}",
            "raw/mapped/post-fixture 64-byte identity=True",
        )
        print(
            build.path.split("/")[0],
            results,
            "completed original calls:",
            probe.calls,
            "intercepted final draws:",
            probe.draw_calls,
        )
    print("All 72 original ordering rows match between editions; every row is a slot permutation.")
    print("Only final draws intercepted; synthetic descriptors, direct-palette branch 1 in matrix.")
    print("Palette helper failures remain blocked; no helper is replaced or patched.")
    print("Shadow selection skips negative offsets, accepts zero; image selection forwards -1.")
    print("Zero frame counts/negative frame IDs are unchecked; final draw remains intercepted.")
    print(
        "No input/tick/body rendering, real-frame availability or automatic weapon fallback claim."
    )

    if blocked:
        print("Partial matrix completed; original palette-helper path remains incomplete.")
        print("Failure cause unknown; missing runtime initialization/protection is a hypothesis.")
    return int(bool(blocked))


if __name__ == "__main__":
    raise SystemExit(main())
