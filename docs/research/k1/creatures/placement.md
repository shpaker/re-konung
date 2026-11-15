# Creature placement and load-time observations

[Research and confidence](README.md). Implementation and refusal-report contracts:
[simulation](../../../development/rust/simulation.md).

## Roaming squad placement on map load

Confirmed 2026-11-04.

Confirmed against the reference and implemented with tests
(`k1-game/src/placement.rs`, `place_roaming`; synthetic cases in the module
and both editions in `k1-game/tests/original_rules.rs`,
`roaming_squads_are_re_placed_on_free_cells_inside_their_areas`). On map load
the game does not leave a roaming squad (kind 0x41–0x50) at the cells its
records carry. Squads are processed in `MapChar_ARRAY` order — the group
table as the file holds it, group slots ascending (`game.cpp:637`,
`game_state.cpp:364-368`) — and the members of one squad in file order, the
contiguous `Characters` slice at `CharacterIndex + j` (`game.cpp:649`); every
member is re-placed near the centre of the squad's spawn area
(`game.cpp:647-656`, `game_character.cpp:328-360`):

- the candidate: from the centre `((left + right + 1) / 2, (top + bottom + 1)
  / 2)` — the cell the static render already draws for the squad's first
  member — with the original's draw order: a `y` distance `rand() %
  ((bottom - top) / 2)` drawn only when that divisor reaches two, then a coin
  `rand() % 2` picking subtract or add, then the `x` distance and its coin
  (`game_character.cpp:334-354`); the spans are exclusive (`Width() = right -
  left`, `common.h:187`), so a candidate of a span of two or more never
  leaves `left + 1..right - 1` — arithmetic, not a measurement; the
  placement test asserts containment of every squad of both editions, and a
  degenerate point area can only hold its own cell (map 37 carries one);
- the check: up to 100 attempts; a candidate is accepted when its foot cell
  is free — `FootMap(Tile).ID == 0`, which counts cells taken by members
  placed earlier on this load (`game_character.cpp:356-357`);
- no free cell in 100 attempts: `GroupSize = j` and the squad stops placing —
  that member and every later one are out of the game (`game.cpp:654-655`);
- a dead record is placed like everyone else — the original places first and
  asks who is alive afterwards — but it marks the ground only when alive, so
  its cell can be given twice (by Slavik, `game.cpp:660-664`).

Our differences, all three deliberate:

- the cells of the creatures that are not re-placed (villagers, lone NPCs,
  the player squad) are held from the start, and so is every cell a squad has
  already been given; the original marks the ground group by group in file
  order, so an earlier squad can be placed onto a later one's villager — with
  us what stops depending on the visit order is where a creature that is not
  re-placed sits in it, not the order of the squads: one shared random stream
  and sequentially taken cells make the squad order visible, and we mirror
  the original's (`game.cpp:637`);
- a member may not share a cell with anybody, living or dead (see above);
- the generator is ours (`kn_game::movement::Rng`, splitmix64), asked with
  the original's draw order, so a run with a fixed seed replays.

**Our measurement**, both editions: 379 squads, 2284 members across the 41
maps and `Game.0`–`Game.2` — every member finds a free cell inside its area
within the 100 attempts and no squad of the shipped data is ever trimmed; the
same seed places the same map twice (`original_rules.rs`). The re-placement
also explains the file cells: they are the squad's park position, not where
the game stands it (see [monster decisions](behavior.md#the-monster-decision-the-return-is-confirmed-combat-is-not-2026-11-03)).

## Load-time refusals

Confirmed 2026-11-05 (#82).

**Our measurement** — after the load rule above stands every roaming squad,
`simulate --all` of the viewer still cannot put eleven records of `Game.0`
into the world, identically in both editions: map 7 record 433 with
`CellTaken` (a villager shares its cell with another record — the same fact
the harness test pins as `Denied::SharedCell`), and records 507 (map 10),
513 (17), 517 and 518 (25), 520 (30), 838 (33), 482 (37), 525 (42), 527
(44), 529 (47) with `ImpassableCell` — the file stands each of them in
blocked foot data. These are facts about the shipped files, not engine
faults, and the diagnosis of #82 says which of the three suspects they are
**not**: not a passage divergence (the façade's walkability is the confirmed
foot rule alone, `FootMap(Tile).ID == 0`, as the harness test's `Foot` is),
not an edition difference (the two lists are equal cell for cell), not a
placement gap — the eleven carry no roaming group and `PlaceMob` never
touches them (the load loop calls it under `!(kind & 0x80) && kind < 0x51`
only, `game.cpp:647-656`; `game_character.cpp:328-361` is the placement the
roaming members got instead). What the original does with a record whose
cell cannot hold it is **unknown**: the clone keeps such a creature at its
record cell in its own arrays and draws it there; the engine's own check we
have not recovered. We keep them out of the movement world, report each with
its reason, and give the renderer the record cell to draw them at — the
viewer's report prints them as data, and only a failed order or a build
failure moves the exit code (`apps/viewer/src/lib.rs`, the shipped test
`tests/original_simulate.rs` holds the exact list for both editions). A trim
(`GroupSize = j`, above) is reported the same way; the shipped data never
produces one, so the shipped test treats a trim as a regression of the load
rule, not as data.

## Goal reach

Confirmed 2026-11-05 (#82).

The report counts, per creature, how often a completed order landed the
creature exactly on the cell the rules had commanded — counters of movement
events (`Arrived` with the cell equal to the last accepted `MoveTo`
target), never a position log. Only targets of the confirmed rules are
counted, which are the villager's two (`docs/research/k1/creatures/behavior.md`, «The
villager decision», [cf5a332](https://github.com/Marisa-Chan/Slavik/tree/cf5a3320dc93859e3dc51f37ee823fed30f399b6)
`src/game/game_play.cpp:2288-2385`):

- **workplace** — arrival exactly on the place cell of a job slot the
  creature itself holds (the dynamic branch), or on a free neighbour of the
  place cell of the job its class bits 3–5 name while that slot is occupied
  (the static branch, `rules.rs::static_work`);
- **walk** — arrival on a cell strictly inside the creature's spawn area,
  the band `left + 1..=right - 1` the jobless half roll picks from
  (`rules.rs::walk_target`).

Anything else an order could aim at — a monster's return to the centre, say,
or the neighbour a blocked goal fell back to — earns no counter until its
rule is confirmed. **Our measurement**, `simulate --all` at seed 42 over
5000 ticks, both editions and identical there: eleven village maps produce
walk arrivals (11 to 20 per map — the jobless roll is the only order the
shipped data ever completes on them); workplace arrivals are rare because
every holder of a job slot stands on its place cell already at load — the
single exception across all maps is map 1 record 369, a class 0x52 villager
whose bits 3–5 name the merchant: its slot is occupied by record 357, who
stands on the place cell (30, 115) himself, and the arrival is the free
neighbour the order sent 369 to. `tests/original_simulate.rs` pins the map
19 counters and this arrival. The counters ride the seeded generators only
(placement draws on `Rng::new(seed)`, decisions on `Rng::new(seed ^
0xDEAD_BEEF)`), so the same seed replays them identically — the shipped test
asserts the equality of two full runs.
