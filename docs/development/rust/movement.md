# Movement engine

The shared layer: the isometric fine grid, cell
walkability and occupancy, route search, walking a route and the game tick.
Neither Bevy nor K1 — the part's specifics live in `k1-*` and the display in
`kn-render`, so only the mechanics are here. Modules:

- `grid` — the cell (`Cell`), the eight directions (`Direction`), cell ↔ pixels,
  lattice coordinates without the row shift (`lattice`) and distance in cells;
- `passage` — the `Passage` trait: a data supplier brings cell walkability;
- `pace` — the `Pace` trait: how many ticks a step lasts in a direction — also
  the supplier's job;
- `occupancy` — `Occupancy`: one creature per cell, standing or held by a step
  reservation;
- `route` — `find_route` and `Route`: A\* (`pathfinding::astar`) over the grid;
- `walk` — `Walker`: walking a finished route, steps per tick and the sprite
  state (`WalkState`);
- `world` — `World`: several creatures, the tick, disputes over cells, repathing
  and events (`Event`); `tick_step` is the only event channel (see "Decision: one
  channel for events");
- `rng` — `Rng`: splitmix64, our own deterministic generator.

Search and walking are deliberately far apart: `route` knows neither the tick nor
the step duration, `walk` knows neither walkability nor how its route was found.
`World` is the only place they meet and the only place time is counted.
Walkability and occupancy are split the same way: `Passage` is the map's data,
`Occupancy` is engine state, because only the engine knows who is mid-step and
where. The original's facts — [k1/movement.md](../../research/k1/movement.md); the sections
below are what we chose, not what the game shows.

## Decision: step cost and the A* heuristic

A step costs as many pixels as it travels: 464 units west and east, 256 north and
south, 265 along the diagonals — all in 1/8 pixel, the diagonal being
`ceil(8·√(29² + 16²))` rounded up. The heuristic is the straight-line distance
between cell points in the same units (`u64::isqrt`, rounded down).

The reason not to use "10/14 in cells": a cell rhombus is 58×32 px while
neighbours stand 58, 32 and ≈33.1 px away, so orthogonal and diagonal steps have
different lengths. A "10/14" metric would give the diagonal a cost of 14 against a
north step's 10, though in pixels it is 33.1 against 32: with such weights the
heuristic overestimates and A\* returns a route longer than the shortest. Once a
step costs its own length, a straight-line estimate is admissible and monotone by
the triangle inequality, so the search returns a genuinely shortest path and the
route looks natural on an isometric picture
(`route_cost_stays_close_to_a_straight_line`). Rounding the diagonal's cost up is
deliberate: a step may never cost less than it really is.

For the neighbourhood of a taken goal (several possible final cells) the
heuristic measures the distance to the centre of the neighbourhood and subtracts
the largest distance to a cell of that ring: the estimate stays admissible and no
`isqrt` is needed per candidate cell.

Distance in cells (`Cell::lattice_distance`, Chebyshev in unshifted lattice
coordinates) kept a different role: it measures the neighbourhood of a taken goal.

## Decision: routes without the 32-step limit

A\* searches for a route of any length. The original's limit was the price of
memory in 1999; it is reflected nowhere in the data and is not carried into modern
limits (the product principles; the fact —
[k1/movement.md](../../research/k1/movement.md), "Route limit: 32 steps in the original"). The
saving is not one we need: a search over the whole fine grid costs 0.2–0.4 ms (see
[Engine cost](#engine-cost)) — under one percent of a 60 ms tick.

## Decision: no corner cutting

Directions are divided not by the screen's diagonal but by how a cell touches its
neighbour: east, west, north and south step through a vertex of the rhombus, and
they are allowed only when both cells standing across the path are walkable
(`Direction::corner_flanks`). Steps along a screen diagonal share a whole edge
with the current cell — there is no corner left for them to cut. In unshifted
lattice coordinates this is exactly the ordinary ban on diagonal moves in an
8-connected grid: the four "screen-diagonal" steps are orthogonal there, while
east/west/north/south are the diagonal ones.

Flank cells are checked for map walkability only: another creature in a flank cell
keeps a creature from entering the destination cell, and that is `Occupancy`'s
job, not a corner rule. Otherwise a creature standing off to the side would block
every passage.

## Decision: one channel for events

`tick_step` is the only place engine events leave. An order `move_to` given
between ticks puts its own event (`RouteSet` or `NoRoute`) into the world's shared
channel, and it is the first thing to arrive on the next tick, in the order the
orders were given: outside a tick an event is neither lost nor emitted twice, and
a consumer needs a single source. `move_to`'s return value stays the answer to the
order itself — a step count or `Err(NoRoute)` — so the caller can decide at once
without waiting for a tick.

The rule "nobody stands in an impassable cell" is checked when a creature is
placed as well: `World::add` refuses a cell outside the grid (`OutsideGrid`), a
taken one (`CellTaken`) and an impassable one (`ImpassableCell`) — a creature
inside a wall would break the invariant the world checks on every tick.

`World::remove` puts its own event into no channel: the outer world ordered the
creature out and knows why — the engine only releases cells and stops walking.
A search event of the creature that was still waiting in the channel when the
removal ran is dropped with it, so the channel names no creature the world has
let go; see "Decision: taking a creature out keeps the numbers".

## Decision: who yields a cell

Starting a step, a creature reserves its destination cell and holds the
reservation until the step ends (`Holder { standing: false }`). From that:

- a head-on exchange is impossible: a foreign cell is held by a creature or by a
  reservation, so the step does not start;
- two creatures claiming one cell on one tick do not collide: a reservation is
  exclusive and creatures are visited in number order, so the lower number gets
  through first — and that is deterministic;
- the cell a creature stepped out of is freed on the same tick, so whoever
  follows enters it without waiting an extra tick;
- if the destination cell became impassable while the creature was walking, the
  step is rolled back (`Event::TerrainBlocked`), the creature stays on its own
  cell and waits for a repath — it never ends up in an impassable cell.

No separate "yielding rule" was invented: in Slavik a creature simply does not
enter an occupied cell (`game.cpp:2889-2895`), and which of two creatures gives
way in the game has not been worked out. There is no separate deadlock guard
either: a cell frees itself after a few ticks, and if it does not, repathing kicks
in. What happens to a creature with no detour — see "Open questions".

## Decision: repathing without thrash

A route search is the engine's most expensive operation, so it is called from one
place only and under two conditions: the creature is standing with a route it
cannot start (the cell ahead is taken or has become impassable) and has stood for
`wait_before_repath_ticks` (12 ticks ≈ 0.72 s by default), and at least
`repath_cooldown_ticks` (60 ticks ≈ 3.6 s) have passed since its last search. The
whole route is not rechecked every tick: a creature learns about a blockage only
when it is about to take a step. A brand-new order searches at once, and a repeat
of an order whose search just failed is held to the same window — see "Decision: a
failed goal is retried on a cooldown".

Both numbers are `WorldRules`, not constants: there are no confirmed values from
the game, and a consumer can configure them. That a creature does not get stuck is held
by a test: after a blockage the route is recomputed, and once the obstacle is
removed the creature reaches its goal.

A repath inserts no empty tick: a new route is spent by its first step on the very
tick it was found
(`a_deferred_order_starts_its_first_step_on_the_border_tick`). Before this, an
order arriving while the creature was walking was executed one tick late — 60 ms
of idling on every change of goal, while there is no pause between the steps of
one route.

## Decision: a repeated order is idempotent

A rules tick loop re-sends a creature's current goal every tick. `World::move_to`
remembers the last goal it accepted (`Actor::last_commanded`) and treats a repeat
of that goal as a no-op: it returns the steps the current route still has and
neither resets the waiting counter nor runs a fresh search. Without this the loop
would clear `waiting_ticks` and call A\* every tick — the opposite of "repathing
without thrash", and a route would never be allowed to complete.

The memory is kept across arrival, so an order re-sent to a creature already at
rest on that goal is still a no-op; `World::stop` clears it, and a different goal
overwrites it. A failed goal is not retained as an accepted order; retrying it must remain
possible when a crowd clears. A separate failed-goal memory throttles those retries.

## Decision: a failed goal is retried on a cooldown

Accepted-goal idempotency alone would allow an unreachable order to search
every tick. Route search is the most expensive operation — the
search over a whole unreachable area costs 8.9–9.6 ms, a sixth of a 60 ms tick for
one creature (#54) — so `Actor::failed_goal` remembers the goal whose last search
found no route, and a repeat of exactly that goal inside
`repath_cooldown_ticks` of that search is refused without a search and without a
new `Event::NoRoute` (`repeated_unreachable_order_is_throttled_to_the_cooldown`,
`a_retry_inside_the_window_neither_searches_nor_reports_a_refusal`). The refusal is
told to the caller by the return value of `move_to`; the event channel carries only
the search that actually ran.

It cannot be the same mechanism as `last_commanded`, and this is what #52 turned
up: the idempotency memory has to be *cleared* when a search fails, otherwise the
order sticks and a goal the crowd has freed is never retried
(`deferred_no_route_can_be_retried_after_the_path_clears`). So the two memories
pull in opposite directions — one keeps an order it must not re-search, the other
keeps an order it must search again — and the throttle window is what separates
them: the retry is late, never absent
(`a_repeated_unreachable_order_gets_a_route_once_the_window_passes`). Late is the
best the engine can promise: `Passage` is a query, not a notifier, so nothing tells
the world that walkability changed and no search can be woken the moment a path
clears. Notifying the world of a terrain change is its own design, taken up when a
consumer exists (#54). `World::stop` clears the failed goal too, so
a withdrawn order followed by the same order is a real search
(`stop_clears_a_failed_goal_so_the_next_order_searches`), and a different goal is
never held up by a failed one
(`a_failed_goal_does_not_hold_up_an_order_to_another_one`).

The saving follows the [recorded search cost](#engine-cost): a creature whose
goal is cut off by a wall column costs 4 searches over 240 ticks instead of 241
(the test asserts the number), that is ≈37 ms against ≈2.2 s of one creature's
worst operation over 14 s of game time.

This is a modern-experience guard, not a recovered rule: how often the original
retries an unreachable order has not been worked out, and the window is the
engine's own `WorldRules` number, not a confirmed one. Its bound is one remembered
goal: a loop that switches between two unreachable orders searches on every switch,
inside the window (`alternating_unreachable_orders_each_search_inside_the_window`).

The engine keeps one failed goal rather than a set; cadence across different
AI goals belongs to the rules layer. Rules can re-roll targets every tick, so
an unreachable area otherwise pays a search for every new goal. The bound
is a per-creature window one layer up — `kn_game::rules::FailedOrderCooldown` holds
a movement order back for `window_ticks` after one of its found no route and answers
`Applied::Held` without calling the engine
(`an_unreachable_area_costs_one_search_per_window`: 4 searches over 40 ticks instead
of 40, and `a_cooldown_bounds_the_searches_of_an_isolated_creature`). It lives
beside the rules rather than in `World`, so a player's order is never delayed by a
failed AI one, and a turn — which costs no search — is never held. The two memories
stay one each: the engine remembers the goal, the application remembers the tick.

## Decision: an occupied goal takes the nearest ring

If the goal is taken or impassable, the search walks the rings of cells around it
from the centre outward (`goal_fallback_radius`, 4 by default) and stops at the
first ring it can route to: the creature goes to the closest free cell, not to the
one cheapest to reach. The rings work **only for a goal that cannot be entered**:
if the goal is free and walkable but unreachable, there is no answer — `find_route`
returns `None` and the creature gets `Event::NoRoute`. Substituting a neighbouring
cell for such a goal is not allowed, because the caller would not learn that the
creature stopped somewhere other than where it was sent: it would see an arrival
(`Event::Arrived`) where the order was not carried out
(`a_free_but_unreachable_goal_does_not_take_a_ring_cell`). Without the ring sweep
A\* would take the cheapest goal and stand on the cell nearest to itself rather
than by the goal. Rings are enumerated centre outward, each with its own search:
in the usual case — a free goal — there is exactly one. The radius bounds the
worst case: a taken goal in a dead end will not force a sweep of the whole map,
and a goal with no free cell even within the radius gives no route rather than a
panic. A creature already standing in a free cell beside a taken goal gets an
empty route: it has nowhere to go, and the order counts as done.

## Decision: taking a creature out keeps the numbers

`World::remove` takes a creature out: every cell it stood on or reserved with a
step is released, its movement state is dropped, and an order to it — or a
second removal — answers `UnknownCreature`. Two questions the ticket left open
got an answer each:

- **a number is never reused**. A slot keeps its creature's number for the
  world's life: a new creature takes the next number, not the freed one. The
  outer world keeps ids past the engine — #75 lets a corpse remember the
  creature that lay in it — and a recycled number would make one id name two
  creatures in one save; a stale handle to a removed creature answers
  `UnknownCreature` instead of ordering a stranger. The cost is a number that
  only grows: a `u32` runs out after four billion placements in one world, and
  a world lives as long as one map does;
- **`remove` stops the creature itself**: a step that had started is ended
  where it stands and its reservation is released with the standing cell, so a
  following creature enters the freed cell on the next order. Requiring a
  `stop` first would make death forget a reservation, and a forgotten
  reservation is a cell nobody can enter again.

Removal emits no `Event` — the outer world is the one that knows a creature
died — and no event names a creature the world has let go either: a search
event still waiting in the channel when the removal runs is dropped with the
creature (`removal_drops_a_pending_search_event_of_the_removed_creature`),
while the events of the others keep their place. The creatures that stay are
visited in the same number order as before, so the same seed and the same
removal order replay the same trace
(`removals_replay_identically_under_the_same_seed`). No despawn rule of the
original is confirmed here: the engine's removal is our API.

The slot of a removed creature stays in the world — the number is not reused —
but the tick does not walk it: taken slots are held in a list ordered by
number, and the tick walks that list, so the order is the walking-the-slots
order. Release build, Apple M4 Pro, `removal_cost.rs` under `#[ignore]`, the
spread over three runs: 100 standing creatures behind 2 000 empty slots cost
0.24–0.28 µs a tick and 0.35–0.37 µs a `states()` call; the same world walking
every slot costs 2.6–2.9 µs and 1.6–2.0 µs, and the pair `add` + `remove`
measures 0.05–0.06 µs on both sides — the bookkeeping is invisible there. A
tick arrives every 60 ms, placements and removals are rare: the difference is
bought in the right direction (the churn itself is the #49 debug-player case).

## Decision: the seed breaks ties

The order directions are tried in is rotated by a value from `Rng`, so a choice
between routes of equal cost depends on the seed and repeats under the same seed.
The seed does not affect the data: where the choice is not a tie, the route is the
same under any seed — determinism would otherwise be out of reach. The generator
sets no game rules: wherever the choice is in the data, the data is what arrives.

## Decision: what the engine does not know

The engine works in whole ticks; how many milliseconds a tick holds is the
supplier's knowledge (`kn_game::TICK_MS`, 60 ms for K1). Step duration and
walkability also arrive from outside, and a zero duration is counted as one tick:
a zero-length step does not exist, and data must not panic the engine.

What leaves the engine is `WalkState` (the cell, its pixel point, the cell it was
entered from, the goal, the direction, whether a step is running and how many
ticks are left in it) and `Event` (step start and end, arrival, waiting, blockage,
repath, refusal). The direction index matches the original's table, so a
visualization picks the walk animation by it; how to draw a sprite is not the
engine's business.

## Open questions

- **A crowd with no detour.** If a repath around a taken cell gives no route, the
  creature drops the order and stops (`Event::NoRoute`): the search does not repeat
  forever, but neither does it resume by itself — whoever sent the order has to
  send it again, and a repeat is throttled to one search per
  `repath_cooldown_ticks` ("Decision: a failed goal is retried on a cooldown").
  Whether to hold the goal and wait for the crowd to clear remains a behavior question; no unverified wait/resume rule is added.
- **A blockage mid-route.** A creature notices a closed cell only once it reaches
  it, so a tree fallen in the middle of a route is worked around a few cells late.
  Revisiting every route whenever walkability changes costs more and produces more
  thrash.
- **Rewinding.** The engine runs one way: `Event` is enough to rebuild state by
  stepping forward through the ticks, and the viewer rebuilds/replays when it needs an earlier tick. A viewer `Step`
  back is deliberately *not*
  an engine reverse — it rebuilds the map's `Simulation` and replays it forward to
  the rewound clock tick, so the display shows that tick without the core gaining a
  reverse tick it was never designed for (review #68, `replay_simulation`).
- **Differences in K2.** Running changes only the step duration, and that comes
  from data; no other movement differences in K2 have been looked into yet.

## Engine cost

Release build, Apple M4 Pro, a fine grid of 160×320; measured by the
`kn-game/tests/movement_cost.rs` test under `#[ignore]` (an ordinary `cargo test`
does not run it):

```bash
cargo test --release --manifest-path crates/Cargo.toml -p kn-game \
    --test movement_cost -- --ignored --nocapture
```

| Operation | Time |
|---|---|
| a world tick with 50 creatures walking | 1.7–1.8 µs (0.034–0.037 µs per creature) |
| a world tick with 500 creatures walking | 14.2–15.1 µs (0.028–0.030 µs per creature) |
| a tick with a blocked cell (amortized repath) | 1.9–2.2 µs |
| a route across the whole map over open ground | 0.21–0.37 ms |
| a route among walls | 2.0–2.2 ms |
| a search next to 400 occupied cells | 2.8–3.0 ms |
| a route to an unreachable goal (the whole reachable area) | 8.9–9.6 ms |
| cell occupancy (a lookup) | 0.017–0.018 µs |
| the A* heuristic (`isqrt`) | 0.005 µs |
| a neighbour cell / cell to pixels | 0.002 / 0.001 µs |

The spread over three runs in a row is given: under a percent for the short
operations, nearly twofold for a route across open ground, so for that one the
order of magnitude matters more than the number.

Neighbouring operations for comparison (see [layer-switch cost](rendering.md#layer-switch-cost)):
switching a group of 5 131 sprites — 0.15 ms, parsing a render list — 26 ms. A
tick of 50 creatures is about 90 times cheaper than an overlay switch and about
15 000 times cheaper than parsing a map; a typical route (0.2–0.4 ms) is 1–3
overlay switches and 1/65…1/125 of a map parse; the worst search is a third of a
map parse and 15% of a 60 ms tick.

The upshot: an engine tick costs microseconds and the entire cost is in route
search, which is why `World` calls it no more often than `repath_cooldown_ticks`.
The search for an unreachable goal stays the most expensive operation: a rules loop
re-sending such an order now costs one search per creature per window, not one per
tick (#54, "Decision: a failed goal is retried on a cooldown").
