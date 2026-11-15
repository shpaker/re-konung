# Creature decisions and orders

[Research and confidence](README.md). Animation-state details are in
[animation](animation.md); hit, death and target-selection gaps in [combat](combat.md).

## Order and presentation flags

Character byte 0x02 combines player orders and the need for a behavior decision.
The table is a **Slavik source reading**, with file census below.

| Bit | Meaning | Source |
|---|---|---|
| 0x01 | Attack | game_play.cpp:550,2489,3289; cleared 3335 |
| 0x02 | Follow leader | Set by game_input.cpp:395-397; cleared game_play.cpp:969,2994 |
| 0x04 | Shooting with bow/arrows | game_play.cpp:2421-2431,3918-3954; cleared 2953,3912. Usage confirmed as shooting, although the clone gives it no name. |
| 0x08 | Move | game_input.cpp:395 and game_play.cpp:1943 set 0x48; cleared 826,3550 |
| 0x20 | Player-squad follower | Nonleader, same group, no orders; game_play.cpp:302-303 |
| 0x40 | Needs behavior decision | Set at game_play.cpp:300,831,2458,3051,3149; cleared on decision at 1962,3949 |

The low nibble contains orders: `(field & 0xf) == 0` means none
(game_play.cpp:302,774; game_pathfind.cpp:37); `&= 0xf0` clears them
(game_play.cpp:220,269).

Byte 0x03:

| Bit | Meaning | Source |
|---|---|---|
| 0x04 | Weapons in hands; selects state pairs and equipment tables | game_character.cpp:167-169; without it use the last eight EXE slot-order rows, EqLookUp1 |
| 0x40 | Pure move, no attack/pickup on arrival | game_play.cpp:812-824 |
| 0x80 | Dynamic sprite not loaded | Set game.cpp:687,744; cleared with frame recompute at 1036-1042; while set image is treated as one pixel, game_character.cpp:157-160 |

**Our census:** byte 0x02 is 0x40 in 3176 records or 0x60 in three; byte 0x03
is 0x04 in 2547 or zero in 632. Order bits 0x01/02/04/08 and presentation bits
0x40/80 are absent in starting files; their runtime use is not a file default.

## Squad AI state — group byte 0x1D

**By Slavik:** loaded by game_state.cpp:230; roaming kinds 0x41–0x50 recompute
every 16 ticks (game_play.cpp:1623-1638, `_counter & 0xf == 0`). The scan at
1024-1062 returns 1 if a player-group member is inside the spawn rectangle,
0x20 if an own member is outside, otherwise zero. Village elders update on
the pass offset by seven (1696,1533-1561). Other groups retain the byte; the UI
reads only `unk4 & 1` for work/hints (game_character.cpp:755,757).

**Our census:** zero in three groups, 0x80 in 569. No observed reference-code
writer sets 0x80; it occurs in villager 0x80–0x82 and NPC 0x53 groups that are
not periodically recomputed. Its meaning is **unknown**; its low bit behaves
as zero. File state is a starting snapshot, not a persistent behavior setting.

### The villager decision (confirmed 2026-11-03)

Confirmed against the reference and implemented with tests
(`k1-game/src/rules.rs`, `villager_decision`; synthetic cases and both
editions of `Game.0`–`Game.2`, `k1-game/tests/original_rules.rs`). The game
calls the peaceful decision for members of a villager squad — group kind
0x80–0x82, dispatch at `Marisa-Chan/Slavik/game_play.cpp:737-757` — through
`FUN_00416934` (`game_play.cpp:2288-2385`), and only when the character needs
a decision: flag 0x40 of byte 0x02 is set on arrival and cleared once a
decision was made (`game_play.cpp:2834-2846`, `3477-3489`). A dead character
(class bit 0x80, state 3 or 9) and a walking one take no decision. The order
of the branches matters:

1. **Woman (class 0x51) and child (0x52)**: one roll of `rand() % 100`; below
   75 the creature stands, otherwise it walks (branch 5) — a quarter walks
   (`game_play.cpp:2294-2309`).
