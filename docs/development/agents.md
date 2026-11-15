<a id="agent-workflow"></a>

# Работа агентов

[AGENTS.md](../../AGENTS.md) содержит обязательные правила для всех агентов.
`CLAUDE.md` импортирует его через `@AGENTS.md`. Проверки работают общими
git-хуками, а не хуками отдельного агента; исходная схема следовала практике tank8000.

<a id="skills-and-plugins"></a>

## Навыки и плагины

Оба вида навыков имеют плоскую структуру `<skill>/SKILL.md`:

| Расположение | Назначение и контракт |
|---|---|
| `skills/` | Для установки плагина без рабочего каталога; без зависимости от `just`, `uv` и `original/`. Корень плагина — на два уровня выше каталога навыка. |
| `.claude/skills/` | Для разработки внутри репозитория, включая `konung-reverse-format`. |

Пользовательский навык `konung-tiled-install` устанавливает расширение,
проверяет его загрузку в Tiled и открытие Konung.N. Команды установки —
в [руководстве Tiled](../guides/tiled.md#installation-through-an-agent).

Манифесты: `.claude-plugin/` для Claude Code, `plugin.json` и
`.agents/plugins/marketplace.json` для Codex, `qwen-extension.json` для Qwen Code.
`.claude/settings.json` объявляет маркетплейс/плагин tiled-mcp через
`extraKnownMarketplaces` и `enabledPlugins`.

Навык создаётся только для повторяемой последовательности в нескольких
задачах, которую не заменяет ссылка на документацию. Навык исследования
задаёт процедуру; [исследования](../research/README.md) хранят доказательства.

<a id="tiled-inspection"></a>

## Проверка в Tiled

Используйте [tiled-mcp](https://github.com/shpaker/tiled-mcp) и его навык `tiled`:
`tiled_status` → `tiled_open` на Konung.N → `map_summary`, `list_objects`,
`render_map`, `run_script`. Оригинальные карты открываются через `tiled/konung/`;
их представление описано в [устройстве расширения](tiled.md).

Полноразмерное изображение для сравнения сохраняется через `run_script`;
задайте абсолютный путь внутри `out/` рабочего каталога:

```js
const image = map.toImage(); image.save("<repository-root>/out/k1/tiled.png"); return [image.width, image.height]
```

`tiled_install_extension` устанавливает собственное расширение tiled-mcp
с разрешения пользователя. Установка отдельного расширения игры — в руководстве.
Внешние ошибки сообщаются по правилу AGENTS.md; молча обходить их нельзя.

<a id="checks-and-data"></a>

## Проверки и данные

`just setup` устанавливает шимы prek. `just check` запускает проверки файлов,
форматирование/линты Python и Rust и тесты; [проверки](testing.md) описывают
действующие хуки и CI. Формат issues/PR и его проверку задаёт
[CONTRIBUTING.md](../../CONTRIBUTING.md).

Игровые файлы читаются из `original/`; производные помещаются в `out/`.
Оба каталога не коммитятся. Ограничения распространения заданы в
[AGENTS.md](../../AGENTS.md#copyrighted-content).
