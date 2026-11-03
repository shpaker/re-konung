# Rules for AI agents

re-konung is the study and recreation of the engine behind the «Князь» game series (1C/Snowball): first «Князь: Легенды Лесной страны» (1999), then «Князь 2» and «Князь 2.5» if a shared engine is confirmed (see [docs/games.md](docs/games.md)). Agents do the work, so anything learned about the formats goes into [docs/](docs/README.md) instead of staying in the chat.

## Stage

Stage 1 — the maps of the first part in Tiled. There is a reference Python parser (`konung/`), a Tiled extension (`tiled/konung/`) that opens the original `Konung.N` together with the game objects from `Game.N` and saves them back, and format descriptions in `docs/k1/formats/`. Only maps (`Konung.N`) and the starting state of a new game (`Game.N`: characters, groups, caches, transitions) are written; sprites, `GRAPH.RES` and `Objects.res` are only read.

- Any change to map writing or to `Game.N` is verified the same way: every file of both editions must match the original byte for byte after «read → write» (tests `tests/k1/test_maps.py` and `tests/k1/test_game_state.py` for Python; for the extension — save from Tiled and compare).

- `konung/` — a Python package, CLI `konung` (`uv run konung --help`). What is shared between the parts lives at the package root (`binary`, `rle`, `palette`, `disc`), what belongs to one part lives in `konung/k1/`, later `konung/k2/`.
- `tiled/konung/` — the Tiled extension: `lib/k1.mjs` parses without the Tiled API, `konung.mjs` builds the Tiled map. The logic mirrors Python, and where they disagree Python wins: it has the tests.
- `crates/` — a Rust workspace: the `k1-data` crate holds K1 game data (repository `repo`, the `map` and `game_state` formats, roster editing `game_edit`, graphics `graph_res`, `objects_res`, `lights_res`, `flames_res`, `rle` and the tables from the EXE); `k1-game` holds the K1 specifics (resources, textures, the map render list, creature data and rules); `kn-game` is the shared facade, the Bevy-independent scene model and the shared movement engine (grid, routes, passability, movement, game tick): movement mechanics in K1 and K2 are presumably nearly identical, the noticeable difference in K2 is running, so what is shared lives here rather than in a part's layer. Rust mirrors Python and replaces nothing yet; where they disagree Python wins. A trait abstraction is declared by the consumer, when a mock is needed. Details — [docs/rust.md](docs/rust.md).
- `tests/` — pytest. Tests on data take files from `original/` and skip when those are absent.
- `original/` — files of one's own copy of the game (disc images and unpacked content), never committed. How to prepare them — [docs/k1/editions.md](docs/k1/editions.md).
- `out/` — everything generated (PNG, dumps), never committed.

## Reverse engineering

The loop is the `konung-reverse-format` skill (`.claude/skills/konung-reverse-format/SKILL.md`): hypothesis → check with a script over real files → test → write it up in `docs/`.

