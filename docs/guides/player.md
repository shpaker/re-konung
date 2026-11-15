# Run the player

[Set up the checkout](setup.md) and [prepare your game files](game-files.md).
The player currently provides the menu, settings, character creation and the
start-map simulation; the full playable game is still in development.

```sh
just game
GAME_DIR=original/k1/eng/game just game
just game-release
```

Choose **New game**, select the hero, occupation and specializations, and adjust
attributes. Decreasing an attribute refunds its price; increasing spends points.
Changing occupation resets allocation; changing hero or specialization preserves
it. Unspent points carry into the session. **Begin** opens the selected hero's
start map. **Back** or Escape cancels creation. Tab moves focus; Enter/Space
activate the focused control.

Settings support Russian/English, fullscreen and vsync. A language draft previews
inside settings; the rest of the UI changes when the save succeeds. On failure,
the draft remains available for retry and runtime settings roll back.

## CLI and local settings

For direct Cargo invocation:

```sh
cargo run --profile dev-runtime --manifest-path crates/Cargo.toml -p kn-player \
    --features kn-fe-player/dynamic -- \
    --game-dir original/k1/rus/game --new-game --settings out/player-settings.json
```

`--new-game` opens creation directly. `--settings` selects a JSON settings file;
otherwise the OS configuration directory supplies `re-konung/settings.json`.
`RE_KONUNG_GAME_DIR` and `RE_KONUNG_SETTINGS` provide environment fallbacks;
explicit flags take precedence. Missing settings use defaults. An unreadable,
corrupt or newer file is diagnosed and left untouched when loading.

For a Finder application and interactive checks, see
[macOS testing](../development/macos-testing.md). Implementation contracts are in
[player/UI](../development/rust/player-ui.md); original creation rules in
[K1 research](../research/k1/new-game.md).
