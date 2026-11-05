# Time of day and light phases

What the game does with the hour: how a day counter is kept, where the light phases of
the cycle begin and end, what a phase does to the picture, and which option switches
turn which part of it off. Nothing here is implemented yet — this page is the evidence
base for the clock (#95), the rendered lighting (#96) and the switches (#97) under #87.

The tick itself is [movement.md](movement.md); the light masks that a phase lights up
are `LIGHTS.N` and `LIGHTS.RES` ([formats/map.md](formats/map.md),
[formats/lights-res.md](formats/lights-res.md)).

## Sources and markers

**Confirmed** means we checked it ourselves: either a byte-level read of our own copy of
the game (both editions and the tutorial binary), or a render. Where the fact also has a
line in the read-only clone, the permalink is given, pinned to
[`cf5a332`](https://github.com/Marisa-Chan/Slavik/tree/cf5a3320dc93859e3dc51f37ee823fed30f399b6).
A statement resting on the clone alone is marked **by Slavik** and is not our fact. The
clone's code is never copied — it is cited by file and line and described in our words.

Byte-level reads were made against `original/k1/rus/game/konung.res`,
`original/k1/eng/game/KONUNG.EXE` and `original/k1/eng/game/TUTORIAL.EXE`. Offsets are
file offsets; `VA` is the virtual address from that build's own PE section table
(Watcom: `BEGTEXT` at `0x410000`, `DGROUP` at `0x450000`). The three binaries are built
separately, so every address below is per-build and was read out of the build itself
rather than reused across builds.

## Confirmed by our own read of the game

### The day counter and the length of a cycle

There is one counter, and the hour is a remainder of it:

| Fact | Evidence |
|---|---|
| The counter is a single global int32 | `VA 0x81D640` in both editions; read twice at the modulo site (dividend and sign fix-up) |
| The cycle length is 21600 and it is used as the **divisor** of the counter | the immediate `0x5460` is loaded into `ebx` and used by a signed `idiv`; only the remainder is kept. One occurrence in each binary: rus `0x193F7` (`VA 0x428FF7`), eng `0x1956F` (`VA 0x42916F`), tutorial `0x1956F` |
| The remainder is cached in one global | `VA 0x81D608`, written at exactly one site in each binary, in the same routine as the modulo |
| The counter is advanced at two sites, never guarded by a lighting option | `inc dword [0x81D640]` at rus `0x26AE5`/`0x26B19`, eng `0x26D29`/`0x26D5D`. The second sits in a loop bounded by a separate global (`0x81D634`, the game speed); neither is inside a branch on either option global listed below |
| A per-day coin flip decides one of two night palettes | immediately after the modulo: `call rand`, `mov ebx, 100`, signed `idiv`, `cmp edx, 0x5A` (90) — that is a 90-in-100 chance, taken when the remainder is 0 |

So a cycle is 21600 ticks. At the 60 ms tick that is 21.6 minutes **at game speed 0**;
each speed level adds one more counter step per frame, so the cycle shortens to
`21600 / (1 + speed)` frames of 60 ms (speed 0..5, **by Slavik** `game.h:64`,
`game.h:1487`, [`game_play.cpp:122-133`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L122-L133),
[`game_input.cpp:183-190`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_input.cpp#L183-L190)).
The two increment sites and the loop bound are ours; the cap of 5 is the clone's.

### The phase boundaries

Thirteen thresholds, stored as one contiguous array of **signed int16**, identical in all
three binaries:

```
0, 1800, 1830, 3570, 3600, 5400, 8100, 11700, 11880, 11910, 16155, 16200, 18900
```

| Build | array file offset | array VA |
|---|---|---|
| rus `konung.res` | `0x4100E` | `0x45AE0E` |
| eng `KONUNG.EXE` | `0x42246` | `0x45AE46` |
| eng `TUTORIAL.EXE` | `0x41652` | `0x45AE52` |

The values are strictly increasing, they fit int16, and the array is not the clone's
shaping: the clone holds the same thirteen numbers as int32
([`game_data.cpp:25`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_data.cpp#L25)),
which is how a decompiler normalises a table it read as dwords.

The array is bound to the light code, which is what makes it a phase table rather than a
coincidence. In the routine that computes the remainder, the very next instructions are:

- `cmp [0x81D5EC], 0` then `je` — the day/night option gate;
- `mov eax, [<array>]`, `sar eax, 16` — the high half of the first dword, i.e. threshold
  index 1 = 1800, compared against the cached remainder;
- `sub [remainder], 1800` and `cmp …, 0x18` (24) — the ramp window of the first phase.

That is the branch structure of `CalculateLight`
([`game.cpp:1763-1893`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1763-L1893))
read out of the original, not inferred from the clone.

### What the two option switches gate

The options live at `VA 0x81D5EC` (day/night) and `VA 0x81D5DC` (the constant-light flag
of the current map, written when a map is loaded). Both default to on, together with a
third option for light effects (**by Slavik**
[`game.h:1461-1466`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.h#L1461-L1466)).

The day/night option gates **the lighting computation only**: the gate is a test of that
one global immediately before the phase comparison, and its failure jumps to the zero
result. The counter is nowhere near that branch — its two increment sites are about 55 KB
away in the tick loop, and every other reference to the option is a write from the saved
options or a read in an unrelated routine. This is the wording #97 builds on:
**the lighting computation is gated, the day counter keeps running**.

### Maps with a constant light

The routine that reads the phases can return before ever reaching them, and the value it
returns comes from a per-map array: the code loads the current map number, shifts it by
two and indexes an array of int32 — one packed triple of signed bytes per map, low byte
first.

| Build | array file offset | array VA |
|---|---|---|
| rus `konung.res` | `0x40EE0` | `0x45ACE0` |
| eng `KONUNG.EXE` | `0x42118` | `0x45AD18` |
| eng `TUTORIAL.EXE` | `0x41524` | `0x45AD24` |

In all three builds the 56 slots are the same: `(-1,-1,-1)` everywhere except

| Map | red | green | blue |
|---|---|---|---|
| 42 | -70 | 0 | 0 |
| 44 | -20 | -1 | -70 |
| 45 | 0 | -50 | -50 |

which agrees with the clone's table
([`game_data.cpp:14-24`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_data.cpp#L14-L24))
once the channel order is undone: the game packs blue first, the clone names the triple
red first. A triple of `-1,-1,-1` is read as "no constant light for this map" — the
hypothesis, and the one thing about this array we did not confirm is the test that
decides whether a map is a constant-light map at all: that is the per-map flag at
`VA 0x81D5DC`, written at map load, whose table we have not located. So the colours are
ours; the **list** of such maps (the clone adds 43 and 46..50 with the sentinel value)
stays **by Slavik**.

### A unit of 1440 ticks

A signed division of a counter difference by 1440 exists in both editions (rus `0x21C76`,
eng `0x21EA6`: `mov ebx, 0x5A0` then `idiv`), which matches the village timers
(**by Slavik**
[`game_quest.cpp:711`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_quest.cpp#L711)).
21600 / 1440 = 15, so 1440 is not an hour of a 24-hour day. What the unit means is
unknown, and nothing here depends on it.

## What a phase does to the picture

The thresholds cut the cycle into thirteen windows. For a remainder `d` inside a window,
`t` is the distance from the window's start and `u = t / 2` (integer division). The
values below are the colour **before** it is divided by 100 on its way to the shader
(**by Slavik** `game.cpp:1763-1893` for the table, our read for the windows and their
lengths, which follow from the confirmed thresholds above).

| # | window | ticks | tile light on | ambient (r, g, b) |
|---|---|---|---|---|
| 1 | 0 – 1800 | 1800 | no | 0, 0, 0 |
| 2 | 1800 – 1830 | 30 | no | `t < 25`: u, u/2, 0 · else 12, 6, 0 |
| 3 | 1830 – 3570 | 1740 | no | 12, 6, 0 |
| 4 | 3570 – 3600 | 30 | no | `t < 25`: (24−t)/2, (24−t)/4, 0 · else 0, 0, 0 |
| 5 | 3600 – 5400 | 1800 | no | 0, 0, 0 |
| 6 | 5400 – 8100 | 2700 | no | `t < 101`: 0.2·u, −0.4·u, −u · else 10, −20, −50 |
| 7 | 8100 – 11700 | 3600 | **yes** | `t < 61`: 10 − 0.33·u, −20 − u, −50 − 0.66·u · else 0, −50, −70 |
| 8 | 11700 – 11880 | 180 | **yes** | `t < 121`: −u, −50 − u/3, −70 · else −60, −70, −70 |
| 9 | 11880 – 11910 | 30 | **yes** | no coin flip: −60, −70, −70 · `t < 41`: −60, −70 + t/2, −70 · else −60, −50, −70 |
| 10 | 11910 – 16155 | 4245 | **yes** | coin flip: −60, −50, −70 · else −60, −70, −70 |
| 11 | 16155 – 16200 | 45 | **yes** | no coin flip: −60, −70, −70 · `t < 41`: −60, −50 − t/2, −70 · else −60, −70, −70 |
| 12 | 16200 – 18900 | 2700 | **yes** | `t < 101`: 0.5988·u − 60, u/5 − 70, u − 70 · else −30, −60, −20 |
| 13 | 18900 – 21600 | 2700 | no | `t < 121`: u/2 − 30, u − 60, u/3 − 20 · else 0, 0, 0 |

Two things follow that a four-phase sketch would miss.

A phase is not a discrete step. Nine of the thirteen windows open with a ramp of 25, 61,
101 or 121 ticks — 1.5, 3.7, 6.1 and 7.3 seconds at speed 0 — and hold a constant value
afterwards. Transitions are interpolated, but only inside those windows; between them a
new window changes the value in one tick.

Neutral is `(0,0,0)`, and it appears **three** times: windows 1, 5 and the tail of 13.
A bright stretch in the middle of the cycle (window 5) sits between the warm window and
the dark one, so the cycle is not "bright half, dark half".

### How the ambient value reaches the pixels

The ambient colour is a shader uniform, and a channel's sign selects the operation
(**by Slavik**
[`gfx.cpp:24-84`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/gfx.cpp#L24-L84)):

- a **negative** channel multiplies: `c *= 1 + clamp(blend + light, −1, 0)` — darkening,
  partially cancelled by the tile's light;
- a **positive** channel blends toward white: `c += (1 − c) · blend` — the warm glow of
  window 3;
- the blue channel darkens **without** the light cancellation, so a lit tile stays dark
  blue at night.

Three surfaces are treated differently, and this is the part #96 has to reproduce:

| Surface | when it is darkened | source |
|---|---|---|
| ground and its upper layer | only while the tile-light window is open **and** the light-effects option is on — otherwise it is drawn with no blend at all | **by Slavik** [`game.cpp:949-1013`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L949-L1013) |
| characters and dynamic objects | per creature, from the fine block of the map's `LIGHTS.N`: a creature in a cell whose fine value is 0 is blended toward the ambient, one in a lit cell is not | **by Slavik** [`game.cpp:1089-1100`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1089-L1100), [`game.cpp:1611-1626`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1611-L1626); our own read of the fine block in [formats/map.md](formats/map.md#lightsn) |
| the tile light itself | the mask named by `LIGHTS.N` for that tile is sampled for brightness; where it is bright it adds the light colour and cancels the darkening | **by Slavik** [`game.cpp:997`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L997), [`gfx.cpp:55-84`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/gfx.cpp#L55-L84) |

The tile-light window is windows 7 to 12 — the same span as the **yes** column above —
and the light colour it adds is itself random per draw, `16..23, 8..11, 0` before the
division, halved for the ground pass
(**by Slavik** [`game.cpp:925`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L925),
[`game.cpp:979`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L979)).
A per-draw random is a frame-rate dependency; we will not reproduce it (see below).

### The two options are not one switch

There are two independent switches, and a third state that is not a switch at all:

| | gates | source |
|---|---|---|
| day/night | the whole phase computation: off means the ambient result is `(0,0,0)` and the counter is untouched | our own read (the gate) + **by Slavik** [`game.cpp:1776`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1776) |
| light effects | the ground's tile light and its ambient blend: with it off the ground is drawn unblended, whatever the phase | **by Slavik** [`game.cpp:949`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L949) |
| the tile-light window | a property of the phase, not of a switch: outside windows 7..12 there is no tile light to draw | the table above |

So "no tile light" has two different causes — the phase and the option — and #97 needs
both switches; one control for both would misrepresent the original.

## Phase names and preset ticks

The game has thirteen thresholds, not four named phases. The names below are **ours**, a
reading aid over the table, and the preset ticks are **our choice** inside the windows —
#96 may move them, the windows themselves are the fact.

| Our name | cycle span | preset | why there |
|---|---|---|---|
| day | 0 – 1800 | 900 | neutral, before the warm window |
| dusk | 1800 – 3600 | 2700 | the middle of the warm plateau of window 3 |
| night | 8100 – 18900 | 11000 | past the window-8 ramp, deep in the dark span |
| dawn | 18900 – ~19021 | 18960 | inside the 121-tick ramp back to neutral |

## Hypotheses

- **A new game starts the cycle at 0.** The counter is initialised to 0 and no reset at
  map load was found (**by Slavik**
  [`game.cpp:2426`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L2426)).
  Our own check stops short of it: we confirmed the counter has exactly two increment
  sites, not that nothing ever writes it.
- **The hour is not saved.** The clone's save and load code does not mention a counter;
  we did not test it against a written save. A loaded game would then begin its cycle at
  0 regardless of when it was saved.
- **A `(-1,-1,-1)` slot means "this map has no constant light".** It is the only reading
  consistent with the three non-sentinel entries being the three maps the clone lists.
- **1440 ticks is one unit of village business**, not an hour.

## Unknown, and what does not rest on it

- **What the per-day coin flip is for.** It selects between `(−60,−70,−70)` and
  `(−60,−50,−70)` in windows 9 to 11. The clone calls the variable a light effect; whether
  the original meant lightning, moonlight or nothing visible is not established. Night
  lighting may be built on the two palettes as data, but no cause may be claimed for it.
- **Where in the cycle a load begins**, beyond the hypothesis above.
- **Which maps the game treats as constant-light maps**, as distinct from which slots
  carry a colour: the deciding flag's table is not located.
- **Whether every map really uses the same thresholds.** The array is global and static,
  so the same thirteen numbers serve all maps, but a map can bypass them through the
  constant-light array — and we have not ruled out a per-map override elsewhere.
- **Night-time behaviour of ordinary NPCs.** Unknown in part 1 (the cells in
  [games.md](../games.md) stay `?`), out of scope for #87, and nothing on this page
  depends on it.

## Where we will differ from the original

Deliberate, recorded next to the facts they bend:

- **One tick per game tick, no speed multiplier.** The original adds `1 + speed` counter
  steps per frame, which makes the length of a day depend on a control that has nothing
  to do with the world. Our day is 21600 ticks of the single scene clock, 21.6 minutes at
  the 60 ms tick; a modern game speed, when it comes, will change how fast the clock
  advances and never the cycle.
- **No per-draw random.** The light colour `16..23, 8..11, 0` is re-rolled every draw, so
  it is a function of frame rate. A flicker we keep must become a function of the tick.
- **Deterministic night variants.** If the coin flip is implemented, it is drawn from the
  simulation's seeded generator against the day index, not from an unseeded random, so a
  replayed session shows the same night.

## What the next tickets take from this page

- #95 takes the cycle length, the two increment sites and the wording "the lighting
  computation is gated, the day counter keeps running" — and the game-time value must
  come from the one simulation tick position the viewer already keeps, not from a second
  clock.
- #96 takes the window table, the per-channel blend rules, the three surfaces treated
  differently, the constant-light array and the preset ticks.
- #97 takes the two-option table.
