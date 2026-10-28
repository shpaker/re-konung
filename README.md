# re-konung

Изучение и воссоздание движка игр серии «Князь» (1С/Snowball): «Князь: Легенды Лесной страны» (1999), затем «Князь 2» и «Князь 2.5».

Сейчас (этап 1) карты первой части открываются в [Tiled](https://www.mapeditor.org) напрямую из оригинальных файлов, редактируются и сохраняются обратно в формат игры. Форматы карт, тайлов и объектов разобраны и описаны в [docs/](docs/README.md).

Файлов игры в репозитории нет: нужен свой образ диска ([docs/k1/editions.md](docs/k1/editions.md)).

## Быстрый старт

Нужны [uv](https://docs.astral.sh/uv/), [just](https://just.systems), 7-Zip, Tiled 1.12.

```bash
just setup
```

```bash
just k1-unpack rus original/k1/rar/rus.rar
```

```bash
uv run konung k1 map original/k1/rus/game out/k1/maps -n 1
```

```bash
just tiled-install
```

После этого в Tiled: «Файл → Открыть» → `original/k1/rus/game/Konung.1`.

Как вносить изменения — [CONTRIBUTING.md](CONTRIBUTING.md); правила для агентов — [AGENTS.md](AGENTS.md).
