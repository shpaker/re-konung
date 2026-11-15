<a id="inspect-maps-in-the-viewer"></a>

# Inspecting maps

[Prepare the environment](setup.md) and [your game files](game-files.md).
The viewer is a read-only development workspace: it displays maps and runs
creature simulation. Original-file editing belongs to [Tiled](tiled.md).

```sh
just viewer 17
GAME_DIR=original/k1/eng/game just viewer 4 tutorial
just viewer-release 17
```

Without a number, the first map of the source opens. Tutorials are available
in ENG. `GAME_DIR` defaults to `original/k1/rus/game`.

<a id="controls"></a>

## Workspace controls

The top row filters maps by source and number. The right inspector finds
creatures by ID or name and shows their recorded state, current simulation
state and actual rendered body clip. Expanded diagnostics show individual
render layers and scene counters. The centre is the map camera's viewport;
interface panels use a separate full-window camera.

| Action | Control |
|---|---|
| Select a creature / clear selection | Select tool, left click a body pixel / empty map |
| Pan | Pan tool + left drag, or middle drag with either tool |
| Pan with keyboard | WASD or arrows while interface controls have no focus |
| Zoom in discrete pixel-art steps | Wheel over the map |
| Focus selection / fit the displayed map | Focus / `F`; Fit / `Home` |
| Close disclosures, cancel a gesture, return to Select | `Esc`; selection remains |
| Play / pause | Bottom bar or `Space` |
| Step one tick forward / backward and pause | `+ tick` / `− tick`, or `.` / `,` |
| Hold day lighting while the clock continues | Constant day, or `N` |
| Explore the day cycle | Expand Seek simulation; scrub or use a preset |
| Reset creature positions | Reset positions; clock and animation state remain |
| Restart creature animation | Reset animation; positions and clock remain |

A drag that starts over the interface stays owned by it until release, even
when the pointer enters the map. Focused interface controls own keyboard
input. Hidden bodies, transparent pixels and shadows do not receive map
selection; hiding Characters preserves selection for the inspector.

Characters, lighting and diagnostic overlays are grouped under Display.
Playback speed (0.25–8×) applies to the same clock for simulation, all animation
and lighting; it survives pause and map changes. Playback,
current tick and displayed time stay visible at the bottom. The inspector
scrolls on smaller windows. A failed map switch retains the old scene and
selection, identifies requested and displayed maps, and offers Retry.
Only a successful map change clears selection. Constant day survives map
changes; overlays do not rebuild the scene or reset playback.

<a id="session-control"></a>

## Local session control

Control is disabled unless the window is started with a new session-file path:

```sh
cargo run --manifest-path crates/Cargo.toml -p kn-viewer -- \
  --game-dir original/k1/rus/game --map 17 --control-file out/viewer-session.json
cargo run --manifest-path crates/Cargo.toml -p kn-viewer -- \
  control --session out/viewer-session.json --request '{"type":"state"}'
```

Create `out/` first. The session file contains an ephemeral loopback TCP
address, a random session token and protocol version 1. Existing files are
rejected; a normal shutdown removes the file. Keep its token local. The
`control` client does not require `--game-dir` or open game data.

Each connection accepts one newline-terminated JSON request, up to 64 KiB:
`{"id":"review-1","token":"…","action":{"type":"state"}}`.
The CLI supplies the envelope and prints one response. Responses preserve
the request ID, include `success`, `state`, and either `data` or an error
object with `code` and `message`. Invalid IDs, fields, actions and tokens
produce errors. Mutations execute serially on the viewer's main thread. Read-only State and
Creatures requests can inspect progress during a pending seek or capture; they
do not change its timeout or target. UI mutations wait behind an active control
action, while camera input and hover remain responsive.

| Action JSON | Purpose |
|---|---|
| `{"type":"state"}` | Read the complete workspace projection |
| `{"type":"creatures","search":"guard"}` | List creatures; optional ID/name substring |
| `{"type":"select","id":12}` | Select an existing creature; explicit `null` clears |
| `{"type":"pick","x":500,"y":300}` | Select at logical window pixels using actual body alpha |
| `{"type":"mode","mode":"pan"}` | Select `select` or `pan` tool |
| `{"type":"map","source":"game","number":19}` | Request a game/tutorial map |
| `{"type":"retry_map"}` | Retry the currently requested map |
| `{"type":"display","layer":"foot","visible":true}` | Set characters, foot, lights or markers visibility |
| `{"type":"playback","playing":false}` | Pause/resume |
| `{"type":"speed","rate":4}` | Set 0.25, 0.5, 1, 2, 4 or 8× for the single scene clock |
| `{"type":"step","ticks":1}` | Step ticks and pause |
| `{"type":"seek","tick":11000}` | Seek an absolute scene-clock tick and pause |
| `{"type":"constant_day","enabled":true}` | Set lighting hold |
| `{"type":"reset_positions"}` / `{"type":"reset_animation"}` | Perform the two distinct resets |
| `{"type":"focus_selection"}` / `{"type":"fit_map"}` | Frame selection/map |
| `{"type":"window","width":960,"height":640}` | Resize logical window content; dimensions 1–8192 pixels |
| `{"type":"camera","x":100,"y":200,"zoom":5}` | Optional centre coordinates and zoom-ladder index |
| `{"type":"capture","path":"out/workspace.png"}` | Save the current workspace and paired `out/workspace.json` |
| `{"type":"escape"}` | Close disclosures and return to Select |

`state` distinguishes requested/displayed maps, clock/simulation ticks,
readiness phase, render acknowledgement frame, selection, mode, visible
layers, camera, inspector facts and actual rendered animation layers.
The projection distinguishes wrapper elapsed `simulation_local_tick`, the
simulation's logical `simulation_tick_number`, and `simulation_epoch`.
`simulation_tick` maps completed catch-up progress onto the scene clock,
normally `simulation_epoch + simulation_local_tick`. Relative steps before the
load/reset epoch clamp local progress at zero; the reached scene-clock value can
then be earlier than that epoch without inventing earlier creature history.
Reset positions resets wrapper elapsed time and moves the epoch while preserving
logical simulation time. Absolute Seek and released UI scrubs instead share the
same rebase to epoch zero and seeded forward/backward replay.
`lighting_preview` identifies a held scrub; `preview_tick` names its target.
Held scrubs preview lighting and animation frames while creature positions wait;
release recomputes their movement. UI scrub preserves the previous play/pause
preference; the explicit control Seek pauses. Seeks advance within
a 64-tick/4-ms frame budget. A timed-out seek stops at its last completed tick.
Map switching, seek and capture wait for the relevant scene/simulation/render
work. An error retains the displayed scene and remains visible in read state.

Capture holds playback and workspace input until screenshot readback and both
file writes finish, then restores the previous play state. Its paired JSON
records the state of the captured render frame, rather than the later frame
when GPU readback returns. Save failures and readiness timeouts return errors;
a fixed frame count or the existence of a file is not a success condition.
Captures contain local game-derived pixels and must remain in `out/`.

<a id="headless-checks"></a>

## Headless checks

```sh
just check-maps
GAME_DIR=original/k1/eng/game just check-maps tutorial
cargo run --release --manifest-path crates/Cargo.toml -p kn-viewer -- \
  --game-dir original/k1/rus/game check --all --format json
```

`check` creates no window/GPU context and returns a nonzero exit code when a
map or texture fails. Use `check --map 1` for one map. The existing `simulate`,
`--screenshot`, `--benchmark` and `--benchmark-secs` commands remain available;
see `--help` and [implementation and measurements](../development/rust/viewer.md).
