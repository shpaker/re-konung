# Agent workflow

[AGENTS.md](../../AGENTS.md) supplies the mandatory rules for every agent.
`CLAUDE.md` imports it through `@AGENTS.md`. Checks are shared git hooks, rather
than agent-specific hooks; the original arrangement followed tank8000's practice.

## Skills and plugins

Both kinds of skill use a flat `<skill>/SKILL.md` layout:

| Location | Audience and contract |
|---|---|
| `skills/` | Users installing the repository plugin without a checkout; no dependency on `just`, `uv` or `original/`. Resolve the plugin root two levels above the skill directory. |
| `.claude/skills/` | Development within a checkout; includes `konung-reverse-format`. |

The user skill `konung-tiled-install` installs the extension and verifies that
Tiled loads it and opens Konung.N. Installation commands belong to the
[Tiled guide](../guides/tiled.md#installation-through-an-agent).

Plugin manifests: `.claude-plugin/` for Claude Code, `plugin.json` and
`.agents/plugins/marketplace.json` for Codex, `qwen-extension.json` for Qwen Code.
Claude's `.claude/settings.json` declares the tiled-mcp marketplace/plugin via
`extraKnownMarketplaces` and `enabledPlugins`.

Create a skill only for a recurring sequence across several tasks that cannot
be replaced by a documentation link. The reverse-engineering skill owns its
workflow; [research](../research/README.md) owns its evidence.

## Tiled inspection

Use [tiled-mcp](https://github.com/shpaker/tiled-mcp) and its `tiled` skill:
`tiled_status` → `tiled_open` on Konung.N → `map_summary`, `list_objects`,
`render_map`, `run_script`. Original maps are opened through `tiled/konung/`;
see [extension internals](tiled.md) for their representation.

A full-size comparison image can be saved by `run_script`; use an absolute
path under the checkout's `out/`:

```js
const image = map.toImage(); image.save("<repository-root>/out/k1/tiled.png"); return [image.width, image.height]
```

`tiled_install_extension` installs tiled-mcp's own Tiled extension with user
permission. Installation of the game's separate extension is covered by the guide.
External errors are reported under AGENTS.md's rule; do not silently work around them.

## Checks and data

`just setup` installs prek shims. `just check` runs file checks, Python and Rust
format/lint checks and tests; [testing](testing.md) describes the current hooks
and CI. PR/issue form validation is owned by
[CONTRIBUTING.md](../../CONTRIBUTING.md).

Read game files only from `original/`; put all derived output in `out/`.
Both stay uncommitted. [AGENTS.md](../../AGENTS.md#copyrighted-content) defines
the distribution restrictions.
