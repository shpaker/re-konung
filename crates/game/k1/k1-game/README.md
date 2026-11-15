# k1-game

K1 resources, scene assembly and recovered rules, independent of Bevy.

- resources/textures supply graphics and RGBA decoding; scene constructs an ordered draw list.
- creatures, placement, rules and creature_anim hold game-specific knowledge.
- new_game, time_of_day and lighting expose pure rule/model operations.
- Consumers see this through kn-game. This crate does not import the shared engine.

Details: [topic documentation](../../../../docs/development/rust/simulation.md).
Checks: `just cargo-test`, `just cargo-lint`. Pure logic tests need no window/GPU;
real-data cases use local original/ and skip when absent.
