# Rust implementation

The Cargo workspace uses edition 2024. Rust implements the game; Python and Tiled
remain independent reference tools. Python is authoritative on format disagreements.
[Architecture](../../../ARCHITECTURE.md) owns crate responsibilities and dependencies.

| Topic | What it explains |
|---|---|
| [Data and scenes](data-scene.md) | Loading, parser boundaries, graphics, draw order, piece groups, frames and pose cost |
| [Movement](movement.md) | Grid, occupancy, routing, ticks, events, repathing and engine cost |
| [Simulation](simulation.md) | Creature rules, placement, live state, time and lighting models, allocation cost |
| [Rendering](rendering.md) | Sprites, textures, animation, creature nodes, depth, interpolation and camera |
| [Viewer](viewer.md) | Tool state, playback, diagnostics and full-window performance measurements |
| [Player and UI](player-ui.md) | Application flow, settings, design-system mechanics and new-game runtime contracts |

Start with [builds](../building.md) and [tests](../testing.md) for commands.
For operation use the [player](../../guides/player.md) or
[viewer](../../guides/viewer.md) guide. Facts behind game-specific behavior are
in [K1 research](../../research/k1/README.md).
