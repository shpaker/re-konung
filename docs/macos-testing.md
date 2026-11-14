# Interactive testing on macOS

Build a local development application with `just game-app`, then open
`out/macos/re-konung.app` in Finder or select that absolute application path in
the desktop automation tool. `GAME_DIR=original/k1/eng/game just game-app`
selects the English edition. Quit the game before rebuilding and relaunch it
afterwards so that the next test uses the new executable.

The package has the stable identifier `io.github.shpaker.re-konung.dev`.
Its launch environment supplies the original-data directory and dynamic-library
paths that `cargo run` normally supplies. `RE_KONUNG_GAME_DIR` and
`RE_KONUNG_SETTINGS` are CLI fallbacks; explicit flags take precedence.
Development settings live in `out/macos/settings.json`, independently of
the normal game's preferences. The package references the local checkout and
Rust toolchain; it is not a redistributable release and contains no game data.

## Desktop tool access

Our local check found that `unified-computer-use` did not list the unbundled
Cargo executable and rejected its path with `Invalid app`. The `.app` path was
recognized and reached the separate per-application permission check instead.
The user must allow access to re-konung in the desktop tool before pointer or
screenshot testing can proceed. Do not change system privacy permissions or
substitute synthetic control events for actual mouse acceptance.

The bundle's declared executable must be the game binary itself. Our initial
shell launcher could display a window but coordinate input failed with
`noWindowsAvailable` and screenshots stopped updating. Putting launch defaults
in `LSEnvironment` and launching the binary directly resolved this in our local
test; no desktop-tool or permission configuration was modified.

## New-game acceptance

Open New game, click each hero portrait and verify the name, description,
selected portrait and sprite change. Change occupation and specializations;
decrease an attribute, spend its refund elsewhere and verify the point balance,
carrying capacity and party capacity update. Changing occupation resets the
allocation; changing hero or specialization preserves it.

Check keyboard focus, Escape/Back, scrolling, readable labels and the separate
Back/Begin footer at different window sizes. Begin must reach the selected
hero's start map without a loading error. The automated runtime tests cover
the full parameter and equipment transfer; see [k1/new-game.md](k1/new-game.md).
Record which pointer steps were actually observed. Screenshots contain original
game content: keep them in `out/`, never in git, PRs or issues.