2. **Static work from bits 3–5 of the class**: `job = (class >> 3) & 7` names
   a village slot; a village has exactly five slots and index 0 is the elder
   (`game.h:702`, and our check: every village in the files carries five job
   records, `docs/research/k1/formats/game-state.md`). An empty slot
   (`Jobs[job].CharID == 0`) is passed over; the original also clears the work
   bits in memory (`game_play.cpp:2340`), we do not touch the record — the
   branch depends only on data that never changes mid-run, so the next tick
   takes the same path (our difference). A filled slot sends every *other*
   creature of the village to it (`game_play.cpp:2323-2338`): unless the
   creature stands on one of the eight neighbours of the place's cell (the
   direction lookup `FUN_0041165c`, `game_play.cpp:1973-1983`; standing on the
   place itself is not adjacency), it is sent to the first free neighbour of
   the place in direction order 0..7 (`FUN_00410ffc`,
   `game_play.cpp:1985-1992`); once adjacent, it faces the place's direction
   byte, and once facing — it stands. The branch never falls through.
   - **Our measurement**: no creature of the shipped starting data reaches
     this branch — of the villager classes 1, 2, 3, 5, 0x45, 0x51, 0x52 the
     first five carry zero work bits and 0x51/0x52 leave through the woman and
     child branch first. The branch is covered by synthetic tests only.
   - Bits naming slot 5, 6 or 7 read past the five-slot array in the original
     (`std::array<Job, 5>`); with us they are no static work either way (our
     difference).
   - When no neighbour of the place is free, `FUN_00410ffc` returns `-1`,
     which the original treats as success and walks the creature to an
     uninitialized point (`game_play.cpp:2327-2331`); we give no order (our
     difference).
   - The original's free-neighbour test checks terrain only; we also require
     that nobody stands there (our difference).
   - The original turns by one direction per tick (`FUN_004391bc`,
     `game_play.cpp:1995-2013`, listed under the deliberate-limit differences); our
     `Decision::Turn` faces the creature in one step.
3. **The dynamic workplace**: the village slot whose `CharID` equals the
   creature's record number — `GetVillageCharacterJob`
   (`game_village.cpp:522-536`) scans the five slots in order and returns the
   first match as `i + 1`. Standing on the place's cell: face the place's
   direction when the facing differs, else stand; anywhere else: move exactly
   onto the place's cell (`game_play.cpp:2354-2381`).
   - **Our measurement**: `Job.character` is the 0-based record number
     (the village of map 19 points at records 181–185, and group 19 starts at
     character 181), and `0` marks the empty slot — the very marker the rules
     test — so the player's hero, record 0, never owns an open slot. In the
     starting data every job holder stands exactly on its place's cell, in
     state 6, and already faces the place's direction: over 41 maps ×
     `Game.0`–`Game.2` of both editions the rules turn nobody
     (`k1-game/tests/original_rules.rs`), so the turn path is covered by the
     synthetic tests alone.
4. **No work at all**: one roll of `rand() % 100`; below 50 the creature
   stands — half walk (`game_play.cpp:2346-2348`).
5. **The walk**: a free cell strictly inside the squad's spawn area
   (`left < x < right`, `top < y < bottom`), up to ten attempts; no attempt
   lands free, or the area has no interior, and there is no order.
   - The original does not pick inside the area at all: it offsets from the
     character's own cell by at most `ViewTiles * 2`, and `ViewTiles` is the
     viewport size in tiles (`game_play.cpp:1889-1949`, `game.cpp:921-922`).
     Behavior must not depend on the window size, so we pick uniformly inside
     the spawn area, drawing `x` and then `y` per attempt (our difference).
6. The original's first decision for a villager with a weapon in hands is
   putting it away (bit 0x04 of byte 0x03 → state 6, `game_play.cpp:2312-2317`).
   That is an animation-state change, not a movement; the rules layer owns no
   animation state, and we do not implement it.

**Our measurement** of the village shape, both editions: one village per map,
15 villages on 15 distinct maps; `village.elder_group` is the squad slot, and
it is the map's only villager squad — every villager of a map is a member of
it (map 19 → squad slot 19, kind 0x80, 19 members). The earlier note that
among workplaces `character` is filled only for the elder is wrong: map 19
fills all five slots, map 14 only the elder.

### The group state of a roaming squad (confirmed 2026-11-03)

Confirmed and implemented with tests (`k1-game/src/rules.rs`, `group_states`;
synthetic cases and both editions). For a roaming squad (group kind 0x41–0x50,
`FUN_00412c30`, `game_play.cpp:1024-1062`):

1. any alive member of the player squad inside the squad's spawn area, bounds
   included (`game_play.cpp:1028-1042`) — state 1: combat mode; the game also
   writes the player squad's number into the squad record at the same moment
   (`game_play.cpp:1038`, byte 0x1A);
2. else any alive member of the squad itself standing outside the area —
   state 0x20: the strayed members return, «The monster decision» below;
3. else state 0, calm.

«Alive» is the original's own test: class bit 0x80 clear, state neither 3 nor
9. Every other squad — villagers, the player squad, lone NPCs — takes no
recomputed state and reads calm. The byte stored in the file (0x1D,
`ai_state`) is never read: the game zeroes it for roaming squads on map load
(`game.cpp:645`), and in the shipped files it is the unknown 0x80 for every
squad (Squad AI state above).

Deliberate differences from the original:

- the state is recomputed every tick, not every 16 (`game_play.cpp:1623-1638`
  — a cost cap of 1999, listed under the deliberate-limit differences);
- the original's "outside" test for its own members is tangled by `&&`/`||`
  precedence into `y < top` alone (`game_play.cpp:1050-1052`); we use the
  plain negation of the inclusive inside test;
