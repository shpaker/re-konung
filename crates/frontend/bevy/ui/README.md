# kn-ui

The shared Bevy UI design system for the «Князь» frontends: one look for the
player and the viewer. What lives here:

- `palette`, `spacing`, `typo` — the design tokens: a dark warm-neutral palette
  with a muted amber accent, spacing and typography scales, and the bundled
  PT Sans faces (full Cyrillic and Latin; license — `assets/fonts/OFL.txt`);
- `theme` — the konung theme over Bevy Feathers: the Feathers dark theme with
  the semantic colors remapped onto the konung palette;
- `widgets` — BSN helpers that compose Feathers controls with the konung fonts
  (controls are adopted, not re-implemented);
- `KonungUiPlugin` — Feathers, the konung theme and the `KonungFonts` handles
  in one `add_plugins` call;
- `sync_checked` — checkbox mirroring: Bevy's `Checkbox` computes the new
  value as `!Has<Checked>` and never writes `Checked` itself, so the state has
  to be written back to the widget by hand;
- `pointer_over_ui` — the pointer-capture check: the cursor counts as being
  over the interface when a `Hovered` **value** under it is true, not when the
  component merely exists (Feathers attaches it to every control at spawn).

The `showcase` example shows all of it alive:

```bash
just ui-showcase
# or: cargo run --profile dev-runtime --manifest-path crates/Cargo.toml -p kn-ui --example showcase
```

What does not live here: app-specific panels, widget state, input bindings,
rendering and game rules — those stay in the frontends (`kn-fe-viewer`,
`kn-fe-player`) and their layers.

Details — [docs/rust.md](../../../../docs/rust.md).

## Checks

```bash
just cargo-test
just cargo-lint
```
