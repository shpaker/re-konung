# Time of day and light phases

What the game does with the hour: how a day counter is kept, where the light phases of
the cycle begin and end, what a phase does to the picture, and which option switches
turn which part of it off. **Status:** the counter, the cycle, the thirteen windows, the
ambient arithmetic and the gates are **confirmed** against our own byte-level read of all
three binaries; the constant-light map list, the light-effects option and everything the
coin flip means stay **hypothesis** or **by Slavik**, and are named as such where they
appear. This page is the evidence base for the clock (#95), the rendered lighting (#96)
and the switches (#97) under #87.

The tick itself is [movement.md](movement.md); the light masks that a phase lights up
are `LIGHTS.N` and `LIGHTS.RES` ([formats/map.md](formats/map.md),
[formats/lights-res.md](formats/lights-res.md)).

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

## Confirmed by our own read of the game

### The day counter and the length of a cycle

There is one counter, and the hour is a remainder of it:

| Fact | Evidence |
|---|---|
| The counter is a single global int32 at `VA 0x81D640`, identical in all three builds | read at the head of the light routine (rus `0x193FB` / `VA 0x428FFB`), incremented, zeroed, serialized and gated elsewhere; the same absolute address in every build |
| The cycle length is 21600 and it is used as the **divisor** of the counter | `mov ebx, 0x5460` feeding a signed `idiv`; only the remainder is kept, and it lives in the routine's stack local `[ebp-4]` — there is no cached-remainder global (rus `0x193F6` / `VA 0x428FF6`) |
| The counter is the game's general running tick, not only the hour | its reference census besides the light routine: two increments, the serialization sites, the zeroing site, and readers gating periodic logic on its low bits — `test byte [0x81D640], 0xF` at rus `0xD456` (`VA 0x41D056`) takes a 200-iteration pass every 16th tick, and `(counter & mask) == mask` sits at rus `0x2B2D` (`VA 0x41272D`). The clone reads the same variable outside the light code — a loot condition and a two-frame alternation (**by Slavik** [`game.cpp:861`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L861), [`game.cpp:1554`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1554)) |
| The counter is advanced at two sites, never guarded by a lighting option | `inc dword [0x81D640]` (`ff05 40d68100`) at rus `0x26AE5` / `VA 0x4366E5` and rus `0x26B19` / `VA 0x436719`; eng `0x26D29` / `0x26D5D`; tutorial `0x26C39` / `0x26C6D` — the pattern occurs exactly twice in each build. The second sits in a loop bounded by a separate global (`0x81D634`, the game speed); neither is inside a branch on any option global |
| A per-day coin flip decides one of two night palettes | immediately after the modulo: `call rand`, `mov ebx, 100`, signed `idiv`, `cmp edx, 0x5A` (90) — a 90-in-100 chance taken when the remainder is 0 — the result normalized into `[ebp-0xC]` and stored to `[0x81D608]` (rus `0x1943F` / `VA 0x42903F`). That global is written nowhere else and read only in windows 9–11 (rus `0x19722`/`0x1978C`/`0x197BA`, eng and tutorial `0x1989A`/`0x19904`/`0x19932`): it is the clone's `_bLightEff`, a per-day computed flag, not a user option |
| A map-activation routine zeroes the counter | `mov [0x81D640], esi(=0)`, where `esi` is a freshly zeroed `[0x81D710]`, inside the routine that memsets the two per-map buffers (`[0x81cf80]` 0x38 bytes, `[0x81a814]` 0x28 bytes), copies the 64-dword map record (`rep movsd`, `ecx=0x40`, `edi=[0x81D45C]`) and sets the current-map base: rus file `0x281EB` / `VA 0x437DEB`, eng `0x28434` / `VA 0x438034`, tutorial `0x2781E` / `VA 0x43781E`. What is confirmed is the context — the store fires whenever a map record is activated; which moments dispatch there is not (the family is jump-table entered, so caller counts are not authoritative) |
| The counter travels with the session state, round-tripping through a field serializer | a 4-byte round trip: address-pass sites hand the counter's address and `push 4` to a write helper and, at a second site, to its read counterpart — rus write `0x14BB7` / `VA 0x4247B7` → `0x43EE9E`, read `0x15061` / `VA 0x424C61` → `0x43F186`; eng and tutorial write `0x14CDB` / `VA 0x4248DB`, read `0x15185` / `VA 0x424D85`, helpers shifted with the code drift (eng serializer side `0x43F0EE`). The helpers index a field table (`[0x831C80 + idx·4]`) and call stream helpers. Separately, a category-dispatched commit routine copies the counter into three consecutive slots (`+0x3FC`, `+0x400`, `+0x404`) of the record reached through `[0x81D580]` — rus `0x2488A`/`0x248BA`/`0x248EA`, eng and tutorial `0x24ABA`/`0x24AEA`/`0x24B1A`; each copy is guarded by a category byte (`[rec+0x3DA]`, `[rec+0x3E2]`, `[rec+0x3EA]`) |

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
coincidence: the routine reads it in the comparisons that select the branch whose value
the next section records, and the ramp constants of that arithmetic live in the same
routine. The ramp conditions are `≤`, not `<`: at rus `0x198C2` / `VA 0x4294C2` the tail
ramp of window 13 gates on `cmp …, 0x78 / jle` — the ramp formula still runs at distance
120 and produces the plateau value there, and the plateau branch begins at 121. That is
the one ramp condition we read byte for byte; the other ramps are taken to gate the same
way, which their clone counterparts (`< 25`, `< 61`, `< 101`, `< 121`) agree with.

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
| `[0x82D168]` | the **ground-light flag**, computed — the clone's `_bLightGround` | exactly three stores inside the routine: cleared at its top (rus `0x193EC`), set for windows 7–12 (rus `0x195FF` / `VA 0x4291FF`), cleared again in the last branch (rus `0x198AD` / `VA 0x4294AD`); eng and tutorial `0x19564`/`0x19777`/`0x19A25`. No absolute reader: the ground-draw path consumes it base-relative |
| `[0x81D608]` | the **per-day palette flag**, computed — the coin flip's target, the clone's `_bLightEff` | once, by the coin flip (above) | three comparisons in windows 9–11 only |

A fifth global, the **user's light-effects option** (the clone's `_bConfLightEffects`,
which the ground-draw guard reads next to `_bLightGround`:
[`game.cpp:918-1013`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L918-L1013)),
is real in the clone but **not pinned to an address by our read**: it is reached
base-relative and no absolute reference exposes it. It stays **by Slavik**.

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

### A unit of 1440 ticks

A signed division of a counter difference by 1440 exists in both editions (rus `0x21C76`,
eng `0x21EA6`: `mov ebx, 0x5A0` then `idiv`), which matches the village timers
(**by Slavik**
[`game_quest.cpp:711`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_quest.cpp#L711)).
21600 / 1440 = 15, so 1440 is not an hour of a 24-hour day. What the unit means is
unknown, and nothing here depends on it.

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
  [What the next tickets take](#what-the-next-tickets-take-from-this-page);
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

The tile-light window is windows 7 to 12 — the same span as the **yes** column above,
which is the three stores to `[0x82D168]` we read ourselves — and the light colour it
adds is itself random per draw, `16..23, 8..11, 0` before the division, halved for the
ground pass
(**by Slavik** [`game.cpp:925`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L925),
[`game.cpp:979`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L979)).
A per-draw random is a frame-rate dependency; we will not reproduce it (see below).

### The switches and the flags are not one control

There are two user switches and two computed states, and the routine consults a third
thing that is neither:

| | what it gates | kind | source |
|---|---|---|---|
| day/night (`[0x81D5EC]`) | the phase computation from the gate on: off means the ambient result is `(0,0,0)` — **except on constant-light maps**, which returned already | user option, data-driven default | our own read (gate, writers); default unconfirmed |
| light effects | the ground's tile light and its ambient blend: with it off the ground is drawn unblended, whatever the phase | user option | **by Slavik** [`game.cpp:949`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L949); our read pins no address for it |
| the ground-light flag (`[0x82D168]`) | whether the phase lights the ground at all: outside windows 7..12 there is no tile light to draw | computed from the phase | our own read (three stores) |
| the palette flag (`[0x81D608]`) | which night palette windows 9–11 return | computed per day (coin flip) | our own read (write and three reads) |
| the constant-light flag (`[0x81D5DC]`) | the whole phase computation, by returning early with the per-map colour | property of the current map | our own read (gate); its writer is not pinned |

So "no tile light" has two different causes — the phase and the option — and #97 needs
both switches; one control for both would misrepresent the original. And "day/night off"
does **not** mean "the picture is always `(0,0,0)`": the constant-light maps bypass the
gate.

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

The spans above are the **branch** spans, where the original's comparisons switch. The
**values** switch a tick or two aside, because the ramps halve as integers: the warm band
really begins at 1802, the ambient really returns to neutral at 3593 and at 19020 (the
tails of windows 4 and 13 carry `(0,0,0)` while their ramp branches are still running).
Which of the two a consumer should use is recorded with the model in
[What the next tickets take](#what-the-next-tickets-take-from-this-page).

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

### Camera presets (ours)

The presets below stay useful as places to stand a camera. They are named after the interval each
one stands in, not after the four phase words above: those words are display vocabulary of the
implementation, and a preset that borrowed one could contradict it the way "dusk at 09:00" would.
The presets are not boundaries and no test should rest on them.

| Preset (named for its interval) | tick | window | what is on screen there |
|---|---|---|---|
| light | 900 | 1 | neutral ambient, ground unlit — 07:00 on our clock |
| warm band | 2700 | 3 | the warm plateau, ground unlit — the middle of the warm band, 09:00 |
| ground-lit plateau | 11000 | 10 | the dark plateau with the tile light on, past the window-8 ramp |
| final ramp | 18960 | 13 | inside the 121-tick ramp back to neutral, ground unlit |

Where a display names phases of the day, it maps its words over the interval table; a phase word
is never an extra position in this page.

## Hypotheses

- **A new game starts the cycle at 0.** The clone zeroes the counter in its new-game path
  ([`game.cpp:2426`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L2426)),
  and our read shows a zeroing store — but inside the **map-activation** routine (rus
  `0x281EB` / `VA 0x437DEB`, ours), not an identified new-game one. Whether activation
  is exactly a new game or fires on every map load is the open half.
- **The hour travels with a saved game.** The counter is handed by address with size 4
  to the field-table serializer on both the write pass and the read pass, and a
  category-dispatched commit routine copies it into three record slots (the table
  above — ours). What is inferred is the purpose: that the serializer is the save
  writer and the reader the save loader. The clone's save code never names the counter.
- **A `(-1,-1,-1)` slot means "this map has no constant light".** It is the only reading
  consistent with the three non-sentinel entries being the three maps the clone lists;
  the decision itself is the `[0x81D5DC]` flag.
- **1440 ticks is one unit of village business**, not an hour.

## Unknown, and what does not rest on it

- **What the per-day coin flip is for.** It selects between `(−60,−70,−70)` and
  `(−60,−50,−70)` in windows 9 to 11. The clone calls the variable a light effect; whether
  the original meant lightning, moonlight or nothing visible is not established. Night
  lighting may be built on the two palettes as data, but no cause may be claimed for it.
- **Which maps the game treats as constant-light maps**, as distinct from which slots
  carry a colour: the deciding flag `[0x81D5DC]` is confirmed, its writer is not pinned,
  and the field it is copied from is not known.
- **The light-effects option's global.** The clone reads `_bConfLightEffects` in the
  ground-draw guard; our read cannot pin it to an address (base-relative). #97 may
  implement it and #95's model ignores it without contradiction; a byte-level check of
  the two switches still rests on finding it.
- **The defaults of the user options.** `.bss` zeros, populated from the settings table
  at `[0x831C5C]`; the data has not been read.
- **Which moments run the map-activation routine.** The zero store fires with it; a save
  load reaching it would make the serialization round trip moot.
- **What the windows mean as hours of a day.** Neither source names a phase, so which of them the
  authors thought of as evening or midnight is not recovered — see
  [Phases of the day are our vocabulary](#phases-of-the-day-are-our-vocabulary--unknown). Nothing
  here depends on it: every claim is about an interval of ticks.
- **Whether every map really uses the same thresholds.** The array is global and static,
  so the same thirteen numbers serve all maps, but a map can bypass them through the
  constant-light flag — and we have not ruled out a per-map override elsewhere.
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
- **The day is not re-zeroed by a map load.** The original's counter is zeroed when a map
  record is activated — every activation we can see the code for — while our time is one
  continuous function of the session's tick and a map change does not rewind it. A per-map
  restart of the hour is an artefact of the original's activation store; a world whose day
  jumps at every door is not what #87 asks for.
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
  computation is gated, the day counter keeps running" (now with evidence: the increments
  sit nowhere near the gates). Game time is the simulation's own tick
  (`Simulation::tick_number()`): the viewer derives everything from it and accumulates
  nothing, and the contract a test asserts is the **interval** trace — the twelve
  thresholds and the ground-light interval — because the four day names are ours, not the
  game's. Because the ramps halve as integers, the model's **observable** trace — what a
  player sees change — is `1802, 3593, 5402, 8100, 18900, 19020`: a ramp leaves the value
  it comes from on the tick whose integer half first turns non-zero, and window 13 carries
  `(0,0,0)` for the tick after its ramp ends. #95 follows the values, since that is what
  a screen shows; the branch spans of the interval table stay the contract for
  comparisons. The displayed hour is the linear 24-hour reading defined
  [above](#the-hour-we-display--our-convention-not-a-fact-about-k1).
- #96 takes the interval table, the window table with its exact ramp branches, the
  per-channel blend rules, the three surfaces treated differently, the constant-light
  array, the coin flip against the day index, and the two unresolved coefficients (the
  `0.33`/`1/3` pair and the rounding of the byte-packed channels) — the presets as places
  to stand the camera, never as boundaries.
- #97 takes the switches-and-flags table: both user switches, both computed flags, and
  the constant-light bypass of the day/night gate.
