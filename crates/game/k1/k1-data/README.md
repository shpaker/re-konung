# k1-data

Данные игры «Князь: Легенды Лесной страны» (1999): репозиторий и форматы.
Часть Cargo-workspace [`crates/`](../Cargo.toml); эталон — Python-пакет
`konung/`, при расхождении прав Python (AGENTS.md).

## Слои

- `repo` — `GameDir`: поиск файлов игры без учёта регистра, номера карт и
  состояний, чтение байтов; наборы `GAME_FILES` / `TUTORIAL_FILES`.
- Форматы — чистые `parse(&[u8])`, файловую систему не трогают:
  - запись: `map` (`Konung.N`, `LIGHTS.N`), `game_state` (`Game.N`),
    `game_edit` (правки состава);
  - только чтение: `graph_res` (тайлы, спрайты), `objects_res` (объекты),
    `lights_res` (маски света), `flames_res` (огни), `executable` (таблицы
    из EXE), `characters` (сборка кадра персонажа), `rle`, `palette`.

Форматы описаны в [docs/k1/formats/](../../docs/k1/formats/); общая картина —
[docs/rust.md](../../docs/rust.md).

## Проверки

```bash
just cargo-test   # тесты: синтетические всегда, на данных — если есть original/
just cargo-lint   # cargo fmt --check + clippy -D warnings
```

Тесты на данных (`tests/original*.rs`) берут файлы из `original/` и
пропускаются без них; как их получить — [docs/k1/editions.md](../../docs/k1/editions.md).
