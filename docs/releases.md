# Версии и релизы

Версия вычисляется автоматически по Conventional Commits, а тег и GitHub Release появляются **только по ручному запуску**. Сам по себе релиз не происходит, после мержа в `main` тоже. Схема та же, что в koleya и kinescope, а версию и текст релиза считает [git-cliff](https://github.com/orhun/git-cliff) (`cliff.toml`, запуск — `uvx git-cliff`).

## Как выпустить

GitHub → Actions → **version** → Run workflow (ветка `main`). Workflow `.github/workflows/version.yml`:
1. проверяет, что CI (`ci.yml`) на этом коммите `main` зелёный;
2. если коммит уже с тегом или выпускать нечего — завершается с пометкой;
3. считает версию (`just next-version`), ставит тег `vX.Y.Z` и пушит его;
4. вызывает `release.yml`: текст релиза — `just changelog`, файл — `konung-tiled-vX.Y.Z.zip` (`just tiled-package`).

Агенты релиз не запускают и теги не ставят: это решение пользователя.

## Как считается версия

Релиз собирают по мерж-коммитам PR с последнего тега `vX.Y.Z`: заголовком мерж-коммита GitHub пишет
заголовок PR (настройка репозитория `merge_commit_title=PR_TITLE`, тело пустое), а коммиты внутри ветки
`cliff.toml` пропускает — иначе один PR попадал бы и в текст релиза, и в расчёт версии дважды. Мерж в
`main` потому только merge-коммитом, squash и rebase в репозитории выключены: у rebase мерж-коммита нет,
и такой PR в релиз не попал бы вовсе.

| Заголовок PR | До 1.0 | После 1.0 |
|---|---|---|
| `feat` | minor | minor |
| `fix`, `perf`, `refactor` | patch | patch |
| несовместимые (`feat!:`, `BREAKING CHANGE:`) | minor | major |
| только `chore`, `ci`, `docs`, `test`, `build`, `style` | релиза нет | релиза нет |

Мержи до 2026-11-01 выглядели как «Merge pull request #N from <ветка>», а их заголовок PR лежал в теле —
`commit_preprocessors` в `cliff.toml` переносит его в первую строку, чтобы и старая история считалась по PR.
Слияния ветки в ветку (зависимые PR, пока их база не переключена на `main`) под этот вид не подпадают и в
релиз не идут: их содержимое попадает в `main` мерж-коммитом своего PR.

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
