# How to contribute

## Language of Issues and PRs

Titles and the whole body are English, in both issues and pull requests. The description ends with
one collapsed block holding the complete translation:

```markdown
<details>
<summary>Русский перевод</summary>

…the same sections, the same order, the same links and the same technical details…

</details>
```

- The English text is the source of truth: a disagreement with the translation is fixed in it first,
  then the translation is rewritten.
- Descriptions are concise: one or two sentences per section, or a short list of what cannot be
  guessed otherwise. The diff and the history are not retold — the «what» is already in the code and
  the tests, the description carries the why, how to check it and what it risks. A ticket does not
  copy `docs/`: a confirmed fact is cited.
- Code, commands, identifiers and paths are not translated.
- No empty sections, no placeholders: what does not apply is written as `None` or `N/A`.
- What is not confirmed is marked as such — a hypothesis or unknown ([AGENTS.md](AGENTS.md),
  "Reverse engineering").
- The collapsed block is the only one and it is last: the check reads the English part before it, so
  headings inside the translation cannot get past it.

The ticket author writes the translation, a reviewer corrects it against the text; there is no
machine translation in CI.

GitHub-facing files (`.github/ISSUE_TEMPLATE/`, `.github/pull_request_template.md`, `.github/**`
generally, together with their comments) are English: the text ships into the ticket. One exception —
the section headings inside the translation block, which are Russian by definition. Comments there
belong only to what cannot be guessed, and take one or two lines: the section itself says the rest.

## Issue titles: type and result

Issues only: pull request titles stay in Conventional Commits (see "Pull request description").

An issue title starts with one of four prefixes, which say what kind of work it is:

| Prefix | When |
|---|---|
| `bug:` | behavior diverges from what is recorded |
| `feat:` | the ticket promises a new observable result: a map opens, a creature walks, the game launches |
| `task:` | work without a new observable result: a layer, preparation, refactoring |
| `research:` | the result is a fact in `docs/`, not code: a format, a game rule |

- The title names the expected result, not the action: `feat: creatures move along the original maps
  on their own`, not `task: implement pathfinding`.
- A scope like `(kn-render)` is optional: a ticket usually spans several layers, and they are listed in
  `## Scope`. Use a scope when it is exactly one and not debatable.
- A label and a prefix do not compete: the templates attach the label (`bug`, `enhancement`) at
  creation, and `gh issue create` without a template adds it with `--label` — the label is for
  filters, the prefix is for reading the list.
- Pull request titles stay in Conventional Commits (`feat:`, `fix:`, `chore:` …) and carry a
  mandatory subsystem scope: `type(scope): subject`. The scope is exactly one of `game` (the
  engine and code shared across the series), `konung1`, `konung2`, `konung2.5` (a specific game of
  the series), `ui` (the display layer and its interface: pieces to sprites, textures, animation,
  camera, widgets), `tiled` (the Tiled extension), `arch` (the crate and layer structure: new
  crates, moves between layers, dependency direction), `docs`, `ci`, `deps`, `repo` (repository
  infrastructure and rules: justfile, prek, templates, contribution docs). No job validates the
  scope — review enforces it. git-cliff reads those, and they decide the release and the version
  ([docs/releases.md](docs/releases.md)). Issue prefixes affect no release.

## Branches and PRs

Every change reaches `main` through a PR only. One PR covers one area of responsibility: project
scaffolding, disc unpacking, graphics, maps, the Tiled extension, documentation of one topic. A
change that touches several areas is split into several PRs; dependent PRs are stacked (the base of
the next one is the branch of the previous). When a lower PR is squash-merged its branch is deleted
and its commits collapse into a single `main` commit, so rebase the dependent branch onto the updated
`main` (or the next surviving base), repoint the dependent PR's base there, and force-push it.

Branch names are short and meaningful: `feat/k1-maps`, `fix/rle-shadow`, `docs/sources`.

Merges into `main` are squash commits only: merge and rebase are disabled in the repository and
`main` requires a linear history. GitHub writes the squash subject from the PR title with its number —
`feat: parse k1 map files (#29)` — and the branch's commits do not reach the `main` history at all, so
the type a release is computed from has to be set in the PR title ([docs/releases.md](docs/releases.md)).

## Commits

