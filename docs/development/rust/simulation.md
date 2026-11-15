# Creature rules and simulation

K1 behavior splits
from the shared world in two crates: evaluation lives in `k1-game::rules`
(it reads the roster plus a live picture of the tick and declares its own
`Ground`, `Random` and `Live` abstractions, so it needs no dependency on
`kn-game` — the dependency runs the other way), application lives in
`kn-game::rules` (it takes explicit `(CreatureId, Decision)` pairs, calls
`World::move_to` and `World::turn`, and hands the world itself to the rules as
their `Ground`). The confirmed rules are in [creature behavior](../../research/k1/creatures/behavior.md).

## Decision: separate evaluate from apply

Two phases per tick — evaluate the whole roster first, then apply — so that
decisions do not depend on the order creatures are visited in. If a single pass
mutated the world as it went, the second creature's decision would see the first
already moved and the roster's iteration order would leak into behavior. A
contested cell is settled by the world itself (see "Decision: who yields a cell"),
not by whoever happened to evaluate earlier.

## Decision: classification is by group kind, not class byte

`0x53` in the ticket is a *group kind* (see `Group.kind` in
[game-state.md](../../research/k1/formats/game-state.md)), not a character class; villager groups
are `0x80`–`0x82`. `Creature` carries both `class_id` (byte 0x04, describes the
character: sprite source, dead bit, work bits) and `group_kind` (byte 0x1E of the
group, describes the band), and `classify()` reads only the second. A warrior
class_id inside a villager group still classifies as `Villager`.

## Decision: the rules read live positions, not the file

`Game.N` is the starting state; the rules decide over where creatures stand
now, so `evaluate` and `group_states` take a parallel `Live` picture (cell,
facing, whether the creature is still walking) and the group state byte is
recomputed every tick from those positions — never read from the file, which
for roaming squads the game itself overwrites within the first 16 ticks
(`game.cpp:645`, `game_play.cpp:1623-1638`; the period is a 1999 cost cap we
do not reproduce). A creature whose squad state the caller did not supply, or
whose live picture is missing, gets the calm answer and no decision rather
than a panic: the arrays are indexed like the roster and may lag it. A creature
the world does not know is `None` in that picture, never a made-up cell — an
invented `(0, 0)` would decide the state of every squad whose spawn area covers
the origin (`an_unknown_player_member_never_makes_a_squad_hostile`).

The villager walk picks a cell uniformly inside the squad's spawn area; the
original offsets from the creature by the viewport size (`ViewTiles * 2`,
`game_play.cpp:1889-1949`), and behavior must not depend on the window. The
monster return keeps the original's exact draw order (coin, then `y`, then
`x`), so a run with a fixed seed replays. Villager squads are always calm
until the village rules (`FUN_00416700`) are recovered — our villagers never
turn hostile; monster combat (state 1) gives no order rather than an invented
one. Both are recorded as open questions in
[creatures.md](../../research/k1/creatures/README.md), not as confirmed behavior.

