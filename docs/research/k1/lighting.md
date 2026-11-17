<a id="lighting-and-colour-phases"></a>

# Lighting and colour phases

Original K1 lighting rules, sources and confidence. The counter and thresholds
are described in [time.md](time.md); implementation contracts are in
[simulation.md](../../development/rust/simulation.md#time-and-lighting-contracts).
The evidence and reproducible checks are recorded below.

## Sources and markers

**Confirmed** means we checked it ourselves: either a byte-level read of our own copy of
the game (both editions and the tutorial binary), or a render. Where the fact also has a
line in the read-only clone, the permalink is given, pinned to
[`cf5a332`](https://github.com/Marisa-Chan/Slavik/tree/cf5a3320dc93859e3dc51f37ee823fed30f399b6).
A statement resting on the clone alone is marked **by Slavik** and is not our fact. The
clone's code is never copied — it is cited by file and line and described in our words,
and where the clone reconstructs — a division it writes as a float where the original
divides with integers — the original wins.

Byte-level reads were made against `original/k1/rus/game/konung.res`,
`original/k1/eng/game/KONUNG.EXE` and `original/k1/eng/game/TUTORIAL.EXE`. Offsets are
file offsets; `VA` is the virtual address from that build's own PE section table
(Watcom: `BEGTEXT` at `0x410000`, `DGROUP` at `0x450000`). The three binaries are built
separately, so every address below is per-build and was read out of the build itself
rather than reused across builds. Two structural facts make the per-build list shorter
than it looks: the light routines have matching control flow with per-build table and helper
addresses. Their operands must be resolved separately; the routines are not byte-identical. Globals at `0x81xxxx`/`0x82xxxx` live in `.bss`: the image
backs none of them, so a power-on value is zero and any non-zero "default" must come
from a runtime store.

### The order of the checks

The routine's head is confirmed (RUS file `0x193EC` / VA `0x428FEC`; ENG and
TUTORIAL start at file `0x19564` / VA `0x429164`, with per-build operands):

1. the ground-light flag `[0x82D168]` is cleared;
2. the counter is read and reduced modulo 21600 into the stack local;
3. at remainder 0 the coin flip rewrites `[0x81D608]` (the per-day palette flag);
4. **the constant-light gate comes first**: `cmp [0x81D5DC], 0` (`je` skips it) and, when
   set, the routine indexes the per-map colour table by the current map number
   (`[0x81D6C4] · 4 + table`), stores the colour and **returns** — before any day/night
   test. So a constant-light map keeps its colour even when the day/night option is off;
5. only then the day/night gate: `cmp [0x81D5EC], 0` with the jump to the zero result,
   the clone's `if (!_bConfDayNight || dayTime < _lightPhazes[1])`.

The deciding per-map **flag** is loaded during map activation, not inferred from
a color sentinel. The audit found the direct store that the earlier investigation
missed: rus `0x43AE0A`, eng `0x43B052`, tutorial `0x43A422`. See the table below.

### The four globals, separated

Three different kinds of state were mixed in earlier drafts. They are ours, per build:

| Global | What it is | Written by | Read by |
|---|---|---|---|
| `[0x81D5EC]` | the **user's day/night option** (the clone's `_bConfDayNight`) | four copy sites that take the value from a settings/serialization table — e.g. rus `0x4363D5` (`VA 0x4363D5` region) does `mov eax, [0x831C5C] ; mov [0x81D5EC], eax`; rus writers `0x1B8E1`/`0x267D5`/`0x27E04`/`0x2805C`, eng `0x1BAA1`/`0x26A1A`/`0x28048`/`0x282A0`, tutorial `0x1BAA1`/`0x2692A`/`0x27431`/`0x27689` — and its address is handed to both serializer passes (rus `0x14B8F`, `0x15039`) | the day/night gate (`je`) plus one plain read (rus `0x27097`) and two comparisons |
| `[0x81D5DC]` | the **current map's constant-light flag** — a property of the map, not a user setting | confirmed direct store: rus `0x43AE0A`, eng `0x43B052`, tutorial `0x43A422` | exactly one read: the gate ahead of day/night |
| `[0x82D168]` | the **ground-light flag**, computed — the clone's `_bLightGround` | exactly three stores inside the routine: cleared at its top (rus `0x193EC`), set for windows 7–12 (rus `0x195FF` / `VA 0x4291FF`), cleared again in the last branch (rus `0x198AD` / `VA 0x4294AD`); eng and tutorial `0x19564`/`0x19777`/`0x19A25`. One absolute reader, now ours: `cmp dword [0x82D168], 0` at rus `0x15754` / `VA 0x425354` (eng `0x15884`), the ground-pass gate — see ["How the ambient value reaches the pixels"](#how-the-ambient-value-reaches-the-pixels) |
| `[0x81D608]` | the **per-day palette flag**, computed — the coin flip's target, the clone's `_bLightEff` | once, by the coin flip (above) | three comparisons in windows 9–11 only |