[Conventional Commits](https://www.conventionalcommits.org/) in English, authored by the user:
`feat: ...`, `fix: ...`, `docs: ...`.

## Pull request description

The title is English, in Conventional Commits: `feat: parse k1 map files`. It becomes the squash
commit subject in `main` and a release note entry.

The description has seven English sections, all mandatory, in this order. An empty section is a
check failure; where there is nothing to say, write `None`.

```markdown
## What

## Why

## Related

## How to Test

## Review Guide

## Risks

## Environment
```

- `What` — what changed, in prose.
- `Why` — which problem it solves and where the task came from.
- `Related` — the issues and PRs involved; none — `None`.
- `How to Test` — steps that let a reviewer see the change themselves: `konung` commands, tests, a
  map in Tiled.
- `Review Guide` — where to start and what to look at.
- `Risks` — what can break; no risks — `None`.
- `Environment` — only what matters: OS, Python, Tiled, the game edition it was verified on.

After the sections comes the translation: `## Что`, `## Зачем`, `## Имеет отношение к`,
`## Как проверить`, `## Как делать ревью`, `## Риски`, `## Окружение`.

The template is `.github/pull_request_template.md`. CI checks the form
(`.github/workflows/pr.yml`); the check itself is `.github/scripts/contrib_form.py`, covered by
`tests/test_contrib_form.py`, which validates the repository's own templates with the same code.

## GitHub Issues

A ticket is filed with one of the two templates (`.github/ISSUE_TEMPLATE/`); there is no blank
issue (`blank_issues_enabled: false`). This takes away no rights: if neither template fits, the
ticket is created with `gh issue create`, keeping the same sections and the translation.

One ticket — one self-contained task. A ticket describes the problem and the expected result rather
than prescribing an implementation: type, module and structure names belong to the coding stage.
Related tasks reference each other by number and list their dependencies explicitly.

**Bug report** — `Summary`, `Steps to Reproduce`, `Expected Behavior`, `Actual Behavior`,
`Environment`; optionally `Evidence`, `Impact`, `Related Issues`. Reproduction steps are never
invented: if they are unknown, write `Reproduction unknown` and the conditions the problem appeared
under. Expected behavior is what `docs/` or the original game records, with a link to the check that
established it. An intentional difference from the original is not a bug: state the expected modern
behavior and cite the note next to the fact in `docs/`.

**Feature request** — `Problem`, `Proposed Behavior`, `Acceptance Criteria`, `Scope`; optionally
`Implementation Notes`, `Alternatives`, `Dependencies / Related Issues`. Acceptance criteria are
checkable line by line: the command and what its output must show. Confirmed facts with their sources
and open questions go into `Implementation Notes`, so that a hypothesis does not read as a fact.

Performance problems are described with measured numbers and the conditions of the measurement, not
with an eyeball estimate (the cost of operations — [docs/rust.md](docs/rust.md)).

## Linking tickets and PRs

A bare issue number (`#33`) or its full URL is a cross-reference: an event appears in both tasks and
nothing else. Links that have to work on their own are made with GitHub keywords and the
"Relationships" panel.

- **A PR closes its ticket** with a line in the `Related` section: `Closes #33`, `Fixes #33` or
  `Resolves #33`. Without such a line the ticket survives the merge. Keywords: `close|closes|closed`,
  `fix|fixes|fixed`, `resolve|resolves|resolved` — they work in the PR description and in a commit
  message, and the merge into the default branch is what closes the issue. A task in another
  repository is addressed as `owner/repo#33` or by its full URL.
- **The keyword goes only to what is finished completely.** A PR that delivers part of a ticket
  references it by number, without the keyword.
- **Dependencies between tickets are not set by text.** The "Relationships" panel → "Mark as blocked
  by" / "Mark as blocking", or `gh issue edit 36 --add-blocked-by 35` (when creating —
  `gh issue create --blocked-by 34,35`). `## Dependencies / Related Issues` lists them by number and
  the panel is kept in step with that list: without the panel nothing marks the ticket as blocked.
  Dependencies close nothing by themselves — they are only the order of work.
- **A duplicate** is marked with a `Duplicate of #33` comment.

## No mentions of AI

Nowhere — commits and their metadata, branches, PRs, issues, comments, tags and releases — no
mentions of AI or neural networks: no `Co-Authored-By`, no `Generated with ...`, no branch prefixes
like `claude/` or `ai/`.

## Checks

Requires [uv](https://docs.astral.sh/uv/), [just](https://just.systems),
[prek](https://prek.j178.dev) (`uv tool install prek`) and 7-Zip to unpack the originals. Once after
a clone:

```bash
just setup
```

After that git runs prek itself: on commit — formatting and file checks, on the commit message —
Conventional Commits and the ban on AI mentions, before push — ruff and the tests. All of it by hand:
`just check`.

## Copyrighted content

The project is unofficial. No game files and nothing derived from them (sprites, renders, sound,
texts, dumps) in the repository, in PRs, in issues or in releases. No links saying where to get the
game either: only your own disc or a store purchase. Details — the "Copyrighted content" section of
[AGENTS.md](AGENTS.md).

Files of your own copy live in `original/`, derived ones in `out/` (both in `.gitignore`). Tests on
data skip without them; how to prepare the files — [docs/k1/editions.md](docs/k1/editions.md).
