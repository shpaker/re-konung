# Inspect maps in the viewer

[Set up the checkout](setup.md) and [prepare your game files](game-files.md).
The viewer is a read-only development tool: it inspects and simulates maps;
editing original files is a [Tiled](tiled.md) task.

```sh
just viewer 17
GAME_DIR=original/k1/eng/game just viewer 4 tutorial
just viewer-release 17
```

Without a map number, the first map of the source opens. Tutorial maps exist in
ENG only. `GAME_DIR` defaults to `original/k1/rus/game`.

## Controls

| Action | Control |
|---|---|
| Choose a map or show overlays | Left panel |
| Pan | WASD, arrows or left-button drag |
| Zoom in discrete steps | Mouse wheel / trackpad, outside the panel |
| Play/pause simulation and animation | Play checkbox or Space |
| Step one tick forward/back | `.` / `,` |
| Keep daylight while the clock runs | Constant day checkbox or `N` |
| Return creatures to their spawn positions | Reset positions |
| Restart animation and simulation at tick zero | Reset animation |
| Inspect the day cycle | Time slider and phase presets |

A new map fits the camera to its content. Overlays do not rebuild the map or
reset playback. Hidden creature overlays hide names too. A map switch starts an
independent simulation; the constant-day preference survives until the viewer
exits. A paused special resumes where it stopped. Reset positions preserves the
clock, camera and pause state; Reset animation starts the sequence again.

The panel distinguishes visible sprites from all assembled sprites and reports
texture failures. Creature labels show `#<id> <name>` when a name exists.
An invalid CLI map number fails instead of silently choosing another map.

## Headless checks

```sh
just check-maps
GAME_DIR=original/k1/eng/game just check-maps tutorial
cargo run --release --manifest-path crates/Cargo.toml -p kn-viewer -- \
    --game-dir original/k1/rus/game check --all --format json
```

`check` needs no window or GPU context and returns nonzero for map construction
or texture-decode errors. Use `check --map 1` to select one map.
The viewer also exposes `simulate`; `cargo run --release --manifest-path
crates/Cargo.toml -p kn-viewer -- --help` lists its options.

For benchmark flags, debug variables and measured frame-time gates, see
[viewer implementation](../development/rust/viewer.md).
