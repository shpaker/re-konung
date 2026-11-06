# Movement on the map: fine grid, directions, step

What is known about how a creature moves around a location in the first part of
the series. File layout — [formats/map.md](formats/map.md) (cell walkability) and
[formats/game-state.md](formats/game-state.md) (a character's cell). The shared
engine and every project decision — [../rust.md](../rust.md), the section
"Movement engine".

**Status:** the grid geometry, the eight directions, the condition that makes a
cell impassable, cell occupancy, step duration and the length of a tick are
**confirmed** against Slavik and over the files of both editions. The rules for
*who* goes *where* and *why* (orders, villager behavior, combat) are not
recovered here: that is epic #33 part 3. What is not confirmed is not invented.

## Fine grid

Movement runs over the fine walkability grid, not the tile grid: 160 columns ×
320 rows, a cell rhombus of 58×32 pixels, a column step of 58, a row step of 16,
odd rows shifted right by 29. Cell rhombi tile the plane with no gaps and no
overlap: the fundamental domain of the centre lattice is exactly 928 px², the
area of one rhombus.

- A character's or a cache's point in cell `(x, y)`:
  `(58·x + (2 − y%2)·29, 16·y + 16)` — the centre of its rhombus.
- The reverse conversion: the cell nearest a point, halves rounded to even — as
  `round()` does in the Python reference.

**Confirmed**: the formula and the reverse conversion are checked over all
51 200 cells, and the forward formula against the Python reference
(`konung/k1/game_state.py`, `character_position`) and against `k1-data`
(`game_state::character_position`, `character_cell`). The test
`fine_grid_matches_k1_data` (`crates/game/kn-game/tests/movement_grid.rs`) holds this.

In the basis of the south-east and north-east steps (`u = x + y//2`,
`v = x − (y+1)//2`) the grid becomes an ordinary square field: the eight
neighbours are `(±1, 0)`, `(0, ±1)` and `(±1, ±1)`, and the distance in cells is
the Chebyshev distance. The pixel offsets show this: east = north-east +
south-east, south = south-east + south-west, north = north-east + north-west. So
the four steps that look diagonal on screen are orthogonal in these coordinates,
while west, east, north and south become the diagonals.

## Eight directions

| Index | Screen direction | Cell, even row | Cell, odd row | Pixels |
|---|---|---|---|---|
| 0 | west | (−1, 0) | (−1, 0) | (−58, 0) |
| 1 | north-west | (0, −1) | (−1, −1) | (−29, −16) |
| 2 | north | (0, −2) | (0, −2) | (0, −32) |
| 3 | north-east | (1, −1) | (0, −1) | (29, −16) |
| 4 | east | (1, 0) | (1, 0) | (58, 0) |
| 5 | south-east | (1, 1) | (0, 1) | (29, 16) |
| 6 | south | (0, 2) | (0, 2) | (0, 32) |
| 7 | south-west | (0, 1) | (−1, 1) | (−29, 16) |

**Confirmed** against Slavik `game.cpp:2918-2947` — the table `d[8][2]`, the
column chosen by `tilepos.y & 1`, bounds checked over `0..160 × 0..320`. Repeated
in `crates/game/kn-game/src/movement/grid.rs`; the test
`neighbour_offsets_match_pixel_offsets` checks both rows of the table. A
direction index is the key into walk-animation frames, so the order is kept.

The pixel length of a step differs: 58 px west and east, 32 px north and south,
≈33.1 px along the diagonals. A step north or south changes the row by two, so
neighbourhood depends on row parity while the pixel offset does not.

West, east, north and south are steps **through a vertex** of the rhombus: the
neighbour touches the current cell at one point, and exactly four cells meet
there, two of them standing across the path. North-east and the other screen
diagonals share a whole edge with the current cell.

## Cell walkability

A cell is impassable when the low 4 bits of its `foot` byte equal `0xF`; the
high four are flags (0x40 — hypothesis: where arrows get stuck).
**Confirmed**: Slavik `game.cpp:1672-1692`, checked over every map of both
editions (`tests/k1/test_maps.py`), code — `k1_data::map::is_blocked`. Details in
the walkability part of [formats/map.md](formats/map.md).

The movement engine knows nothing of `foot`: a data supplier brings it cell
walkability — the trait `kn_game::movement::passage::Passage`.

## Cell occupancy

One creature occupies a cell: the cell structure holds the character's number,
and while a route is being searched the creature takes its own marker off so the
cell does not block the route, putting it back afterwards — Slavik
`game.cpp:2889-2895`, `game_play.cpp:1951-1973`. **Confirmed** against Slavik; in
the engine — `movement::occupancy::Occupancy`.

What the game does when two creatures claim one cell at the same time, who
yields, and whether anything guards against a mutual deadlock — **unknown**: that
has to be worked out from behavior, not from mechanics. How we do it is a project
decision in [../rust.md](../rust.md), "Decision: who yields a cell".

## Step duration

A step to a neighbouring cell lasts a fixed number of ticks — the number of
walk-animation frames in that direction, not a value the code picks: Slavik
`game_play.cpp:3727-3740` counts `Frame` up to `FrameCount`. **Confirmed**
against Slavik; with us the duration comes from a supplier — `movement::pace::Pace`.
The per-direction frame counts are measured over both editions and agree between
them: humans and almost every dynamic object walk 12 frames in every direction,
dynamic slot 23 walks 6, slot 19 walks 2 in every direction but south-east (12),
slot 20 walks 1 in every direction but north and south-west (12), and slot 3 has no
walk frames at all — with us a duration of 0 counts as one tick
(`movement::pace::Pace`). **Confirmed** over `original/k1/rus/game` and
`original/k1/eng/game`. The record's own counter is not a source: see
[formats/game-state.md](formats/game-state.md), byte 0xD7.

The glide between cells lives in the offsets of the walk-animation frames, not in
code: the pixel position slides the rest of the way into the new cell at the end
of the animation — Slavik `game.cpp:2884-2908`, `game_play.cpp:798-800` and our
own experiment over `GRAPH.RES` and `Objects.res`. **Confirmed**. That is why the
engine hands out the cell, the cell's point, the direction and the step's tick
count, rather than an interpolated position.

Measured over the walk animation of all three human models: from the first frame to
the last the offset slides 1–11 px short of the cell step on each axis (west −54
against −58, east +47 against +58, north −30 against −32, north-west −27 against
−29), identically in both editions. The frames carry almost the whole way and the
remainder arrives as a snap when the step ends — the original never interpolates the
last pixels. **Confirmed** by measurement; we draw the offsets as they are instead of
smoothing them, because interpolation would be a rule the data does not show.

Because the frames carry the travel, a display layer must keep the creature's node
on the step's starting cell pixel for the whole step and index the walk clip from
the step's start (the original's `Frame` counter, Slavik `game_play.cpp:3727-3740`).
A node interpolated toward the target cell on top of the frame offsets counts the
travel twice: the image runs at double speed and the frame cycle — whose length is
the step — snaps it back a cell at every wrap (the review #68 bug, held off by the
viewer test `a_walking_creature_image_never_jumps_back`).

