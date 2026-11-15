# Viewer implementation and frame measurements

[User controls](../../guides/viewer.md) · [Rendering](rendering.md) ·
[Simulation](simulation.md). Measurements below are recorded historical runs,
with their original hardware, versions and conditions; this reorganization
introduces no benchmark results. Later corrected attribution takes precedence
over earlier hypotheses without erasing the earlier numbers.

## Viewer state and diagnostics

kn-fe-viewer separates Viewer state from Feathers controls. Handlers change
state; sync systems apply it. Rebuild is raised only for a map change, which
builds SceneOptions::ALL and frames the camera. The viewer selects no hero
(SceneOptions.hero is None); the map source selects Game.N or TGame.N.

Overlay toggles update group Visibility, not sprites or textures. The panel
counts true InheritedVisibility on sprites (visible/assembled), which includes
hidden groups and empty animation frames. Summing children would overcount.
TextureCache::load preserves decode-error text; logs and panel counts expose
missing images. Invalid CLI map numbers return failure.

Bevy Checkbox reports !Has<Checked> but does not write Checked itself;
kn-ui::sync_checked mirrors actual state after click or keyboard changes.
The panel root carries Hovered too, so camera input is captured over gaps
between controls. Captions are English because this is a development tool;
KonungUiPlugin applies the shared theme/PT Sans, including Feathers composite
interiors. Characters are on by default, mirrored by the checkbox.

Labels are built once per map by k1_game::creatures::labels from EXE names and
record class/name/nickname indices, exposed as Simulation::labels. The viewer
imports no K1 name tables. Each label is #<id> plus the assembled name, or bare
#<id> when unnamed. Synthetic name tests store no game strings. PT Sans covers
Cyrillic; no game font or user-specified font path is used. LABEL_Z=500 stays
above map pieces (largest recorded z about 172) and inside the 2D camera's
±1000 far range; the former z=9999 was clipped. World y points up, so a positive
label offset belongs above the creature, not below its feet.

## Playback and lighting controls

One Play state drives Clock.ticks for frames and simulation: Space toggles
Playing/Paused, . and , step. Pausing holds both; the checkbox never chooses
Off (play_toggle_never_stops_the_animation_behind_the_simulation_s_back).
apply_playback changes the clock state, not Rebuild; sync_playboxes mirrors it.

Reset positions responds only to its ResetPositions marker's Activate events,
not the map selector's. reset_creature_sim recreates Game::simulation with the
same SIM_SEED used by map loads and the player, pins the epoch to current
Clock.ticks, and zeros relative sim_ticks. Overlays, camera, map, pause, texture
cache, spawned nodes and labels stay. Nodes/labels sync to new positions.
Without the epoch pin a fresh simulation would catch up through old history.

Reset animation instead sets Off, zeroes the clock and recreates/replays to tick
zero. A later Play runs state-4 specials from frame zero. Paused → Play resumes
the held stage; positions reset preserves the existing clock. These are distinct
operations. Backward stepping rebuilds/replays rather than reversing the engine;
[rendering](rendering.md) describes catch-up ordering and the measured long replay.

Viewer::constant_day is a session preference. Checkbox or N changes it;
sync_daynight_boxes mirrors it. update_lighting_state passes
params_gated(tick,seed,!constant_day) without moving Clock or rebuilding entities.
It survives map changes and resets only on app restart to
DEFAULT_CONSTANT_DAY=false (cycle on). The original default is unknown.
--constant-day and --light-tick pin capture/benchmark conditions. The player
has no equivalent control yet. Original gates and deliberate differences:
[lighting research](../../research/k1/lighting.md).

## Recorded lighting-render cost

**Render.** The window meter, not the CPU one. Methodology: release build,
the viewer's `--benchmark 600` mode — 40 settle frames, then 600 measured
frames of map 17 of the rus edition (12 487 sprites, characters overlay on);
the samples are the `DiagnosticsStore` frame time that
`FrameTimeDiagnosticsPlugin` writes each update, the percentiles are
nearest-rank over the 600 samples, and each row is three runs in a row. The
benchmark window opens with `PresentMode::AutoNoVsync` (`--benchmark` runs
only; interactive runs keep the default Fifo), and that choice is the
methodology, not a tweak: with vsync on, a finished frame sits on the
monitor's quantum of 16.67 ms — day and night would both read as the
display's pace and the meter would measure the screen, not the scene.
Hardware: AMD Ryzen 9 9950X, NVIDIA GeForce RTX 2070 SUPER, wgpu backend
Vulkan (the adapter log line of the runs). A draw-call counter is not
reachable from `bevy_render` 0.20 without extra features, so the table counts
sprites (`ScenePiece` entities of the scene) and the materials of the
lighting pool instead.

