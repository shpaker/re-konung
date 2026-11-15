# kn-fe-viewer

The viewer Bevy app: Viewer state, Feathers controls, overlays, playback and diagnostics.

- Receives ViewerConfig; CLI parsing stays in kn-viewer.
- Builds maps with all layers; toggles visibility rather than rebuilding.
- Adds creature labels and measurements around the shared render/creature pipeline.
- Remains read-only: no map/state file writing.

Details: [topic documentation](../../../../docs/development/rust/viewer.md).
Checks: `just cargo-test`, `just cargo-lint`. Pure logic tests need no window/GPU;
real-data cases use local original/ and skip when absent.