A fifth global, the **user's light-effects option** (the clone's `_bConfLightEffects`):
`[0x81D604]`, ours. It is read right after the ground-light flag in the ground-pass
gate (rus `0x42535D` / `VA 0x42535D`, `cmp dword [0x81D604], 0` — eng also `0x20E97`
and `0x28051`, the options and save code), and it gates exactly what the clone says:
with it off the ground takes the plain tile path, window or not. The default stays
unknown — same `.bss` story as the day/night option.

There is **no confirmed default** for either user option. `[0x81D5EC]` is `.bss`: zero at
power-on, and every writer copies from the settings table `[0x831C5C]` — no code in any
of the three builds stores a literal 1 into it. The clone's `= true` member
initializers are the clone's own choices
([`game.h:1461-1466`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.h#L1461-L1466));
whether the shipped settings data turns the options on is a data question the current
read does not answer. An earlier draft of this page said "both default to on" on the
clone's authority alone; that is withdrawn.

### Maps with a constant light

The per-map colour table the gate indexes is ours: 56 int32 slots, identical in all
three builds.

| Build | array file offset | array VA |
|---|---|---|
| rus `konung.res` | `0x40EE0` | `0x45ACE0` |
| eng `KONUNG.EXE` | `0x42118` | `0x45AD18` |
| eng `TUTORIAL.EXE` | `0x41524` | `0x45AD24` |

All 56 slots are `0x00FFFFFF` — the `(-1,-1,-1)` triple, blue packed first — except

| Map | red | green | blue |
|---|---|---|---|
| 42 | -70 | 0 | 0 |
| 44 | -20 | -1 | -70 |
| 45 | 0 | -50 | -50 |

The flag is independently read from a byte table. The activation code loads a
dword at `table + map`, takes its signed high byte, and stores it to `[0x81D5DC]`:

| Build | flag writer VA | effective flag byte VA for map 0 |
|---|---|---|
| RUS | `0x43AE0A` | `0x45AE2F` |
| ENG | `0x43B052` | `0x45AE67` |
| TUTORIAL | `0x43A422` | `0x45AE73` |

**Confirmed:** over the 56 color slots, flags are zero for 0..41 and one for
42..55 in all three builds. Slots 43 and 46..55 therefore **do** apply `(-1,-1,-1)`;
that value is not an absence sentinel. The three colored slots above retain their
special colors. Constant lighting bypasses both the cycle and the day/night option,
and leaves the ground-light flag clear. `tools/audit_k1_lighting.py` executes the
flag writer and checks every slot against `constant_map_color`.

## What a phase does to the picture

The thresholds cut the cycle into thirteen windows. For a remainder `d` inside a window,
`t` is the distance from the window's start and `u` is **the integer half of it** — the
Watcom signed-divide idiom on the routine's stack local (`sar`/`sub`/`sar` for `/2`, a
true `idiv` for `/3`), read off rus `0x198D1…0x19927` (`VA 0x4294D1…0x429527`) with equivalent operations and per-build operands in ENG and TUTORIAL. This matters twice:

- **the arithmetic is integer, not the clone's floats.** The clone writes
  `float tmp = t / 2.0`
  ([`game.cpp:1763-1893`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1763-L1893));
  the original halves with integers and packs the channels as bytes. A channel that the
  clone would show at a half-integer is a whole integer here, and a ramp leaves the value
  it comes from one tick later than the clone's floats would — see the model note in
  [the implementation contracts](../../development/rust/simulation.md#time-and-lighting-contracts);
- **the ramp conditions are `≤`, not `<`.** The branch runs while `t` is at most the
  constant (`cmp …, 0x78 / jle` for the 121-length ramp): the ramp formula produces the
  plateau value at its last tick, and the plateau branch begins one tick later. Where
  the two differ — window 13 — this page records both ticks.

The binary64 constant pool is **confirmed** in all three builds:
`{0.4, 0.2, 1/3, 10.0, 2/3, -50.0, 0.5988023952095808, -60.0}` at RUS
`VA 0x45022C..0x450264`, ENG `0x450240..0x450278`, TUTORIAL `0x450248..0x450280`.
The entries are the nearest binary64 representations, not exact decimal rationals.
Window 7 uses the floating pool thirds. Windows 8, 12 and 13 use integer division
for their `/3`, `/5` and `/2` terms respectively. Every final channel is truncated
toward zero before packing. The model evaluates the pool products as exact binary
rationals before truncation; early f32/f64 rounding would change boundary bytes.

All rows below are now **confirmed** by the procedure and full-cycle comparison.
`u = floor(t/2)`; all `/` terms in the table are integer divisions, except the
explicit pool-coefficient multiplications. These are packed palette channels,
not shader values; the original indexes palette tables instead of dividing by 100.

| # | window | ticks | tile light on | ambient (r, g, b) |
|---|---|---|---|---|
| 1 | 0 – 1800 | 1800 | no | 0, 0, 0 |
| 2 | 1800 – 1830 | 30 | no | `t ≤ 24`: u, u/2, 0 · else 12, 6, 0 |
| 3 | 1830 – 3570 | 1740 | no | 12, 6, 0 |
| 4 | 3570 – 3600 | 30 | no | `t ≤ 24`: (24−t)/2, (24−t)/2/2, 0 · else 0, 0, 0 |
| 5 | 3600 – 5400 | 1800 | no | 0, 0, 0 |
| 6 | 5400 – 8100 | 2700 | no | `t ≤ 100`: 0.2·u, −0.4·u, −u · else 10, −20, −50 |
| 7 | 8100 – 11700 | 3600 | **yes** | `t ≤ 60`: 10 − (1/3)·u, −20 − u, −50 − (2/3)·u (binary64 pool coefficients, then truncate) · else 0, −50, −70 |
| 8 | 11700 – 11880 | 180 | **yes** | `t ≤ 120`: −u, −50 − u/3, −70 · else −60, −70, −70 |
| 9 | 11880 – 11910 | 30 | **yes** | coin flip off: −60, −70, −70 · `t ≤ 40`: −60, −70 + t/2, −70 · else −60, −50, −70 |
| 10 | 11910 – 16155 | 4245 | **yes** | coin flip: −60, −50, −70 · else −60, −70, −70 |
| 11 | 16155 – 16200 | 45 | **yes** | coin flip off: −60, −70, −70 · `t ≤ 40`: −60, −50 − t/2, −70 · else −60, −70, −70 |
| 12 | 16200 – 18900 | 2700 | **yes** | `t ≤ 100`: 0.5988023952095808·u − 60, u/5 − 70, u − 70 · else −30, −60, −20 |
| 13 ★ | 18900 – 21600 | 2700 | no | `t ≤ 120`: u/2 − 30, u − 60, u/3 − 20 (all divisions integer — read from the byte-packed branch, rus `0x4294D1`) · else 0, 0, 0 |

Two things follow that a four-phase sketch would miss.

A phase is not a discrete step. Nine of the thirteen windows open with a ramp of 25, 61,
101 or 121 ticks — 1.5, 3.7, 6.1 and 7.3 seconds at speed 0 — and hold a constant value
afterwards. Transitions are interpolated, but only inside those windows; between them a
new window changes the value in one tick.

Neutral is `(0,0,0)`, and it appears **three** times: windows 1, 5 and the tail of 13.
A bright stretch in the middle of the cycle (window 5) sits between the warm window and
the dark one, so the cycle is not "bright half, dark half".

### How the ambient value reaches the pixels

The chain is ours now, read end to end from the binaries (rus offsets; eng
control-checked):

1. **The packed triple.** The light routine returns the ambient as one dword —
   blue | green<<8 | red<<16, three signed bytes, whole numbers (the window
   formulas are `fistp`-truncated toward zero at the packing sites, e.g. rus
   `0x429143-0x42914E`) — and the single caller stores it to `[0x5076BC]`
   (rus `0x42866D`). No division happens anywhere: the clone's `/100` is its
   own float translation. The address also sits in the save-field table at rus
   `0x437D9x`; interpreting that serializer specifically as saved-game I/O remains
   an inference, as described in [time.md](time.md#hypotheses-and-unknowns).
2. **One shared transform, no state.** Every drawn sprite resolves its palette
   through the same function (rus `0x43D971`): a zero triple (daylight) returns
   the palette untouched; anything else maps each palette entry per channel
   through lookup tables and caches the result per palette bank. There is no
   fade state to set or inherit — the clone's `SetFade(bool)` state machine is
   its own modeling, and its "decorations inherit the previous pass's state"
   question does not exist in the original.
3. **The tables.** Six tables are built at startup (rus `0x439B08-0x439D18`,
   565 layout; a twin cluster at `0x43A012+` for 555): per channel, 100 values
   each, negative and positive apart. The value `v` selects an entry through a
   float32 accumulator `k` that takes `v` steps of the pool constant `0.01f`
   (VA `0x4506C7` — the f32 accumulation, not `v/100`), and the entry maps a
   component `c` (5-bit, 6-bit for green) as:
   - negative: `trunc(c · (1 − k))` — darkening;
   - positive: `c + trunc((MAX − c) · k)` with `MAX` = 31/63 — **brightening
     toward the component maximum**;
   - truncation toward zero, selected by the conversion helper; executing the
     RUS builder verifies all 25,600 entries against this arithmetic.
   The clone's plain-palette shader path follows these negative/positive transforms;
   its masked path has the discrepancy described below. The warm band brightens
   the picture in the original.
4. **The ground-pass gate.** The flag `[0x82D168]` has one reader (rus
   `0x425354`): with the flag set **and** the light-effects option
   `[0x81D604]` on, the ground pass takes the lit path (per tile: the
   `LIGHTS.N` value picks a `LIGHTS.RES` mask, `value × 0x1C80 + 0x5A1010`,
   and the lit blit `0x43CC49` adds the light colour); otherwise — window
   closed or option off — the same tiles draw through the plain blit with the
   **same ambient-transformed palettes** (the transform is called per tile at
   rus `0x42541D`/`0x425472` for the ground and overlay bytes alike).

The surface rules have the confidence shown in the source column:

| Surface | when it is darkened | source |
|---|---|---|
| ground, its upper layer, decorations, objects | with the ambient, whenever the triple is non-zero — dusk and the warm band fade the ground together with everything on it; the window and the option gate only the tile light | our own read (the transform call sites, the gate) |
| characters and dynamic objects | per creature, from the fine block of the map's `LIGHTS.N`: a creature in a cell whose fine value is 0 is blended toward the ambient, one in a lit cell is not | **by Slavik** [`game.cpp:1089-1100`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1089-L1100), [`game.cpp:1611-1626`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1611-L1626); our own read of the fine block in [formats/map.md](formats/map.md#lightsn) |
| the tile light itself | only inside the window and with the option on: the mask named by `LIGHTS.N` for that tile is sampled for brightness; where it is bright it adds the light colour and cancels the darkening | our own read (the gate, the lit blit); the clone agrees |

The tile-light window is windows 7 to 12. The nonzero-mask path is confirmed by
RUS `0x43CC49` and its setup `0x43E7FA`: red and green first use a positive fade
table selected by the light level, then a negative table selected by
`ambient_byte + mask_byte`, clamped at zero. Positive ambient is ignored on that
path. Blue and zero-mask pixels use the ordinary signed ambient tables.

The usual ground setup halves integer light levels before indexing the tables
(RUS `0x4254E6..0x42553C`), so an odd level is rounded down, not divided by 200 as
a float. A separate full-strength branch tests an unresolved group field; it is
recorded as an open applicability question in the audit. The random ranges and
per-draw behavior remain **by Slavik** (`game.cpp:925`). Our variation is a pure
function of tick and seed, deliberately independent of draw rate.

The GPU preserves the original **display-encoded** color domain: it converts the
linear sample of an sRGB texture back to encoded RGB, applies lighting, then returns
to linear RGB for Bevy's tint and compositing. Floating output, mask interpolation
and omitted intermediate 5/6-bit truncation remain deliberate precision differences.
The previous universal one-quantization-step bound was unproven and is withdrawn.
See the audit's synthetic GPU pixel checks and limitations.

Two more surfaces on the same screen never fade at all, and the fade-off reads are the
clone's (**by Slavik**, pinned above). A character's shadow is drawn right after
`SetFade(false)`
([game.cpp:1300-1301](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1300-L1301)),
an object's shadow the same way
([game.cpp:1348-1349](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1348-L1349))
— a shadow carries no fade state ever. An object's fire is drawn with `SetFade(false)`
before `DrawFlame`
([game.cpp:1592-1593](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1592-L1593)),
and which objects carry a fire is decided by the fade gate itself: an object whose flame
list holds two or more entries is drawn without the ambient fade, a list of one or none
fades with the ambient (`Flames.size() >= 2`, **by Slavik**
[game.cpp:1563-1580](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1563-L1580)).
The data behind that gate is **confirmed** (read ourselves): over both editions
`Objects.res` holds exactly 74 objects with flame lists of two or more entries, and not
one inactive slot (`flame_id < 0`) among them — the length reading and the active-slot
reading agree today. The test
`objects_with_a_two_entry_flame_list_always_have_two_active_slots`
(`k1-data/tests/original_graphics.rs`) pins that agreement, so a future data sample
cannot flip the self-lit semantics unnoticed.

### The switches and the flags are not one control

There are two user switches and two computed states, and the routine consults a third
thing that is neither:

| | what it gates | kind | source |
|---|---|---|---|
| day/night (`[0x81D5EC]`) | the phase computation from the gate on: off means the ambient result is `(0,0,0)` — **except on constant-light maps**, which returned already | user option, data-driven default | our own read (gate, writers); default unconfirmed |
| light effects (`[0x81D604]`) | the ground's tile light: with it off the ground takes the plain tile path, window or not — the ambient fade is not affected | user option | our own read (the gate at rus `0x42535D`); the clone agrees on the effect |
| the ground-light flag (`[0x82D168]`) | whether the phase lights the ground's tiles at all: outside windows 7..12 there is no tile light to draw — the ambient fade is not affected | computed from the phase | our own read (three stores, one reader) |
| the palette flag (`[0x81D608]`) | which night palette windows 9–11 return | computed per day (coin flip) | our own read (write and three reads) |
| the constant-light flag (`[0x81D5DC]`) | the whole phase computation, by returning early with the per-map colour | property of the current map | our own read (gate and activation writer) |

So "no tile light" has two different causes — the phase and the option — and the two
switches answer different questions; one control for both would misrepresent the
original. #97 ships only the day/night one. And "day/night off" does **not** mean "the
picture is always `(0,0,0)`": the constant-light maps bypass the gate (ours does not
reach that bypass — see the divergence bullet below).

<a id="deliberate-rendering-differences"></a>

## Deliberate rendering differences

The packed channels and original coefficients are retained. Final lighting uses
display-encoded floats, smooth mask sampling and Bevy's linear alpha compositing
instead of intermediate 5/6-bit truncation. No universal one-quantization-step
error bound is claimed. The [audit](lighting.md#remaining-limits-and-intentional-differences)
separates these choices from confirmed rule errors and unresolved surface exceptions.

Palette selection and flicker are deterministic functions of the simulation tick
and seed. The 16-tick flicker period is our approximation, not a recovered rule.
Both frontends apply constant-map colors before the day/night gate; the separate
light-effects model gate is implemented, with no additional menu control.
See [implementation contracts](../../development/rust/simulation.md#time-and-lighting-contracts).

## Original evidence and reproduction

`uv run tools/audit_k1_lighting.py` loads the owner's binaries into an isolated x86
emulator. It executes the lighting procedure at every cycle tick for each palette,
compares with the Rust `lighting_trace` example, checks thresholds, tests the option
bypass and all constant-color slots, and executes the map-flag writer for every slot.
The script dependencies are isolated and pinned; nothing is added to game dependencies.
No executable bytes or full traces are stored in the repository.

| Build | lighting body VA | body exit VA | flag store VA | effective flag byte base |
|---|---|---|---|---|
| RUS `konung.res` | `0x428FEC` | `0x42952A` | `0x43AE0A` | `0x45AE2F` |
| ENG `KONUNG.EXE` | `0x429164` | `0x4296A2` | `0x43B052` | `0x45AE67` |
| ENG `TUTORIAL.EXE` | `0x429164` | `0x4296A2` | `0x43A422` | `0x45AE73` |

**Confirmed:** the three traces agree. Before the corrections each had 160 mismatches
out of 43,200 palette/tick states; afterwards each has zero. The complete trace
(R, G, B as signed bytes, then ground flag, Deep before Green) has SHA-256
`3fe6c2dafcb46ed813a7311c3fd38022d79094b1756c13ee6a0335a03c62d40b`.
The ordinary Rust unit test pins separate FNV-1a checksums for both complete cycles,
so CI covers every tick without distributing fixtures derived from game resources.

**Emulation limitation:** random selection is replaced with a controlled result to
exercise both branches. RUS runs its original rounding helper at `0x43EAAE`.
The protected ENG/TUTORIAL helpers are not executable as stored on disk; their calls
are intercepted with the RUS-confirmed chop semantics, letting the original `fistp`
perform truncation. Those runs verify the intact light routines and pools under that
explicit rounding assumption, not the protected loaders or a complete game session.
The routines have matching control flow, not byte-identical per-build operands.

The RUS 565 palette-table builder is also executed independently and its 25,600
entries compared with the documented f32-accumulation/truncation arithmetic. The
lit-path evidence comes from the original's pointer-table setup and pixel blit:
nonzero mask bytes select the negative ambient table after the positive light table;
zero-mask pixels and blue use the ordinary ambient transform. This is independent
of the reference reimplementation's fragment shader.

Slavik remains a read-only, unlicensed secondary reference. Its
[light calculation](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1762)
and [shader](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/gfx.cpp#L56)
are not authoritative for integer division, encoded color, or positive-ambient lit
pixels. These discrepancies were reported in the chat; no change was made upstream
and no reference code was copied.


## Render and regression checks

The synthetic probe exercises the actual material, shader, sRGB texture decoding
and output encoding without game content:

```sh
cargo run --profile dev-runtime --manifest-path crates/Cargo.toml -p kn-lighting \
  --example lighting_probe -- out/lighting-audit/probe.png
uv run python tools/check_lighting_probe.py out/lighting-audit/probe.png
```

All six RGB cases passed, within one 8-bit output step: unchanged gray, negative
ambient, positive ambient, masked night, masked positive red, and masked neutral
ambient. Expected/observed pixels are `(128,128,128)`, `(51,38,38)`, `(143,136,128)`,
`(83,67,38)`, `(138,133,64)`, `(138,133,128)`. Alpha is passed through the lighting
shader to Bevy's standard tint/discard/compositing path.

Real-map before/after captures use identical map/tick/camera inputs and are kept only
in `out/lighting-audit/{before,after}`. The capture command is:

```sh
cargo run --profile dev-runtime --manifest-path crates/Cargo.toml -p kn-viewer -- \
  --game-dir original/k1/rus/game --map 17 --light-tick 12000 \
  --screenshot out/lighting-audit/after/rus-12000.png
```

The matrix includes both editions, map 17 through neutral/warm/dusk/local-light/
dawn states, buildings on map 1, local light on map 19, fixed-color maps 42/44/45,
and map 46 without a mask file. With seed 42, tick 12000 is the Green palette;
tick 55200 is the Deep palette (day 2). Tick 33600 is another Green day, not a
second palette. Live creatures/flames and diagnostic text are not frozen by
`--light-tick`, so full-screen image equality is not an acceptance criterion.

Rust tests cover the complete packed trace, boundary cases, separate gates,
constant-map precedence, viewer map changes, material assignment and moving
creatures. `just check` remains the complete repository gate. Performance measurements
and their limitations are recorded in [rendering.md](../../development/rust/rendering.md#lighting-audit-measurements).

## Remaining limits and intentional differences

- **Unknown applicability:** RUS `0x4254A9..0x425514` chooses full-strength ground light
  when a nonnull record at `[0x81D580]` selects a group whose final byte equals 1
  (`[group * 32 + 0x67C0A6] >> 24`). Otherwise levels are integer-halved. The group
  field is still unknown in the format description, and the active-record lifecycle
  is not recovered. The normal half-strength path is implemented; no invented
  meaning or guessed player bonus is attached to this exception.
- **By Slavik:** per-draw random light ranges, and the object/flame/shadow/creature
  exemptions above still need independent behavioral or binary confirmation.
- **Unknown:** shipped option defaults, the narrative meaning of the two night
  variants, exact save/map-activation lifecycle, and nighttime NPC schedules.
- **Intentional:** per-day palette selection and flicker are deterministic functions
  of seed/tick; flicker changes every 16 ticks. The clock is one simulation counter,
  not the original per-frame speed multiplier. UI hour/phase names are conventions.
- **Intentional precision differences:** display-encoded float lighting avoids the
  original intermediate 5/6-bit truncation. Masks retain smooth sampling and an
  8-bit normalized texture representation; filtered values below one percent use
  the plain palette path. Bevy blends alpha in linear space. The earlier claim
  that every resulting pixel differs by at most one original quantization step
  was not established and is withdrawn, especially for the two-stage lit path.
  The GPU probe verifies the chosen formulas, not universal screenshot identity
  with the original framebuffer.
