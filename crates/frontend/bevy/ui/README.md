# kn-ui

Shared Bevy UI mechanics for the «Князь» frontends. Three items, nothing more:

- `dark_theme` — the shared Feathers dark theme resource the frontends insert
  into their `App`;
- `sync_checked` — checkbox mirroring: Bevy's `Checkbox` computes the new
  value as `!Has<Checked>` and never writes `Checked` itself, so the state has
  to be written back to the widget by hand;
- `pointer_over_ui` — the pointer-capture check: the cursor counts as being
  over the interface when a `Hovered` **value** under it is true, not when the
  component merely exists (Feathers attaches it to every control at spawn).

What does not live here: app-specific panels, widget state, input bindings,
rendering and game rules — those stay in the frontends (`kn-fe-viewer`, `kn-exec`)
and their layers.

Details — [docs/rust.md](../../../../docs/rust.md).

## Checks

```bash
just cargo-test
just cargo-lint
```