The player squad the state scan looks for is part of the roster the caller
passes, and `k1_game::creatures::roster` lists it only on the map the game
starts on: a simulation of any other map adds the arriving player squad itself
before asking for group states (#38, #49). The rules' eight-direction offset
table is a second copy of the engine's, because `k1-game` cannot import
`kn-game`; `kn-game/tests/movement_grid.rs` holds the two together.

## The marked hero's footprint

The player squad is a creature of the simulation like any other: it draws no
decision (`PlayerOrWarrior` returns before the shared RNG, `group_states`
maps `PlayerInside` to what `Calm` does today), so no hero-specific behavior
machinery exists (`k1-game/tests/original_rules.rs`, «a marked hero squad
earns no decision and changes nobody else's»). Its physical presence,
though, is a fact of the world: the squad's occupied cells enter the
occupancy test of `walk_target` and of the static-work free neighbour, and
one rejected attempt spends two more draws of the shared decision RNG —
every later draw of every creature shifts. Measured over `Game.0`–`Game.2`
of both editions on the start maps (#121): with the squad marked, villager
walk counters differ from a squad-less world on map 1, whose village holds
the hero's start; NPC load positions never move. The coupling is
hero-agnostic — any creature standing there does the same — so a whole-world
counter identity between a marked and an unmarked run is not a contract the
engine can hold; the placement contract and the marked replay determinism
live in `kn-game/tests/original_hero_start.rs`.

## Decision: turning in place

`Decision::Turn(u8)` turns a standing creature to face the original's
direction byte 0..7; `Applied::Turned` reports acceptance. The engine gains
the smallest thing criterion 3 needs: `Walker` remembers an explicit face,
and `facing()` answers in order — the running step, the route's first
direction, the face, the direction of arrival. A step consumes the face, so
after a walk the creature faces where it came from exactly as it does without
a turn, and a creature sent to a workplace is turned again once it stands —
which is also the original's order of events. `World::state` exposes the
facing, so turning emits no event. Turning is immediate,
not one direction per tick (the original creeps by one step at a time,
`FUN_004391bc`, `game_play.cpp:1995-2013` — a 1999 limit,
see the deliberate-limit differences in [creatures.md](../../research/k1/creatures/README.md)).

## Decision: load-time placement mirrors the original, occupancy does not

A roaming squad does not stand where its records put it — the
game re-places every member on map load. `k1_game::placement::place_roaming`
follows `PlaceMob` cell for cell: jitter from the area's centre with the
original's draw order (a distance then a coin on `y`, then on `x`, a distance
drawn only when its half-span reaches two), one hundred tries, and the squad
trimmed at the first member that finds no free cell (`GroupSize = j`). The
facts and the three deliberate differences are in
[creatures.md](../../research/k1/creatures/README.md), «Roaming squad placement on map load». The
one worth restating here: the engine and the original differ on occupancy, and
placement follows neither blindly. It holds the file cells of the creatures it
does not move and every cell it has handed out, so a placement answer depends
only on the roster and the ground — not on the order groups happen to be
visited in, which is what makes it reproducible under a fixed seed and what
`roaming_squads_are_re_placed_on_free_cells_inside_their_areas` checks over
all 379 shipped squads.

## Decision: rules and the engine stay in separate crates, wired at the seam

`k1_game::rules` declares its own `Ground`, `Random` and `Live` and depends on
no engine type; `kn_game::rules` owns the `World` and applies decisions to it.
Neither reaches into the other, so a caller that only wants starting cells
(criterion 2) calls `placement::place_roaming` over the rules' `Ground` with
the engine's `Rng`, without a `World` at all. `k1-game` still cannot import
`kn-game`, which is why `place_one` re-derives the centre and spans from the
spawn rect rather than borrowing a grid helper.

## Rules tick cost

Release build, Apple M4 Pro, both editions agree to within a microsecond; the
spread of three runs. Measured by `kn-game/tests/rules_cost.rs` under
`#[ignore]`:

```bash
cargo test --release --manifest-path crates/Cargo.toml -p kn-game \
    --test rules_cost -- --ignored --nocapture
```

| Operation | Time |
|---|---|
| decide only, calm map 17 (29 monsters, none walking) | 1.2–1.6 µs/tick |
| decide only, map 19 (20 creatures, 14 walking) | 0.49–0.56 µs/tick |
| full tick — decide + apply + `tick_step` — calm map 17 | 1.28–1.85 µs/tick |
| full tick, map 19 (14 walking, routes live) | 18–22 µs/tick |
| load of one map: foot-data load + placement (`Game.0`, averaged over all maps — 987 creatures in all) | 0.27–0.49 ms |

The decide pass over dozens of creatures is 0.5–1.6 µs — three to four orders
of magnitude under the 60 ms tick — so the rules are not the thing to cache.
The full tick's 18–22 µs on the walking map is route search, not decisions:
A\* across the fine grid dwarfs everything else, as [movement cost](movement.md#engine-cost)
shows (an open-ground cross-map route is 0.21–0.37 ms), and it is
paid by the walking creatures, not by whoever decides. Load placement — the
foot-data load and the placement together — is a quarter of a millisecond per
map and runs once when a map opens, so it is
noise next to the 16–25 ms of parsing the graphics; no placement cache is
worth its bookkeeping. The loop itself runs over the real maps in
`kn-game/tests/original_simulation.rs` (criterion 9): map 19 with fourteen
creatures mid-step in the same tick for 400 ticks, map 17 with its calm
squads not taking one step.

## Game time cost

Release build, AMD Ryzen 9 9950X, the spread of three runs. Measured by
`k1-game/tests/time_of_day_cost.rs` under `#[ignore]` (issue #95) — no
`original/` needed, the model is arithmetic over the tables of
[k1/time-of-day.md](../../research/k1/time.md):

```bash
cargo test --release --manifest-path crates/Cargo.toml -p k1-game \
    --test time_of_day_cost -- --ignored --nocapture
```

| Operation | Time |
|---|---|
| empty loop (the floor of the harness) | 0.19–0.22 ns/tick |
| `clock_face` — three divisions by the cycle constants | 0.83–0.95 ns |
| `ground_window` — the window and a range check | 1.83–2.17 ns |
| `ambient_value` — the window and the original's branch | 3.8–4.4 ns |
| `ambient_kind` — the triple and its sign | 4.3–5.1 ns |
| `day_phase` — the ambient, then the ground window | 5.7–6.9 ns |
| `game_time` — the whole readout the panel shows | 12.9–14.8 ns |

The day model is at most fifteen nanoseconds a tick: two orders under the
cheapest full tick above (1.28 µs on a calm map) and seven under the 60 ms
budget, so nothing in it is worth caching. The most expensive parts are the
twelve-threshold scan and the branch over the ramp formulas that computes the
ambient triple with the original's integer halving; a binary search or a
precomputed per-tick table would save a few nanoseconds and cost a second table
to keep in step with the windows documented in
[k1/time-of-day.md](../../research/k1/time.md). It is also paid by the asker, not by
the simulation: game time is derived from `Simulation::tick` on demand
(issue #95) and the simulation carries no accumulator of its own.

## Lighting cost

The rendered lighting (#96) has two halves measured separately: the pure
model in `k1_game::lighting` on the CPU, and the material pipeline in the
viewer's window.

**CPU model.** Release build, AMD Ryzen 9 9950X, the spread of three runs.
Measured by `k1-game/tests/lighting_cost.rs` under `#[ignore]` — no
`original/` needed, the model is arithmetic over the tables of
[k1/time-of-day.md](../../research/k1/time.md):

```bash
cargo test --release --manifest-path crates/Cargo.toml -p k1-game \
    --test lighting_cost -- --ignored --nocapture
```

| Operation | Time |
|---|---|
| `params` — the full `LightingParams` of a tick, the render layer's per-frame entry point | 8.46 ns |
| `apply_channel` — one channel of the per-channel blend, the shader-equivalent reference | 1.61–1.63 ns |
| `night_palette` — the per-day coin, `(day, seed)` → variant | 1.34–1.35 ns |
| `light_color` — the tile-light colour, re-rolled every 16 ticks | 1.64–1.65 ns |
| full pipeline — `params` + the three-channel apply | 12.53–12.64 ns |

A confirmation run of this session reproduced the table within a tenth of a
nanosecond on both ends (`params` 8.42–8.45, full pipeline 12.52–12.55).
The verdict follows the game-time table above: the whole pipeline costs about
one `game_time` readout (12.9–14.8 ns) — two orders under the cheapest engine
tick (1.28 µs on a calm map) and seven under the 60 ms budget. And this is a
per-frame cost paid by the asker, not a per-pixel one: `apply_channel` runs
on the GPU; the CPU number exists as the reference the WESL fragment is
checked against. Nothing in the model is worth caching.

## Open questions

- Whether a villager squad can enter combat mode, and what monster combat is
  (target choice, attack, shooting), stays unrecovered — the rules give no
  order for those states and the engine waits for the village and combat rules.

## New-game allocation cost

Measured locally on macOS arm64 in the release profile with
`cargo run --release --manifest-path crates/Cargo.toml -p kn-game --example inspect_new_game`:
100,000 repetitions of allocation plus one decrement took **53 ns (RUS run)**
and **71 ns (ENG run)** per operation; 100 parses of the corresponding Game.0
averaged **121,990 ns** and **154,969 ns**. The pure allocation rule is independent
of the edition; the two timings show run variation. Recomputing the conserved
point budget is about 2,200 times cheaper than the neighboring state parse in
this measurement. Rules and verification: [k1/new-game.md](../../research/k1/new-game.md).

## One-shot specials

k1_game::creature_anim::Machine owns a dynamic creature's Special → Idle
transition without Bevy or its own clock. Simulation supplies ticks; rendering
reads the stage and swaps clips. Both windows use CreatureAnimationPlugin,
including the player creature tests. Source behavior and our explicit map-load
trigger are in [animation research](../../research/k1/creatures/animation.md#deliberate-play-mode-difference).
The unrecovered monster decision that releases a frame-zero hold is not
replaced with an invented behavior rule.

Machine::enter_world starts a state-4 record at frame zero. Sequence exhaustion
enters idle and never re-enters Special; Machine::restart_special is the explicit
event seam for replay. The viewer's Play toggle is Playing/Paused; Reset
animation enters Off and recreates/replays. Idle/walk, objects and flames remain
loops. The game layer decides stage; the render layer owns display, never rules.
No file writing is involved.

## Time and lighting contracts

k1_game::time_of_day derives from Simulation::tick_number; neither frontend
owns a second day accumulator. Tests assert the thirteen windows/twelve
thresholds and ground-light interval, not a four-name day sequence. Named
phases are a display classification with no new boundaries; viewer HH:MM is
an explicit [convention](viewer.md#the-hour-we-display--our-convention-not-a-fact-about-k1).

The model follows original integer-halved ramps. Window 4 reaches neutral at
3593 before its branch ends at 3595; window 13 reaches neutral at 19020 before
its branch ends at 19021. The observable value trace is
1802,3593,5402,8100,18900,19020; comparison tests still use branch intervals.
Float division as written in the clone would shift ramp values one tick early.

k1_game::lighting supplies pure params/channels/apply_channel/night_palette/
light_color/fades functions and the CPU reference for the shader. The model
keeps confirmed fade_factor/fade_table_entry byte arithmetic and golden tests.
Window 7 red uses the clone's unresolved 0.33 with confidence documented at
ambient_value; rendering does not resolve that research question.

KonungLightingPlugin applies a KonungLightMaterial (MaterialExtension2d),
whose WESL fragment is shaders/terrain_light_material.wesl. LightingMaterials
pools 21 assets: 19 LIGHTS.RES masks plus ground/plain ambient variants.
FineLightGrid supplies LIGHTS.N cells for creature lighting. Both frontends
write LightingState from the simulation tick or the explicit --light-tick
capture pin. Ambient fade reaches every surface; the window gates tile-light
gain. The viewer slider/presets select times, never phase boundaries.

Intentional runtime choices, recorded beside the [original facts](../../research/k1/lighting.md#deliberate-rendering-differences):

- The final color blend is in 8-bit floats while the original used 5/6-bit
  integer lookup palettes. Retain the formulas, whole-byte ramps, f32
  accumulator and truncation; differences are bounded by output quantization.
- One simulation tick advances one world tick; the original 1+speed passes per
  frame are not reproduced. A future speed control must change tick pacing.
- A future player map transition must carry the session day. Viewer map changes
  are independent scenes with fresh simulations, enforced by
  switching_to_another_map_recreates_the_day_with_the_new_simulation.
- light_color(tick,seed) uses tick/16: one deterministic color per 16-tick block,
  1350 changes per cycle, replacing per-draw randomness. The block period is
  our approximation, not observed original timing.
- night_palette(day,seed) mixes day/seed through a golden-ratio multiply and
  splitmix64 finalizer (mix_day_seed), then `% 100 < 90`: the original 90-in-100
  chance without consuming simulation RNG or depending on frame rate.
- params_gated(tick,seed,day_night) returns neutral ambient and no ground light
  when off, leaving the counter running. Viewer control/default/reset policy
  is in [viewer](viewer.md#playback-and-lighting-controls). The constant-map
  bypass and separate light-effects switch remain unimplemented.

CPU costs above are paid on demand/per frame, not per pixel; apply_channel on
CPU exists as a shader-equivalence reference. The lighting model never
re-derives, resets or caches the session day.

## Runtime error and passability contracts

- **Order outcomes are consumed, not dropped (P2 #4).** `Simulation::tick`
  applies decisions through `rules::FailedOrderCooldown` (one route search per
  `WorldRules::repath_cooldown_ticks` for a creature whose goals keep failing,
  otherwise the re-deciding loop pays a search every tick) and keeps every
  rejection that is not `NoRoute` in `Simulation::order_failures` — the rules
  only aim at cells `Ground::is_free` vetted, so anything else is a bug or bad
  data; the headless report lists them as errors and fails the run
  (`a_driven_run_walks_a_villager_into_its_area_without_order_failures`,
  `an_unreachable_area_costs_throttled_searches_not_order_failures`).
  `initial_pixels` deliberately covers the whole roster, unplaced creatures
  included: the original places a record wherever the file says, so such a
  creature keeps rendering on its load cell — the record cell for everyone
  the load rule keeps in place, the jittered cell for a roaming member the
  world then refused — while `spawn_failures`/`order_failures` carry the
  refusal to the user
  (`an_unplaced_creature_is_reported_and_keeps_its_load_cell_for_the_renderer`).
- **Load refusals are data, errors decide the exit code (#82).** The façade
  itself now runs the load rule — `Simulation::new` calls
  `placement::place_roaming` before the world exists, so the `simulate` CLI
  places roaming squads exactly as the harness test and the viewer do (the
  CLI used to stand the file cells verbatim and refuse whole squads as
  `ImpassableCell`). After that only records no rule can move remain
  refused: `Simulation::spawn_failures` (reason per roster record, e.g. a
  villager standing in blocked foot data) and `Simulation::load_trimmed`
  (the `GroupSize` shrink) are facts about the shipped files, which the
  headless report prints as data — the exact lists both editions give are
  pinned by `apps/viewer/tests/original_simulate.rs` and written into
  [creatures.md](../../research/k1/creatures/README.md), «Load-time refusals»; the run fails only
  on `order_failures` or a build error. `Simulation::goal_reaches` counts
  completed confirmed orders per creature (workplace and walk arrivals,
  «Goal reach» there) from `Arrived` events, and the report carries the
  counters; the same seed replays them, shipped tests included. The same
  test file now verifies both editions where the shipped checks were
  RUS-only before. - **Roads are unconfirmed (P2 #10).** There is no road layer in `Konung.N`: the `foot`
  grid has only `is_blocked` (`foot & 0x0F == 0x0F`). `MapPassage` is locked to exactly
  `!is_blocked` and no road/terrain preference is applied — `passage_is_the_confirmed_foot_rule_only`
  fails if a future unconfirmed rule is added. The original's route *preference* for roads,
  if any, is not recovered and is not invented here ([k1/movement.md](../../research/k1/movement.md)).
