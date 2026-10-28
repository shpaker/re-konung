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

Как вносить изменения — [CONTRIBUTING.md](CONTRIBUTING.md); правила для агентов — [AGENTS.md](AGENTS.md).
