# Set up a checkout

Install [uv](https://docs.astral.sh/uv/), [just](https://just.systems) and
[prek](https://prek.j178.dev). Python 3.14 is managed by uv. For Rust applications,
install a stable Rust toolchain; for map editing, Tiled 1.12. Extracting a disc
also needs 7-Zip.

From the repository root:

```sh
uv tool install prek
just setup
```

This installs Python dependencies and the shared git hooks. Run `just check`
after making changes. For Rust profiles, platform dependencies and CI, see
[builds](../development/building.md); check coverage is in
[testing](../development/testing.md).

Prepare [your game files](game-files.md), then choose:

- [Player](player.md): `just game`.
- [Viewer](viewer.md): `just viewer 17`.
- [Tiled](tiled.md): `just tiled-install`.

The Tiled extension can also be installed through the repository's plugin
without a development checkout; its [guide](tiled.md#installation-through-an-agent)
explains that route.
