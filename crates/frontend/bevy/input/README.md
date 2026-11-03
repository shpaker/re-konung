# kn-input

Input mechanisms for the «Князь» frontends: keyboard direction
(`pan_direction`), left-button drag (`drag_delta`) and wheel accumulation
(`ZoomScroll`, `wheel_steps`). Only `bevy` — no camera types, no UI, no game
rules.

The mechanisms are not systems: each app binds them to its own camera in a
small binding system registered in the `CameraInput` set of
[`kn-render`](../render). The layout and the set order are described in
[docs/rust.md](../../../../docs/rust.md).

## Checks

```bash
just cargo-test   # wheel accumulation, key map, drag gate
just cargo-lint
```
