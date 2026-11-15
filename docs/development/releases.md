<a id="versions-and-releases"></a>

# Версии и релизы

Версию и примечания вычисляет [git-cliff](https://github.com/orhun/git-cliff)
(`cliff.toml`, запуск через uvx git-cliff). Тег и GitHub Release создаются
**только ручным запуском workflow**, не автоматически после слияния.
Агенты не запускают его и не создают теги.

<a id="publish-a-release"></a>

## Публикация релиза

Пользователь открывает GitHub Actions → **version** → Run workflow на main.
`.github/workflows/version.yml`:

1. Требует успешный запуск ci.yml для этого конкретного коммита main.
2. Останавливается с объяснением, если коммит уже помечен тегом или выпускать нечего.
3. Вычисляет версию (`just next-version`), создаёт vX.Y.Z и отправляет тег.
4. Вызывает release.yml: примечания из `just changelog`, архив
   `konung-tiled-vX.Y.Z.zip` из `just tiled-package`.

В архиве только наш код расширения, никаких игровых ресурсов.

<a id="how-the-version-is-computed"></a>

## Расчёт версии

Релиз учитывает PR после последнего тега `vX.Y.Z`. Каждый PR попадает в main
одним squash-коммитом; GitHub формирует тему из заголовка PR и номера
(`squash_merge_commit_title=PR_TITLE`, `squash_merge_commit_message=COMMIT_MESSAGES`).
`cliff.toml` считает только темы с маркером `(#N)`: один PR не учитывается
дважды, обычный коммит внутри ветки не учитывается. Тело сохраняет сообщения
коммитов, поэтому `BREAKING CHANGE:` по-прежнему повышает версию.
В main допускается только squash: merge и rebase отключены; при rebase
не было бы коммита с номером PR и изменение пропало бы из расчёта.

| Заголовок PR | До 1.0 | После 1.0 |
|---|---|---|
| `feat` | minor | minor |
| `fix`, `perf`, `refactor` | patch | patch |
| Несовместимость (`feat!:`, `BREAKING CHANGE:`) | minor | major |
| Только `chore`, `ci`, `docs`, `test`, `build`, `style` | Без релиза | Без релиза |

До 2026-11-01 тема слияния имела вид «Merge pull request #N from <branch>»,
а заголовок PR находился в теле. `commit_preprocessors` в `cliff.toml` поднимает
его в первую строку, и правило `(#N)` учитывает старую историю.
Пока база зависимого PR не переключена на main, его squash попадает в нижнюю
ветку. Самостоятельно в релиз он не входит: содержимое достигает main через
squash нижнего PR.

Начальная версия — v0.1.0. Посмотреть следующую без публикации:

```sh
just next-version
uvx git-cliff --unreleased
```

<a id="version-consumers"></a>

## Где используется версия

- Python hatch-vcs читает теги (`pyproject.toml`); между тегами получается
  X.Y.Z.devN+g<sha>. Проверка: `uv run konung --version`.
- Архив расширения Tiled содержит konung/VERSION с тегом.
- Манифесты плагинов агентов независимы от тегов: Claude Code и Codex
  не задают версию и следуют коммитам main; qwen-extension.json использует 0.0.0.

<a id="install-the-release-extension"></a>

## Установка расширения из релиза

Распакуйте konung-tiled-vX.Y.Z.zip в каталог расширений Tiled так, чтобы появился
extensions/konung/konung.mjs (Windows: %LOCALAPPDATA%/Tiled/extensions).
Пути платформ и проверки — в [руководстве Tiled](../guides/tiled.md).
