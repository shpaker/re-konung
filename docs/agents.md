# Работа агентов

Правила — [AGENTS.md](../AGENTS.md). Claude читает его через `CLAUDE.md` (`@AGENTS.md`). Практики взяты из tank8000: один набор навыков и проверок на всех агентов, проверки — git-хуки prek, а не хуки агентов.

## Навыки

`.claude/skills/<навык>/SKILL.md`, плоско:
- `konung-reverse-format` — цикл разбора формата: гипотеза → проверка на всех файлах → парсер и тест → документ → Tiled.
- `tiled` — из плагина tiled-mcp: работа с открытыми в Tiled картами.

Новый навык заводится, только когда цикл из нескольких шагов повторяется в разных задачах и его нельзя свести к ссылке на `docs/`.

## Tiled

Карты агент смотрит в открытом Tiled через [tiled-mcp](https://github.com/shpaker/tiled-mcp): `tiled_status` → `tiled_open` с путём к `Konung.N` → `map_summary`, `list_objects`, `render_map`, `run_script`. Оригинальные карты открывает расширение `tiled/konung/` (`just tiled-install`). Как карта ложится на слои — [k1/tiled.md](k1/tiled.md).

Полноразмерный рендер для сверки с Python — через `run_script` (путь — абсолютный, к `out/` своего клона):

```js
const image = map.toImage(); image.save("<корень репозитория>/out/k1/tiled.png"); return [image.width, image.height]
```

Claude получает плагин из `.claude/settings.json` (`extraKnownMarketplaces`, `enabledPlugins`), как в tank8000. Расширение tiled-mcp в Tiled ставит сам агент (`tiled_install_extension`), спросив разрешения.

## Проверки: prek

`prek.toml`:
- на коммите — пробелы и концы строк, yaml/toml/json, большие файлы (больше 512 КБ — признак того, что в коммит попал оригинал), файлы игры по именам (`no-game-files`), ссылки на раздачи (`no-download-links`), ruff;
- на сообщении коммита — Conventional Commits, запрет `Co-authored-by` от агентов и упоминаний ИИ;
- перед push — `just test`.

Шимы ставит `just setup`; `just check` прогоняет всё вручную. CI (`.github/workflows/ci.yml`) повторяет ruff и тесты; тесты на данных игры там пропускаются, потому что оригиналов в репозитории нет. Форму PR проверяет `.github/workflows/pr.yml`.

## Данные игры

Оригиналы лежат в `original/` (в `.gitignore`), производные — в `out/`. Что можно выносить из них в репозиторий — раздел «Авторский контент» в [AGENTS.md](../AGENTS.md). Агент читает их свободно, но не меняет `original/`: всё, что получено из оригиналов, пишется в `out/`.
