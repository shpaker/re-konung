# kn-game

Общий фасад игр серии «Князь»: Bevy-независимая модель сцены и подготовка
данных для отображения. Потребители (`kn-io`, бинарники) видят только этот
крейт; специфика частей серии спрятана за фасадом (сейчас — `k1-game`,
позже `k2-game`).

## API

- `Game::open(путь)` — открыть каталог установленной игры.
- `Game::maps()` / `Game::heroes()` — список карт (игра + обучение) и `Game.N`.
- `Game::scene(map, options)` — `Scene`: упорядоченный список кусков
  (позиция, размер, ключ текстуры). Порядок = порядок наложения.
- `Game::textures(ids)` — RGBA-картинки ключей; рендер кэширует по `TextureId`.
  Каждому ключу соответствует `DecodedTexture`: картинка, пустой слот или
  причина, по которой картинку не собрать.
- `Game::check(maps)` — headless-проверка: сцена строится, все текстуры
  декодируются, статистика детерминирована (`CheckReport`, текст и JSON).
  Начальное состояние берётся по источнику карт (`Game.N` для игры,
  `TGame.N` для обучения), а ошибка одной карты попадает в её строку отчёта,
  не прерывая проверку остальных.

## Movement

`kn_game::movement` — the shared movement engine: the isometric fine grid, cell
walkability and occupancy, route search, walking a route and the game tick. It
knows neither Bevy nor K1: the layer lives in `kn-game` because the movement
mechanics of K1 and K2 are presumably the same
([docs/k1/movement.md](../../docs/k1/movement.md) — the facts,
[docs/rust.md](../../docs/rust.md) — the project decisions).

- `Grid`/`Cell`/`Direction` — the grid, eight directions, cell ↔ pixels.
- `Passage` — whose trait: a data supplier brings cell walkability.
- `Pace` — whose trait: the supplier brings the step duration in ticks too.
- `find_route(request, rng)` — A\* over the grid; `Route` — steps to the goal, or
  to the nearest free cell beside it; `None` — there is no route.
- `Walker` — walking a finished route: `start_step`, `tick`, `state`.
- `World::add` / `move_to` / `tick_step` — several creatures, the tick, disputes
  over cells and repathing; outward `WalkState` and `Event`.

No game rules here (who goes where, and why) — epic #33 part 3 adds them.

The facade and the scene model exist for `kn-io` and the binaries; the crate has
no "shared game rules" and gains none: the engine walks where it is told and
never decides where to go (AGENTS.md, docs/rust.md).

## Проверки

```bash
just cargo-test   # синтетические всегда, на данных — если есть original/
just cargo-lint
```
