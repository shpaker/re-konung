# re-konung

Изучение и воссоздание движка игр серии «Князь» (1С/Snowball): «Князь: Легенды Лесной страны» (1999), затем «Князь 2» и «Князь 2.5».

Сейчас (этап 1) карты первой части открываются в [Tiled](https://www.mapeditor.org) напрямую из оригинальных файлов, редактируются и сохраняются обратно в формат игры. Форматы карт, тайлов и объектов разобраны и описаны в [docs/](docs/README.md).

Неофициальный фанатский проект, с 1С и Snowball не связан. Права на игры серии и их название принадлежат правообладателям.

## Файлы игры

Файлов игры в репозитории нет и не будет. Нужна своя лицензионная копия: образ своего CD или установленная игра. Сейчас первая часть продаётся в цифровом виде, например на [GOG](https://www.gog.com/en/game/konung_1_2). Как подготовить файлы — [docs/k1/editions.md](docs/k1/editions.md).

## Быстрый старт

Нужны [uv](https://docs.astral.sh/uv/), [just](https://just.systems), 7-Zip, Tiled 1.12.

```bash
just setup
```

```bash
just k1-unpack rus
```

```bash
uv run konung k1 map original/k1/rus/game out/k1/maps -n 1
```

```bash
just tiled-install
```

После этого в Tiled: «Файл → Открыть» → `original/k1/rus/game/Konung.1`.

## Устройство

Рядом живут две реализации. Python-пакет `konung/` и расширение `tiled/konung/` — эталон разбора форматов: при расхождении прав Python. Rust в `crates/` — будущий движок на [Bevy](https://bevyengine.org):

```text
   re-konung            re-konung-viewer
   (kn-exec: игра)      (kn-viewer: окно, UI, headless-диагностика)
         \                    /
          ▼                  ▼
              kn-io — Bevy: рендер сцены, камера, ввод, кэш текстур
                     │
                     ▼
              kn-game — общий фасад серии: Game, Scene, check;
                        скрывает специфику частей
                     │
                     ▼
              k1-game — «Князь 1»: ресурсы, текстуры, список отрисовки
                     │
                     ▼
              k1-data — форматы: Konung.N, Game.N, GRAPH.RES,
                        Objects.res, LIGHTS.RES, FLAMES.RES, EXE
```

Зависимости идут только вниз: `kn-io` и бинарники не знают о K1, `kn-game` не знает о Bevy, `k1-data` — ни о том ни о другом. Пока часть серии одна, в `kn-game` нет «общих игровых правил», только фасад и модель сцены: общее выделяется, когда появится второй потребитель. Форматы описаны в [docs/k1/formats/](docs/k1/formats/), Rust-стороны — в [docs/rust.md](docs/rust.md).

## Просмотр карты в Bevy

```bash
cargo run --release --manifest-path crates/Cargo.toml -p kn-viewer -- \
    --game-dir original/k1/rus/game
```

Панель слева: выбор карты и отладочные оверлеи — проходимость, свет, маркеры `Game.N` и персонажи (включены по умолчанию). При открытии карта целиком вписывается в окно; камера: WASD и стрелки или перетаскивание левой кнопкой, колесо — масштаб ступенями, на целых ступенях картинка пиксель-в-пиксель. Переключение оверлея кадр не сбрасывает.

Проверка без окна и GPU — для CI и агентов:

```bash
cargo run --release --manifest-path crates/Cargo.toml -p kn-viewer -- \
    --game-dir original/k1/rus/game check --all --format json
```

Ненулевой код возврата, если карта не собралась или её текстура не декодировалась.

## Через агента

Репозиторий — плагин для Claude Code, Codex и Qwen Code с навыком `konung-tiled-install`: агент ставит расширение в Tiled и проверяет, что карта открывается. Клон и `just` для этого не нужны, только Tiled 1.12 и своя копия игры.

Claude Code:

```
/plugin marketplace add shpaker/re-konung
/plugin install konung@re-konung
```

Codex: `codex plugin marketplace add shpaker/re-konung`, затем `konung` в `/plugins`.

Qwen Code:

```bash
qwen extensions install https://github.com/shpaker/re-konung
```

Потом попросить агента поставить расширение «Князя» для Tiled и открыть карту.

Как вносить изменения — [CONTRIBUTING.md](CONTRIBUTING.md); правила для агентов — [AGENTS.md](AGENTS.md).
