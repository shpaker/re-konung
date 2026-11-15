# kn-game

The shared facade, scene model, movement engine and simulation, without Bevy.

- Game::open/maps/heroes/scene/textures/check exposes K1 through common types.
- Scene/Piece/TextureId/Animation/PieceGroup carry presentation data without frontend types.
- movement owns Grid, Cell, Direction, Occupancy, find_route, Walker and World; Passage/Pace supply game-specific data.
- Simulation evaluates K1 decisions and applies orders through the shared world, including failed-order cooldowns.
- The movement engine never decides where to go; recovered behavior is evaluated in k1-game.

Details: [topic documentation](../../../docs/development/rust/README.md).
Checks: `just cargo-test`, `just cargo-lint`. Pure logic tests need no window/GPU;
real-data cases use local original/ and skip when absent.
