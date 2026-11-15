<a id="rust-builds-and-ci"></a>

# Сборка Rust и CI

<a id="build-profiles"></a>

## Профили сборки

Три профиля в `crates/Cargo.toml` разделяют задачи. Обычный цикл локальной
разработки использует `dev-runtime`; `release` запускается явно для замеров
и отдельных проверок.

| Профиль | Где используется | Наш код | Зависимости |
|---|---|---|---|
| `dev` | `just cargo-test`, `just cargo-lint`, хуки prek: clippy перед коммитом, проверки перед push | opt-level 0, полная отладочная информация | opt-level 0 |
| `dev-runtime` | `just viewer`, `just game` | opt-level 1, `debug = "line-tables-only"` | opt-level 3, включая build scripts и proc macros (`build-override`) |
| `release` | `just viewer-release`, `just game-release`, `just check-maps`, ручные замеры, артефакты релиза | opt-level 3 | opt-level 3 |

На Ubuntu требуются системные библиотеки, перечисленные в действующем CI:

```sh
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
    g++ pkg-config libx11-dev libxkbcommon-x11-0 libxkbcommon-dev \
    libwayland-dev libasound2-dev libudev-dev
```

`dev-runtime` нужен потому, что Bevy требует оптимизированных зависимостей,
а наш код должен быстро пересобираться. Исторический замер: Windows,
rustc 1.99, прогретый кеш, правка одной строки; первый результат исключался,
если содержал разовые затраты:

| Прогретый цикл | `release` до изменений | `dev-runtime`, статически | + rust-lld | + bevy_dylib |
|---|---|---|---|---|
| Правка `kn-render` → бинарный файл просмотрщика | 7,6 с | 6,8 с | 5,9 с | **2,2 с** |
| Правка бинарного крейта → перелинковка | 4,2 с | 7,5 с | 4,5 с | **1,0 с** |
| Без изменений | 0,3 с | 0,3 с | 0,4 с | 0,4 с |

Три измеренных приёма:

- **rust-lld** (`.cargo/config.toml`, `[target.x86_64-pc-windows-msvc] linker = "rust-lld"`,
  `-C linker-flavor=lld-link`). Входит в toolchain (rustc ≥ 1.70); установка
  не нужна. Секция действует на Windows, включая dev-тесты, не на Linux CI.
  Добавление rustflags один раз пересобирает кеш всех единиц (~2,5 мин на профиль).
  Удаление этой настройки возвращает `link.exe`.
- **`debug = "line-tables-only"`** для `dev-runtime`: запись PDB — часть
  стоимости линковки Windows; backtrace сохраняет файл и строку.
  Отдельного сравнения с `debug = 2` не было: вместе с остальными приёмами
  перелинковка уже занимает 1 с.
- **`bevy/dynamic_linking`** за feature `dynamic` крейтов `kn-fe-viewer`/`kn-fe-player`,
  включаемой рецептами `viewer`/`game`. Bevy уходит в `bevy_dylib.dll`
  (137 МБ в `crates/target/dev-runtime/deps`), приложение линкуется за ~1 с.
  Оба окна проверены на Windows/Vulkan. EXE запускается через `cargo run`/`just`,
  которые добавляют deps в PATH; прямому запуску нужна DLL рядом с EXE.
  Наш код должен оставаться на opt-level ≤ 1, как в `dev-runtime`.
  Уберите `--features …/dynamic`, чтобы линковать статически.

`just check-maps` намеренно использует `release`: безоконный разбор ограничен
процессором и нашим кодом, без рендера Bevy. В `dev-runtime` он на порядок
медленнее: исторически 77 с для полного набора 50 номеров RUS против ~5 с
в release. Обычные `viewer`/`game` используют `dev-runtime`, явные варианты
`*-release` и ручные замеры сохраняются.

Возможности Bevy объявлены один раз в `[workspace.dependencies]` с
`default-features = false`: 2D-спрайты, Feathers UI, текст со встроенным шрифтом,
picking, winit. Цепочка 3D/PBR/glTF/audio/gizmos/anti-alias/post-process
по умолчанию выключена: ссылок на неё в нашем коде нет. Оставшиеся
`bevy_gizmos`, `bevy_gilrs`, `bevy_scene`, `bevy_animation`, `bevy_state`
приходят через зависимости `bevy_feathers`. После сокращения проверены оба окна.
Обнаруженная регрессия — отсутствие `bevy_ui_render`/`ui_bevy_render`:
без них панель Feathers паникует с `DrawFunctions<TransparentUi> must be added`.
Единый набор возможностей Cargo объединяет для всех пяти Bevy-крейтов;
изменение одного не должно незаметно включать стандартный набор остальных.

sccache необязателен и не входит в настройку: после отдельной установки
`RUSTC_WRAPPER=sccache` позволяет рабочим деревьям использовать общий кеш
компиляции. У каждого остаётся собственный `crates/target`; без кеша новое
дерево оплачивает холодную сборку зависимостей каждого профиля
(в записанном окружении ≈2,5–3 мин).

## CI (`.github/workflows/ci.yml`)

Замеры на GitHub-hosted runners, rustc 1.99.1, 2026-11-13; каждая строка —
отдельный запуск PR #120: 43404019876 холодный, 43407464572 прогретый.

| Задание | Runner | Холодный | Прогретый |
|---|---|---|---|
| `rust`: fmt, clippy, test (`dev`) | ubuntu-latest | 7 мин 32 с | 3 мин 56 с |
| `build`: полная линковка `kn-viewer` + `kn-player` (`dev-runtime`) | ubuntu-latest | 14 мин 50 с | 1 мин 39 с |
| `build-windows`: та же сборка | windows-latest | 21 мин 1 с | 1 мин 44 с |

`build` — проверка PR; `build-windows` запускается только при push в main
и `workflow_dispatch`. Тот же профиль на windows-latest оказался в 1,4 раза
медленнее ubuntu-latest: `cargo build --timings`, 503 единицы, 3379 с rustc
за 859 с общего времени на Linux против 4635 с за 1204 с на Windows.
Поэтому проверка PR работает на Linux. Регрессию только Windows ловит сборка
после слияния в main — принятый компромисс; релиз требует зелёного main
(`version.yml`). В холодной сборке `bevy_*` занимают 56–58% времени rustc:
на Linux `bevy_feathers` 141 с, `bevy_render` 137 с, `bevy_ui` и
`bevy_sprite_render` по 121 с. Основные затраты — зависимости;
`kn-render` при opt-level 1 компилируется за 39 с.

Кеш зависимостей — `Swatinem/rust-cache@v2`, `workspaces: crates`.
Крейты workspace намеренно пересобираются каждый раз; идентификатор задания
по умолчанию входит в ключ, поэтому три задания имеют отдельные кеши:
≈1,1 ГБ (`dev`), ≈0,8 ГБ (`dev-runtime` Linux), ≈0,7 ГБ (Windows).
Восстановление — 23–34 с на задание, точное совпадение ключа не сохраняется повторно.
Обе сборки запускаются с `--timings` и публикуют `cargo-timing.html` как
артефакт запуска для разбора затрат по крейтам. Ночного расписания нет:
при записанной частоте push в main несколько раз в день успешные запуски
обновляли кеши. При снижении частоты это стоит пересмотреть: GitHub удаляет
неиспользуемые 7 дней кеши, лимит репозитория в записанной конфигурации — 10 ГБ.