## Tick

A game tick is 60 ms: Slavik `game.h:30` (`FrameTime = 60`). **Confirmed**; in
Rust — `k1_data::objects_res::TICK_MS`, outward — `kn_game::TICK_MS`. The engine
itself counts only ticks; milliseconds are none of its business.

What the running tick count means for the hour of the game world — the day cycle
and its light phases — is in [time-of-day.md](time-of-day.md).

## Route limit: 32 steps in the original

The original's route search is capped at 32 steps: a creature goes no farther,
and `dist` is clipped to 31 at the end of the search — Slavik
`game_pathfind.cpp:105-107`, `game_pathfind.cpp:495-508`. **Confirmed** against
Slavik.

**We do not reproduce this limit — a deliberate departure** (AGENTS.md,
"Priority: modern experience", the decision of epic #33): the limit was the price
of memory in 1999 and is reflected nowhere in map or state data. Our route is
searched with A* without a length cap, held by the test
`route_is_not_limited_to_32_steps` (120 cells in one search); the cost of the
search is measured — 0.2–0.4 ms over the whole fine grid in release, see "Engine
cost" in [../rust.md](../rust.md). A cap would only make sense if the saving were
visible: on our numbers it is not.

## What is not confirmed and not implemented

- Who goes where: player orders, patrols, villager behavior, pursuit — unknown,
  part 3. The rules are not invented.
- Per-direction step durations from the resources are not parsed — a data
  supplier brings them; until they are parsed the engine accepts any numbers.
- Running and the other differences in K2 — per [games.md](games.md) the engine
  is presumably shared; in movement the difference fits inside the step duration,
  and that comes from data.
- Two creatures claiming a cell at once, a head-on exchange, a mutual deadlock —
  there are no facts about the original, so our behavior is written down as a
  project decision, not as a game rule.
