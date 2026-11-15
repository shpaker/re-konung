# k1-data

K1 binary formats and repository access, without Bevy or game rules.

- repo::GameDir handles case-insensitive file discovery; parsers consume bytes separately.
- Maps, light and starting states support byte-exact writing; graphics and EXE tables are read-only.
- game_edit preserves dense roster reserves and cross-references.
- Unknown bytes are preserved. Python is authoritative on format disagreements.

Details: [topic documentation](../../../../docs/research/k1/README.md).
Checks: `just cargo-test`, `just cargo-lint`. Pure logic tests need no window/GPU;
real-data cases use local original/ and skip when absent.
