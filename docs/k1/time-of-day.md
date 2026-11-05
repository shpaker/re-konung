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
| The counter is the game's general running tick, not only the hour | 18 references to `VA 0x81D640` in each of rus and eng (20 in the tutorial). Besides the modulo site, readers gate periodic logic on its low bits: `test byte [0x81D640], 0xF` at rus `0xD456` (`VA 0x41D056`) takes a 200-iteration pass every 16th tick, and `(counter & mask) == mask` sits at rus `0x2B2D` (`VA 0x41272D`). The clone reads the same variable outside the light code — a loot condition and a two-frame alternation (**by Slavik** [`game.cpp:861`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L861), [`game.cpp:1554`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1554)) |
| A state-init routine sets it to zero | `mov [0x81D710], 0` then `mov esi, [0x81D710]` then `mov [0x81D640], esi` in the routine that also clears and copies the other session globals, rus `0x281DB` (`VA 0x437DEB` for the store). Whether this routine runs on a load is not established |
| It travels with the session state | three sites copy it into three consecutive slots (`+0x3FC`, `+0x400`, `+0x404`) of the record reached through `[0x81D580]` — rus `0x2488B`, `0x248BB`, `0x248EB`; and its address is passed with size 4 to the routine every other state field goes through (rus `0x14BB8`, `0x15062`, routine at `VA 0x43EE9E`, which indexes a field table and calls a stream helper). The reads are ours; that this routine is the save writer is a hypothesis below |

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

## What the source gives: intervals, not day phases

The table above is the whole structure either source offers: thirteen windows of one cycle, one
ambient triple each, and a flag for whether the ground is lit. Three intervals follow from it, and
they are what the confirmed facts are about:

| Interval | cycle span | what is true in it |
|---|---|---|
| light span | 19021 → 5400 (through 0) | the ambient is neutral `(0,0,0)` except in the warm band, and the ground is never lit |
| warm band | 1800 → 3600 | inside the light span: positive red and green blend the picture toward a warm tint |
| dark span | 5400 → 19021 | the ambient is non-neutral throughout, with a ramp at each end |
| ground-light interval | 8100 → 18900 | strictly **inside** the dark span: the tile light is computed and the ground is faded |

Two consequences a four-phase sketch hides. The ground-light interval is narrower than the dark
span — the picture starts dimming at 5400, three hours of our clock before the tile lights come on,
and they go off at 18900 while the ambient is still ramping back. And the dark span is longer than
the light one: 13 621 ticks against 7 979, so the cycle is not half and half.

### Phases of the day are our vocabulary — unknown

Neither source names a phase. In the original the routine is a chain of numeric comparisons against
the threshold array; the clone names nothing but the table and the two flags — `_lightPhazes`,
`_bLightGround`, `_bLightEff`
([`game.cpp:1762-1893`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1762-L1893)).
There is no morning, noon, evening, dusk, night or dawn in the light code, and the clone's other
reads of the counter name nothing either.

So:

- the confirmed object is an **interval of the cycle** — a `tick % 21600` range with its ambient
  triple and its ground-light flag;
- **day / dusk / night / dawn are our words**. Any assignment of windows to them is a convention of
  ours and must be shown as one;
- in particular, a four-part trace **day → dusk → night → dawn is not a confirmed property of the
  cycle**. The windows do not fall into four consecutive named spans: the neutral `(0,0,0)` returns
  three times (window 1, the tail of 4 plus 5, and the tail of 13), and the warm band sits inside a
  light span rather than at its edge. A test or a UI that needs a trace asserts the **interval**
  trace — the twelve thresholds, the two ramp lengths at each end of the dark span and the
  ground-light interval — and not four names changing in a fixed order.

The four presets below stay useful as places to stand a camera; they are not boundaries and no test
should rest on their names.

### The hour we display — our convention, not a fact about K1

AC2 of #87 asks for a clock reading, and the original shows no confirmed one, so the mapping is
ours and is recorded here as such:

- **21600 ticks of one cycle stand for 24:00**, so 15 ticks to a minute and 900 to an hour;
- **cycle tick 0 reads 06:00**, which makes window 1 a morning rather than a midnight;
- the hour is a **pure function of the simulation tick** — the viewer keeps no time of its own and
  accumulates nothing;
