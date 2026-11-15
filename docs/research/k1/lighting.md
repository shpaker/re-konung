# Lighting and colour phases

The original lighting routines, with source confidence and per-build evidence.
The [time reference](time.md) owns the counter and threshold array;
[runtime contracts](../../development/rust/simulation.md#time-and-lighting-contracts)
explain our implementation. The lighting-only option does not stop time.

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
than it looks: the light routine and the serialization region are **byte-identical
between eng and tutorial**, and rus carries the same code and the same absolute operands
with a constant file offset. Globals at `0x81xxxx`/`0x82xxxx` live in `.bss`: the image
backs none of them, so a power-on value is zero and any non-zero "default" must come
from a runtime store.

### The order of the checks

The routine's head is ours, byte for byte (rus file `0x193EC…0x19465` / `VA
0x428FEC…0x429065`; eng and tutorial the same bytes at file `0x19564…` / `VA 0x429164…`):

1. the ground-light flag `[0x82D168]` is cleared;
2. the counter is read and reduced modulo 21600 into the stack local;
3. at remainder 0 the coin flip rewrites `[0x81D608]` (the per-day palette flag);
4. **the constant-light gate comes first**: `cmp [0x81D5DC], 0` (`je` skips it) and, when
   set, the routine indexes the per-map colour table by the current map number
   (`[0x81D6C4] · 4 + table`), stores the colour and **returns** — before any day/night
   test. So a constant-light map keeps its colour even when the day/night option is off;
5. only then the day/night gate: `cmp [0x81D5EC], 0` with the jump to the zero result,
   the clone's `if (!_bConfDayNight || dayTime < _lightPhazes[1])`.

The gate is a test of the per-map **flag** `[0x81D5DC]` — that is what makes a map a
constant-light map — not a sentinel test of the colour triple; reading `(-1,-1,-1)` as
"no constant light" stays the sensible reading of the table, but the decision is the
flag's. What the flag is written from is not pinned: no absolute store to `[0x81D5DC]`
exists (the writer reaches it base-relative), so which maps the game treats as
constant-light maps remains **hypothesis** at the flag and **by Slavik** at the list
(the clone's `_dungeonMaps` adds 43 and 46..50 with a sentinel value,
[`game_data.cpp:14-24`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_data.cpp#L14-L24)).
Which field of the per-map table the flag is copied from is the open question.

### The four globals, separated

Three different kinds of state were mixed in earlier drafts. They are ours, per build:

| Global | What it is | Written by | Read by |
|---|---|---|---|
| `[0x81D5EC]` | the **user's day/night option** (the clone's `_bConfDayNight`) | four copy sites that take the value from a settings/serialization table — e.g. rus `0x4363D5` (`VA 0x4363D5` region) does `mov eax, [0x831C5C] ; mov [0x81D5EC], eax`; rus writers `0x1B8E1`/`0x267D5`/`0x27E04`/`0x2805C`, eng `0x1BAA1`/`0x26A1A`/`0x28048`/`0x282A0`, tutorial `0x1BAA1`/`0x2692A`/`0x27431`/`0x27689` — and its address is handed to both serializer passes (rus `0x14B8F`, `0x15039`) | the day/night gate (`je`) plus one plain read (rus `0x27097`) and two comparisons |
| `[0x81D5DC]` | the **current map's constant-light flag** — a property of the map, not a user setting | not pinned: no absolute writer exists; the write happens base-relative somewhere in map activation | exactly one read: the gate ahead of day/night |
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

which agrees with the clone's table
([`game_data.cpp:14-24`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_data.cpp#L14-L24))
once the channel order is undone: the game packs blue first, the clone names the triple
red first. A triple of `(-1,-1,-1)` is read as "no constant light for this map" — the
only reading consistent with the three coloured slots being the three maps the clone
lists — but the deciding flag is `[0x81D5DC]`, not the sentinel, so the **list** of maps
that really get a constant colour stays **by Slavik** until the flag's source is found.

## What a phase does to the picture

The thresholds cut the cycle into thirteen windows. For a remainder `d` inside a window,
`t` is the distance from the window's start and `u` is **the integer half of it** — the
Watcom signed-divide idiom on the routine's stack local (`sar`/`sub`/`sar` for `/2`, a
true `idiv` for `/3`), read off rus `0x198D1…0x19927` (`VA 0x4294D1…0x429527`) and
byte-identical across eng and tutorial. This matters twice:

- **the arithmetic is integer, not the clone's floats.** The clone writes
  `float tmp = t / 2.0`
  ([`game.cpp:1763-1893`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1763-L1893));
  the original halves with integers and packs the channels as bytes. A channel that the
  clone would show at a half-integer is a whole integer here, and a ramp leaves the value
  it comes from one tick later than the clone's floats would — see the model note in
  [What the next tickets take](../../development/rust/simulation.md#time-and-lighting-contracts);
- **the ramp conditions are `≤`, not `<`.** The branch runs while `t` is at most the
  constant (`cmp …, 0x78 / jle` for the 121-length ramp): the ramp formula produces the
  plateau value at its last tick, and the plateau branch begins one tick later. Where
  the two differ — window 13 — this page records both ticks.

The scaling constants sit in one pool, identical across builds — the doubles
`{0.4, 0.2, 1/3, 10.0, 0.66, -50.0, 0.5988, -60.0}` at rus `VA 0x45022C…0x450264`, eng
`VA 0x450240…0x450278`, tutorial `VA 0x450248…0x450280` — and they settle one clone
detail and leave one open: there is **no `0.33` in the pool**, so where the clone shows
`0.33` (window 7's red) the original uses either the pool's `1/3` or the integer third
that window 13 demonstrably takes; which of the two, and the rounding of the packed
bytes, are unresolved. Neither choice moves any boundary (both windows stay dark
throughout).

The values below are the colour **before** it is divided by 100 on its way to the
shader. Sources per row: the thirteen **windows, their lengths and their ground-light
flag follow from the confirmed thresholds and the three stores to `[0x82D168]`** — they
are ours; the triples and ramp shapes are read as a table from the clone unless the row
says otherwise, that is **by Slavik**, with the two rows marked ★ read off the binaries
ourselves:

| # | window | ticks | tile light on | ambient (r, g, b) |
|---|---|---|---|---|
| 1 | 0 – 1800 | 1800 | no | 0, 0, 0 |
| 2 | 1800 – 1830 | 30 | no | `t ≤ 24`: u, u/2, 0 · else 12, 6, 0 |
| 3 | 1830 – 3570 | 1740 | no | 12, 6, 0 |
| 4 | 3570 – 3600 | 30 | no | `t ≤ 24`: (24−t)/2, (24−t)/2/2, 0 · else 0, 0, 0 |
| 5 | 3600 – 5400 | 1800 | no | 0, 0, 0 |
| 6 | 5400 – 8100 | 2700 | no | `t ≤ 100`: 0.2·u, −0.4·u, −u · else 10, −20, −50 |
| 7 | 8100 – 11700 | 3600 | **yes** | `t ≤ 60`: 10 − c·u, −20 − u, −50 − 0.66·u (c = 0.33 **by Slavik**, 1/3 or an integer third in the original — unresolved) · else 0, −50, −70 |
| 8 | 11700 – 11880 | 180 | **yes** | `t ≤ 120`: −u, −50 − u/3, −70 · else −60, −70, −70 |
| 9 | 11880 – 11910 | 30 | **yes** | coin flip off: −60, −70, −70 · `t ≤ 40`: −60, −70 + t/2, −70 · else −60, −50, −70 |
| 10 | 11910 – 16155 | 4245 | **yes** | coin flip: −60, −50, −70 · else −60, −70, −70 |
| 11 | 16155 – 16200 | 45 | **yes** | coin flip off: −60, −70, −70 · `t ≤ 40`: −60, −50 − t/2, −70 · else −60, −70, −70 |
| 12 | 16200 – 18900 | 2700 | **yes** | `t ≤ 100`: 0.5988·u − 60, u/5 − 70, u − 70 · else −30, −60, −20 |
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
   `0x437D9x`: the triple travels with a saved game.
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
   - truncation toward zero (the conversion helper sets the x87 control word to
     `0x1F`); the intermediates are exact in the 80-bit registers.
   So the clone's per-channel shader (`gfx.cpp:56-84` — negative multiplies,
   positive blends toward white) transcribes the tables faithfully, and the
   warm band really does brighten the whole picture in the original.
4. **The ground-pass gate.** The flag `[0x82D168]` has one reader (rus
   `0x425354`): with the flag set **and** the light-effects option
   `[0x81D604]` on, the ground pass takes the lit path (per tile: the
   `LIGHTS.N` value picks a `LIGHTS.RES` mask, `value × 0x1C80 + 0x5A1010`,
   and the lit blit `0x43CC49` adds the light colour); otherwise — window
   closed or option off — the same tiles draw through the plain blit with the
   **same ambient-transformed palettes** (the transform is called per tile at
   rus `0x42541D`/`0x425472` for the ground and overlay bytes alike).

The consequences for the surfaces, all **confirmed** by that chain:

| Surface | when it is darkened | source |
|---|---|---|
| ground, its upper layer, decorations, objects | with the ambient, whenever the triple is non-zero — dusk and the warm band fade the ground together with everything on it; the window and the option gate only the tile light | our own read (the transform call sites, the gate) |
| characters and dynamic objects | per creature, from the fine block of the map's `LIGHTS.N`: a creature in a cell whose fine value is 0 is blended toward the ambient, one in a lit cell is not | **by Slavik** [`game.cpp:1089-1100`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1089-L1100), [`game.cpp:1611-1626`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1611-L1626); our own read of the fine block in [formats/map.md](formats/map.md#lightsn) |
| the tile light itself | only inside the window and with the option on: the mask named by `LIGHTS.N` for that tile is sampled for brightness; where it is bright it adds the light colour and cancels the darkening | our own read (the gate, the lit blit); the clone agrees |

The tile-light window is windows 7 to 12 — the same span as the **yes** column above,
which is the three stores to `[0x82D168]` we read ourselves — and the light colour it
adds is itself random per draw, `16..23, 8..11, 0` before the division, halved for the
ground pass
(**by Slavik** [`game.cpp:925`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L925),
[`game.cpp:979`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L979)).
A per-draw random is a frame-rate dependency; we will not reproduce it (see below).

The byte-level arithmetic — whole-byte ramps, the f32 accumulator, the
truncation — is reproduced by the model (`k1_game::lighting`: `channels`,
`fade_factor`, `fade_table_entry` and its golden tests). The GPU path computes
the same formulas in 8-bit floats; the difference is bounded by the original's
own output quantization (16-bit colour) and is recorded with the divergences
below.

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
| the constant-light flag (`[0x81D5DC]`) | the whole phase computation, by returning early with the per-map colour | property of the current map | our own read (gate); its writer is not pinned |

So "no tile light" has two different causes — the phase and the option — and the two
switches answer different questions; one control for both would misrepresent the
original. #97 ships only the day/night one. And "day/night off" does **not** mean "the
picture is always `(0,0,0)`": the constant-light maps bypass the gate (ours does not
reach that bypass — see the divergence bullet below).

## Remaining hypotheses and unknowns

- The deciding constant-light flag [0x81D5DC] is confirmed, but its writer
  and source map field are not pinned. The (-1,-1,-1) sentinel interpretation
  and which maps activate the bypass remain hypothesis/source-only as above.
- The per-day coin selects (-60,-70,-70) or (-60,-50,-70) in windows 9–11.
  Its intended meaning (lightning, moonlight or neither) is unknown.
- User-option defaults are unknown: .bss zeros and copies from [0x831C5C]
  establish no shipped default without reading settings data.
- Unresolved arithmetic coefficients remain identified with their evidence;
  the model uses the clone's 0.33 for window 7 red rather than claiming a
  confirmed original coefficient. See the model's ambient_value documentation.

The light-effects address [0x81D604] is now confirmed by the ground-pass
read described above. An older open-question entry saying its address was
unresolved was superseded by that byte-level finding; it is not an open issue.

## Deliberate rendering differences

Our last pixel-blend stage uses 8-bit floats over the original's 5/6-bit integer
palette arithmetic; formulas, whole-byte ramp steps, f32 accumulation and
truncation are retained, with final differences bounded by original output
quantization. Original per-draw random light is replaced by a tick/seed-derived
color, and night variants are deterministic per day/seed. These are explicit
approximations for reproducible rendering, not recovered timing rules.

The viewer defaults to day/night enabled because the original default is unknown;
its Constant day preference is per viewer session, not a saved game option.
It affects only lighting and survives map changes. The original constant-map
bypass and separate light-effects switch are not implemented. See
[simulation](../../development/rust/simulation.md#time-and-lighting-contracts)
and [viewer](../../development/rust/viewer.md#playback-and-lighting-controls).
