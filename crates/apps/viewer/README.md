# kn-viewer

Приложение просмотрщика карт «Князя»: бинарь `re-konung-viewer`. Здесь — CLI,
разрешение стартового выбора карты и headless-диагностика `check`; само окно
(состояние, панель, оверлеи, диагностика) живёт в
[kn-fe-viewer](../../frontend/bevy/viewer).

## Запуск

```bash
re-konung-viewer --game-dir <каталог игры> [--map N] [--source game|tutorial]
re-konung-viewer --game-dir <каталог игры> check --map 1
re-konung-viewer --game-dir <каталог игры> check --all [--format text|json]
```

Каталог игры — содержимое `inst/setup.dat`; как его получить,
[docs/k1/editions.md](../../../docs/k1/editions.md).

Из репозитория без установки:

```bash
just viewer                            # окно
just check-maps                        # то же без окна, GPU и рендерера
cargo run --release --manifest-path crates/Cargo.toml -p kn-viewer -- \
    --game-dir original/k1/rus/game
```

## Устройство

- `cli` — аргументы; `viewer_config` — открытие игры и разрешение стартовой
  карты; `check` — подкоманда, которая не открывает окно и не требует GPU.
- неверный `--map` — ошибка с ненулевым кодом возврата, а не переход на первую
  карту.
- `check` печатает отчёт по картам (текст или JSON) и возвращает ненулевой
  код, если карта не собралась или её текстура не декодировалась — так
  сцену проверяют CI и агенты без окна.
- окно, панель и оверлеи — в `kn-fe-viewer`; приложение передаёт ему
  `ViewerConfig` (открытая игра, каталог, список карт, стартовый выбор) и
  типы CLI туда не переходят.

## Проверки

```bash
just cargo-test   # разбор CLI, отчёт об ошибке
just cargo-lint
```
