# re-konung

Study and recreation of the engine behind «Князь: Легенды Лесной страны»
(*Konung: Legends of the North*, 1999), then K2 and K2.5 if a shared engine is
confirmed. An unofficial fan project, unaffiliated with 1C or Snowball; the games
and their names belong to their respective owners.

The original supplies the rules, content, atmosphere and visual identity. The
project improves responsiveness, controls and obsolete technical limits; the
[product principles](docs/development/design.md) explain the reference for that work.

## Start here

You need your own licensed copy: your disc or a store purchase. No game assets
are distributed. [Prepare the files](docs/guides/game-files.md), then follow
[environment setup](docs/guides/setup.md).

| Goal | Start |
|---|---|
| Run the player and create a character | [Player guide](docs/guides/player.md) — `just game` |
| Open, edit and save original maps | [Tiled guide](docs/guides/tiled.md) — `just tiled-install` |
| Inspect maps, creatures and lighting | [Viewer guide](docs/guides/viewer.md) — `just viewer 17` |
| Develop the engine or tools | [Development](docs/development/README.md) |
| Read the formats and recovered game rules | [Research](docs/research/README.md) |

The Rust player and viewer render maps and run the creature simulation. The
player has menus, settings and a new-game constructor; a complete playable loop
is still in progress. Python and the Tiled extension remain independent
reference tools, and Python wins format disagreements.

The [architecture](ARCHITECTURE.md) owns components, boundaries and current status.
The [documentation index](docs/README.md) connects the guides, implementation
notes and research. To contribute, read [CONTRIBUTING.md](CONTRIBUTING.md);
agent instructions are in [AGENTS.md](AGENTS.md).