| Version | Light | avg ms | p95 ms | p99 ms | sprites | light materials |
|---|---|---|---|---|---|---|
| a95db9f — the historical local main baseline, full default Bevy features | none | 1.49–1.52 | 1.86–1.94 | 2.06–2.10 | 12 487 | 0 |
| ba22cda — the lighting study base (#105 build profiles) | none | 1.28–1.29 | 1.64–1.68 | 1.86–1.93 | 12 487 | 0 |
| spike 3838657 — material on terrain only, CLI-pinned lighting | day, `--light-tick 0` | 3.00–3.01 | 3.46–3.47 | 3.71–3.77 | 12 487 | 20 |
| spike 3838657 | night, `--light-tick 11000` | 2.96–3.02 | 3.38–3.43 | 3.64–3.67 | 12 487 | 20 |
| lighting version 16e4371 | day — clock at tick 0 | 3.30–3.33 | 3.82–3.86 | 4.03–4.20 | 12 487 | 21 |
| lighting version 16e4371 | night, `--light-tick 11000` | 3.27–3.30 | 3.76–3.80 | 3.97–4.06 | 12 487 | 21 |

The baseline rows carry a caveat the comparison needs: the local `main` ref
(a95db9f) is one build-infra commit behind `origin/main` (ba22cda, #105,
which trimmed the Bevy feature set and toolchain), and that commit alone is
worth ~0.2 ms of average frame time here (1.49 → 1.28). The lighting work
sits on ba22cda, so the honest lighting deltas read against it:

- **The lighting version's day against ba22cda — the full cost of the materials on
  every surface:** +2.0 ms of average frame time (1.28 → 3.30–3.33), p95
  1.64 → 3.82–3.86. Where the base bound no materials, the lighting version hands
  every terrain, decoration, object and dark-cell creature sprite a light
  material (21 in the pool, 19 LIGHTS.RES masks plus the two plain ones).
- **Night against day — the cost of the formula:** none. The night triple
  changes the uniforms, not the work: 3.27–3.30 against 3.30–3.33 ms of
  average, inside the runs' spread. The per-frame CPU model is nanoseconds
  ([CPU lighting cost](simulation.md#lighting-cost)), and the day's zero blend takes the shader's no-op branch,
  so both phases draw the same material graph.
- **The spike against the lighting version — extending the material past the
  terrain:** +0.3 ms of average (3.00 → 3.30–3.33 day, p95 3.46 → 3.82–3.86).
  The terrain-only material cost 1.7 ms over the base (1.28 → 3.00); covering
  decorations, objects and the per-creature material swaps adds another
  third of a millisecond and the one extra plain material (20 → 21).

**Unmeasured allocation candidate:** update_creature_lighting still reads
Simulation::states(), as do creature display/label systems; the cost of that
state Vec at these populations was not isolated. The per-creature dark map
itself now reuses Local<HashMap>. Claims that allocation is negligible remain
unmeasured and require profiling before optimization.

**Scrub catch-up cost.** Releasing the time slider walks the simulation to
the clock in one frame — forward by plain ticks, backward by the deterministic
seed replay. Measured by `benchmark_scrub_catchup_cost` in `kn-fe-viewer`
(`#[ignore]`, release, needs `original/`) on the busiest simulation of the
rus edition — map 36 with 65 creatures — as the whole release frame
(`app.update()` around the settle), three fresh runs per jump:

| Jump | avg settle frame |
|---|---|
| 0 → 18960 (forward, 18 960 ticks) | 54–76 ms (worst of six runs 75.8) |
| 18960 → 900 (backward, seed replay of 900) | 44–48 ms |

Both sit below a tenth of a second, so a release is one dropped frame at
worst, and the backward replay branch is consistently the cheaper one; no
optimization is warranted until the frame-time gate (#98) says otherwise.

## Viewer costs

The per-frame numbers of the viewer window itself — the simulation plus the
display on the creature maps, at least the roaming-group map 17 (28 creatures,
none of which walks in 600 ticks) and the village map 19 (20 creatures, 14
walking), both editions (issue #83).

**Method.** Release build, Windows 11, AMD Ryzen 9 9950X, 64 GiB RAM, NVIDIA
GeForce RTX 2070 SUPER (the wgpu adapter of every run; the CPU's iGPU sits
idle), window 1280×800. The window benchmark is
`--benchmark 8000`: 40 settle frames, then 8000 measured frames — at the
village's ~2.4 ms frame that is 300–350 simulation ticks, several step cycles
per walker. The frame is bounded the same way the render table above bounds
it: `PresentMode::AutoNoVsync` (benchmark runs only), so the meter reads the
scene and not the display's 16.67 ms quantum; the samples are the
`DiagnosticsStore` frame time that `FrameTimeDiagnosticsPlugin` writes each
update — the same wall-clock delta the diagnostics panel shows — and the
percentiles are nearest-rank over the 8000 samples. Every `BENCH` line now
carries `max_ms` (the worst measured frame), `ticks` (sim ticks advanced
across the measured window, snapshotted when the window opens so settle time
does not leak in), `creatures`, `walking` and `labels` — a recorded line
self-describes the run, and the identity `ticks · 60 ms ≈ frames · avg_ms`
marks the honest ones. Runs while the desktop was loaded are discarded, not
averaged in: they announce themselves by `p95` shifting to 16–17 ms in
*every* configuration at once — pause included, which a paused scene cannot
cause — and they still satisfy the identity, so they measure the machine,
not the scene. The tables keep the clean rounds; both editions' clean rounds
agree within 0.1–0.3 ms, which is the reproducibility check.

**Instrumentation (AC choice, stated here):** the numbers reproduce from
instrumentation left in the code behind two environment variables — no
build flag, and no behavior change unless a variable is set.
`KN_VIEWER_LABELS=0` spawns no creature id labels (default: spawn, as
shipped), `KN_VIEWER_PAUSE=1` starts the playback paused (default: playing),
which holds the simulation and the walk clips out and leaves the settled
scene — the display floor the playing runs compare against. `knob_value`
forces the factor off on `0`, keeps the shipped default when the variable
is absent and forces it on for any other value, pinned by
`the_benchmark_knobs_default_to_the_shipped_behavior`. The two isolated
measurements below are `#[ignore]`d tests run by the recorded recipe; the
data stays in `original/`, reached through `GAME_DIR`.

```bash
cargo run --release --manifest-path crates/Cargo.toml -p kn-viewer -- \
    --game-dir original/k1/rus/game --map 17 --benchmark 8000
GAME_DIR=original/k1/rus/game cargo test --release --manifest-path \
    crates/Cargo.toml -p kn-fe-viewer --lib tests::benchmark_pose_swap_cost \
    -- --ignored --exact --nocapture
GAME_DIR=original/k1/rus/game cargo test --release --manifest-path \
    crates/Cargo.toml -p kn-fe-viewer --lib tests::benchmark_depth_slot_cost \
    -- --ignored --exact --nocapture
```

**The visualization tick with the creatures moving.** Playing runs (default
knobs), 8000 frames each, the spread of the clean rounds:

| Map (edition) | playing | paused (display floor) | ticks | p95 / p99 / max, playing |
|---|---|---|---|---|
| 17 RUS — 28 creatures, 0 walking | 3.19–3.57 ms | 3.24–3.78 ms | 425–476 | 3.70–4.28 / 4.28–5.12 / 10.6–12.8 |
| 17 ENG — 28 creatures, 0 walking | 3.15–3.20 ms | 3.06–3.15 ms | 420–426 | 3.56–3.66 / 3.95–4.12 / 10.4 |
| 19 RUS — 20 creatures, 14 walking | 2.34–2.62 ms | 2.24–2.61 ms | 311–349 | 2.73–3.23 / 3.12–3.84 / 9.3–13.4 |
| 19 ENG — 20 creatures, 14 walking | 2.33 ms | 2.23 ms | 310 | 2.75 / 3.13 / 9.1 |

The comparison the ticket asks for: **playing − paused, same round, same
map.** On the village it is +0.09/+0.10/+0.10 ms (RUS rounds 2–3, ENG round
1) — a reproducible ≈ +0.1 ms for the living scene; the calm map's rounds
span −0.59…+0.33 ms (RUS) but sit at +0.05…+0.10 ms in all three ENG rounds,
at most a few percent of the frame. Of the
village's +0.1 ms the *simulation logic* is measured to own ~1.5 µs per
frame amortized: a tick lands every ~24th frame and carries the full engine
tick of 18–22 µs ([rules tick cost](simulation.md#rules-tick-cost)), one walk-swap batch of
~10 µs (the pose change below) and the depth recompute of ≲0.5 µs — three
orders below the frame, which is why pausing is invisible in the percentiles
and the reproducible part of the difference tracks the sprites' churn
instead (the ≤85 animated pieces of the village, 37 of the calm map, cycling
their frames every tick, the walkers among them). The headless decide pass
(map 17, 1.2–1.6 µs/tick) is invisible in the window, as the calm map's
drift confirms.

**One pose change, isolated.** `benchmark_pose_swap_cost` runs the shared
creature subsystem headless (the window app minus the window, `MinimalPlugins`)
on map 19, one simulation tick per update, and times 400 updates against the
creatures whose sprite sequences swapped on each: a steady tick frame (the
simulation advances, no creature changes pose) is the baseline, and frames
where exactly one creature changes pose isolate the change. Over three runs
per edition the single-pose-change frames sit above the steady median by
+10.2–13.7 µs (RUS) and +6.9–12.2 µs (ENG) — a creature's pose change
re-installs 3.7–3.8 sprite clips (body layers plus shadow), so ~2–4 µs per
clip install and sprite rewrite. Neighbors: one full engine tick is 18–22 µs
on the same map; one data-layer pose *build* is 57–72 µs ([pose cost](data-scene.md#pose-cost))
but happens once per state or direction change, while this swap is the
per-change display cost on top of clips already built; one depth call is
~8–19 ns (below). At ~1.1 step starts per tick across the village's walkers,
pose changes amortize to ~0.2 µs per frame — the AC6 bound (one rebuild per
state change, none per frame) is what keeps it there.

**The depth recompute while creatures move.** `benchmark_depth_slot_cost`
times `DepthMap::creature_z` — the binary search over the static stacks, the
interpolation and the per-id slot. That lookup is what `sync_creature_nodes`
pays per creature per frame for the shadow's slot, and as `creature_z` for
the body z whenever a creature's bottom moves; the test runs it over the
real depth maps with the bottoms the walking creatures actually carry (live
row plus body height). Spread over
six runs × both editions × both maps: **8–19 ns per call**, so a frame's
depth recompute for the whole population is **0.2–0.5 µs** (28 creatures on
map 17, 20 on map 19). Neighbors: a thousandth of one walk-swap batch
(~10 µs) and ~1/5000–1/12 000 of the village's frame — no throttling or
caching of depth recomputes is warranted at these populations.

**The creature labels, with and without.** The `KN_VIEWER_LABELS=0` runs sit
next to the playing runs of the same round: the difference is below the
round drift on most rounds and never exceeds +0.16 ms (village RUS, 20
labels) or +0.15 ms (roaming RUS, 28 labels); the clean pairs give
+0.01–0.16 ms, i.e. ≲5 µs per label per frame — text nodes already spawned,
per-frame repositioning plus their share of the render. Neighbor: a label
costs about half a pose change per frame, and 28 labels cost under 1% of the
60 ms budget — "labels only for visible creatures" has no numbers behind it
on these maps: all creatures are visible by default, and their labels are
measured cheap.

**Historical worst frames and attribution limits.** The quiet viewer-cost
runs recorded village maxima about 9.1–9.7 ms playing versus 3.5–3.7 ms paused
(reproduced in RUS rounds 2–3 and ENG round 1), including 9.17 ms with labels
off. Calm-map spikes were about 10 ms even paused (9.5–10.6 ms RUS).
An early inference blamed tick-frame sprite churn; this was not a measured
positive cause. The later gate corrected wall-time/tick pairing and found its
worst intervals ran no simulation tick. Desktop-load runs with p95 16–17 ms
were discarded, but scheduler/driver attribution remains unconfirmed.
Keep the historical values separate from those hypotheses. The isolated CPU
work was at most about 35 µs on a tick frame, far below these spikes.

## Frame time gate

The gate turns the recorded viewer costs into a verdict (issue #98): pass =
average and p95 frame time ≤ 33.3 ms — 30 FPS — on each reference map. The
gate runs after the ambient lighting (#96) and the constant-day toggle (#97)
with lighting on, so the numbers are the final render, not a diagnostic
stage; a failing number opens its own optimization ticket, it is never
optimized inside the gate's ticket.

**Protocol.** Release build, Windows 11, AMD Ryzen 9 9950X, 64 GiB RAM,
NVIDIA GeForce RTX 2070 SUPER (the wgpu adapter of every run — Vulkan — and
the machine the "Lighting cost" table above was measured on), window
1280×800, zoom
step 3 (scale 1.0 — the startup default; a benchmark run never touches the
camera or the scene), playback playing (all creatures active, the shipped
default), day/night lighting on (the cycle follows the sim tick;
`--constant-day` off). One command runs the gate — maps 17 and 19, both
editions, 60 seconds each:

```bash
just viewer-gate
```

which is four `cargo run --release … -p kn-viewer` runs of
`--game-dir original/k1/<rus|eng>/game --map <17|19> --benchmark-secs 60`
(data in `original/`, never committed). One run prints one `BENCH …` line
and one `GATE map=.. pass=.. avg_ms=.. p95_ms=.. limit_ms=33.3` line.

**Frame bound.** The same bound the "Viewer costs" method records, so the
numbers stay comparable: benchmark runs render with `PresentMode::AutoNoVsync`
so the meter reads the scene and not the display's 16.67 ms quantum; the
samples are the raw per-frame wall-clock deltas `FrameTimeDiagnosticsPlugin`
writes into `DiagnosticsStore` each update (the diagnostic's latest
measurement, not its smoothed value); the 40 settle frames are excluded; the
percentiles are nearest-rank. The benchmark system is ordered after the
plugin's own frame-time write and after the creature chain, so the sample
and the tick counter are both complete for the iteration that reads them.
The delta itself anchors at `First` and carries the previous iteration's
wall time (bevy_time 0.20.1 `time_system` computes it there;
bevy_diagnostic 0.20.1 `diagnostic_system` writes it in `Update` from
`Time<Real>`), so `max_on_tick` pairs each sample with the previous
iteration's tick count — the ticks whose execution the sample's interval
includes — and the first sample of a window, whose interval belongs to the
settle frames, reads 0: the attribution is exact on both sides of the
pairing. A `--benchmark-secs` run closes the window
when the samples' sum crosses 60 000 ms — 60 seconds of measured frame time
whatever the hardware delivers — so the frame count is an output of the run
(`frames=` in the line), not part of the recipe. The window self-describes:
`ticks` (sim ticks across the window; a 60-second window is exactly 1000 —
`TICK_MS` is 60, `k1-data::objects_res`), `creatures`/`walking` (the
population it ended on), `labels` (whether the name labels drew), and the
identity `ticks · 60 ms ≈ frames · avg_ms` marks the honest runs — all four
runs below satisfy it to within 0.2 %.

**Reproducibility.** Three passes back to back — twice through the recorded
commands, once through the `just viewer-gate` recipe itself — were run on
the pre-integration tree: average and p95 reproduced within 0.04 ms in every
configuration, p99 within 0.21 ms, the worst frame within 0.41 ms. The
recorded tolerance stays, scoped to one machine state: repeated back-to-back
runs on a settled machine count as reproducing while average and p95 stay
within ±0.05 ms; a larger shift — across power states or across days —
means the machine state changed, not the scene. Cross-state spread is
recorded, not averaged in: the tree's passes on this table sit within
0.06 ms average and 0.11 ms p95 of each other at the widest pairwise
comparison, with the GPU idling in different power states between passes,
and the single runs around the table match them. Two shifted states were
discarded on the way: a pass taken right after a game session had exited
read 0.5–0.9 ms higher in every configuration (the driver's post-game power
state, not the scene), and runs under the desktop-load signature — p95
jumping to 16–18 ms — were discarded with it.

The table is the re-measurement the integration required — one pass per
configuration on the tree with #115–#117 in, so what the gate measures now
includes #116's per-frame name-label rendering, part of main since the
integration:

| Map (edition) | avg ms | p95 ms | p99 ms | worst ms | frames | population |
|---|---|---|---|---|---|---|
| 17 RUS | 3.08 | 3.41 | 3.90 | 10.15 | 19 478 | 28 creatures, 0 walking, labels on |
| 19 RUS | 2.28 | 2.65 | 3.01 | 8.87 | 26 281 | 20 creatures, 14 walking, labels on |
| 17 ENG | 3.11 | 3.49 | 3.94 | 10.17 | 19 319 | 28 creatures, 0 walking, labels on |
| 19 ENG | 2.33 | 2.76 | 3.15 | 9.77 | 25 715 | 20 creatures, 14 walking, labels on |

Every window covered exactly 1 000 sim ticks.

**Verdict: pass, all four configurations, with ~9× headroom.** The slowest
p95 is 3.49 ms against the 33.3 ms limit. The recorded round sits on the
pre-integration ranges — the gate's own 2.2–3.3 ms average covers this
round's 2.28–3.11 ms, the "Viewer costs" playing runs' 3.15–3.57 ms bound
the calm map from above — so #116's per-frame name labels, drawing over
every creature every frame while the `BENCH` line counts the same sprites
and light materials as before, do not show above the machine's spread. One
same-round pair prices the labels on the village (19 RUS): 2.30 → 2.28 ms
average and 2.71 → 2.65 ms p95 across `KN_VIEWER_LABELS=0`/on — under the
round's own reproducibility, so the recorded round leaves the draw cost
unpriced; the ~0.2 ms an earlier same-round pair measured on the superseded
integrated table (3.40 → 3.22 ms average, 3.98 → 3.77 ms p95) stays the
only resolved number, and which machine state lets the pair resolve it is
unconfirmed. The editions' spread on the same map is 0.03–0.05 ms here
against the superseded round's 0.8–1.0 ms ("English names draw wider than
Cyrillic ones") — the same machine-state dependence, hypothesis either
way. The pre-integration ordering (village 19 faster than calm 17) holds
on both editions: 2.28 against 3.08 ms on RUS, 2.33 against 3.11 ms on
ENG.

**p99 and the worst frame, with cause.** p99 is 3.01–3.94 ms — the tail is
the scene's own spread, not a stall class. The worst frame is 8.9–10.2 ms,
and its `max_on_tick` attribute is 0 in all four windows — measured under
the corrected pairing: a sample carries the previous iteration's wall time
(see "Frame bound"), so the flag reads the tick count of that same
iteration, and none of the intervals the worst samples measured ran a
simulation tick. Independence predicts the same (a
tick lands every ~19–26 frames here, so ~4–5 % of the window carries one), and
the "Viewer costs" isolation bounds what a tick could add — a full engine
tick is 18–22 µs, a pose swap ~10 µs, three orders below the spike. What
that leaves the worst frame to be — scheduler noise, a driver stall — stays
unconfirmed: the exclusion of tick work is measured, the positive cause is
not observed. The worst frame's position in the window (`max_at`) moves
freely — 14799 and 15615 on the calm map, 6999 and 13854 on the village —
which fits noise, not a recurring stage of the frame. Nothing here warrants
an optimization ticket: every percentile sits an order of magnitude under
the limit.

## The hour we display — our convention, not a fact about K1

AC2 of #87 asks for a clock reading, and the original shows no confirmed one, so the mapping is
ours and is recorded here as such:

- **21600 ticks of one cycle stand for 24:00**, so 15 ticks to a minute and 900 to an hour;
- **cycle tick 0 reads 06:00**, which makes window 1 a morning rather than a midnight;
- the hour is a **pure function of the simulation tick** — the viewer keeps no time of its own and
  accumulates nothing;
- **nothing in the phase or lighting logic reads the displayed hour.** Phases and light are
  computed from ticks and the [research interval table](../../research/k1/time.md#what-the-source-gives-intervals-not-day-phases); the hour is a label on the same tick.

Under that offset the intervals land at: light 03:08 → 12:00, warm band 08:00 → 10:00, dimming from
12:00, ground lights 15:00 → 03:00, neutral again at 03:08. The night is longer than the day
because the dark span is longer, which is the data, not the mapping; another offset would move the
same asymmetry, not remove it. This hour is unrelated to the 1440 divisor in the code
([research](../../research/k1/time.md#a-unit-of-1440-ticks)).

## Camera presets (ours)

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
