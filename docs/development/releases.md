# Versions and releases

Version numbers and notes are computed by [git-cliff](https://github.com/orhun/git-cliff) (`cliff.toml`, invoked by
uvx git-cliff). A tag and GitHub Release are created **only by a manual workflow
run**, never automatically after a merge. Agents do not run it or create tags.

## Publish a release

The user opens GitHub Actions → **version** → Run workflow on main.
`.github/workflows/version.yml`:

1. Requires a green ci.yml run for that exact main commit.
2. Stops with an explanation if the commit is tagged or there is nothing to release.
3. Computes the version (`just next-version`), creates vX.Y.Z and pushes it.
4. Calls release.yml: notes from `just changelog`, archive
   `konung-tiled-vX.Y.Z.zip` from `just tiled-package`.

The archive contains only our own extension code, never game resources.

## How the version is computed

A release is built from the PR commits since the last tag `vX.Y.Z`: each PR lands on `main` as one
squash commit whose subject GitHub writes from the PR title with its number
(`squash_merge_commit_title=PR_TITLE`, `squash_merge_commit_message=COMMIT_MESSAGES`), and `cliff.toml`
counts a commit only when its subject carries that `(#N)` marker — so one PR is never counted twice and
an ordinary commit inside its branch never counts at all. The body keeps the commit messages, so a
`BREAKING CHANGE:` footer still raises the version. Merges into `main` are squash commits only: merge and
rebase are off (a rebase has no commit carrying the PR number, so such a PR would miss the release).

| PR title | Before 1.0 | After 1.0 |
|---|---|---|
| `feat` | minor | minor |
| `fix`, `perf`, `refactor` | patch | patch |
| Breaking (`feat!:`, `BREAKING CHANGE:`) | minor | major |
| Only `chore`, `ci`, `docs`, `test`, `build`, `style` | No release | No release |

Before 2026-11-01 merges read «Merge pull request #N from <branch>» with the PR title in the body, so
`commit_preprocessors` in `cliff.toml` lifts that title to the first line and the same `(#N)` rule still
counts the old history. Until a dependent (stacked) PR's base is switched to `main`, its squash commit
lands on the lower branch rather than `main`, so it does not match the rule and stays out of the release:
its content reaches `main` through the lower PR's own squash commit.

The initial version is v0.1.0. Inspect the next version without publishing:

```sh
just next-version
uvx git-cliff --unreleased
```

## Version consumers

- Python hatch-vcs reads tags (`pyproject.toml`); between tags it yields
  X.Y.Z.devN+g<sha>. Inspect with `uv run konung --version`.
- The Tiled release archive includes konung/VERSION with the tag.
- Agent plugin manifests are independent of release tags: Claude Code and
  Codex omit a version and follow main commits; qwen-extension.json uses 0.0.0.

## Install the release extension

Extract konung-tiled-vX.Y.Z.zip into Tiled's extensions directory so it contains
extensions/konung/konung.mjs (Windows: %LOCALAPPDATA%/Tiled/extensions).
[The Tiled guide](../guides/tiled.md) lists platform paths and validation steps.
