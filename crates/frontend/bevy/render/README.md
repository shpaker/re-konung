# kn-render

Shared Bevy presentation of kn-game scenes, without format knowledge or game rules.

- TextureCache shares Image handles by TextureId; scene maps pieces to sprites and visibility groups.
- animation owns the map Clock and sprite sequences; camera owns framing, zoom and bounds, without input bindings.
- CreatureAnimationPlugin owns composite nodes, clip swaps, depth/interpolation and simulation catch-up ordering.
- Lighting applies game-supplied parameters; the game layer decides creature states and behavior.

Details: [topic documentation](../../../../docs/development/rust/rendering.md).
Checks: `just cargo-test`, `just cargo-lint`. Pure logic tests need no window/GPU;
real-data cases use local original/ and skip when absent.
