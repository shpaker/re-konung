# Game time

The counter, cycle and thresholds below are confirmed by our own byte-level
reads of RUS, ENG and the tutorial executable. Color arithmetic and option
gates are in [lighting](lighting.md); runtime choices are in
[simulation](../../development/rust/simulation.md#time-and-lighting-contracts).
The tick itself is documented in [movement](movement.md).

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
| A per-day coin flip decides one of two night palettes | immediately after the modulo: `call rand`, `mov ebx, 100`, signed `idiv`, `cmp edx, 0x5A` (90) — a 90-in-100 chance taken when the remainder is 0 — the result normalized into `[ebp-0xC]` and stored to `[0x81D608]` (rus `0x1943F` / `VA 0x42903F`). That global is written nowhere else and read only in windows 9–11 (rus `0x19722`/`0x1978C`/`0x197BA`, eng and tutorial `0x1989A`/`0x19904`/`0x19932`): it is the clone's `_bLightEff`, a per-day computed flag, not a user option; see [lighting globals](lighting.md#the-four-globals-separated) |
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
the [lighting page](lighting.md#what-a-phase-does-to-the-picture) records, and the ramp constants of that arithmetic live in the same
routine. The ramp conditions are `≤`, not `<`: at rus `0x198C2` / `VA 0x4294C2` the tail
ramp of window 13 gates on `cmp …, 0x78 / jle` — the ramp formula still runs at distance
120 and produces the plateau value there, and the plateau branch begins at 121. That is
the one ramp condition we read byte for byte; the other ramps are taken to gate the same
way, which their clone counterparts (`< 25`, `< 61`, `< 101`, `< 121`) agree with.

### A unit of 1440 ticks

A signed division of a counter difference by 1440 exists in both editions (rus `0x21C76`,
eng `0x21EA6`: `mov ebx, 0x5A0` then `idiv`), which matches the village timers
(**by Slavik**
[`game_quest.cpp:711`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_quest.cpp#L711)).
21600 / 1440 = 15, so 1440 is not an hour of a 24-hour day. What the unit means is
unknown, and nothing here depends on it.

## What the source gives: intervals, not day phases

The [lighting window table](lighting.md#what-a-phase-does-to-the-picture) is the whole structure either source offers: thirteen windows of one cycle, one
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
[time contracts](../../development/rust/simulation.md#time-and-lighting-contracts).

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

## Hypotheses and unknowns

- A new game starts at cycle zero: [Slavik game.cpp:2426](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L2426) zeroes its counter,
  but our confirmed zero store (RUS 0x281EB / VA 0x437DEB) is inside map
  activation, whose callers and exact lifecycle moments are not pinned.
- The hour travels with saved games: the four-byte field-serializer round-trip
  and three category record slots are confirmed; their purpose as actual save
  writer/loader remains an inference. Slavik's save code does not name it.
- 1440 ticks may be a village-business unit; it cannot be a 24-hour clock hour.
- Original phase names and hour labels are unknown; only numeric intervals
  are confirmed. Ordinary NPC night behavior in K1 remains unknown in the
  [comparison](../comparison.md#requires-verification).
- The threshold array is global/static, but constant-light maps can bypass it;
  other per-map overrides have not been ruled out. Map-activation resets could
  also interact with save loading; dispatch moments are unknown.

## Deliberate runtime differences

Our day follows the simulation's own tick, 21600 ticks / 21.6 minutes at 60 ms,
without copying the original frame-speed multiplier. A future game transition
must preserve session time; switching a viewer map opens an independent scene
and starts a fresh simulation. The displayed HH:MM and phase words are our
conventions, not recovered game rules. Details:
[time contracts](../../development/rust/simulation.md#time-and-lighting-contracts)
and [viewer clock](../../development/rust/viewer.md#the-hour-we-display--our-convention-not-a-fact-about-k1).