- Every field in a format description carries a confidence marker: **confirmed** (a test or a check over every file exists), **hypothesis**, **unknown**. Unknown bytes are never dropped: the parser keeps them in an `unknown` field.
- Game rules — character states, movement, the behavior of villagers and monsters — are recovered with the same loop and marked with the same confidence. What is not confirmed is not invented: the rule is marked unknown in `docs/` and not implemented. Replacing an unknown rule with an invented one — random wandering instead of unverified patrolling, say — is not allowed.
- The source of a fact is always named: our own experiment, or an external source from [docs/sources.md](docs/sources.md).
- [Marisa-Chan/Slavik](https://github.com/Marisa-Chan/Slavik) is a C++ reimplementation of the engine; the clone sits in `../../Marisa-Chan/Slavik` (`E:\github\Marisa-Chan\Slavik`). It is a **read-only** reference: it carries no license, so its code is never copied. Formats are described in our own words with a link to file and line, and the implementation is written from scratch.
- Editions differ (RUS, ENG, later GOG and the collector's edition): before generalizing, check the fact against every edition in `original/`.

## Priority: modern experience

The goal is a live and responsive game, not a byte-for-byte copy of a 1999 engine.

- The formats, gameplay and atmosphere of the original are preserved: data is read and written byte-exactly, behavior rules come from the original rather than from our heads.
- Outdated mechanics and technical limits are improved. The original's internal algorithms and artificial caps are not reproduced: they were the price of the hardware of the day. Examples — a route no longer than 32 steps, turning by one direction per tick, no more than three "expensive" behavior decisions per tick.
- Slavik is a reference for facts (formats, field meanings, rules), not a model for decisions: how exactly it computes something is not our constraint.
- Where we do things differently, write it into `docs/` next to the fact about the original — otherwise a divergence gets found but not understood as intentional.
- The cost of decisions is measured, not eyeballed: the number and its comparison with neighboring operations go into `docs/rust.md`.

## Copyrighted content

The games belong to someone else. The project studies the formats and writes its own code, but distributes no game.

- No game files or anything derived from them — sprites, map renders and PNGs, sound, music, texts, videos, resource dumps — in git, in PRs, in issues, in comments or in releases. All of it lives in `original/` and `out/`, which are not committed.
- Never say where to download the game or its disc images: no links to pirated distributions, no sites archiving old games, no file hosts. Only two routes: your own disc, or a store purchase.
- Test fixtures are synthetic only. Tests on game data run locally, over `original/`.
- In `docs/` formats are described in our own words: offsets, sizes, file names, short header fragments. No resource excerpts.
- Releases contain only our own code (`just tiled-package`).
- The prek hook `no-game-files` keeps game files out of a commit, `no-download-links` keeps distribution links out. When a hook fires, fix the text or the file — do not work around the hook.

## Agents

Claude Code and other agents work on the project; the rules for all of them are this file. Details — [docs/agents.md](docs/agents.md).

- Development skills live in `.claude/skills/`. Skills for people live in `skills/`: the repository ships them as a plugin for Claude Code, Codex and Qwen Code ([docs/agents.md](docs/agents.md)).
- An agent views and edits maps in Tiled through the [tiled-mcp](https://github.com/shpaker/tiled-mcp) plugin (skill `tiled`). Original maps open directly, through the `tiled/konung/` extension (`just tiled-install` or the `konung-tiled-install` skill).
- Found a bug or missing functionality in someone else's tool or library (Tiled, uv, Pillow, 7-Zip, Slavik and others) — tell the user in the chat explicitly: what is wrong, how to reproduce it, the repository link and the place in the code if it is known. Do not offer a fix and do not silently work around it: what to do about it is the user's call.
- Same for `shpaker/*` repositories (for example [tiled-mcp](https://github.com/shpaker/tiled-mcp)) — a PR with the fix may be opened in that repository right away, but in a **separate session** and after asking the user. In the current work, do not work around it; report it exactly as an external bug.
- Checks run through [prek](https://prek.j178.dev) (`prek.toml`): git calls it on commit and before push. Shims are installed by `just setup`.

## Coding style

- Python 3.14, ruff. English is used for everything GitHub-facing: `.github/**` with the checks and the tests that cover them, this file, [CONTRIBUTING.md](CONTRIBUTING.md), and the text of issues and pull requests. New code and new documentation are English as well: comments, doc-comments, assertion and test names, and the `docs/` pages a change adds. Text that already exists keeps its language until a change touches it, and what is touched is rewritten in English — even when that leaves one English row in a Russian table. Identifiers are English and unabbreviated.
- **Comments are English — always**, whatever language the file around them is in, and they exist only where the decision is not self-evident: a hidden constraint, an invariant, a workaround, a deliberate divergence from the original. Never a restatement of what the code does. One or two lines, exact wording; a comment whose removal loses no information should not be there.
- A format parser is a pure function `parse(data: bytes)`; the filesystem lives separately (`load`).
- Tests are simple: synthetic ones for algorithms (RLE), on real files for formats.

## Required

- After edits: `just check` (ruff, tests, file checks).
- Never bypass prek checks: no `--no-verify`, no `SKIP=`, no `prek uninstall`.
- A format change shows up in the picture — verify with a picture: `uv run konung k1 map …` and a render in Tiled.
- Decisions and anything found about the formats go into `docs/`.

## Git

- All changes go through a PR. One PR closes one area: scaffolding, disc unpacking, graphics, maps, the Tiled extension, documentation of one topic. Unrelated work goes to separate branches and PRs.
- Any write git operation (commit, push, merge, rebase, reset, tag and so on) only with the user's explicit confirmation.
- Never push to or merge into `main` (including `gh pr merge`) without the user's consent for exactly that push or merge.
- Merges into `main` are merge commits only (squash and rebase are disabled in the repository). GitHub takes the merge subject from the PR title (`merge_commit_title=PR_TITLE`, empty body), so do not pass `-t` or `-b` to `gh pr merge`: your own text overrides the repository setting and the history ends up with a subject the PR never had.
- Commits are made in the user's name only, in Conventional Commits, in English.
- Pull request titles carry a mandatory subsystem scope on top of Conventional Commits: `type(scope): subject`, the scope exactly one of `game` (the engine and code shared across the series), `konung1`, `konung2`, `konung2.5` (a specific game of the series), `ui` (the display layer and its interface: pieces to sprites, textures, animation, camera, widgets), `tiled` (the Tiled extension), `arch` (the crate and layer structure: new crates, moves between layers, dependency direction), `docs`, `ci`, `deps`, `repo` (repository infrastructure and rules: justfile, prek, templates, contribution docs). No job validates scopes — the rule is enforced by agents and in review. Issue titles keep their own optional scope.
- Nowhere — commits and their metadata, branches, PRs, issues, comments, tags and releases — no mentions of AI or neural networks (`Co-Authored-By`, `Generated with ...`, `claude/`, `ai/` and the like).
- When creating a PR, read [CONTRIBUTING.md](CONTRIBUTING.md) and fill in every one of its sections.
- Issue and PR titles and bodies are English, the full translation goes last as one collapsed block `<details><summary>Русский перевод</summary>` at the end of the description. The English text is the source of truth; code, commands, identifiers and paths stay untranslated. Descriptions are concise: one or two sentences per section, no retelling of the diff and no copies of `docs/` — a fact is cited. No empty sections; what does not apply is written as `None`. The form is checked by `.github/scripts/contrib_form.py` — in CI and in `tests/test_contrib_form.py`.
- One ticket — one self-contained task: the problem and the expected result, not the implementation — type and module names belong to the coding stage. Tell confirmed original behavior apart from a hypothesis ("Reverse engineering"), describe an intentional difference from the original as the expected modern behavior ("Priority: modern experience"), and never invent reproduction steps. Templates — `.github/ISSUE_TEMPLATE/`, sections — [CONTRIBUTING.md](CONTRIBUTING.md).
- An issue title — issues only: it starts with a type prefix (`bug:`, `feat:`, `task:` or `research:`) and names the expected result, not the action; a scope like `(kn-render)` is optional, the layers are listed in `## Scope`. Pull request titles stay in Conventional Commits as before — git-cliff reads those, issue types affect no release.
- Ticket links use GitHub keywords: a PR closes its ticket with `Closes #NN` in the `Related` section (without it the ticket survives the merge), and a ticket it only partly covers is referenced by number without the keyword. Dependencies between tickets are set in the "Relationships" panel or with `gh issue edit --add-blocked-by` — text alone creates no link. Details — [CONTRIBUTING.md](CONTRIBUTING.md).
- The version is computed automatically (git-cliff); tags and releases are made by hand only, through the **version** workflow ([docs/releases.md](docs/releases.md)). An agent does not run it and does not tag. Release notes are built from PR titles (git-cliff skips the commits inside a branch), so the type has to be chosen honestly in the PR title itself: `feat` is a new capability, `fix` is a repair, anything else gives no release entry.