- the elder squad's byte is set in the village pass from village flags and the
  player's weapons (`FUN_00416700`, `game_play.cpp:1516-1562`, `4451`), not
  from positions. Until the village rules are recovered, a villager squad is
  calm with us — our villagers never turn hostile; this is an open question,
  not confirmed behavior (see the unknown rules).

### The monster decision (the return is confirmed; combat is not 2026-11-03)

Confirmed and implemented for the return — `FUN_00412e00`
(`game_play.cpp:3156-3181`, `3286`, `3337-3392`), tested at the synthetic and
real-data level. The dispatch gives it to members of a roaming squad (group
kind bit 0x40, `game_play.cpp:737-745`); classes 0x51, 0x52 and 0x53 return
from the decision untouched (`game_play.cpp:3163-3164`). Dead and walking
characters take no decision, as with villagers. By the squad state:

- **Calm (0)**: stand. The original only resets the animation state
  (`game_play.cpp:3166-3177`).
- **Player inside (1)**: combat — target choice, attack and shooting are not
  reverse-engineered, so no order is given rather than one invented
  (`game_play.cpp:3179-3283`).
- **Strayed (0x20)**: a creature already inside the spawn area, bounds
  included, stands. Outside, it returns: take the centre
  `((left + right) / 2, (top + bottom) / 2)`, with `rh = bottom - top - 1`
  and `rw = right - left - 1`; one coin `rand() % 2` picks *subtract* (0) or
  *add* (1) for both axes together; then `rand() % (rh / 4)` to `y`, then
  `rand() % (rw / 4)` to `x`, each drawn only when its divisor reaches two
  (`game_play.cpp:3353-3375` — the draw order is the coin, then `y`, then
  `x`, and we keep it for reproducibility). From that point the area is
  scanned over at most `rh * rw` cells: the first cell with empty foot
  terrain becomes the target; otherwise one step east with wraps — past
  `right - 1` the row restarts at `left + 1`, past `bottom - 1` at `top + 1`
  (`game_play.cpp:3377-3391`). Nothing free — stand.
- Our difference: the original tests "inside the area" with `x >= r.top`
  where `r.left` is meant — the reference itself marks it as an error
  (`game_play.cpp:3339`); we use `r.left`. The original's scan checks terrain
  only; our `Ground` also requires that nobody stands in the cell.

**Our measurement**, both editions: the cells of roaming members in the file
are not where the game puts them — the map load re-places the squad ([load placement](placement.md#roaming-squad-placement-on-map-load)) — so recomputing from the file cells flags
`MemberOutside` for thousands of squad members (2 123 across the runs of
`original_rules.rs` per edition), and every return order the rules give lands
strictly inside the squad's spawn area and on a free cell; the test asserts
that on all 41 maps per edition that carry creatures.

## Creature names

Monster species uses `(class & 0x7F) − 0x41` in the EXE table; the first 20
species names are filled. Our ENG census found a nonempty name for each placed
monster. Humans, women and children use bytes 0xD4/0xD5, name plus optional
space/nickname; zero nickname means name only. **Our census:** every Game.0–2
has one unnamed class-0x51 villager (both indices zero), group 31, map 37.
This is recorded data, not a parsing error. [EXE tables](../formats/executable.md#character-names).

## Hypotheses and unknown rules

- Target group byte 0x1A may couple to AI state: game_play.cpp:1038 writes
  the player group into unk3 alongside unk4=1. Later reads remain unchecked.
- The leash rectangle (group 0x0A–0x16) appears to limit pursuit: squad-state
  scan uses spwnTlRect (1026), but a walking attacker's keep/give-up test
  drops an enemy outside someRect (3088-3105). **By Slavik**, not confirmed
  against original behavior; target choice is [combat research](combat.md).
- Weapon slot byte 0x12 cycles through 0,1,2 on attacks and is randomized at
  spawn (game_play.cpp:3521-3531); the meaning of 0xFF is unverified.
- Village hostility depends on elder flags and player weapons
  (FUN_00416700, game_play.cpp:1516-1562), not yet recovered. Our villagers
  remain calm; that is an explicit unknown rather than original behavior.
- Cadence of expensive decisions: SkipBecauseOfHardCompute,
  game_play.cpp:770,3485. Meaning of group AI state 0x80 and low kind bits
  beyond the [known village sides](../formats/game-state.md#group--32-bytes).
- Healer/smith/commander/tribute schedules: record timers exist, rules unknown.

## Deliberate differences from old technical limits

We do not reproduce 32-step routes, one-direction-per-tick turning
(game_play.cpp:3511-3515), one expensive decision per character/tick with a
single-character-per-frame queue (HardComputeUnit,770-781), 16-tick group-state
updates or walking skip-mask scheduling (all shipped masks zero). The recovered
rules stay; their old computation budgets do not. See
[product principles](../../../development/design.md) and
[simulation decisions](../../../development/rust/simulation.md).