- **nothing in the phase or lighting logic reads the displayed hour.** Phases and light are
  computed from ticks and the interval table above; the hour is a label on the same tick.

Under that offset the intervals land at: light 03:08 → 12:00, warm band 08:00 → 10:00, dimming from
12:00, ground lights 15:00 → 03:00, neutral again at 03:08. The night is longer than the day
because the dark span is longer, which is the data, not the mapping; another offset would move the
same asymmetry, not remove it. This hour is unrelated to the 1440 divisor in the code
([below](#a-unit-of-1440-ticks)).

### Display presets (ours)

| Our name | preset tick | window | what is on screen there |
|---|---|---|---|
| day | 900 | 1 | neutral ambient, ground unlit — 07:00 on our clock |
| dusk | 2700 | 3 | the warm plateau, ground unlit — the middle of the warm band |
| night | 11000 | 10 | the dark plateau with the ground lit, past the window-8 ramp |
| dawn | 18960 | 13 | inside the 121-tick ramp back to neutral, ground unlit |


## Hypotheses

- **A new game starts the cycle at 0.** A state-init routine really does store 0 into the counter
  (rus `0x281DB`, `VA 0x437DEB` — ours), and the clone shows the same at
  [`game.cpp:2426`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L2426).
  What is not established is which moments run that routine: we did not check that a map load or a
  save load reaches it.
- **The hour travels with a saved game.** Three sites copy the counter into consecutive slots of a
  record and its address is handed to the routine other state fields are serialized through (the
  table above — ours). The clone's save code never names the counter, so we keep this a hypothesis:
  deciding it needs a written save, which we have not parsed.
- **A `(-1,-1,-1)` slot means "this map has no constant light".** It is the only reading
  consistent with the three non-sentinel entries being the three maps the clone lists.
- **1440 ticks is one unit of village business**, not an hour.

## Unknown, and what does not rest on it

- **What the per-day coin flip is for.** It selects between `(−60,−70,−70)` and
  `(−60,−50,−70)` in windows 9 to 11. The clone calls the variable a light effect; whether
  the original meant lightning, moonlight or nothing visible is not established. Night
  lighting may be built on the two palettes as data, but no cause may be claimed for it.
- **What the windows mean as hours of a day.** Neither source names a phase, so which of them the
  authors thought of as evening or midnight is not recovered — see
  [Phases of the day are our vocabulary](#phases-of-the-day-are-our-vocabulary--unknown). Nothing
  here depends on it: every claim is about an interval of ticks.
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
  to do with the world. Our day is 21600 ticks of the simulation's own tick — the counter
  `Simulation::tick` advances, not a second clock; 21.6 minutes at the 60 ms tick. A modern
  game speed, when it comes, will change how fast that tick advances and never the cycle.
- **A clock face the original may not have.** The original's interface shows no confirmed hour,
  so the HH:MM the viewer shows is our display convention over the tick, and it stays a label:
  no rule reads it.
- **No per-draw random.** The light colour `16..23, 8..11, 0` is re-rolled every draw, so
  it is a function of frame rate. A flicker we keep must become a function of the tick.
- **Deterministic night variants.** If the coin flip is implemented, it is drawn from the
  simulation's seeded generator against the day index, not from an unseeded random, so a
  replayed session shows the same night.

## What the next tickets take from this page

- #95 takes the cycle length, the two increment sites and the wording "the lighting
  computation is gated, the day counter keeps running". Game time is the simulation's own tick
  (`Simulation::tick_number()`): the viewer derives everything from it and accumulates nothing,
  and the contract a test asserts is the **interval** trace — the twelve thresholds and the
  ground-light interval — because the four day names are ours, not the game's. The displayed hour
  is the linear 24-hour reading defined
  [above](#the-hour-we-display--our-convention-not-a-fact-about-k1).
- #96 takes the interval table, the window table, the per-channel blend rules, the three surfaces
  treated differently, the constant-light array and the preset ticks — as places to stand the
  camera, never as boundaries.
- #97 takes the two-option table.
