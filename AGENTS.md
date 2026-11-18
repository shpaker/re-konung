# Rules for agents

re-konung studies and recreates the «Князь» engine: K1 first; K2 and K2.5 only
if their shared engine is confirmed. Python and Tiled are reference tools; Rust
is the game engine. Current capabilities and boundaries: [ARCHITECTURE.md](ARCHITECTURE.md).

## Required reading

Read the relevant document before working; follow the linked procedures.

| Task | Read |
|---|---|
| Find a topic or place new documentation | [Documentation](docs/README.md) |
| Components, dependencies, shared versus game-specific code | [Architecture](ARCHITECTURE.md) |
| UX, UI, camera or controls | [Product principles](docs/development/design.md) |
| Formats or original game rules | [Research](docs/research/README.md), the K1 topic, and [reverse-engineering skill](.claude/skills/konung-reverse-format/SKILL.md) |
| Rust implementation or performance | [Rust topics](docs/development/rust/README.md) and [builds](docs/development/building.md) |
| Tiled editing or extension changes | [Editing guide](docs/guides/tiled.md) and [extension internals](docs/development/tiled.md) |
| Issues, PRs or git workflow | [CONTRIBUTING.md](CONTRIBUTING.md) |
| Agent tools and checks | [Agent workflow](docs/development/agents.md) and [testing](docs/development/testing.md) |

## Delegation

- The primary agent plans, assigns scoped work directly, reviews results and owns
  acceptance and critical checks. A scout investigates without editing; a worker
  implements an assigned scope. Assign one writer per file set.
- In Codex, use `sol_scout` for scouting, `sol_worker` for complex implementation,
  and `terra_worker` for scoped changes and visual checks when available. See
  [Agent workflow](docs/development/agents.md#delegation) for details.
- Codex defaults to Sol 6.2 with medium effort. Handle small tasks directly;
  delegate only independent questions or bounded file sets. Trust scoped reports
  unless gaps or conflicting evidence justify further investigation.
- For difficult reverse engineering or architecture, recommend an explicit switch
  to Sol 6.2 high, then Astra 6.2 medium if conflicting evidence remains. Do not
  assume the primary model switches automatically.
- Subagents do not delegate further. Escalate ambiguity or conflicting evidence
  to the primary agent. Use parallel work only when it helps the task.

## Reverse engineering

- Use the hypothesis → real-file check → test → documentation loop. Every format
  field and recovered rule carries its source and confidence: **confirmed**,
  **hypothesis** or **unknown**. A reference-only claim is identified as such.
- Do not invent unknown rules or implement guesses as gameplay. Record what is
  missing and leave it unimplemented. Check every available edition before generalizing.
- Python is authoritative when implementations disagree. Unknown bytes are
  preserved, never discarded. Parsers are pure `parse(data)` functions; loading is separate.
- [Slavik](https://github.com/Marisa-Chan/Slavik) is read-only and has no license:
  never copy its code. Describe facts in our own words with file/line citations,
  then implement independently. Local clone: `../../Marisa-Chan/Slavik`
  (`E:\github\Marisa-Chan\Slavik` on Windows).
- Keep original rules, content and atmosphere; improve obsolete algorithms and
  artificial hardware limits. Slavik supplies facts, not implementation constraints.
- Document intentional differences beside the original fact. Measure decision
  costs and compare neighboring operations; record the conditions and numbers.

## Copyrighted content

- Never change `original/`. It holds the owner's game files; generated content
  goes to `out/`. Neither directory is committed.
- No game files or derivatives in git, PRs, issues, comments or releases: sprites,
  renders, sound, music, texts, videos and dumps all stay local. Releases contain
  only our own code (`just tiled-package`). Fixtures are synthetic; game-data tests
  read local originals. Describe formats with offsets, sizes and short headers,
  without resource excerpts.
- Never link to game downloads, disc-image hosts or archives. The only routes
  are the user's own disc or a store purchase.

## Implementation and checks

- Write project documentation in Russian, including `docs/`, root guides and crate
  READMEs. Keep code, identifiers, comments, doc-comments, tests, agent/skill instructions
  and GitHub-facing files in English. Preserve commands, paths and API names in translation.
  Comments explain a hidden constraint or decision in one or two lines, never
  restate the code. Identifiers are unabbreviated. Python: 3.14 and ruff.
- Shared code belongs above the game-specific packages only when commonality is
  established; shared movement is the documented exception. Consumers declare
  traits when a mock is needed. Follow the architecture's dependency boundaries.
- Map and `Game.N` writing must round-trip every file of both editions byte for
  byte: Python tests `tests/k1/test_maps.py`, `tests/k1/test_game_state.py`; Rust
  uses the same contract; the extension is saved from Tiled and compared.
- Plan validation from the stages in `prek.toml`. Use focused tests during
  development; do not manually repeat formatting, lint or full suites immediately
  before Git hooks run the same checks on unchanged code.
- When a push is next, let the pre-push hook perform full validation instead of
  first running `just check` or an equivalent full suite. When no push is planned,
  run `just check`. Pre-commit checks alone do not replace pre-push tests.
- Repeat checks only after relevant changes, failures or new evidence invalidating
  earlier results. Report checks that actually ran; a future hook is not a pass.
- Never bypass prek: no `--no-verify`, `SKIP=` or `prek uninstall`. Fix rejected
  files or text instead of evading a hook.
- A visible format change requires Python rendering (`uv run konung k1 map …`)
  and comparison in Tiled. Agents use the tiled-mcp plugin to view and edit maps.
- Report external tool/library bugs explicitly: what fails, reproduction, the
  repository link and code location if known. Do not offer a fix or work around
  them silently. For `shpaker/*`, a fix PR requires the user's agreement and a
  separate session; do not work around the bug in this one.

## Finishing a task

- Before finishing, remove build and debugging debris created for this task:
  disposable build outputs, scratch scripts, temporary dumps and logs, and
  temporary worktrees. Inspect ownership and active processes before removal.
- Keep source changes, original game files, requested deliverables and useful
  verification evidence. Preserve shared caches that still serve active work;
  do not indiscriminately clean `out/`, build caches or other sessions' worktrees.
- Stop only the temporary processes you started. Record retained artifacts and
  their purpose when they are needed for review or unresolved diagnostics.

## Git

- All changes go through a PR, one area per PR. Any write git operation requires
  explicit user confirmation. Never push to or merge into `main` without consent
  for that exact push or merge. Preserve unrelated edits and their staging.
- Read and follow CONTRIBUTING before creating issues or PRs. Descriptions are concise.
  It defines titles/scopes, required sections, the final Russian translation,
  truthful issue descriptions, closing keywords and dependency relationships.
- Commits are English Conventional Commits in the user's name. No mentions of
  AI or neural networks in commits/metadata, branches, PRs, issues, comments,
  tags or releases; no generated-attribution trailers.
- Squash merges only. Do not pass `-t` or `-b` to `gh pr merge`: the repository
  derives the subject from the PR title and number. Rebase stacked PRs after a
  base is squash-merged, retarget them and force-push as CONTRIBUTING describes.
- Agents never run the version workflow or create release tags. Releases are
  manual; [release procedures](docs/development/releases.md) explain the version rules.

## Documentation ownership

Original facts, sources and confidence go in `docs/research/`; implementation
choices and measurements in the relevant `docs/development/` topic; usage in
`docs/guides/`. Keep indexes short, link canonical pages, and record discoveries
in the repository rather than leaving them only in chat.
