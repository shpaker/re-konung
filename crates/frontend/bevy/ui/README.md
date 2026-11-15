# kn-ui

The shared Bevy UI design system for the player and viewer.

- palette/spacing/typo define tokens and bundled PT Sans (Cyrillic/Latin, assets/fonts/OFL.txt).
- KonungUiPlugin applies the Feathers theme and font normalization; widgets composes controls.
- sync_checked mirrors state to checkboxes; pointer_over_ui tests the Hovered value.
- App panels, widget state, bindings, rendering and game rules belong to their respective consumers.

Details: [topic documentation](../../../../docs/development/rust/player-ui.md).
Checks: `just cargo-test`, `just cargo-lint`. Pure logic tests need no window/GPU;
real-data cases use local original/ and skip when absent.

Live design-system catalogue: `just ui-showcase` (statically linked example).
