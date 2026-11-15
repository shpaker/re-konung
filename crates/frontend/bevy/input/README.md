# kn-input

Shared Bevy input mechanisms; camera bindings belong to frontends.

- pan_direction reads keyboard direction; drag_delta supplies mouse movement.
- ZoomScroll/wheel_steps accumulate wheel/trackpad input into bounded zoom steps.
- No camera types, UI state or game rules; pointer capture belongs to kn-ui.

Details: [topic documentation](../../../../docs/development/rust/rendering.md#camera-and-input-seam).
Checks: `just cargo-test`, `just cargo-lint`. Pure logic tests need no window/GPU;
real-data cases use local original/ and skip when absent.
