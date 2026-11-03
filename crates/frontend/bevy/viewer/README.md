# kn-fe-viewer

Bevy frontend of the «Князь» map viewer: viewer state (`Viewer`), the Feathers
panel, overlay and playback toggles, diagnostics, the camera binding and the
App assembly. The app hands over a `ViewerConfig` (an opened game, its
directory, the map list and the startup selection); this crate knows no CLI.

Not here:

- CLI parsing, startup resolution and the headless `check` — the app crate
  [crates/apps/viewer](../../../apps/viewer) (`kn-viewer`, bin
  `re-konung-viewer`);
- rendering, input and UI mechanics — `kn-render`, `kn-input`, `kn-ui`.

Details of the display layer: [docs/rust.md](../../../../docs/rust.md).

## Checks

```bash
just cargo-test
just cargo-lint
```
