# Architecture

This document is the single source of truth for the overall architecture of re-konung: what
components exist, what each one owns, how they are allowed to depend on each other, and what the
target shape looks like. Other documents link here instead of repeating the picture —
[docs/rust.md](docs/rust.md) keeps the Rust-side technical detail (crate-by-crate status, format
layers, decisions and their measurements), [docs/games.md](docs/games.md) keeps the series and
edition comparison, [docs/README.md](docs/README.md) indexes the format documentation.

## Overview

re-konung studies and recreates the engine behind the «Князь» series (1C/Snowball). Three
implementations live side by side and each has its own role:

- **Rust (`crates/`)** — the engine itself: a Bevy-based remaster. It is the only part intended to
  become a game.
- **Python (`konung/`)** and the **Tiled extension (`tiled/konung/`)** — independent reference
  tooling: they parse and render the original formats and let a person edit maps in Tiled. The Rust
  side links nothing from them and calls nothing in them; there is no runtime dependency in either
  direction. Python is the reference implementation of the formats: where the two disagree, Python
  is right, because it has the tests.
- **`original/`** — the owner's own game files. Never committed, never distributed; everything the
  loaders read comes from a licensed copy.

Two properties hold across the whole Rust stack:

- **Written formats are byte-exact.** Read → write reproduces the original file byte for byte for
  every file of every edition, and the round-trip is what the tests assert — today for the two
  formats that are written at all: K1 maps (`Konung.N`) and starting states (`Game.N`). Formats
  that are only read (`GRAPH.RES`, `Objects.res`, lights, flames, the EXE tables) carry no such
  guarantee yet: byte-exact writing is the goal for them, not the current state.
- **Data and game logic are Bevy-free.** `k1-data`, `k1-game` and `kn-game` compile and are tested
  headless, without a window or a GPU. Bevy appears only from layer 03 upward.

Statuses in the diagram are honest about what exists today:

| Marker | Meaning |
|---|---|
| `[X]` | Implemented in the repository. |
| `[~]` | Partial: a first slice exists, the rest is missing. |
| `[ ]` | Planned, not started. |
| `[?]` | Potential candidate only: not in the workspace, not scheduled, no dependency implied. |

## Architecture Diagram

The diagram exists only in this file.

