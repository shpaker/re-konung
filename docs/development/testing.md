# Testing and reference parity

From the repository root:

```sh
just check
just test
just cargo-test
just cargo-lint
```

`just check` invokes `prek run --all-files --stage manual`: whitespace/newline
checks, YAML/TOML/JSON and file checks, copyright/download restrictions, ruff,
Rust formatting/clippy, Python tests and Rust tests. Some hooks format files;
inspect the resulting diff. Never bypass hooks. `just setup` installs shims.

## Hooks and CI

`prek.toml` is authoritative: pre-commit checks/formatters include Rust clippy;
commit-msg checks Conventional Commits and forbidden attribution; pre-push adds
Python and Rust tests. `.github/workflows/ci.yml` runs ruff/pytest, Rust
fmt/clippy/tests, a Linux app build gate and a Windows post-merge smoke build.
Dependency caching **is configured**; profiles, system libraries and timings
are in [builds](building.md). PR/issue form rules and their tests are described
in [CONTRIBUTING.md](../../CONTRIBUTING.md).

## Synthetic and headless tests

Synthetic algorithm tests need no game data. Movement tests (`kn-game/tests/
movement.rs`, `movement_grid.rs`) exercise invented grids, routes, steps,
crowds and deterministic traces without Bevy. movement_grid also compares
K1/Python coordinate formulas over all 51200 cells: a test-only dependency,
not an engine-layer dependency. Half-cell rounding in character_cell/rect_cells
uses round_ties_even, matching Python round rather than Rust's ordinary .5 rule.

Game crates are Bevy-free. Presentation tests use MinimalPlugins without a
window/GPU to check piece transforms/z, camera fitting/zoom, overlays, CLI,
error reports, tick-frame selection, fractional time, pause/off and changed
frame centers. Player creature tests use the real window pipeline without the
window: start-map creatures exist and state-4 specials remain visible until
idle; real-data cases skip when originals are absent.

## Real-file tests

[Prepare the owner's files](../guides/game-files.md) under original/.
Python data tests skip without them. Rust discovers editions dynamically and
prints a skip reason when original/ is absent. If original/k1 exists but no
edition can be found, Rust fails rather than passing vacuously. CI has no game
files and skips correctly. Fixtures are synthetic, never resource excerpts.

### Format writing and graphics

Rust k1-data tests original.rs, original_game_state.rs, original_game_edit.rs
and original_graphics.rs mirror Python tests/k1/test_maps.py,
test_game_state.py, test_game_edit.py and test_graphics.py. Coverage includes:

- Byte-exact parse→serialize for every Konung.N, Game.N/TGame.N and
  LIGHTS.N/TLIGHTS.N of both editions, plus ENG Tutorial.4/5's raw slot 994.
- State links, hero start, independently checked character-field offsets
  (block starts 0x2A69A), dense reserves before/after edits, reserve growth,
  NPC copies with their own group/items and atomic capacity failures.
- Graphics parse to EOF/block boundaries, simple/dynamic object composition,
  unique EXE table discovery, flame range coverage and character rendering.
  The one unnamed villager is a recorded data exception, not a failed parser.

Serialize checks exact row geometry before writing: an 81-cell row paired
with a 79-cell row has the expected total byte count but wrong row boundaries.
Python and Rust enforce the same rule (Python parity added in #18).

Read-only graphics have no serialization round-trip. Cross-language digests
compare decoded RGBA tiles/sprite frames and the GRAPH.RES color bank on local
data. EXE text test text_tables_match_the_python_reference invokes Python via
KONUNG_PYTHON, the checkout's .venv, uv run python, then python3. It compares
entry counts, FNV-1a 64 over UTF-8 character/map-name tables and names assembled
for every Game.N character. Only the digest algorithm is stored: no game
strings or constants derived from them. Without editions it spawns no process.

### Scene and renderer parity

k1-game original_scene and kn-game original_facade check reference positions,
nonempty tiles and drawable decorations, data-driven overlays, deterministic
draw lists, texture decoding and reproducible check reports.

A scene built with all layers and group-filtered equals a scene built for
those layers: group_filter_matches_a_scene_without_the_layer checks every
map and each nonempty layer; pieces_carry_their_group checks facade transport.
Animation's base image/corner matches the pre-animation static piece; visible
counts therefore remain unchanged, while layers absent at the base get hidden
pieces. Flames traverse their EXE ranges at one tick/frame; object sequences
have frame_count frames of frame_time ticks. Each piece keeps its layer's
texture identity. Synthetic layer_track/pad_frames cases retain missing frames,
omit animation for one frame and omit pieces for wholly empty layers.

kn-render original_sprites runs a real Bevy world without a window. Every
group has a node in PieceGroup::ALL order; GlobalTransform z matches draw-list
position. VisibilityPlugin verifies inherited hiding, including animated and
empty frames. Hidden groups retain Animator. Group bounds match scene_bounds
for visible layers, supporting camera-bound recomputation.

## End-to-end inspection and measurements

[Viewer headless checks](../guides/viewer.md#headless-checks) build scenes and
decode every referenced texture. Recorded ENG runs covered 43 game maps and
both tutorials in about 5 s release, reporting piece/texture counts, animated
pieces, frame counts and pieces visible at the base.

Benchmarks are ignored tests or explicit release runs, not routine checks.
Commands and historical conditions live with
[movement](rust/movement.md#engine-cost), [poses](rust/data-scene.md#pose-cost),
[simulation](rust/simulation.md) and [viewer](rust/viewer.md) measurements.
For a visible format change compare Python output with a Tiled render; keep
images local in out/. Native pointer acceptance is separately recorded in
[macOS testing](macos-testing.md).
