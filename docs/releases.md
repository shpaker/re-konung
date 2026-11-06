# Версии и релизы

Версия вычисляется автоматически по Conventional Commits, а тег и GitHub Release появляются **только по ручному запуску**. Сам по себе релиз не происходит, после мержа в `main` тоже. Схема та же, что в koleya и kinescope, а версию и текст релиза считает [git-cliff](https://github.com/orhun/git-cliff) (`cliff.toml`, запуск — `uvx git-cliff`).

## Как выпустить

GitHub → Actions → **version** → Run workflow (ветка `main`). Workflow `.github/workflows/version.yml`:
1. проверяет, что CI (`ci.yml`) на этом коммите `main` зелёный;
2. если коммит уже с тегом или выпускать нечего — завершается с пометкой;
3. считает версию (`just next-version`), ставит тег `vX.Y.Z` и пушит его;
4. вызывает `release.yml`: текст релиза — `just changelog`, файл — `konung-tiled-vX.Y.Z.zip` (`just tiled-package`).

Агенты релиз не запускают и теги не ставят: это решение пользователя.

## How the version is computed

A release is built from the PR commits since the last tag `vX.Y.Z`: each PR lands on `main` as one
squash commit whose subject GitHub writes from the PR title with its number
(`squash_merge_commit_title=PR_TITLE`, `squash_merge_commit_message=COMMIT_MESSAGES`), and `cliff.toml`
counts a commit only when its subject carries that `(#N)` marker — so one PR is never counted twice and
an ordinary commit inside its branch never counts at all. The body keeps the commit messages, so a
`BREAKING CHANGE:` footer still raises the version. Merges into `main` are squash commits only: merge and
rebase are off (a rebase has no commit carrying the PR number, so such a PR would miss the release).

| Заголовок PR | До 1.0 | После 1.0 |
|---|---|---|
| `feat` | minor | minor |
| `fix`, `perf`, `refactor` | patch | patch |
| несовместимые (`feat!:`, `BREAKING CHANGE:`) | minor | major |
| только `chore`, `ci`, `docs`, `test`, `build`, `style` | релиза нет | релиза нет |

Before 2026-11-01 merges read «Merge pull request #N from <branch>» with the PR title in the body, so
`commit_preprocessors` in `cliff.toml` lifts that title to the first line and the same `(#N)` rule still
counts the old history. Until a dependent (stacked) PR's base is switched to `main`, its squash commit
lands on the lower branch rather than `main`, so it does not match the rule and stays out of the release:
its content reaches `main` through the lower PR's own squash commit.

Первая версия — `v0.1.0`. Локально посмотреть, что выйдет:

```bash
just next-version
```

```bash
uvx git-cliff --unreleased
```

## Версия в коде

- Python-пакет: версию из тега берёт `hatch-vcs` (`pyproject.toml`), между тегами получается `X.Y.Z.devN+g<sha>`. Показывает её `uv run konung --version`.
- Расширение Tiled: в zip релиза лежит файл `konung/VERSION` с тегом.
- Плагин для агентов (`.claude-plugin/`, `plugin.json`, `qwen-extension.json`): версия с тегами не связана. В манифестах Claude Code и Codex её нет, и плагин обновляется по коммитам `main`. В `qwen-extension.json` стоит заглушка `0.0.0`.

## Установка расширения из релиза

Распаковать `konung-tiled-vX.Y.Z.zip` в папку расширений Tiled (Windows: `%LOCALAPPDATA%\Tiled\extensions`), чтобы получилось `extensions/konung/konung.mjs`.