```text
[X] Implemented   [~] Partial   [ ] Planned   [?] Potential
Double box = component/crate; single box = subsystem
Arrows indicate logical layers, not exact Cargo dependencies.

┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│   01  APPLICATIONS                                                           CLI / BINARIES  │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│  ╔══════════════════════════════════════════╗  ╔══════════════════════════════════════════╗  │
│  ║              [X] kn-player               ║  ║              [X] kn-viewer               ║  │
│  ╠══════════════════════════════════════════╣  ╠══════════════════════════════════════════╣  │
│  ║ Binary: re-konung                        ║  ║ Binary: re-konung-viewer                 ║  │
│  ║ [X] CLI / game directory / hero          ║  ║ [X] Map / scene inspection               ║  │
│  ║ [~] Game session bootstrap               ║  ║ [X] Headless checks / diagnostics        ║  │
│  ║ [ ] Full playable loop                   ║  ║ Read-only: no editing                    ║  │
│  ╚══════════════════════════════════════════╝  ╚══════════════════════════════════════════╝  │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
                                                │
                                                ▼
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│   02  FRONTENDS                                                           BEVY APP ASSEMBLY  │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│  ╔══════════════════════════════════════════╗  ╔══════════════════════════════════════════╗  │
│  ║             [X] kn-fe-player             ║  ║             [X] kn-fe-viewer             ║  │
│  ╠══════════════════════════════════════════╣  ╠══════════════════════════════════════════╣  │
│  ║ [X] Window / camera / initial map        ║  ║ [X] Feathers panel / map selector        ║  │
│  ║ [ ] Gameplay input / HUD / menus         ║  ║ [X] Overlays / animation / camera        ║  │
│  ║ [ ] Dialogue / inventory UI              ║  ║ [X] Inspection / diagnostics             ║  │
│  ╚══════════════════════════════════════════╝  ║ Read-only: no editing                    ║  │
│                                                ╚══════════════════════════════════════════╝  │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
                                                │
                                                ▼
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│   03  PRESENTATION                                                              SHARED BEVY  │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│   ╔══════════════════════════╗  ╔══════════════════════════╗  ╔══════════════════════════╗   │
│   ║      [X] kn-render       ║  ║       [X] kn-input       ║  ║        [X] kn-ui         ║   │
│   ╠══════════════════════════╣  ╠══════════════════════════╣  ╠══════════════════════════╣   │
│   ║ [X] Scene / sprites      ║  ║ [X] Keyboard / mouse     ║  ║ [X] Feathers theme       ║   │
│   ║ [X] Animation            ║  ║ [X] Drag / wheel zoom    ║  ║ [X] Checkbox sync        ║   │
│   ║ [X] Camera / cache       ║  ║ [ ] Gamepad / touch      ║  ║ [X] Pointer capture      ║   │
│   ║ [X] Light overlays       ║  ║ Bevy-specific input      ║  ║ [ ] Gameplay widgets     ║   │
│   ╚══════════════════════════╝  ╚══════════════════════════╝  ╚══════════════════════════╝   │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
                                                │
                                                ▼
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│   04  GAME ENGINE                                                       PURE RUST / NO BEVY  │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│  ╔══════════════════════════════════════════════════════════════════════════════════════════╗│
│  ║                                       [X] kn-game                                        ║│
│  ╠══════════════════════════════════════════════════════════════════════════════════════════╣│
│  ║┌──────────────────────────────────────────┐  ┌──────────────────────────────────────────┐║│
│  ║│          GAME API / WORLD MODEL          │  │          MOVEMENT / SIMULATION           │║│
│  ║├──────────────────────────────────────────┤  ├──────────────────────────────────────────┤║│
│  ║│ [X] Game facade / maps / check           │  │ [X] Isometric grid / walkability         │║│
│  ║│ [X] Scene / Piece / TextureId            │  │ [X] A* / repathing / occupancy           │║│
│  ║│ [X] Animation / groups / frames          │  │ [X] Walker / ticks / events              │║│
│  ║│ [ ] Multi-game dispatch / save-load      │  │ [X] Deterministic RNG                    │║│
│  ║└──────────────────────────────────────────┘  └──────────────────────────────────────────┘║│
│  ║                                                                                          ║│
│  ║┌──────────────────────────────────────────┐  ┌──────────────────────────────────────────┐║│
│  ║│                RULES / AI                │  │             FUTURE GAMEPLAY              │║│
│  ║├──────────────────────────────────────────┤  ├──────────────────────────────────────────┤║│
│  ║│ [X] Decision evaluation / apply          │  │ [ ] Combat / damage / death              │║│
│  ║│ [X] Failed-order cooldown                │  │ [ ] Inventory / equipment                │║│
│  ║│ [~] NPC / monster behavior               │  │ [ ] Dialogues / quests                   │║│
│  ║│ [ ] Full runtime integration             │  │ [ ] Transitions / persistence            │║│
│  ║└──────────────────────────────────────────┘  └──────────────────────────────────────────┘║│
│  ║                                                                                          ║│
│  ║┌──────────────────────────────────────────┐  ┌──────────────────────────────────────────┐║│
│  ║│               WORLD TIME                 │  │              GAME STATE                  │║│
│  ║├──────────────────────────────────────────┤  ├──────────────────────────────────────────┤║│
│  ║│ [ ] Game clock / day-night cycle         │  │ [ ] Persistent world state               │║│
│  ║│ [ ] Time-dependent events                │  │ [ ] Save / load                          │║│
│  ║│ [ ] NPC schedules / availability         │  │ [ ] World / location transitions         │║│
│  ║│ [ ] Quest / production timers            │  │ [ ] Edition-specific state               │║│
│  ║└──────────────────────────────────────────┘  └──────────────────────────────────────────┘║│
│  ║                                                                                          ║│
│  ║                      Pure Rust / no Bevy / shared game mechanics                         ║│
│  ╚══════════════════════════════════════════════════════════════════════════════════════════╝│
└──────────────────────────────────────────────────────────────────────────────────────────────┘
                                                │
                                                ▼
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│   05  GAME IMPLEMENTATIONS                                              GAME-SPECIFIC LOGIC  │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│  ╔══════════════════════════════════════════╗  ╔══════════════════════════════════════════╗  │
│  ║               [X] k1-game                ║  ║               [ ] k2-game                ║  │
│  ╠══════════════════════════════════════════╣  ╠══════════════════════════════════════════╣  │
│  ║ Konung 1                                 ║  ║ Konung 2 + 2.5 (one family)              ║  │
│  ║ [X] Resources / textures                 ║  ║ [ ] Shared resources / scene assembly    ║  │
│  ║ [X] Scene assembly / frames              ║  ║ [ ] Shared rules / edition profiles      ║  │
│  ║ [~] Creature-specific rules              ║  ║ [ ] K2.5 differences if required         ║  │
│  ║ [ ] Complete gameplay                    ║  ║ No separate k25 crate assumed            ║  │
│  ╚══════════════════════════════════════════╝  ╚══════════════════════════════════════════╝  │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
                                                │
                                                ▼
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│   06  FORMAT ADAPTERS                                                      PURE RUST CODECS  │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│  ╔══════════════════════════════════════════╗  ╔══════════════════════════════════════════╗  │
│  ║               [X] k1-data                ║  ║               [ ] k2-data                ║  │
│  ╠══════════════════════════════════════════╣  ╠══════════════════════════════════════════╣  │
│  ║ Konung 1 formats                         ║  ║ Konung 2 + 2.5 formats                   ║  │
│  ║ [X] Maps / game state                    ║  ║ [ ] Maps / state codecs                  ║  │
│  ║ [X] Graphics / objects / lights          ║  ║ [ ] Graphics / animation formats         ║  │
│  ║ [X] EXE tables / character frames        ║  ║ [ ] Edition-specific differences         ║  │
│  ║ [~] State mutations                      ║  ║ [ ] Roundtrip validation                 ║  │
│  ╚══════════════════════════════════════════╝  ╚══════════════════════════════════════════╝  │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
                                                │
                                                ▼
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│   07  ORIGINAL ASSETS                                                       NOT DISTRIBUTED  │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│  ╔══════════════════════════════════════════╗  ╔══════════════════════════════════════════╗  │
│  ║              KONUNG 1 FILES              ║  ║           KONUNG 2 / 2.5 FILES           ║  │
│  ╠══════════════════════════════════════════╣  ╠══════════════════════════════════════════╣  │
│  ║ original/k1/<edition>/game               ║  ║ User-owned original files                ║  │
│  ║ [X] K1 loader integration                ║  ║ [ ] Loader integration                   ║  │
│  ║ User-owned original files                ║  ║ [ ] Edition / format detection           ║  │
│  ╚══════════════════════════════════════════╝  ╚══════════════════════════════════════════╝  │
└──────────────────────────────────────────────────────────────────────────────────────────────┘

                   INDEPENDENT REFERENCE TOOLING (no runtime dependency)

┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│   08  TOOLING                                                        PYTHON / TILED / TESTS  │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│   ╔══════════════════════════╗  ╔══════════════════════════╗  ╔══════════════════════════╗   │
│   ║    [X] Python / konung   ║  ║   [X] Tiled extension    ║  ║     [X] Tests / docs     ║   │
│   ╠══════════════════════════╣  ╠══════════════════════════╣  ╠══════════════════════════╣   │
│   ║ [X] K1 format reference  ║  ║ [X] K1 map editing       ║  ║ [X] Format roundtrips    ║   │
│   ║ [X] Reference renderer   ║  ║ [X] Import / export      ║  ║ [X] Headless checks      ║   │
│   ║ [ ] K2 / K2.5            ║  ║ [ ] K2 / K2.5            ║  ║ [ ] Gameplay E2E         ║   │
│   ╚══════════════════════════╝  ╚══════════════════════════╝  ╚══════════════════════════╝   │
└──────────────────────────────────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│   09  POTENTIAL COMPONENTS                                               NOT YET COMMITTED   │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                              │
│                  ╔══════════════════════════════════════════════════╗                        │
│                  ║                   [?] kn-data                    ║                        │
│                  ╠══════════════════════════════════════════════════╣                        │
│                  ║ Shared engine data models                        ║                        │
│                  ║ World / Map / Entity / Scene / GameState         ║                        │
│                  ║ Common IDs / value types                         ║                        │
│                  ║ Pure Rust / no gameplay / no Bevy                ║                        │
│                  ║                                                  ║                        │
│                  ║ Extract only when independently reusable         ║                        │
│                  ╚══════════════════════════════════════════════════╝                        │
│                                                                                              │
│   [?] Candidate only. Not in workspace, not scheduled, no dependency implied.                │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

## Component Responsibilities

| Layer | Component | Path | Owns | Must not know |
|---|---|---|---|---|
| 01 | `kn-player` | `crates/apps/player` | Binary `re-konung`: CLI, opening the game directory, resolving the hero from the starting state, handing a `PlayerConfig` to the frontend | Bevy, K1 formats |
| 01 | `kn-viewer` | `crates/apps/viewer` | Binary `re-konung-viewer` plus a library: CLI, startup resolution, headless `check` (JSON report, nonzero exit on failure) — no window, no GPU context | Bevy app construction (linked only transitively) |
| 02 | `kn-fe-player` | `crates/frontend/bevy/player` | Bevy `App` assembly of the player: window, camera binding, start-map loading | CLI, game rules, formats |
| 02 | `kn-fe-viewer` | `crates/frontend/bevy/viewer` | Bevy `App` assembly of the viewer: viewer state, Feathers panel, map selector, overlay toggles, playback, frame diagnostics, scene rebuild policy | CLI, formats, game rules |
| 03 | `kn-render` | `crates/frontend/bevy/render` | Scene pieces → sprites in z-order, texture cache keyed by `TextureId`, per-frame animation, camera state. An overlay is the `Visibility` of a piece-group node, never a map rebuild | K1, formats, game rules, input bindings |
| 03 | `kn-input` | `crates/frontend/bevy/input` | Input mechanisms only: keyboard pan direction, mouse drag delta, wheel accumulation into zoom steps | Camera types, UI, game rules |
| 03 | `kn-ui` | `crates/frontend/bevy/ui` | Shared UI mechanics: the Feathers dark theme, mirroring a checkbox state back onto the widget, the pointer-over-UI test | Panels, widget state, rendering, input bindings, game rules |
| 04 | `kn-game` | `crates/game/kn-game` | The series facade and the shared engine: `Game` (open a directory, list maps, build a `Scene`, decode textures, `check`), the Bevy-free model `Scene`/`Piece`/`TextureId`/`Animation`/`PieceGroup`; the shared movement engine (isometric fine grid, walkability, occupancy, A\* routes, repathing, per-tick walking, events, deterministic RNG); applying creature decisions with a failed-order cooldown | Bevy, window, GPU |
| 05 | `k1-game` | `crates/game/k1/k1-game` | Knowledge of «Князь 1»: resources, texture keys and RGBA decode, the map draw list with piece groups, animation frame assembly, creature-specific rules | Bevy, the other parts of the series |
| 06 | `k1-data` | `crates/game/k1/k1-data` | K1 binary formats as pure `parse(data: bytes)` functions plus a separate filesystem layer: repository (`repo`), `map`, `game_state`, `game_edit`, `graph_res`, `objects_res`, `lights_res`, `flames_res`, `executable`, `characters`, `rle`, `palette` | Game rules, Bevy, the filesystem inside a parser |
| 08 | `konung/` (Python) | `konung/` | Reference parser and reference renderer for K1 formats; CLI `konung`. The arbiter when Rust and Python disagree | — (independent of the Rust stack) |
| 08 | Tiled extension | `tiled/konung/` | Opens original `Konung.N` with `Game.N` objects inside Tiled and saves them back byte-exactly; this is where map editing happens | — (independent of the Rust stack) |

### Boundary rules

- **`kn-game` is the world model, in pure Rust.** Everything the display needs comes through it,
  and it contains no Bevy type. The movement engine reads no game file: the part's knowledge
  arrives through the `Passage` (cell walkability) and `Pace` (step duration in ticks) traits, so
  the same engine serves any part of the series. It keeps no clock of its own — it counts ticks,
  and how long a tick is belongs to the outer world.
- **`k1-game` / `k2-game` hold one part's specifics** — resources, textures, draw order, creature
  rules, and (for K2/K2.5, once their data is studied) the shared family logic. Game rules come
  from the original, never from our heads: an unconfirmed rule is written up as unknown in `docs/`
  and left unimplemented.
- **`k1-data` / `k2-data` hold binary formats and serialization** — reading and byte-exact writing
  of the original files, with unknown bytes preserved rather than dropped.
- **`kn-render`, `kn-input`, `kn-ui` are the Bevy-specific mechanisms**, reusable by any frontend;
  `kn-input` and `kn-ui` depend on nothing but `bevy`.
- **`kn-fe-*` assemble the Bevy apps** and hold the frontend logic (what a click does, when a
  scene is rebuilt); they own no game rule and no format.
- **`kn-player` / `kn-viewer` are executables**: CLI parsing and process exit codes only.
- **The viewer is read-only.** It inspects maps and reports diagnostics; it never writes a game
  file. Editing is a Tiled job through the extension, not a viewer feature.
- **Python and Tiled are reference tooling with no runtime dependency** on the Rust side or
  vice versa; they stay as they are while Rust replaces nothing yet.

## Dependency Boundaries

The arrows in the diagram are logical layers (who is allowed to know about whom), not Cargo edges.
The edges below are what the `Cargo.toml` files actually declare today (workspace
`crates/Cargo.toml`, resolver 3, edition 2024, Bevy 0.20).

| Crate | Declared path dependencies | External |
|---|---|---|
| `kn-player` | `kn-fe-player`, `kn-game` | `clap` |
| `kn-viewer` | `kn-fe-viewer`, `kn-game` | `clap`, `serde_json`, `thiserror` |
| `kn-fe-player` | `kn-game`, `kn-render`, `kn-input`, `kn-ui` | `bevy` |
| `kn-fe-viewer` | `kn-game`, `kn-render`, `kn-input`, `kn-ui` | `bevy` (`bevy_feathers`) |
| `kn-render` | `kn-game` | `bevy`, `thiserror` |
| `kn-input` | — | `bevy` |
| `kn-ui` | — | `bevy` (`bevy_feathers`) |
| `kn-game` | `k1-data`, `k1-game` | `pathfinding`, `serde`, `thiserror` |
| `k1-game` | `k1-data` | `thiserror` |
| `k1-data` | — | `thiserror` |

Dev-dependencies are left out of the table: `tempfile` in `k1-data`, `k1-game`, `kn-game` and
`kn-viewer` for tests on synthetic files; it adds no edge to the graph.

Rules that hold today:

- Dependencies point down the stack; nothing in layers 03–06 names a K1 type except inside the K1
  crates and `kn-game`'s facade.
- `kn-input` and `kn-ui` are leaves: `bevy` and nothing else.
- No Bevy dependency in `k1-data`, `k1-game`, `kn-game`; their tests run headless, and
  `kn-viewer check` creates no window or GPU context.
- The workspace has ten members and no `k2-*` or `kn-data` crate.

## Multi-game Support

The series has four games; this project targets the first three («Князь 3» is 3D and out of scope,
see [docs/games.md](docs/games.md)) — parts two and 2.5 only if the shared engine is confirmed —
and only the first is implemented today:

- **«Князь: Легенды Лесной страны» (1999)** — studied and implemented: `k1-data` + `k1-game`,
  Python reference, Tiled extension.
- **«Князь 2» (2003) and «Князь 2.5» (2004–2005, a continuation built on top of part two)** — one
  target branch: `k2-data` and `k2-game` for the pair, not a separate crate per part. Their formats
  are not studied yet (no data has been examined), so no field of theirs is claimed anywhere in the
  diagram. Years, teams and publishers — [docs/games.md](docs/games.md).

Hypotheses are kept apart from decisions:

- **Confirmed decision.** Shared code is extracted when the commonality has been demonstrated, not
  in advance. The one exception is movement: the shared engine is built in `kn-game` now, on the
  assessment that K1 and K2 movement is nearly identical with running as the visible difference,
  which fits the same model; a part's specifics stay in `k1-game`.
- **Hypothesis.** «Князь 2» and «Князь 2.5» run on one engine, and their formats and rules are
  close enough to share a crate pair. Not yet checked against the files — see
  [docs/games.md](docs/games.md). Until it is checked, `k2-game`/`k2-data` stay `[ ]`.
- **Hypothesis.** K2's noticeable difference from K1 is running; anything else is unverified.
- **Not a plan.** Nothing about K2/K2.5 formats is implemented, and nothing here should be read as
  a schedule.

Adding a part is meant to be additive: `kn-game` keeps one API and a second backend (`k2-game` +
`k2-data`) appears beside the K1 one, with the game selected at the application edge. Today the
facade hardwires K1 (see the next section) — that is the known gap, not an oversight of the model.

## Current vs Target Architecture

The diagram draws the intended layering; the `[X]` and `[~]` markers say what exists today. Three
boundaries differ between the two, and each difference is deliberate rather than accidental:

| Boundary | Target | Today |
|---|---|---|
| Game dispatch | `kn-game` is backend-agnostic and selects a part's implementation behind one API | `kn-game` depends directly on `k1-data` and `k1-game` and calls them by name; there is one backend, so no dispatch exists (`[ ] Multi-game dispatch`) |
| Executables | `kn-player` and `kn-viewer` are thin CLI shells over their frontends | Holds for both, with one extra edge: each app also names `kn-game` directly (it opens the game and builds the config passed to the frontend) |
| Presentation | Layer 03 crates are shared mechanisms | `kn-render` depends on `kn-game` (it consumes the shared scene model); `kn-input` and `kn-ui` depend on nothing but `bevy` |

Working notes, not architecture:

- `kn-viewer` is a library plus a binary (`check` is exercised by tests through the library);
  `kn-player` is a binary only.
- Rules are evaluated in `k1-game` and applied in `kn-game` (`rules`), with the failed-order
  cooldown on the engine side. No Bevy app drives them yet — `Full runtime integration` is `[ ]`.
- Save/load, the game clock, day-night, production and tribute timers, inventory, dialogues and
  transitions exist nowhere in Rust; they are `[ ]` in the diagram and their original behaviour is
  recovered only through the reverse-engineering loop documented in `docs/`.
- The Python package and the Tiled extension remain the reference; Rust replaces nothing yet.

## Potential Components

`[?] kn-data` — a possible crate for shared engine data models (world, map, entity, scene, game
state, common identifiers and value types) with no gameplay and no Bevy. It is **a candidate only**:
not in the workspace, not scheduled, no crate is created for it and no dependency on it exists or is
implied. It becomes real only when a second consumer makes the models independently reusable — the
same rule as everywhere else: shared code follows demonstrated commonality.
