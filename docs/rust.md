# Rust-реализация

The general architecture overview — components, boundaries, the diagram, current versus target shape — lives in the root [ARCHITECTURE.md](../ARCHITECTURE.md); this page keeps the Rust-side detail.

Cargo-workspace в `crates/` (edition 2024). Повторяет Python-пакет `konung/` слой за слоем; Python — эталон, и при расхождении прав Python (AGENTS.md). Пока Rust ничего не заменяет: Python-CLI и расширение Tiled остаются как есть.

The game-layer crates are grouped under `crates/game/` (`game/k1/k1-data`, `game/k1/k1-game`, `game/kn-game`); the display crates live under `crates/frontend/bevy/` and the binaries under `crates/apps/` — the layout refactor is complete. Directory grouping changes no package name and no dependency.

## Крейты

| Крейт | Что внутри | Статус |
|---|---|---|
| `k1-data` | данные игры K1: репозиторий (`repo`), форматы карт и состояния (`map`, `game_state`), правки состава (`game_edit`) и графика (`rle`, `palette`, `graph_res`, `objects_res`, `lights_res`, `flames_res`, `executable`, `characters`) | `Konung.N`, `LIGHTS.N`, `Game.N`: разбор и побайтно точная запись; добавление/удаление персонажей сохраняют укладку; `GRAPH.RES`, `Objects.res`, `LIGHTS.RES`, `FLAMES.RES` и таблицы из EXE читаются; кадры персонажей собираются; тесты на `original/` |
| `k1-game` | специфика K1: ресурсы (`resources`), ключи текстур и RGBA-декодирование (`textures`), список отрисовки карты с группами кусков (`scene`) | сцена строится по эталону `konung/k1/render.py` (порядок, сортировка по нижнему краю, тени, огни, персонажи, оверлеи); каждый кусок помечен группой, какой слой её переключает; кадры анимаций собираются из `OBJECTS.RES`, таблицы огней EXE и последовательностей персонажа; headless, без Bevy; тесты на `original/` |
| `kn-game` | the shared facade: `Game` (open a game directory, maps, scene, textures, headless `check`), the Bevy-independent model `Scene`/`Piece`/`TextureId`, `Animation`/`AnimationFrame`, `PieceGroup` and `group_is_shown`; the shared movement engine `movement` (grid, walkability, routes, a step per tick) | hides K1 behind a common API; `movement` knows neither K1 nor Bevy and reads no game files — the part's knowledge arrives through the `Passage` and `Pace` traits; K1 and K2 movement mechanics are presumably nearly identical ([AGENTS.md](../AGENTS.md), "Priority: modern experience"); `CheckReport` serializes to JSON |
| `kn-input` | frontend input mechanisms: `pan_direction` (keyboard), `drag_delta` (mouse drag), `ZoomScroll`/`wheel_steps` (wheel accumulation) | `bevy` only — no camera types, no UI, no game rules; the bindings that wire the mechanisms to a camera live in the apps (the `CameraInput` set of `kn-render`) |
| `kn-ui` | the shared Bevy UI design system: design tokens (`palette`, `spacing`, `typo` with the bundled PT Sans), the konung theme over Feathers (`konung_theme`, `KonungUiPlugin`), BSN widget helpers (`widgets`), checkbox mirroring (`sync_checked`), the pointer-capture check (`pointer_over_ui`) | `bevy` only (with `bevy_feathers`) — no rendering, no input bindings, no game rules; app-specific panels and widget state stay in the frontends |
| `kn-render` | Bevy: кэш текстур (`textures`), куски сцены → спрайты по узлам групп (`scene`), покадровая анимация (`animation`), камера без ввода (`camera`) | Bevy 0.20, окно и GPU нужны только для запуска, не для тестов; оверлей — `Visibility` узла группы, а не пересборка карты; `animation` не знает ни о K1, ни о форматах |
| `kn-fe-viewer` | Bevy frontend of the viewer: state (`Viewer`), the Feathers panel, overlays, playback, diagnostics, the camera binding and the App assembly; receives a `ViewerConfig` and knows no CLI | the map is built with all layers at once; toggling an overlay changes group visibility, not a rebuild |
| `kn-viewer` | app `re-konung-viewer`: CLI, startup resolution, headless `check` | `check` creates no Bevy App, window or GPU context and returns a nonzero code on failure; bevy is linked only transitively |
| `kn-fe-player` | Bevy frontend of the player: window, camera binding, the AppFlow pipeline over `kn-app` (flush → effects → results → screens; `session_watch` reports readiness only when observed), session build/teardown of the start map, the menu/settings/loading screens, the two-language text (`i18n`) | no CLI, no game rules — only the display pipeline and the App assembly; the app core decides, the frontend executes |
| `kn-app` | the player's application core in pure Rust: `Screen`/`Action`/`Effect`, the pure `AppController` reducer with its transition table, typed session/persist errors | no Bevy, no `kn-game` — the game is reached only through effects the frontend executes; result actions never emit effects, so the frame pipeline has no feedback loops; 22 headless tests pin the whole table |
| `kn-settings` | the player's settings in pure Rust: the model (`Language`, `VideoSettings`), defaults, versioned JSON persistence | `load` never writes; `save` is loss-proof — `tmp`+rename, a `.bak` of anything corrupt or newer before replacing, rollback of the backup on a failed rename; no Bevy |
| `kn-player` | app `re-konung`: CLI (`--game-dir`, `--new-game`, `--settings`), settings bootstrap, game opening, hero resolution | thin `main`: settings load first (typed errors, defaults on a bad file, the file never touched), then `Game`, the hero, `run_player` |

Будущие крейты: `k2-data`, `k2-game`. Общее выделяется, когда общность подтверждена, а не заранее, — та же логика, что в [games.md](games.md) про проверку общности движка на K2. Исключение сделано для перемещения: его механика в K1 и K2 предположительно почти одинакова (заметное отличие — бег), поэтому общий движок сразу строится в `kn-game`, а специфика части остаётся в `k1-game`.

## Слои `k1-data`

```
данные игры (original/k1/<издание>/game)
  → repo: GameDir — поиск файлов без учёта регистра, номера карт и состояний, чтение байтов; FileSet (игра/обучение)
    → map: чистые parse(&[u8]) / serialize — KonungMap, Lights; ФС не трогают
    → game_state: чистые parse(&[u8]) / serialize — GameState (предметы, отряды, клады, переходы, персонажи, деревни), on_map и геометрия мелкой сетки
    → game_edit: add_character / remove_character на GameState — правки держат плотную укладку отрядов и переписывают все ссылки; API принимает номера записей, а не объекты (в Rust нельзя занять запись и одновременно править состояние)
    → графика, только чтение: rle и palette (декодирование картинок, RGB555), graph_res (тайлы, спрайты персонажей), objects_res (объекты и монстры), lights_res (маски света), flames_res (огни), executable (таблицы снаряжения, порядка слотов, огней и тексты из EXE: названия монстров, имена, прозвища, названия карт), characters (сборка кадра персонажа со снаряжением и тенью)
      → {игровые данные, Tiled} — слои выше появятся позже
```

Тестируемость: форматный слой тестируется подменой данных, а не реализаций; `GameDir` dyn-совместим (методы `&self`, конкретные типы возврата). Трейты для моков объявляет потребитель, когда они ему действительно понадобятся («accept traits, return structs»); провайдерских трейтов в `k1-data` нет.

## Слои сцены (`k1-game` → `kn-game`)

```
k1-data (форматы)
  → k1-game: resources — загрузка графики из GameDir; textures — ключи текстур и RGBA-декодирование; scene — список отрисовки карты (порядок render.py)
    → kn-game: Game — фасад (open/maps/heroes/scene/textures/check); Scene/Piece/TextureId — общая модель, K1-типы наружу не выходят
      → kn-render: textures — TextureCache (RGBA → Image, один ассет на ключ); scene — куски → спрайты; animation — часы тиков и последовательности кадров на спрайтах; camera — ступенчатый масштаб, границы, запись камеры
        → kn-fe-viewer (window/systems) → kn-viewer app (re-konung-viewer: CLI, check) — бинарник
        → kn-fe-player (window/systems) → kn-player app (re-konung) — бинарник

kn-game::movement — the shared movement engine — stands aside from this chain:
it depends on neither `k1-*` nor Bevy, and takes the part's knowledge through
the `Passage` and `Pace` traits (see "Movement engine")

kn-input — the input mechanisms — stands beside kn-render: it depends on bevy
only, and each binary binds the mechanisms to its camera (see "Отображение")
kn-ui — the shared UI design system — stands beside them: bevy-only as well
(with bevy_feathers), no rendering and no input bindings; the frontends take
the konung theme (`KonungUiPlugin`), checkbox mirroring and the
pointer-capture check from it
```

Dependencies point downward only: `kn-render` and the binaries know nothing of K1, `kn-game` knows nothing of Bevy, `k1-data` knows nothing of either; `kn_game::movement` knows nothing of K1 and nothing of the display at all.

Порядок наложения сцены повторяет эталон `konung/k1/render.py`: земля → верхний слой → декорации (сырые пропускаются) → слой теней → объекты с огнями и персонажи, отсортированные по нижнему краю (ключ `(bottom, вид, слот)`, устойчивая сортировка — огни объекта остаются над его картинкой) → оверлеи: свет, проходимость, маркеры `Game.N`. Текстовые подписи маркеров не рисуются: в модели сцены нет текста, его показывает UI просмотрщика. Кольца маркеров — растеризованное приближение PIL-эллипса толщиной 3.

### Группы кусков

Каждый кусок помечен группой (`k1_game::scene::PieceGroup`, наружу — `kn_game::PieceGroup`): земля, декорации, тени карты, объекты, персонажи, тени персонажей, маски света, проходимость, маркеры. Группа отвечает только за то, что скрывается и показывается вместе: порядок наложения по-прежнему задаёт позиция куска в списке отрисовки, и куски одной группы идут вперемежку с чужими (персонажи между объектами). `PieceGroup::ALL` — порядок, в котором слой отображения заводит узлы групп.

`PieceGroup::is_shown_by` — единственное место, где известно, каким слоем переключается группа; `kn_game::group_is_shown` повторяет её по `SceneOptions`, чтобы `kn-render` и бинарники не выбирали группы сами: иначе знание про «что такое свет, а что маркеры» утекло бы в слой отображения. Тени персонажей переключаются слоем персонажей: при выключенных персонажах список отрисовки не содержит ни одной их тени, и показывать их было бы нечего.

Инвариант: карта, собранная со всеми слоями и отфильтрованная по группам, — тот же список кусков, что и карта, собранная только с включёнными слоями. Держат его три теста: `group_filter_matches_a_scene_without_the_layer` (все карты обоих изданий, слои поодиночке), `pieces_carry_their_group` (модель сцены) и `every_group_of_a_real_map_gets_its_own_node` (узлы, z и наследование видимости в мире Bevy). `check` от разноса по группам не меняется: у его отчёта свои наборы слоёв, и он прошёл побайтко до и после правки.

GRAPH.RES и OBJECTS.RES заимствуют буфер, поэтому `Resources` хранит их байтами и разбирает на время операции (`with_graphics`); владеющие таблицы EXE, маски света и огни разбираются один раз.

### Кадры и анимации

Анимация идёт по часам тиков, не по номеру кадра в файле: `Piece.animation` — кадры одного слоя (`TextureId`, смещение угла от куска, длительность в **тиках**), `Piece.texture` — базовый кадр. Тик — 60 мс (`k1_data::objects_res::TICK_MS`, Slavik `game.h:30`), длительности — **гипотеза**, в игре не сверены; те же числа использует расширение Tiled ([k1/tiled.md](k1/tiled.md)).

- **Откуда кадры.** Огни — диапазон `first..=last` вида из таблицы EXE, по тику на кадр; простые объекты — `frame_count` кадров `OBJECTS.RES` с длительностью `frame_time` тиков (объект с несколькими кадрами, но без `frame_time` игра не анимирует, и мы тоже); персонажи и монстры — `characters::compose` по всем кадрам последовательности его состояния и направления.
- **Базовый кадр = кадр из данных** (`frame` у объекта, `frame` у персонажа). Порядок наложения и ключ сортировки считаются по нему один раз при сборке сцены, поэтому анимация не переставляет куски, а при выключенной анимации картина совпадает со статичным рендером Python.
- **Скрытые слои.** Слой, которого нет в каком-то кадре (предмет без картинки, тень не у каждого кадра, тело не в каждом кадре), остаётся в последовательности пустым кадром и прячет спрайт через `Visibility::Hidden`. Выбрасывать кадр нельзя — сдвинулся бы тайминг всех остальных слоёв. Слои сопоставляются по устойчивому ключу `characters::LayerKey` (`Body`, `Shadow`, `Gear { slot }`), а не по позиции в списке: пропуски сдвигают позиции.
- **Синхронность** многослойных анимаций — следствие того, что кадры одного объекта или персонажа строятся из одной последовательности с одинаковыми длительностями и общим `base`: часы на карту одни.
- **Текстуры** всех кадров докачиваются при сборке сцены (`Scene::texture_ids` возвращает базовые кадры и все кадры анимаций), поэтому середина цикла не бывает пустой, а `check` видит битый кадр как ошибку карты. На ENG это ~450 уникальных текстур на карту вместо ~300; `check --all` идёт ~5 с в release.

`check` берёт начальное состояние по источнику карт: `Game.N` для игры, `TGame.N` для обучения. Одно состояние на оба источника расставляло бы по картам обучения персонажей основной игры (в ENG это давало 17 и 5 персонажей вместо 8 и 2). Ошибка одной карты попадает в её строку отчёта и не прерывает проверку остальных; недостающая текстура отличается от пустого слота и несёт причину (битое декодирование, палитра вне банка).

Бинарные данные не считаются заведомо целыми: `rle` отсекает размеры за пределом `MAX_DIMENSION` и пропуски за ширину строки, `graph_res` проверяет, что палитра тайла помещается в банк цветов, `objects_res` — что число огней не превышает 20 слотов. Иначе следующее за чтением вычитание или аллокация паниковали бы на битом файле. Проверка пропуска зеркалирована в Python-эталон и в расширение Tiled — все три реализации формата остаются одинаковыми.

## Movement engine (`kn_game::movement`)

The shared layer (issue #35, epic #33 part 2): the isometric fine grid, cell
walkability and occupancy, route search, walking a route and the game tick.
Neither Bevy nor K1 — the part's specifics live in `k1-*` and the display in
`kn-render`, so only the mechanics are here. Modules:

- `grid` — the cell (`Cell`), the eight directions (`Direction`), cell ↔ pixels,
  lattice coordinates without the row shift (`lattice`) and distance in cells;
- `passage` — the `Passage` trait: a data supplier brings cell walkability;
- `pace` — the `Pace` trait: how many ticks a step lasts in a direction — also
  the supplier's job;
- `occupancy` — `Occupancy`: one creature per cell, standing or held by a step
  reservation;
- `route` — `find_route` and `Route`: A\* (`pathfinding::astar`) over the grid;
- `walk` — `Walker`: walking a finished route, steps per tick and the sprite
  state (`WalkState`);
- `world` — `World`: several creatures, the tick, disputes over cells, repathing
  and events (`Event`); `tick_step` is the only event channel (see "Decision: one
  channel for events");
- `rng` — `Rng`: splitmix64, our own deterministic generator.

Search and walking are deliberately far apart: `route` knows neither the tick nor
the step duration, `walk` knows neither walkability nor how its route was found.
`World` is the only place they meet and the only place time is counted.
Walkability and occupancy are split the same way: `Passage` is the map's data,
`Occupancy` is engine state, because only the engine knows who is mid-step and
where. The original's facts — [k1/movement.md](k1/movement.md); the sections
below are what we chose, not what the game shows.

### Decision: step cost and the A* heuristic

A step costs as many pixels as it travels: 464 units west and east, 256 north and
south, 265 along the diagonals — all in 1/8 pixel, the diagonal being
`ceil(8·√(29² + 16²))` rounded up. The heuristic is the straight-line distance
between cell points in the same units (`u64::isqrt`, rounded down).

The reason not to use "10/14 in cells": a cell rhombus is 58×32 px while
neighbours stand 58, 32 and ≈33.1 px away, so orthogonal and diagonal steps have
different lengths. A "10/14" metric would give the diagonal a cost of 14 against a
north step's 10, though in pixels it is 33.1 against 32: with such weights the
heuristic overestimates and A\* returns a route longer than the shortest. Once a
step costs its own length, a straight-line estimate is admissible and monotone by
the triangle inequality, so the search returns a genuinely shortest path and the
route looks natural on an isometric picture
(`route_cost_stays_close_to_a_straight_line`). Rounding the diagonal's cost up is
deliberate: a step may never cost less than it really is.

For the neighbourhood of a taken goal (several possible final cells) the
heuristic measures the distance to the centre of the neighbourhood and subtracts
the largest distance to a cell of that ring: the estimate stays admissible and no
`isqrt` is needed per candidate cell.

Distance in cells (`Cell::lattice_distance`, Chebyshev in unshifted lattice
coordinates) kept a different role: it measures the neighbourhood of a taken goal.

### Decision: routes without the 32-step limit

A\* searches for a route of any length. The original's limit was the price of
memory in 1999; it is reflected nowhere in the data and is not carried into modern
limits (AGENTS.md, "Priority: modern experience"; the fact —
[k1/movement.md](k1/movement.md), "Route limit: 32 steps in the original"). The
saving is not one we need: a search over the whole fine grid costs 0.2–0.4 ms (see
"Engine cost") — under one percent of a 60 ms tick.

### Decision: no corner cutting

Directions are divided not by the screen's diagonal but by how a cell touches its
neighbour: east, west, north and south step through a vertex of the rhombus, and
they are allowed only when both cells standing across the path are walkable
(`Direction::corner_flanks`). Steps along a screen diagonal share a whole edge
with the current cell — there is no corner left for them to cut. In unshifted
lattice coordinates this is exactly the ordinary ban on diagonal moves in an
8-connected grid: the four "screen-diagonal" steps are orthogonal there, while
east/west/north/south are the diagonal ones.

Flank cells are checked for map walkability only: another creature in a flank cell
keeps a creature from entering the destination cell, and that is `Occupancy`'s
job, not a corner rule. Otherwise a creature standing off to the side would block
every passage.

### Decision: one channel for events

`tick_step` is the only place engine events leave. An order `move_to` given
between ticks puts its own event (`RouteSet` or `NoRoute`) into the world's shared
channel, and it is the first thing to arrive on the next tick, in the order the
orders were given: outside a tick an event is neither lost nor emitted twice, and
a consumer needs a single source. `move_to`'s return value stays the answer to the
order itself — a step count or `Err(NoRoute)` — so the caller can decide at once
without waiting for a tick.

The rule "nobody stands in an impassable cell" is checked when a creature is
placed as well: `World::add` refuses a cell outside the grid (`OutsideGrid`), a
taken one (`CellTaken`) and an impassable one (`ImpassableCell`) — a creature
inside a wall would break the invariant the world checks on every tick.

`World::remove` puts its own event into no channel: the outer world ordered the
creature out and knows why — the engine only releases cells and stops walking.
A search event of the creature that was still waiting in the channel when the
removal ran is dropped with it, so the channel names no creature the world has
let go; see "Decision: taking a creature out keeps the numbers".

### Decision: who yields a cell

Starting a step, a creature reserves its destination cell and holds the
reservation until the step ends (`Holder { standing: false }`). From that:

- a head-on exchange is impossible: a foreign cell is held by a creature or by a
  reservation, so the step does not start;
- two creatures claiming one cell on one tick do not collide: a reservation is
  exclusive and creatures are visited in number order, so the lower number gets
  through first — and that is deterministic;
- the cell a creature stepped out of is freed on the same tick, so whoever
  follows enters it without waiting an extra tick;
- if the destination cell became impassable while the creature was walking, the
  step is rolled back (`Event::TerrainBlocked`), the creature stays on its own
  cell and waits for a repath — it never ends up in an impassable cell.

No separate "yielding rule" was invented: in Slavik a creature simply does not
enter an occupied cell (`game.cpp:2889-2895`), and which of two creatures gives
way in the game has not been worked out. There is no separate deadlock guard
either: a cell frees itself after a few ticks, and if it does not, repathing kicks
in. What happens to a creature with no detour — see "Open questions".

### Decision: repathing without thrash

A route search is the engine's most expensive operation, so it is called from one
place only and under two conditions: the creature is standing with a route it
cannot start (the cell ahead is taken or has become impassable) and has stood for
`wait_before_repath_ticks` (12 ticks ≈ 0.72 s by default), and at least
`repath_cooldown_ticks` (60 ticks ≈ 3.6 s) have passed since its last search. The
whole route is not rechecked every tick: a creature learns about a blockage only
when it is about to take a step. A brand-new order searches at once, and a repeat
of an order whose search just failed is held to the same window — see "Decision: a
failed goal is retried on a cooldown".

Both numbers are `WorldRules`, not constants: there are no confirmed values from
the game, and part 3 can set its own. That a creature does not get stuck is held
by a test: after a blockage the route is recomputed, and once the obstacle is
removed the creature reaches its goal.

A repath inserts no empty tick: a new route is spent by its first step on the very
tick it was found
(`a_deferred_order_starts_its_first_step_on_the_border_tick`). Before this, an
order arriving while the creature was walking was executed one tick late — 60 ms
of idling on every change of goal, while there is no pause between the steps of
one route.

### Decision: a repeated order is idempotent

A rules tick loop re-sends a creature's current goal every tick. `World::move_to`
remembers the last goal it accepted (`Actor::last_commanded`) and treats a repeat
of that goal as a no-op: it returns the steps the current route still has and
neither resets the waiting counter nor runs a fresh search. Without this the loop
would clear `waiting_ticks` and call A\* every tick — the opposite of "repathing
without thrash", and a route would never be allowed to complete.

The memory is kept across arrival, so an order re-sent to a creature already at
rest on that goal is still a no-op; `World::stop` clears it, and a different goal
overwrites it. A goal that had no route is not remembered, because re-sending it
is the only way to retry once a crowd clears.

### Decision: a failed goal is retried on a cooldown

The paragraph above leaves a hole: a goal with no route is deliberately *not*
remembered, so a rules loop that re-sends an unreachable order every tick gets a
fresh search every tick. That is the engine's most expensive operation — the
search over a whole unreachable area costs 8.9–9.6 ms, a sixth of a 60 ms tick for
one creature (#54) — so `Actor::failed_goal` remembers the goal whose last search
found no route, and a repeat of exactly that goal inside
`repath_cooldown_ticks` of that search is refused without a search and without a
new `Event::NoRoute` (`repeated_unreachable_order_is_throttled_to_the_cooldown`,
`a_retry_inside_the_window_neither_searches_nor_reports_a_refusal`). The refusal is
told to the caller by the return value of `move_to`; the event channel carries only
the search that actually ran.

It cannot be the same mechanism as `last_commanded`, and this is what #52 turned
up: the idempotency memory has to be *cleared* when a search fails, otherwise the
order sticks and a goal the crowd has freed is never retried
(`deferred_no_route_can_be_retried_after_the_path_clears`). So the two memories
pull in opposite directions — one keeps an order it must not re-search, the other
keeps an order it must search again — and the throttle window is what separates
them: the retry is late, never absent
(`a_repeated_unreachable_order_gets_a_route_once_the_window_passes`). Late is the
best the engine can promise: `Passage` is a query, not a notifier, so nothing tells
the world that walkability changed and no search can be woken the moment a path
clears. Notifying the world of a terrain change is its own design, taken up when a
consumer exists (#54). `World::stop` clears the failed goal too, so
a withdrawn order followed by the same order is a real search
(`stop_clears_a_failed_goal_so_the_next_order_searches`), and a different goal is
never held up by a failed one
(`a_failed_goal_does_not_hold_up_an_order_to_another_one`).

What it saves is arithmetic over the measured search cost above: a creature whose
goal is cut off by a wall column costs 4 searches over 240 ticks instead of 241
(the test asserts the number), that is ≈37 ms against ≈2.2 s of one creature's
worst operation over 14 s of game time.

This is a modern-experience guard, not a recovered rule: how often the original
retries an unreachable order has not been worked out, and the window is the
engine's own `WorldRules` number, not a confirmed one. Its bound is one remembered
goal: a loop that switches between two unreachable orders searches on every switch,
inside the window (`alternating_unreachable_orders_each_search_inside_the_window`).

That is left as it is on purpose — widening the memory to a set costs state for a
caller that does not exist yet, and a rules layer that oscillates between two goals
every tick is a bug in the rules, not something the engine should paper over
(#36 part 3 owns the cadence).

That caller arrived with criteria 3 and 4 of #36: the rules re-roll a target every
tick, so a creature facing an unreachable area paid one search per tick. The bound
is a per-creature window one layer up — `kn_game::rules::FailedOrderCooldown` holds
a movement order back for `window_ticks` after one of its found no route and answers
`Applied::Held` without calling the engine
(`an_unreachable_area_costs_one_search_per_window`: 4 searches over 40 ticks instead
of 40, and `a_cooldown_bounds_the_searches_of_an_isolated_creature`). It lives
beside the rules rather than in `World`, so a player's order is never delayed by a
failed AI one, and a turn — which costs no search — is never held. The two memories
stay one each: the engine remembers the goal, the application remembers the tick.

### Decision: an occupied goal takes the nearest ring

If the goal is taken or impassable, the search walks the rings of cells around it
from the centre outward (`goal_fallback_radius`, 4 by default) and stops at the
first ring it can route to: the creature goes to the closest free cell, not to the
one cheapest to reach. The rings work **only for a goal that cannot be entered**:
if the goal is free and walkable but unreachable, there is no answer — `find_route`
returns `None` and the creature gets `Event::NoRoute`. Substituting a neighbouring
cell for such a goal is not allowed, because the caller would not learn that the
creature stopped somewhere other than where it was sent: it would see an arrival
(`Event::Arrived`) where the order was not carried out
(`a_free_but_unreachable_goal_does_not_take_a_ring_cell`). Without the ring sweep
A\* would take the cheapest goal and stand on the cell nearest to itself rather
than by the goal. Rings are enumerated centre outward, each with its own search:
in the usual case — a free goal — there is exactly one. The radius bounds the
worst case: a taken goal in a dead end will not force a sweep of the whole map,
and a goal with no free cell even within the radius gives no route rather than a
panic. A creature already standing in a free cell beside a taken goal gets an
empty route: it has nowhere to go, and the order counts as done.

### Decision: taking a creature out keeps the numbers

`World::remove` takes a creature out: every cell it stood on or reserved with a
step is released, its movement state is dropped, and an order to it — or a
second removal — answers `UnknownCreature`. Two questions the ticket left open
got an answer each:

- **a number is never reused**. A slot keeps its creature's number for the
  world's life: a new creature takes the next number, not the freed one. The
  outer world keeps ids past the engine — #75 lets a corpse remember the
  creature that lay in it — and a recycled number would make one id name two
  creatures in one save; a stale handle to a removed creature answers
  `UnknownCreature` instead of ordering a stranger. The cost is a number that
  only grows: a `u32` runs out after four billion placements in one world, and
  a world lives as long as one map does;
- **`remove` stops the creature itself**: a step that had started is ended
  where it stands and its reservation is released with the standing cell, so a
  following creature enters the freed cell on the next order. Requiring a
  `stop` first would make death forget a reservation, and a forgotten
  reservation is a cell nobody can enter again.

Removal emits no `Event` — the outer world is the one that knows a creature
died — and no event names a creature the world has let go either: a search
event still waiting in the channel when the removal runs is dropped with the
creature (`removal_drops_a_pending_search_event_of_the_removed_creature`),
while the events of the others keep their place. The creatures that stay are
visited in the same number order as before, so the same seed and the same
removal order replay the same trace
(`removals_replay_identically_under_the_same_seed`). No despawn rule of the
original is confirmed here: the engine's removal is our API.

The slot of a removed creature stays in the world — the number is not reused —
but the tick does not walk it: taken slots are held in a list ordered by
number, and the tick walks that list, so the order is the walking-the-slots
order. Release build, Apple M4 Pro, `removal_cost.rs` under `#[ignore]`, the
spread over three runs: 100 standing creatures behind 2 000 empty slots cost
0.24–0.28 µs a tick and 0.35–0.37 µs a `states()` call; the same world walking
every slot costs 2.6–2.9 µs and 1.6–2.0 µs, and the pair `add` + `remove`
measures 0.05–0.06 µs on both sides — the bookkeeping is invisible there. A
tick arrives every 60 ms, placements and removals are rare: the difference is
bought in the right direction (the churn itself is the #49 debug-player case).

### Decision: the seed breaks ties

The order directions are tried in is rotated by a value from `Rng`, so a choice
between routes of equal cost depends on the seed and repeats under the same seed.
The seed does not affect the data: where the choice is not a tie, the route is the
same under any seed — determinism would otherwise be out of reach. The generator
sets no game rules: wherever the choice is in the data, the data is what arrives.

### Decision: what the engine does not know

The engine works in whole ticks; how many milliseconds a tick holds is the
supplier's knowledge (`kn_game::TICK_MS`, 60 ms for K1). Step duration and
walkability also arrive from outside, and a zero duration is counted as one tick:
a zero-length step does not exist, and data must not panic the engine.

What leaves the engine is `WalkState` (the cell, its pixel point, the cell it was
entered from, the goal, the direction, whether a step is running and how many
ticks are left in it) and `Event` (step start and end, arrival, waiting, blockage,
repath, refusal). The direction index matches the original's table, so a
visualization picks the walk animation by it; how to draw a sprite is not the
engine's business.

### Open questions

- **A crowd with no detour.** If a repath around a taken cell gives no route, the
  creature drops the order and stops (`Event::NoRoute`): the search does not repeat
  forever, but neither does it resume by itself — whoever sent the order has to
  send it again, and a repeat is throttled to one search per
  `repath_cooldown_ticks` ("Decision: a failed goal is retried on a cooldown").
  Whether to hold the goal and wait for the crowd to clear is a question for
  part 3: it is about behavior, not mechanics.
- **A blockage mid-route.** A creature notices a closed cell only once it reaches
  it, so a tree fallen in the middle of a route is worked around a few cells late.
  Revisiting every route whenever walkability changes costs more and produces more
  thrash.
- **Rewinding.** The engine runs one way: `Event` is enough to rebuild state by
  stepping forward through the ticks, and whether rewinding backwards is needed
  (epic #33 part 5) is decided there. A viewer `Step` back is deliberately *not*
  an engine reverse — it rebuilds the map's `Simulation` and replays it forward to
  the rewound clock tick, so the display shows that tick without the core gaining a
  reverse tick it was never designed for (review #68, `replay_simulation`).
- **Differences in K2.** Running changes only the step duration, and that comes
  from data; no other movement differences in K2 have been looked into yet.

### Engine cost

Release build, Apple M4 Pro, a fine grid of 160×320; measured by the
`kn-game/tests/movement_cost.rs` test under `#[ignore]` (an ordinary `cargo test`
does not run it):

```bash
cargo test --release --manifest-path crates/Cargo.toml -p kn-game \
    --test movement_cost -- --ignored --nocapture
```

| Operation | Time |
|---|---|
| a world tick with 50 creatures walking | 1.7–1.8 µs (0.034–0.037 µs per creature) |
| a world tick with 500 creatures walking | 14.2–15.1 µs (0.028–0.030 µs per creature) |
| a tick with a blocked cell (amortized repath) | 1.9–2.2 µs |
| a route across the whole map over open ground | 0.21–0.37 ms |
| a route among walls | 2.0–2.2 ms |
| a search next to 400 occupied cells | 2.8–3.0 ms |
| a route to an unreachable goal (the whole reachable area) | 8.9–9.6 ms |
| cell occupancy (a lookup) | 0.017–0.018 µs |
| the A* heuristic (`isqrt`) | 0.005 µs |
| a neighbour cell / cell to pixels | 0.002 / 0.001 µs |

The spread over three runs in a row is given: under a percent for the short
operations, nearly twofold for a route across open ground, so for that one the
order of magnitude matters more than the number.

Neighbouring operations for comparison (see the cost of switching layers below):
switching a group of 5 131 sprites — 0.15 ms, parsing a render list — 26 ms. A
tick of 50 creatures is about 90 times cheaper than an overlay switch and about
15 000 times cheaper than parsing a map; a typical route (0.2–0.4 ms) is 1–3
overlay switches and 1/65…1/125 of a map parse; the worst search is a third of a
map parse and 15% of a 60 ms tick.

The upshot: an engine tick costs microseconds and the entire cost is in route
search, which is why `World` calls it no more often than `repath_cooldown_ticks`.
The search for an unreachable goal stays the most expensive operation: a rules loop
re-sending such an order now costs one search per creature per window, not one per
tick (#54, "Decision: a failed goal is retried on a cooldown").

### Pose cost

Release build, Apple M4 Pro, `konung.1` of both editions (25 creatures), the spread
of three runs, measured by the `k1-game/tests/pose_cost.rs` test under `#[ignore]`:

```bash
cargo test --release --manifest-path crates/Cargo.toml -p k1-game \
    --test pose_cost -- --ignored --nocapture
```

| Operation | Time |
|---|---|
| parsing `GRAPH.RES` and `Objects.res` | 16–25 ms |
| one pose: the layers and frames of one state and direction | 57–61 µs |
| rebuilding a pose when state or direction changes | 66–72 µs |
| every creature of the map in its resting pose (25 of them) | 0.95–1.06 ms |

**Decision (issue #36, whether lazily built poses are affordable):** a pose is built
when it is needed and kept until the creature changes its state or direction. One
assembly is a hundredth of the 60 ms tick, and a whole map of creatures costs 38–42 µs
per creature, so preloading every state and every direction for a map would multiply
the work by dozens to show what nobody asked for. The part worth caching is the
parsed resources, not the poses: one parse is worth 250–450 poses, which is why
`Resources::with_graphics` hands the tables to a closure instead of returning them,
and `k1_game::scene::Graphics` holds them while poses are built.

## Creature rules (`k1-game::rules` → `kn-game::rules`)

Epic #36 part 3, criteria 3 and 4 on top of the skeleton. K1 behavior splits
from the shared world in two crates: evaluation lives in `k1-game::rules`
(it reads the roster plus a live picture of the tick and declares its own
`Ground`, `Random` and `Live` abstractions, so it needs no dependency on
`kn-game` — the dependency runs the other way), application lives in
`kn-game::rules` (it takes explicit `(CreatureId, Decision)` pairs, calls
`World::move_to` and `World::turn`, and hands the world itself to the rules as
their `Ground`). The confirmed rules themselves are in
[creatures.md](k1/creatures.md), «The villager decision», «The group state of
a roaming squad» and «The monster decision».

### Decision: separate evaluate from apply

Two phases per tick — evaluate the whole roster first, then apply — so that
decisions do not depend on the order creatures are visited in. If a single pass
mutated the world as it went, the second creature's decision would see the first
already moved and the roster's iteration order would leak into behavior. A
contested cell is settled by the world itself (see "Decision: who yields a cell"),
not by whoever happened to evaluate earlier.

### Decision: classification is by group kind, not class byte

`0x53` in the ticket is a *group kind* (see `Group.kind` in
[game-state.md](k1/formats/game-state.md)), not a character class; villager groups
are `0x80`–`0x82`. `Creature` carries both `class_id` (byte 0x04, describes the
character: sprite source, dead bit, work bits) and `group_kind` (byte 0x1E of the
group, describes the band), and `classify()` reads only the second. A warrior
class_id inside a villager group still classifies as `Villager`.

### Decision: the rules read live positions, not the file

`Game.N` is the starting state; the rules decide over where creatures stand
now, so `evaluate` and `group_states` take a parallel `Live` picture (cell,
facing, whether the creature is still walking) and the group state byte is
recomputed every tick from those positions — never read from the file, which
for roaming squads the game itself overwrites within the first 16 ticks
(`game.cpp:645`, `game_play.cpp:1623-1638`; the period is a 1999 cost cap we
do not reproduce). A creature whose squad state the caller did not supply, or
whose live picture is missing, gets the calm answer and no decision rather
than a panic: the arrays are indexed like the roster and may lag it. A creature
the world does not know is `None` in that picture, never a made-up cell — an
invented `(0, 0)` would decide the state of every squad whose spawn area covers
the origin (`an_unknown_player_member_never_makes_a_squad_hostile`).

The villager walk picks a cell uniformly inside the squad's spawn area; the
original offsets from the creature by the viewport size (`ViewTiles * 2`,
`game_play.cpp:1889-1949`), and behavior must not depend on the window. The
monster return keeps the original's exact draw order (coin, then `y`, then
`x`), so a run with a fixed seed replays. Villager squads are always calm
until the village rules (`FUN_00416700`) are recovered — our villagers never
turn hostile; monster combat (state 1) gives no order rather than an invented
one. Both are recorded as open questions in
[creatures.md](k1/creatures.md), not as confirmed behavior.

The player squad the state scan looks for is part of the roster the caller
passes, and `k1_game::creatures::roster` lists it only on the map the game
starts on: a simulation of any other map adds the arriving player squad itself
before asking for group states (#38, #49). The rules' eight-direction offset
table is a second copy of the engine's, because `k1-game` cannot import
`kn-game`; `kn-game/tests/movement_grid.rs` holds the two together.

### Decision: turning in place

`Decision::Turn(u8)` turns a standing creature to face the original's
direction byte 0..7; `Applied::Turned` reports acceptance. The engine gains
the smallest thing criterion 3 needs: `Walker` remembers an explicit face,
and `facing()` answers in order — the running step, the route's first
direction, the face, the direction of arrival. A step consumes the face, so
after a walk the creature faces where it came from exactly as it does without
a turn, and a creature sent to a workplace is turned again once it stands —
which is also the original's order of events. `World::state` exposes the
facing, so turning emits no event. Turning is immediate,
not one direction per tick (the original creeps by one step at a time,
`FUN_004391bc`, `game_play.cpp:1995-2013` — a 1999 limit,
see «Вне объёма» in [creatures.md](k1/creatures.md)).

### Decision: load-time placement mirrors the original, occupancy does not

Criterion 2: a roaming squad does not stand where its records put it — the
game re-places every member on map load. `k1_game::placement::place_roaming`
follows `PlaceMob` cell for cell: jitter from the area's centre with the
original's draw order (a distance then a coin on `y`, then on `x`, a distance
drawn only when its half-span reaches two), one hundred tries, and the squad
trimmed at the first member that finds no free cell (`GroupSize = j`). The
facts and the three deliberate differences are in
[creatures.md](k1/creatures.md), «Roaming squad placement on map load». The
one worth restating here: the engine and the original differ on occupancy, and
placement follows neither blindly. It holds the file cells of the creatures it
does not move and every cell it has handed out, so a placement answer depends
only on the roster and the ground — not on the order groups happen to be
visited in, which is what makes it reproducible under a fixed seed and what
`roaming_squads_are_re_placed_on_free_cells_inside_their_areas` checks over
all 379 shipped squads.

### Decision: rules and the engine stay in separate crates, wired at the seam

`k1_game::rules` declares its own `Ground`, `Random` and `Live` and depends on
no engine type; `kn_game::rules` owns the `World` and applies decisions to it.
Neither reaches into the other, so a caller that only wants starting cells
(criterion 2) calls `placement::place_roaming` over the rules' `Ground` with
the engine's `Rng`, without a `World` at all. `k1-game` still cannot import
`kn-game`, which is why `place_one` re-derives the centre and spans from the
spawn rect rather than borrowing a grid helper.

### Rules tick cost

Release build, Apple M4 Pro, both editions agree to within a microsecond; the
spread of three runs. Measured by `kn-game/tests/rules_cost.rs` under
`#[ignore]`:

```bash
cargo test --release --manifest-path crates/Cargo.toml -p kn-game \
    --test rules_cost -- --ignored --nocapture
```

| Operation | Time |
|---|---|
| decide only, calm map 17 (29 monsters, none walking) | 1.2–1.6 µs/tick |
| decide only, map 19 (20 creatures, 14 walking) | 0.49–0.56 µs/tick |
| full tick — decide + apply + `tick_step` — calm map 17 | 1.28–1.85 µs/tick |
| full tick, map 19 (14 walking, routes live) | 18–22 µs/tick |
| load of one map: foot-data load + placement (`Game.0`, averaged over all maps — 987 creatures in all) | 0.27–0.49 ms |

The decide pass over dozens of creatures is 0.5–1.6 µs — three to four orders
of magnitude under the 60 ms tick — so the rules are not the thing to cache.
The full tick's 18–22 µs on the walking map is route search, not decisions:
A\* across the fine grid dwarfs everything else, exactly as the movement table
above shows (a route across the whole map is tens of microseconds), and it is
paid by the walking creatures, not by whoever decides. Load placement — the
foot-data load and the placement together — is a quarter of a millisecond per
map and runs once when a map opens, so it is
noise next to the 16–25 ms of parsing the graphics; no placement cache is
worth its bookkeeping. The loop itself runs over the real maps in
`kn-game/tests/original_simulation.rs` (criterion 9): map 19 with fourteen
creatures mid-step in the same tick for 400 ticks, map 17 with its calm
squads not taking one step.

### Game time cost

Release build, AMD Ryzen 9 9950X, the spread of three runs. Measured by
`k1-game/tests/time_of_day_cost.rs` under `#[ignore]` (issue #95) — no
`original/` needed, the model is arithmetic over the tables of
[k1/time-of-day.md](k1/time-of-day.md):

```bash
cargo test --release --manifest-path crates/Cargo.toml -p k1-game \
    --test time_of_day_cost -- --ignored --nocapture
```

| Operation | Time |
|---|---|
| empty loop (the floor of the harness) | 0.19–0.22 ns/tick |
| `clock_face` — three divisions by the cycle constants | 0.83–0.95 ns |
| `ground_window` — the window and a range check | 1.83–2.17 ns |
| `ambient_value` — the window and the original's branch | 3.8–4.4 ns |
| `ambient_kind` — the triple and its sign | 4.3–5.1 ns |
| `day_phase` — the ambient, then the ground window | 5.7–6.9 ns |
| `game_time` — the whole readout the panel shows | 12.9–14.8 ns |

The day model is at most fifteen nanoseconds a tick: two orders under the
cheapest full tick above (1.28 µs on a calm map) and seven under the 60 ms
budget, so nothing in it is worth caching. The most expensive parts are the
twelve-threshold scan and the branch over the ramp formulas that computes the
ambient triple with the original's integer halving; a binary search or a
precomputed per-tick table would save a few nanoseconds and cost a second table
to keep in step with the windows documented in
[k1/time-of-day.md](k1/time-of-day.md). It is also paid by the asker, not by
the simulation: game time is derived from `Simulation::tick` on demand
(issue #95) and the simulation carries no accumulator of its own.

### Lighting cost

The rendered lighting (#96) has two halves measured separately: the pure
model in `k1_game::lighting` on the CPU, and the material pipeline in the
viewer's window.

**CPU model.** Release build, AMD Ryzen 9 9950X, the spread of three runs.
Measured by `k1-game/tests/lighting_cost.rs` under `#[ignore]` — no
`original/` needed, the model is arithmetic over the tables of
[k1/time-of-day.md](k1/time-of-day.md):

```bash
cargo test --release --manifest-path crates/Cargo.toml -p k1-game \
    --test lighting_cost -- --ignored --nocapture
```

| Operation | Time |
|---|---|
| `params` — the full `LightingParams` of a tick, the render layer's per-frame entry point | 8.46 ns |
| `apply_channel` — one channel of the per-channel blend, the shader-equivalent reference | 1.61–1.63 ns |
| `night_palette` — the per-day coin, `(day, seed)` → variant | 1.34–1.35 ns |
| `light_color` — the tile-light colour, re-rolled every 16 ticks | 1.64–1.65 ns |
| full pipeline — `params` + the three-channel apply | 12.53–12.64 ns |

A confirmation run of this session reproduced the table within a tenth of a
nanosecond on both ends (`params` 8.42–8.45, full pipeline 12.52–12.55).
The verdict follows the game-time table above: the whole pipeline costs about
one `game_time` readout (12.9–14.8 ns) — two orders under the cheapest engine
tick (1.28 µs on a calm map) and seven under the 60 ms budget. And this is a
per-frame cost paid by the asker, not a per-pixel one: `apply_channel` runs
on the GPU; the CPU number exists as the reference the WESL fragment is
checked against. Nothing in the model is worth caching.

**Render.** The window meter, not the CPU one. Methodology: release build,
the viewer's `--benchmark 600` mode — 40 settle frames, then 600 measured
frames of map 17 of the rus edition (12 487 sprites, characters overlay on);
the samples are the `DiagnosticsStore` frame time that
`FrameTimeDiagnosticsPlugin` writes each update, the percentiles are
nearest-rank over the 600 samples, and each row is three runs in a row. The
benchmark window opens with `PresentMode::AutoNoVsync` (`--benchmark` runs
only; interactive runs keep the default Fifo), and that choice is the
methodology, not a tweak: with vsync on, a finished frame sits on the
monitor's quantum of 16.67 ms — day and night would both read as the
display's pace and the meter would measure the screen, not the scene.
Hardware: AMD Ryzen 9 9950X, NVIDIA GeForce RTX 2070 SUPER, wgpu backend
Vulkan (the adapter log line of the runs). A draw-call counter is not
reachable from `bevy_render` 0.20 without extra features, so the table counts
sprites (`ScenePiece` entities of the scene) and the materials of the
lighting pool instead.

| Version | Light | avg ms | p95 ms | p99 ms | sprites | light materials |
|---|---|---|---|---|---|---|
| `main` a95db9f — the local `main` ref of this clone, full default Bevy features | none | 1.49–1.52 | 1.86–1.94 | 2.06–2.10 | 12 487 | 0 |
| origin/main ba22cda — the commit this branch sits on (#105 build profiles) | none | 1.28–1.29 | 1.64–1.68 | 1.86–1.93 | 12 487 | 0 |
| spike 3838657 — material on terrain only, CLI-pinned lighting | day, `--light-tick 0` | 3.00–3.01 | 3.46–3.47 | 3.71–3.77 | 12 487 | 20 |
| spike 3838657 | night, `--light-tick 11000` | 2.96–3.02 | 3.38–3.43 | 3.64–3.67 | 12 487 | 20 |
| this branch 16e4371 | day — clock at tick 0 | 3.30–3.33 | 3.82–3.86 | 4.03–4.20 | 12 487 | 21 |
| this branch 16e4371 | night, `--light-tick 11000` | 3.27–3.30 | 3.76–3.80 | 3.97–4.06 | 12 487 | 21 |

The baseline rows carry a caveat the comparison needs: the local `main` ref
(a95db9f) is one build-infra commit behind `origin/main` (ba22cda, #105,
which trimmed the Bevy feature set and toolchain), and that commit alone is
worth ~0.2 ms of average frame time here (1.49 → 1.28). The lighting work
sits on ba22cda, so the honest lighting deltas read against it:

- **This branch's day against ba22cda — the full cost of the materials on
  every surface:** +2.0 ms of average frame time (1.28 → 3.30–3.33), p95
  1.64 → 3.82–3.86. Where the base bound no materials, this branch hands
  every terrain, decoration, object and dark-cell creature sprite a light
  material (21 in the pool, 19 LIGHTS.RES masks plus the two plain ones).
- **Night against day — the cost of the formula:** none. The night triple
  changes the uniforms, not the work: 3.27–3.30 against 3.30–3.33 ms of
  average, inside the runs' spread. The per-frame CPU model is nanoseconds
  (table above), and the day's zero blend takes the shader's no-op branch,
  so both phases draw the same material graph.
- **The spike against this branch — extending the material past the
  terrain:** +0.3 ms of average (3.00 → 3.30–3.33 day, p95 3.46 → 3.82–3.86).
  The terrain-only material cost 1.7 ms over the base (1.28 → 3.00); covering
  decorations, objects and the per-creature material swaps adds another
  third of a millisecond and the one extra plain material (20 → 21).

Not measured, and the standing candidate for a profiler pass:
`update_creature_lighting` (`kn-render::lighting`) rebuilds its per-creature
dark map from `Simulation::states()` every frame — a fresh `Vec` of states
per frame, the same pattern the creature display subsystem already pays (the
viewer's label systems read `states()` per frame too). At dozens of
creatures the allocation is plausibly noise next to the 2 ms the materials
add, but that is an eyeball, not a measurement; it goes through the profiler
before anyone optimizes it. (The per-frame map allocation itself is gone
since: `update_creature_lighting` reuses a `Local<HashMap>`.)

**Scrub catch-up cost.** Releasing the time slider walks the simulation to
the clock in one frame — forward by plain ticks, backward by the deterministic
seed replay. Measured by `benchmark_scrub_catchup_cost` in `kn-fe-viewer`
(`#[ignore]`, release, needs `original/`) on the busiest simulation of the
rus edition — map 36 with 65 creatures — as the whole release frame
(`app.update()` around the settle), three fresh runs per jump:

| Jump | avg settle frame |
|---|---|
| 0 → 18960 (forward, 18 960 ticks) | 54–76 ms (worst of six runs 75.8) |
| 18960 → 900 (backward, seed replay of 900) | 44–48 ms |

Both sit below a tenth of a second, so a release is one dropped frame at
worst, and the backward replay branch is consistently the cheaper one; no
optimization is warranted until the frame-time gate (#98) says otherwise.

### Viewer costs

The per-frame numbers of the viewer window itself — the simulation plus the
display on the creature maps, at least the roaming-group map 17 (28 creatures,
none of which walks in 600 ticks) and the village map 19 (20 creatures, 14
walking), both editions (issue #83).

**Method.** Release build, Windows 11, AMD Ryzen 9 9950X, 64 GiB RAM, NVIDIA
GeForce RTX 2070 SUPER (the wgpu adapter of every run; the CPU's iGPU sits
idle), window 1280×800. The window benchmark is
`--benchmark 8000`: 40 settle frames, then 8000 measured frames — at the
village's ~2.4 ms frame that is 300–350 simulation ticks, several step cycles
per walker. The frame is bounded the same way the render table above bounds
it: `PresentMode::AutoNoVsync` (benchmark runs only), so the meter reads the
scene and not the display's 16.67 ms quantum; the samples are the
`DiagnosticsStore` frame time that `FrameTimeDiagnosticsPlugin` writes each
update — the same wall-clock delta the diagnostics panel shows — and the
percentiles are nearest-rank over the 8000 samples. Every `BENCH` line now
carries `max_ms` (the worst measured frame), `ticks` (sim ticks advanced
across the measured window, snapshotted when the window opens so settle time
does not leak in), `creatures`, `walking` and `labels` — a recorded line
self-describes the run, and the identity `ticks · 60 ms ≈ frames · avg_ms`
marks the honest ones. Runs while the desktop was loaded are discarded, not
averaged in: they announce themselves by `p95` shifting to 16–17 ms in
*every* configuration at once — pause included, which a paused scene cannot
cause — and they still satisfy the identity, so they measure the machine,
not the scene. The tables keep the clean rounds; both editions' clean rounds
agree within 0.1–0.3 ms, which is the reproducibility check.

**Instrumentation (AC choice, stated here):** the numbers reproduce from
instrumentation left in the code behind two environment variables — no
build flag, and no behavior change unless a variable is set.
`KN_VIEWER_LABELS=0` spawns no creature id labels (default: spawn, as
shipped), `KN_VIEWER_PAUSE=1` starts the playback paused (default: playing),
which holds the simulation and the walk clips out and leaves the settled
scene — the display floor the playing runs compare against. `knob_value`
forces the factor off on `0`, keeps the shipped default when the variable
is absent and forces it on for any other value, pinned by
`the_benchmark_knobs_default_to_the_shipped_behavior`. The two isolated
measurements below are `#[ignore]`d tests run by the recorded recipe; the
data stays in `original/`, reached through `GAME_DIR`.

```bash
cargo run --release --manifest-path crates/Cargo.toml -p kn-viewer -- \
    --game-dir original/k1/rus/game --map 17 --benchmark 8000
GAME_DIR=original/k1/rus/game cargo test --release --manifest-path \
    crates/Cargo.toml -p kn-fe-viewer --lib tests::benchmark_pose_swap_cost \
    -- --ignored --exact --nocapture
GAME_DIR=original/k1/rus/game cargo test --release --manifest-path \
    crates/Cargo.toml -p kn-fe-viewer --lib tests::benchmark_depth_slot_cost \
    -- --ignored --exact --nocapture
```

**The visualization tick with the creatures moving.** Playing runs (default
knobs), 8000 frames each, the spread of the clean rounds:

| Map (edition) | playing | paused (display floor) | ticks | p95 / p99 / max, playing |
|---|---|---|---|---|
| 17 RUS — 28 creatures, 0 walking | 3.19–3.57 ms | 3.24–3.78 ms | 425–476 | 3.70–4.28 / 4.28–5.12 / 10.6–12.8 |
| 17 ENG — 28 creatures, 0 walking | 3.15–3.20 ms | 3.06–3.15 ms | 420–426 | 3.56–3.66 / 3.95–4.12 / 10.4 |
| 19 RUS — 20 creatures, 14 walking | 2.34–2.62 ms | 2.24–2.61 ms | 311–349 | 2.73–3.23 / 3.12–3.84 / 9.3–13.4 |
| 19 ENG — 20 creatures, 14 walking | 2.33 ms | 2.23 ms | 310 | 2.75 / 3.13 / 9.1 |

The comparison the ticket asks for: **playing − paused, same round, same
map.** On the village it is +0.09/+0.10/+0.10 ms (RUS rounds 2–3, ENG round
1) — a reproducible ≈ +0.1 ms for the living scene; the calm map's rounds
span −0.59…+0.33 ms (RUS) but sit at +0.05…+0.10 ms in all three ENG rounds,
at most a few percent of the frame. Of the
village's +0.1 ms the *simulation logic* is measured to own ~1.5 µs per
frame amortized: a tick lands every ~24th frame and carries the full engine
tick of 18–22 µs (table "Rules tick cost" above), one walk-swap batch of
~10 µs (the pose change below) and the depth recompute of ≲0.5 µs — three
orders below the frame, which is why pausing is invisible in the percentiles
and the reproducible part of the difference tracks the sprites' churn
instead (the ≤85 animated pieces of the village, 37 of the calm map, cycling
their frames every tick, the walkers among them). The headless decide pass
(map 17, 1.2–1.6 µs/tick) is invisible in the window, as the calm map's
drift confirms.

**One pose change, isolated.** `benchmark_pose_swap_cost` runs the shared
creature subsystem headless (the window app minus the window, `MinimalPlugins`)
on map 19, one simulation tick per update, and times 400 updates against the
creatures whose sprite sequences swapped on each: a steady tick frame (the
simulation advances, no creature changes pose) is the baseline, and frames
where exactly one creature changes pose isolate the change. Over three runs
per edition the single-pose-change frames sit above the steady median by
+10.2–13.7 µs (RUS) and +6.9–12.2 µs (ENG) — a creature's pose change
re-installs 3.7–3.8 sprite clips (body layers plus shadow), so ~2–4 µs per
clip install and sprite rewrite. Neighbors: one full engine tick is 18–22 µs
on the same map; one data-layer pose *build* is 57–72 µs (table "Pose cost")
but happens once per state or direction change, while this swap is the
per-change display cost on top of clips already built; one depth call is
~8–19 ns (below). At ~1.1 step starts per tick across the village's walkers,
pose changes amortize to ~0.2 µs per frame — the AC6 bound (one rebuild per
state change, none per frame) is what keeps it there.

**The depth recompute while creatures move.** `benchmark_depth_slot_cost`
times `DepthMap::creature_z` — the binary search over the static stacks, the
interpolation and the per-id slot. That lookup is what `sync_creature_nodes`
pays per creature per frame for the shadow's slot, and as `creature_z` for
the body z whenever a creature's bottom moves; the test runs it over the
real depth maps with the bottoms the walking creatures actually carry (live
row plus body height). Spread over
six runs × both editions × both maps: **8–19 ns per call**, so a frame's
depth recompute for the whole population is **0.2–0.5 µs** (28 creatures on
map 17, 20 on map 19). Neighbors: a thousandth of one walk-swap batch
(~10 µs) and ~1/5000–1/12 000 of the village's frame — no throttling or
caching of depth recomputes is warranted at these populations.

**The creature labels, with and without.** The `KN_VIEWER_LABELS=0` runs sit
next to the playing runs of the same round: the difference is below the
round drift on most rounds and never exceeds +0.16 ms (village RUS, 20
labels) or +0.15 ms (roaming RUS, 28 labels); the clean pairs give
+0.01–0.16 ms, i.e. ≲5 µs per label per frame — text nodes already spawned,
per-frame repositioning plus their share of the render. Neighbor: a label
costs about half a pose change per frame, and 28 labels cost under 1% of the
60 ms budget — "labels only for visible creatures" has no numbers behind it
on these maps: all creatures are visible by default, and their labels are
measured cheap.

**The worst observed frame and what caused it.** `max_ms` of the quiet runs:
the village's worst frame is ~9.1–9.7 ms when the scene plays and
3.5–3.7 ms when paused — reproduced in three independent clean rounds (RUS
rounds 2–3, ENG round 1) — while the calm map's runs spike to ~10 ms in most
configurations, its paused rounds included (9.5–10.6 ms RUS). Attribution:
the village's play-only spike cannot be the scene's CPU work (the tick
frame's measured share is ≲35 µs, above), and it survives
`KN_VIEWER_LABELS=0` (9.17 ms), so by elimination it is the display-side
churn of a tick frame — the ≤85 animated pieces plus the walkers swapping
images at once — with the exact mechanism (extraction, re-batching, driver)
unconfirmed: bevy_render 0.20 exposes no draw-call counter without extra
features, the same caveat the render table above records. The calm map's
configuration-independent spikes and every burst where `p95` shifts to
16–17 ms belong to the desktop, not the scene. Even the worst attributed
frame is 6× under the 60 ms tick, so the standing follow-up is a profiler
pass over the tick-frame churn before anyone optimizes — out of this
ticket's scope, which measures and does not fix.

### Open questions

- The `class` module in `k1-game::rules` is load-bearing since criteria 3 and 4:
  `class::WOMAN` and `class::CHILD` gate the villager branch, and the work bits
  `(class >> 3) & 7` pick the static workplace.
- Whether a villager squad can enter combat mode, and what monster combat is
  (target choice, attack, shooting), stays unrecovered — the rules give no
  order for those states and the engine waits for the village and combat rules.

## Отображение (`kn-render` и бинарники)

Bevy 0.20; окно и GPU нужны только чтобы запустить бинарь, тесты идут без них.

- **Координаты.** Пиксели карты — y вниз, угол слева вверху; спрайт Bevy позиционируется по центру и живёт в мире с y вверх. Поэтому кусок `(x, y, w, h)` получает центр `(x + w/2, −(y + h/2))` (`kn_render::piece_transform`). Отрицательные x и y у объектов и спрайтов не отбрасываются — как `render.blit`.
- **Порядок наложения** передаётся через z: прозрачные спрайты 2D сортируются по возрастанию z (`bevy_sprite_render`: `sort_key = FloatOrd(transform.z)`), поэтому n-й кусок сцены получает `z = n · Z_STEP`.
- **Кэш текстур.** `TextureCache` хранит по ассету на ключ `TextureId`; на карту ~6–13 тыс. кусков и ~250–300 уникальных картинок (с кадрами анимаций — до ~500), так что спрайты делят `Handle<Image>`. Фильтрация — `ImagePlugin::default_nearest()`, графика пиксельная.
- **Анимация** (`kn_render::animation`) — свои типы поверх штатных `Sprite`/`Handle<Image>`: `bevy_animation` для покадровой смены картинок не годится (`Animatable` реализован только для чисел, векторов, цвета, `Transform`/`Quat`/`Rot2`/`bool`, а клипы — это `Reflect`-активы `AnimationClip` с кривыми по `AnimationTargetId`; собственный пример Bevy `examples/2d/sprite_sheet.rs` анимирует спрайт вручную, `Timer` + `atlas.index`). Часы `Clock` одни на карту и считаются в **тиках**, дробный остаток кадра накапливается в `sub_tick` — иначе при 60-мс тике и 120 гц терялась бы часть времени и анимация шла медленнее. `PlayState` различает `Playing`, `Paused` (кадр держится) и `Off` (часы обнулены, показан базовый кадр). Система применения меняет поля спрайта только когда кадр сменился, и пересчитывает центр по размеру кадра (`piece_center` — общая формула со статичными кусками): у кадров K1 разные размеры, и с центром от старого кадра картинка прыгала бы. `Animator` не знает ни о K1, ни о форматах — он берёт готовую `Sequence`.
- **Иерархия.** Спрайты карты — потомки корня `MapRoot`, между ними узел группы (`SceneGroup`): корень → группы → спрайты. Каждому узлу обязательны `Transform` и `Visibility`: `propagate_parent_transforms` обходит деревья от корней с `Transform`, а `sync_simple_transforms` пропускает сущности с `ChildOf`. Без `Transform` у узла его потомки не получают `GlobalTransform` и не попадают в рендер — окно пустое, хотя сущности на месте. Спрайт сам `Visibility::Inherited` (кроме куска без картинки базового кадра — тот `Hidden` до своего кадра анимации): решает его группа. Поэтому `Animator` при смене кадра пишет `Inherited`, а не `Visible` — `Visible` перебивает наследование, и включённая анимация показала бы куски скрытого оверлея.
- **Оверлеи — видимость, а не пересборка.** Просмотрщик собирает карту со всеми слоями (`SceneOptions::ALL`) и раскладывает куски по узлам групп, а переключатель слоя ставит узлу `Visibility::Hidden`, включённый узел остаётся с `Inherited` (см. «Иерархия»). Порядок наложения от этого не зависит: z даёт номер куска в списке отрисовки, а не соседство в дереве, и куски одной группы перемешаны с чужими. Как именно слой включает группу, знает модель (`kn_game::group_is_shown`), не `kn-render`.
- **Границы камеры следуют за слоями.** `SceneGroup` хранит мировые границы своих спрайтов (со всеми кадрами их анимаций), и `sync_groups` при смене видимости пересобирает из них `CameraBounds`: ромбы проходимости идут по всей сетке дальше тайлов карты, и иначе включённый слой оказался бы там, куда камера не пускает. Положение и масштаб при этом не трогаются — `frame_scene` наводит камеру и подбирает масштаб только на новой карте. Тот же расчёт, что и при наведении: `scene_bounds` и границы групп идут от одной `piece_bounds`, поэтому они совпадают — держит `group_bounds_cover_the_pieces_a_layer_shows`.
- **Камера.** Ступени масштаба дискретные (`ZOOM_LEVELS`, `scale = 1 / zoom`), чтобы на целых значениях картинка оставалась пиксель-в-пиксель; одна ступень — щелчок колеса или ~60 пикселей свайпа (`ZoomScroll`: порог нужен, иначе инерция трекпада пролистывает весь диапазон за жест, но потолок `MAX_ZOOM_STEPS_PER_FRAME` стоит на ступенях **за кадр**, а не на событии — иначе быстрый жест схлопывался в одну ступень; `push` копит, `take` отдаёт не больше потолка, остаток доходит в следующих кадрах, а за `ZOOM_ACCUM_LIMIT` всё же отбрасывается; над интерфейсом очередь сгорает целиком, чтобы накрученное над панелью не доехало до карты, когда курсор уберут с виджета). При смене карты `frame_scene` наводит камеру на центр содержимого и подбирает ступень, на которой оно влезает в окно (`fitting_zoom`): наведение отложено в `PendingFrame`, потому что камеру спавнят команды и в `Startup` её ещё нет. The pending frame is consumed before the zoom is written out (`apply_pending_frame` precedes `apply_zoom` in the plugin chain), so the step picked by `frame_scene` reaches the projection on the same frame — it used to arrive one frame late (`a_framed_scene_reaches_the_projection_on_the_same_frame`). Границы камеры — прямоугольник содержимого, а не вся сетка 80×160: нарисована бывает только её часть. Панорамирование: WASD и стрелки, перетаскивание левой кнопкой — над интерфейсом отключено, иначе список карт двигал бы карту. «Курсор над интерфейсом» — это значение `Hovered`, а не наличие компонента: Bevy лишь обновляет этот булев компонент у тех, у кого он уже есть, а Feathers навешивает его на каждый свой контрол при спавне; по наличию карта теряла и перетаскивание, и колесо, пока в панели есть хоть один виджет. Camera input is split by the layout refactor: the mechanisms (`pan_direction`, `drag_delta`, `ZoomScroll`/`wheel_steps`) live in `kn-input`, the pointer-capture check (`pointer_over_ui`) lives in `kn-ui`, and each app wires them to its camera in one binding system registered in the `CameraInput` set; `kn-render` only writes the camera out (`CameraApply`: `apply_pending_frame` then `apply_zoom`), and the set order is declared in `kn-render` rather than left to plugin registration. The per-app glue is ~20 lines; every nontrivial computation (key map, thresholds, accumulation, y-flip constants, ortho scale) has a single implementation.
- **Интерфейс просмотрщика** — Bevy Feathers (фича `bevy_feathers`): состояние (`Viewer`) отделено от виджетов, обработчики правят только его. `Rebuild` остаётся для одного случая — смены карты: сцена пересобирается, камера наводится заново. Переключение оверлея запрос не взводит: система `sync_groups` переносит флаги `Viewer` в видимость узлов групп (как `sync_playboxes` переносит состояние воспроизведения в чекбоксы), и кадр не обходится в despawn/spawn тысяч спрайтов. Героя просмотрщик не выбирает: `SceneOptions.hero` остаётся `None`, и состояние берётся по источнику карты. `Checkbox` в Bevy считает новое значение как `!Has<Checked>` и сам `Checked` не пишет, поэтому обработчик синхронизирует его вручную (как `checkbox_self_update`) — иначе каждый клик шлёт `true`. The captions are English — the viewer is a development tool — while the typeface is the same PT Sans the player screens use: fonts are not themeable in Feathers, so `KonungUiPlugin`'s normalization rewrites every `InheritableFont` handle (including the composite interiors Feathers composes itself) to the bundled PT Sans, and the konung theme unifies the colors. Персонажи включены по умолчанию (`DEFAULT_OVERLAYS`), и `Checked` у этого чекбокса должен этому соответствовать. Колесо над панелью карту не масштабирует — по тому же принципу, что и перетаскивание; чтобы блокировка действовала и над пустыми промежутками панели, `Hovered` висит и на её корневом `Node` (Bevy обновляет компонент только у тех, у кого он уже есть, а Feathers кладёт его лишь на свои контролы). The mirroring helper `sync_checked` now lives in `kn-ui` — a single implementation for all frontends.
- **Диагностика считает спрайты в мире.** Панель показывает `видимых/собраных` кусков: карта собирается со всеми слоями, и одно число без другого сказало бы не то, что на экране. Видимым считается спрайт с истинной `InheritedVisibility` — в неё уже входит и скрытая группа, и кадр анимации без картинки, поэтому счёт по детям групп завышал бы первое.
- **Playback and the simulation share one clock.** A single `Play` checkbox (`DEFAULT_PLAYBACK`) drives creature movement; `Space` toggles play/pause and `.`/`,` step one tick. There is no second "animation off" checkbox: the old pair (`Animation` + `Paused`) let turning off the *visual* sprite cycling silently freeze movement, because both the sprite frames (`apply_animators`) and the simulation (`advance_creature_sim`) read the same `Clock.ticks`. With one control, pausing freezes frames and movement together (an unambiguous pause), and `playback_for`/`toggle_playback` never reach `PlayState::Off` from the UI (`play_toggle_never_stops_the_animation_behind_the_simulation_s_back`). Like an overlay toggle it is state-only: no `Rebuild`, no scene rebuild — `apply_playback` just moves `Clock` to match `Viewer.playback`. `sync_playboxes` mirrors `Viewer.playback == Playing` into the checkbox, because a Bevy `Checkbox` never writes `Checked` itself and the state is also driven by the hotkeys.
- **The day/night switch (`Constant day`, #97) is state-only like `Play`.** A checkbox by the playback controls and the `N` hotkey write one shared state (`Viewer::constant_day`); `sync_daynight_boxes` mirrors it into the widget, because a Bevy `Checkbox` never writes `Checked` itself — the `sync_playboxes` pattern. The flip is a state write, not a rebuild: `Rebuild` stays untouched (the map's entity count cannot change) and the Clock never moves — the phase keeps advancing while `update_lighting_state` hands the lighting pool the constant day value through `k1_game::lighting::params_gated(tick, seed, !constant_day)`, the model's pure function for the original's gate (`[0x81D5EC]`; `docs/k1/time-of-day.md`, "Where we will differ from the original" records the divergence, the default and the hotkey). Reset semantics: a viewer session setting, not simulation state — it survives a map switch (which rebuilds the simulation the flag does not belong to) and returns to the cycle-on default (`DEFAULT_CONSTANT_DAY`) only when the app restarts; the original's flag travels with the saved options and its default is unknown, so ours defaults to the cycle on. `--constant-day` pins the flag for a headless capture or benchmark run, the way `--light-tick` pins the tick.
- **Reset positions rebuilds the simulation, not the map.** The button is a `FeathersButton`; a click emits `bevy_ui_widgets::Activate`, and `reset_positions` filters its own by the `ResetPositions` marker (the map `FeathersSelect` fires `Activate` too). It calls `kn_render::reset_creature_sim`, which recreates the `Simulation` via `Game::simulation(map_ref, SIM_SEED)` — so routes, walk states, the RNG and the creature state machine come fresh from the file — and pins `CreatureSim::epoch` to the current `Clock.ticks` with `sim_ticks` back at zero; otherwise the fresh simulation would replay the ticks the old one had already burned and lurch forward. It keeps the overlays, selected map, camera/zoom and the pause state, and never touches `Rebuild` or the texture cache: no scene rebuild, no texture reload. The nodes and the labels stay spawned — `sync_creature_nodes` and the viewer's `sync_creature_labels` reposition them from the simulation each frame, so they settle over the spawn points on their own. The seed is shared with `rebuild_scene` and with the player's start map, so a reset reproduces the exact first spawn in either window.
- **Two ways back to the start, and only one of them is the file (#70 AC4).** `Play → Paused → Play` is a playback pause: `apply_playback` moves `Clock` only, the machine keeps its stage, and a special that was mid-sequence continues where it stopped. A replay comes from `Play → Off → Play`, which no checkbox reaches (`playback_for` maps the box onto Playing/Paused): the `Reset animation` button puts `Viewer.playback` into `Off`, zeroes the clock — which sends `advance_creature_sim` down its rewind branch, rebuild plus replay to tick zero — and rebuilds the simulation, so the machine seeds every state-4 creature back to `Special { frame: 0 }` and a later `Play` runs the sequence again. `Reset positions` deliberately does not stand in for it: it leaves the clock alone and pins the epoch where the clock stands, so the two buttons answer different questions — where the creatures stand, and which frame of their animation they are on.
- **Creature labels read `#<id> <name>` (#84).** The names are built once per map in the game layer: `k1_game::creatures::labels` joins the executable's texts (#34) with the record's class and the name/nickname ids (bytes 0xD4/0xD5, docs/k1/formats/game-state.md) — the species for a monster, the name and the nickname for a person — and a creature with neither part keeps the bare `#<id>`. The finished strings ride the `Simulation` (`Simulation::labels`, review #116: the viewer's label layer reads them and imports neither `k1_game::creatures` nor `k1_data::executable`). The pure builder is judged on synthetic tables only (k1-game); game texts stay in the window — never a fixture, never a CI report. The labels draw in the bundled PT Sans, the same face the panel uses (Bevy's embedded default is Latin-only and would draw boxes over RUS names). This diverges from issue #38's wording "a system font at the path the user specified": no user font path exists anywhere in the repo, and the design system (#110) has since bundled PT Sans (SIL OFL, full Cyrillic) as the single face of both windows — the rule that carries over from #38 is "no game fonts and no game texts in the repository", not "no font files at all". If a user-chosen font face is ever wanted, it is a settings feature in its own ticket. Two geometry bugs the label fix surfaced — the labels had never reached a pixel: they hung at `z = 9999`, beyond the 2D camera's far plane (±1000), so the whole layer was frustum-clipped away (`LABEL_Z = 500` — above every piece, `z = order · Z_STEP`, at most ~172 on the largest shipped map), and the y offset subtracted where world y points up, dropping each label below its creature's feet instead of over its head.
- **Проблемы данных видны.** `TextureCache::load` отдаёт текст причин по ключам, картинка которых не собралась: бинарники пишут их в лог, а просмотрщик показывает счётчик в диагностике — вместо молчаливой карты без части спрайтов. Неверный номер карты в CLI — ошибка с ненулевым кодом возврата, а не переход на первую.

### Creature nodes (`kn-render::creatures`)

Review #68 collapsed the display of a creature onto **one** model: a creature is a
single stable [`kn_render::CreatureNode`] entity under a `CreatureRoot`, the node owns
its world position, and its body, equipment and shadow sprites are its children that
carry only local offsets and per-frame deltas. The branch had started with two
competing models — the node and separately-moving `CreaturePiece` entities — and both
wrote the same `Transform`; that is what made a creature slide and bounce at once. One
per-tick writer is the invariant (P2 #5): `sync_creature_nodes` is the only system that
sets a creature's world position. `CreaturePiece` and `update_shadow_depth` are gone.

- **The pipeline is one plugin in `kn-render`, and both windows add it (issue #70).** `CreatureAnimationPlugin` owns the resources (`CreatureSim`, `CreatureAnim`, `CreatureSpawns`, `CreatureDepth`, `CreatureMotion`, `CreatureAnimReport`, `CreatureNodesVisible`), the five systems and the ordering between them and the scene clock, so a frontend gets the whole thing by adding the plugin and cannot mis-order it. A frontend asks for a map with `CreatureMotion::request(map, seed, scene)` and provides what any window drawing this game has anyway — `GameResource`, `TextureCache`, `Assets<Image>` and the `Clock` of `AnimPlugin`; the subsystem adds no clock of its own. The viewer's `#id` labels and its diagnostics panel are its own additions, attached after `CreatureSet::Nodes`; the player's whole creature wiring is one request (`kn_fe_player::request_start_creatures`, same seed as the viewer's "Reset positions", so one map spawns the same way in either window). Before that request existed the player drew no creatures at all: `spawn_scene` skips the pieces that carry a `creature_id`, so the composite nodes were simply never built there — covered by `crates/frontend/bevy/player/tests/creatures.rs`.
- **The node owns the world, the children own the frame (P1 #1).** The node gets
  `x, −y` from the simulation and its depth z; `apply_animators` writes each animated
  child's centre relative to that child's own base offset (`Animator::with_base`), never
  a world centre, so the two no longer double-count a move. The node carries no
  `Animator`, so a frame change resizes and repositions a sprite *inside* the creature
  but cannot translate the creature in the world
  (`an_animation_frame_change_does_not_move_the_node_world_position`,
  `body_equipment_and_shadow_ride_the_node`). Both root and node carry a `Transform`
  because `propagate_parent_transforms` reaches children only down a chain rooted at a
  `Transform`.
- **The node, sprite and label queries are provably disjoint.** `sync_creature_nodes` moves the
  node and its shadow in one system, both taking `&mut Transform`, so its node query carries
  `Without<CreatureShadow>` *and* `Without<CreatureSprite>` — the sprite transforms are what the
  animation pass writes. The `#id` labels are the viewer's own (`sync_creature_labels`, riding the
  body image rather than the cell), and that query carries `Without<CreatureSprite>` *and*
  `Without<CreatureNode>` for the same reason, because Bevy otherwise rejects the overlap with a
  `B0001` panic that crashed the running viewer before anything moved (`creature_node_and_label_queries_are_disjoint`).
- **Layers keep their identity by `LayerKey`, not by position (P2 #7, review #68).**
  `kn_game::CreatureClips` hands each layer as `CreatureLayer { base, animation, corner, key }`:
  a layer carrying its resting image, its frames, the absolute corner its base frame was
  composed at and the sprite part it draws (`Pose::layer_keys` in `k1-game` carries the key
  out of the pose build). Within one pose the subsystem still builds the node by the layer's
  place in the list — scene pieces and resting clips come from the same build and agree
  (`the_rest_clips_are_anchored_where_the_scene_draws_the_layer`). Across poses the match
  is by `key` (`pose_matched_layers`): track order is first-appearance *per pose*, so a
  walk pose may list the same layers differently and an index join bolts one part's walk
  clip onto another's piece (`walk_layers_join_to_their_part_not_their_position`). Body
  and equipment keep the scene's pose order in their local z, so equipment reads over the
  body (`body_layers_keep_their_scene_order_depth`).
- **A clip swap re-anchors on the clip's own base corner; the old anchor was the jump (review #68).**
  Every clip's frame `dx/dy` are offsets from *that clip's* base-frame corner
  (`layer_track` in `k1-game`), and the rest and walk poses of one creature are composed
  around different corners. Swapping the sequence while leaving the `Animator` on the
  resting corner rendered the new clip displaced by the corner difference — the periodic
  one-cell image jump at every direction or rest/walk change. `Animator::play_at` now takes
  the anchor and moves it with the sequence (`apply_clip` reduces the clip's absolute
  corner to the creature's local coordinates); the frame change and the anchor change are
  one atomic write (`play_at_reanchors_the_piece_on_the_new_clip_corner`,
  `a_clip_swap_reanchors_the_image_and_never_moves_the_node`,
  `apply_clip_reanchors_on_the_clips_own_corner`). The node is never touched by a swap —
  the jump was always the image, never the world position.
- **A one-frame resting layer would block the walk swap — a structural gap with no data hit (review #68).**
  `layer_track` builds an `animation` only for multi-frame tracks and `clip_seq` requires
  one, so a layer resting on a single frame spawns without an `Animator`, and
  `sync_creature_animation` — which queries `&mut Animator` — could never swap a walk
  clip onto it. Scanned both editions over every map: no creature has a static rest layer
  whose same-`LayerKey` walk layer is animated (body or shadow), so nothing is patched
  blindly; the chain is recorded here so a future format surprise finds the gap named.
- **Depth comes from the static stacks only, and a creature's shadow rides its slot (P1 #4, P2 #6, review #68).**
  `DepthMap` is built from `PieceGroup::Objects` bottoms (`is_depth_relevant` matches only
  `Objects`), never from characters, terrain or markers — those must not decide what a
  creature stands behind. Flames are excluded too: a fire is drawn together with its
  object, and as a contributor its higher bottom would put a creature on the fire's row
  in front of the object it burns on (rus map 1 had exactly one such inversion).
  Equal bottoms collapse to the highest sort key, so two statics sharing an edge resolve
  to the one drawn on top; a contributor lower than the previous boundary adds nothing
  and never drops it — the search and the interpolation both assume z grows with bottom
  (`depth_map_uses_only_static_object_stacks`, `equal_bottoms_collapse_to_the_highest_sort_key`,
  `depth_boundaries_stay_monotonic_on_every_map`,
  `flame_pieces_are_recognised_as_part_of_their_object`).
  A creature then occupies the slot between two boundaries (`DepthMap::creature_slot`):
  the shadow sits above the statics under its feet and strictly under the body — the
  original draws the pair together, while the earlier fixed band below the whole map
  buried the shadow under the ground. Body and every layer stay under the next boundary
  at any bottom: an interpolated base reserves the stack's room, so a stack spawned in
  one slot never draws over the next static after moving into another — the layer
  offsets are never rewritten (`the_slot_orders_shadow_body_and_layers_inside_the_boundaries`,
  `a_narrow_slot_keeps_every_layer_under_the_next_static`,
  `production_spawn_places_the_shadow_in_its_slot`,
  `a_reset_returns_nodes_animators_shadows_and_labels_home`).
- **The visual step is offset by one tick; the logical tick is not (P1 #2).** A walk step
  commits at `step_elapsed == 1`, which is faithful to the original (Slavik increments the
  frame counter before the first render, so the first *visible* frame is index 1) and is
  kept as the logical contract the engine and tests see. The bug was in the display
  interpolation, which read that `1` as "one tick already spent" and jumped a whole step
  forward the instant a step began. `smooth_pixel` maps visual progress to
  `(step_elapsed − 1 + sub_tick)`, clamped to `[0, step_ticks]` — continuous at both step
  boundaries (`one_tick_into_a_four_tick_step_sits_a_quarter_across`, and the
  1/2/4-tick continuity test `movement_is_continuous_across_step_boundaries`).
- **The simulation's own direction is single-valued per step; the display is what bounced.**
  A creature carries one `direction` for the whole step, so it never flickers between
  neighbours on its own — the earlier left/right jitter was purely the interpolation above,
  now fixed. To tell the two apart at runtime, `KN_CREATURE_DEBUG=1` logs per creature id:
  the sim tick, cell, step from/to, `step_elapsed`, the interpolated pixel, the rendered
  node position and its `GlobalTransform`; the viewer's `sync_creature_labels` adds every
  sprite's chosen clip — named against the clips the subsystem built (`clip_identity`) —
  its base corner, anchor, phase and shown frame with its local offset and the world
  position as it will be drawn. Comparing the sprite's world with the node's separates a
  node jump from an image jump without guessing. The logging is the viewer's own: the
  shared subsystem logs nothing, so `KN_CREATURE_DEBUG` stays a diagnostics feature
  rather than something the player inherits.
- **The creature pipeline is ordered by its own plugin, not by the scheduler (review #68).**
  `CreatureAnimationPlugin` registers its systems in `CreatureSet::Prepare` → `Advance` → `Spawn`
  → `Swap` → `Nodes`, chained, with the first four `after(AnimSet::Clock)` and
  `AnimSet::Sprites` configured between `CreatureSet::Swap` and `CreatureSet::Nodes`: the clock
  settles, the simulation catches up to it, clips are chosen, sprites are written, then the nodes
  are placed — never one frame stale by scheduler accident, and never a clip applied after the
  nodes were read. A frontend that listed the systems itself could break that order, which is why
  the chain lives in the plugin both windows add; the viewer only pins its own chain
  `.after(AnimSet::Clock).before(CreatureSet::Prepare)` and hangs its labels after
  `CreatureSet::Nodes`.
- **A map change and a reset both pin the epoch; a backward `Step` rebuilds and replays (P1 #3, P2 #8).**
  `rebuild_scene` (through `CreatureMotion::request`) and `reset_positions` (through
  `reset_creature_sim`) pin `CreatureSim::epoch` to the current `Clock.ticks`
  so a fresh simulation does not re-run the whole tick history in one frame
  (`map_change_pins_the_epoch_so_history_is_not_replayed`). The engine has no reverse
  tick, so `advance_creature_sim` rewinds by rebuilding the map's simulation and replaying it
  forward to the epoch-relative target; the replay already lands at `clock.ticks`, so the
  epoch is pinned there — pinning it to 0 let the catch-up loop tick the fresh simulation
  a second `clock.ticks` times and every creature overshot
  (`a_backward_step_rewinds_the_simulation_to_the_clock_not_beyond_it`,
  `replay_simulation_matches_forward_stepping`). The state machine lives inside the
  simulation and moves once per `tick`, so a rewound creature — a creature in the middle of
  its state-4 special included — shows exactly the state of the tick it landed on, with no
  reversible simulation (#70, `rewinding_the_clock_replays_to_the_positions_a_forward_run_reached`).
  Measured cost of the worst
  rewind — a 30 000-tick (30 game-minute) session on the real first map, full
  rules and engine per tick: 306 ms release (10.2 µs/tick), 4.3 s debug, on an
  Apple M-series (`replaying_a_long_session_stays_a_bargain`, release numbers
  from `cargo test --profile release`). A rewind is a one-off after the press,
  not a per-frame cost, and a reset pins the epoch so rewinding just past it
  replays almost nothing.
- **A pose rebuild is paid per state change, never per frame (issue #70 AC6).**
  The state-4 special is a per-creature state machine in the game layer
  (`k1_game::creature_anim::Machine`) that owns no clock; the subsystem reads its
  stage each frame and rebuilds a creature's pose — the clip swap in
  `sync_creature_animation` — only when the stage *kind* flips (Special ↔ Idle),
  never when a plain tick advances the frame counter inside a running special
  (`the_stage_snapshot_rebuilds_only_on_a_state_kind_flip`,
  `counted_run_shows_one_rebuild_per_state_change_and_none_per_frame`). The
  bound is measured over the real shipped population rather than eyeballed:
  across every `Game.0`–`Game.2` of both editions — all 404 state-4 records
  (249 Skeleton, 152 Eechetik, 3 Ion) — a full map load plus the ticks that
  finish every sequence count exactly two rebuilds per creature (entering the
  special, and its Special → idle edge), and a thousand further idle ticks
  count zero (`pose_rebuilds_count_one_per_state_four_creature_and_no_more`).
  The comparison that matters is against the per-frame path: a creature sitting
  in its idle pose pays no rebuild no matter how long it stands, so the cost is
  O(state changes) per creature, not O(ticks).
- **Order outcomes are consumed, not dropped (P2 #4).** `Simulation::tick`
  applies decisions through `rules::FailedOrderCooldown` (one route search per
  `WorldRules::repath_cooldown_ticks` for a creature whose goals keep failing,
  otherwise the re-deciding loop pays a search every tick) and keeps every
  rejection that is not `NoRoute` in `Simulation::order_failures` — the rules
  only aim at cells `Ground::is_free` vetted, so anything else is a bug or bad
  data; the headless report lists them as errors and fails the run
  (`a_driven_run_walks_a_villager_into_its_area_without_order_failures`,
  `an_unreachable_area_costs_throttled_searches_not_order_failures`).
  `initial_pixels` deliberately covers the whole roster, unplaced creatures
  included: the original places a record wherever the file says, so such a
  creature keeps rendering on its load cell — the record cell for everyone
  the load rule keeps in place, the jittered cell for a roaming member the
  world then refused — while `spawn_failures`/`order_failures` carry the
  refusal to the user
  (`an_unplaced_creature_is_reported_and_keeps_its_load_cell_for_the_renderer`).
- **Load refusals are data, errors decide the exit code (#82).** The façade
  itself now runs the load rule — `Simulation::new` calls
  `placement::place_roaming` before the world exists, so the `simulate` CLI
  places roaming squads exactly as the harness test and the viewer do (the
  CLI used to stand the file cells verbatim and refuse whole squads as
  `ImpassableCell`). After that only records no rule can move remain
  refused: `Simulation::spawn_failures` (reason per roster record, e.g. a
  villager standing in blocked foot data) and `Simulation::load_trimmed`
  (the `GroupSize` shrink) are facts about the shipped files, which the
  headless report prints as data — the exact lists both editions give are
  pinned by `apps/viewer/tests/original_simulate.rs` and written into
  [creatures.md](k1/creatures.md), «Load-time refusals»; the run fails only
  on `order_failures` or a build error. `Simulation::goal_reaches` counts
  completed confirmed orders per creature (workplace and walk arrivals,
  «Goal reach» there) from `Arrived` events, and the report carries the
  counters; the same seed replays them, shipped tests included. The same
  test file now verifies both editions where the shipped checks were
  RUS-only before. Creature nodes hang on the subsystem's `CreatureRoot`,
  born with the visibility the frontend asked for through
  `CreatureNodesVisible`, and the viewer's `#id` labels on its own
  `LabelRoot`; the viewer's `sync_creature_visibility` drives both roots
  from one query, and both are born with the Characters overlay's
  visibility — a fixed `Visible` showed the creatures for the frame
  before `sync_creature_visibility` caught up after a map change
  (`roots_are_born_hidden_when_characters_are_off`); while the overlay is off the roots
  are `Hidden`, so turning creatures off also hides their names.
- **Roads are unconfirmed (P2 #10).** There is no road layer in `Konung.N`: the `foot`
  grid has only `is_blocked` (`foot & 0x0F == 0x0F`). `MapPassage` is locked to exactly
  `!is_blocked` and no road/terrain preference is applied — `passage_is_the_confirmed_foot_rule_only`
  fails if a future unconfirmed rule is added. The original's route *preference* for roads,
  if any, is not recovered and is not invented here ([k1/movement.md](k1/movement.md)).

### Цена переключения слоёв

Замер на ENG, карта 17 (12 501 кусок со всеми слоями, 7 180 без них), release, Apple M4 Pro; ECS и распространение видимости без рендерера:

| Операция | Время |
|---|---|
| разбор списка отрисовки: все слои / чистая карта | 26 мс / 18 мс |
| пересборка спрайтов (despawn корня + spawn 12 501, без разбора) | 6.4 мс, худший кадр 6.5 мс |
| переключение группы из 5 131 спрайта (одна запись видимости узла) | 0.15 мс, худший кадр 0.20 мс |
| счётчик панели: обход 12 501 `InheritedVisibility` | 0.5 мкс на кадр |

Старое переключение оверлея — разбор + пересборка, то есть те самые ~30 мс единым хичем (в issue #30 с рендером получалось 38–43 мс). Новое не делает ни того, ни другого: `Rebuild` взводится только сменой карты. Плата — карта всегда собирается со всеми кусками: на 17-й это 12 501 спрайт вместо 7 180 и +8 мс на открытии карты. Скрытые группы рендер не вытягивает (их `ViewVisibility` ложна), но в мире они живут — см. шаги B и C в issue #30.

Число кусков в панель собирается линейным обходом спрайтов, а не суммой по девяти узлам групп: обход дешевле изменения видимости, который его окружает (0.5 мкс против 150 мкс на переключение), и только он не считает видимыми спрайты, скрытые кадром анимации без картинки.

### Frame time gate

The gate turns the viewer costs above into a verdict (issue #98): pass =
average and p95 frame time ≤ 33.3 ms — 30 FPS — on each reference map. The
gate runs after the ambient lighting (#96) and the constant-day toggle (#97)
with lighting on, so the numbers are the final render, not a diagnostic
stage; a failing number opens its own optimization ticket, it is never
optimized inside the gate's ticket.

**Protocol.** Release build, Windows 11, AMD Ryzen 9 9950X, 64 GiB RAM,
NVIDIA GeForce RTX 2070 SUPER (the wgpu adapter of every run — Vulkan — and
the machine the "Lighting cost" table above was measured on), window
1280×800, zoom
step 3 (scale 1.0 — the startup default; a benchmark run never touches the
camera or the scene), playback playing (all creatures active, the shipped
default), day/night lighting on (the cycle follows the sim tick;
`--constant-day` off). One command runs the gate — maps 17 and 19, both
editions, 60 seconds each:

```bash
just viewer-gate
```

which is four `cargo run --release … -p kn-viewer` runs of
`--game-dir original/k1/<rus|eng>/game --map <17|19> --benchmark-secs 60`
(data in `original/`, never committed). One run prints one `BENCH …` line
and one `GATE map=.. pass=.. avg_ms=.. p95_ms=.. limit_ms=33.3` line.

**Frame bound.** The same bound the "Viewer costs" method records, so the
numbers stay comparable: benchmark runs render with `PresentMode::AutoNoVsync`
so the meter reads the scene and not the display's 16.67 ms quantum; the
samples are the raw per-frame wall-clock deltas `FrameTimeDiagnosticsPlugin`
writes into `DiagnosticsStore` each update (the diagnostic's latest
measurement, not its smoothed value); the 40 settle frames are excluded; the
percentiles are nearest-rank. The benchmark system is ordered after the
plugin's own frame-time write and after the creature chain, so the sample
and the tick counter are both complete for the iteration that reads them.
The delta itself anchors at `First` and carries the previous iteration's
wall time (bevy_time 0.20.1 `time_system` computes it there;
bevy_diagnostic 0.20.1 `diagnostic_system` writes it in `Update` from
`Time<Real>`), so `max_on_tick` pairs each sample with the previous
iteration's tick count — the ticks whose execution the sample's interval
includes — and the first sample of a window, whose interval belongs to the
settle frames, reads 0: the attribution is exact on both sides of the
pairing. A `--benchmark-secs` run closes the window
when the samples' sum crosses 60 000 ms — 60 seconds of measured frame time
whatever the hardware delivers — so the frame count is an output of the run
(`frames=` in the line), not part of the recipe. The window self-describes:
`ticks` (sim ticks across the window; a 60-second window is exactly 1000 —
`TICK_MS` is 60, `k1-data::objects_res`), `creatures`/`walking` (the
population it ended on), `labels` (whether the name labels drew), and the
identity `ticks · 60 ms ≈ frames · avg_ms` marks the honest runs — all four
runs below satisfy it to within 0.2 %.

**Reproducibility.** Three passes back to back — twice through the recorded
commands, once through the `just viewer-gate` recipe itself — were run on
the pre-integration tree: average and p95 reproduced within 0.04 ms in every
configuration, p99 within 0.21 ms, the worst frame within 0.41 ms. The
recorded tolerance stays, scoped to one machine state: repeated back-to-back
runs on a settled machine count as reproducing while average and p95 stay
within ±0.05 ms; a larger shift — across power states or across days —
means the machine state changed, not the scene. Cross-state spread is
recorded, not averaged in: the tree's passes on this table sit within
0.06 ms average and 0.11 ms p95 of each other at the widest pairwise
comparison, with the GPU idling in different power states between passes,
and the single runs around the table match them. Two shifted states were
discarded on the way: a pass taken right after a game session had exited
read 0.5–0.9 ms higher in every configuration (the driver's post-game power
state, not the scene), and runs under the desktop-load signature — p95
jumping to 16–18 ms — were discarded with it.

The table is the re-measurement the integration required — one pass per
configuration on the tree with #115–#117 in, so what the gate measures now
includes #116's per-frame name-label rendering, part of main since the
integration:

| Map (edition) | avg ms | p95 ms | p99 ms | worst ms | frames | population |
|---|---|---|---|---|---|---|
| 17 RUS | 3.08 | 3.41 | 3.90 | 10.15 | 19 478 | 28 creatures, 0 walking, labels on |
| 19 RUS | 2.28 | 2.65 | 3.01 | 8.87 | 26 281 | 20 creatures, 14 walking, labels on |
| 17 ENG | 3.11 | 3.49 | 3.94 | 10.17 | 19 319 | 28 creatures, 0 walking, labels on |
| 19 ENG | 2.33 | 2.76 | 3.15 | 9.77 | 25 715 | 20 creatures, 14 walking, labels on |

Every window covered exactly 1 000 sim ticks.

**Verdict: pass, all four configurations, with ~9× headroom.** The slowest
p95 is 3.49 ms against the 33.3 ms limit. The recorded round sits on the
pre-integration ranges — the gate's own 2.2–3.3 ms average covers this
round's 2.28–3.11 ms, the "Viewer costs" playing runs' 3.15–3.57 ms bound
the calm map from above — so #116's per-frame name labels, drawing over
every creature every frame while the `BENCH` line counts the same sprites
and light materials as before, do not show above the machine's spread. One
same-round pair prices the labels on the village (19 RUS): 2.30 → 2.28 ms
average and 2.71 → 2.65 ms p95 across `KN_VIEWER_LABELS=0`/on — under the
round's own reproducibility, so the recorded round leaves the draw cost
unpriced; the ~0.2 ms an earlier same-round pair measured on the superseded
integrated table (3.40 → 3.22 ms average, 3.98 → 3.77 ms p95) stays the
only resolved number, and which machine state lets the pair resolve it is
unconfirmed. The editions' spread on the same map is 0.03–0.05 ms here
against the superseded round's 0.8–1.0 ms ("English names draw wider than
Cyrillic ones") — the same machine-state dependence, hypothesis either
way. The pre-integration ordering (village 19 faster than calm 17) holds
on both editions: 2.28 against 3.08 ms on RUS, 2.33 against 3.11 ms on
ENG.

**p99 and the worst frame, with cause.** p99 is 3.01–3.94 ms — the tail is
the scene's own spread, not a stall class. The worst frame is 8.9–10.2 ms,
and its `max_on_tick` attribute is 0 in all four windows — measured under
the corrected pairing: a sample carries the previous iteration's wall time
(see "Frame bound"), so the flag reads the tick count of that same
iteration, and none of the intervals the worst samples measured ran a
simulation tick. Independence predicts the same (a
tick lands every ~19–26 frames here, so ~4–5 % of the window carries one), and
the "Viewer costs" isolation bounds what a tick could add — a full engine
tick is 18–22 µs, a pose swap ~10 µs, three orders below the spike. What
that leaves the worst frame to be — scheduler noise, a driver stall — stays
unconfirmed: the exclusion of tick work is measured, the positive cause is
not observed. The worst frame's position in the window (`max_at`) moves
freely — 14799 and 15615 on the calm map, 6999 and 13854 on the village —
which fits noise, not a recurring stage of the frame. Nothing here warrants
an optimization ticket: every percentile sits an order of magnitude under
the limit.

## Build profiles

Three profiles (`crates/Cargo.toml`) with separate jobs; the local development loop never builds `release`.

| Profile | Runs | Own code | Dependencies |
|---|---|---|---|
| `dev` | `just cargo-test`, `just cargo-lint`, prek hooks (pre-commit clippy, pre-push) | opt-level 0, full debug info | opt-level 0 |
| `dev-runtime` | `just viewer`, `just game` | opt-level 1, `debug = "line-tables-only"` | opt-level 3, build scripts and proc macros too (`build-override`) |
| `release` | `just viewer-release`, `just game-release`, `just check-maps`, manual benchmarks, release artifacts | opt-level 3 | opt-level 3 |

`dev-runtime` exists because Bevy is only usable at dependency opt-level 3, while the edit-rebuild loop needs own code to compile fast. It was measured on this machine (Windows, rustc 1.99, warm cache, one-line edit; the first measurement of a case is dropped when the first run carries a one-off cost):

| Warm loop | `release` (before) | `dev-runtime` static | + rust-lld | + bevy_dylib |
|---|---|---|---|---|
| edit in `kn-render` → viewer binary | 7.6 s | 6.8 s | 5.9 s | **2.2 s** |
| edit in the viewer binary → relink | 4.2 s | 7.5 s | 4.5 s | **1.0 s** |
| no-op | 0.3 s | 0.3 s | 0.4 s | 0.4 s |

Three levers, each measured:

- **rust-lld** (`.cargo/config.toml`, `[target.x86_64-pc-windows-msvc] linker = "rust-lld"`,
  `-C linker-flavor=lld-link`). Bundled with the toolchain (rustc ≥ 1.70), nothing to install;
  CI is ubuntu-latest and the section does not touch it. Applies to every local Windows
  build, including dev test binaries. Adding rustflags rebuilds every cached unit once
  (~2.5 min per profile); delete the file to fall back to `link.exe`.
- **`debug = "line-tables-only"`** for `dev-runtime`: PDB writing is the Windows link cost,
  backtraces keep file and line. Not measured against `debug = 2` separately — with the two
  levers below the relink is 1 s anyway.
- **`bevy/dynamic_linking`** behind the `dynamic` feature of `kn-fe-viewer`/`kn-fe-player`,
  passed by the `viewer`/`game` recipes. Bevy moves into `bevy_dylib.dll` (137 MB in
  `crates/target/dev-runtime/deps`), the app binary links in ~1 s. Both windows verified on
  Windows/Vulkan. Caveats: the exe only starts through `cargo run`/`just` (the DLL is found
  via the `deps` directory on the PATH cargo sets); launching it directly needs the DLL
  copied next to it. Own code must stay at opt-level ≤ 1 — true for `dev-runtime`. The
  feature is opt-in per recipe: drop `--features …/dynamic` to link statically.

`just check-maps` intentionally remains on `release`: it is headless parsing, fully CPU-bound
in own code (no Bevy rendering), and runs an order of magnitude slower in `dev-runtime`
(measured 77 s for all 50 RUS maps; ~5 s in release). This exception is the only place where
the release profile is still needed locally — every viewer/player recipe switched to dev-runtime.

Bevy features are declared once in `[workspace.dependencies]` with `default-features = false`
and only what both apps use: 2D sprites, Feathers UI, text with the embedded default font,
picking, winit. The default 3D/PBR/glTF/audio/gizmos/anti-alias/post-process chain is off
(no code references to it — grep over `crates/`); what remains of `bevy_gizmos`, `bevy_gilrs`,
`bevy_scene`, `bevy_animation`, `bevy_state` comes in through `bevy_feathers` implications.
Both windows verified after the cut; the regression found during the audit was the missing
`bevy_ui_render`/`ui_bevy_render` (the Feathers panel panics with `DrawFunctions<TransparentUi>
must be added` without them). The feature set lives in one place now, and cargo unifies it
across all five Bevy crates, so an addition in one crate cannot silently re-enable defaults
for the others.

sccache is optional, not part of the setup: `RUSTC_WRAPPER=sccache` (after installing it
separately) shares compiled dependencies between the agent worktrees, each of which keeps its
own `crates/target` (the cache is per machine, the target directories stay separate). Without
it a fresh worktree pays one cold dependency build per profile (≈2.5–3 min each here).

### CI (`.github/workflows/ci.yml`)

Measured on GitHub-hosted runners, rustc 1.99.1, 2026-11-13, each row its own run
(PR #120 runs 43404019876 cold, 43407464572 warm):

| Job | Runner | Cold | Warm |
|---|---|---|---|
| `rust` — fmt, clippy, test (`dev`) | ubuntu-latest | 7m32s | 3m56s |
| `build` — `kn-viewer` + `kn-player`, full link (`dev-runtime`) | ubuntu-latest | 14m50s | 1m39s |
| `build-windows` — same as `build` | windows-latest | 21m1s | 1m44s |

The `build` job is the PR gate; `build-windows` runs on push to main and `workflow_dispatch` only.
The same profile measures 1.4× slower on windows-latest than on ubuntu-latest (`cargo build
--timings`: 503 units, 3379 s of rustc in 859 s of wall on Linux against 4635 s in 1204 s on
Windows), so the gate lives on Linux. A Windows-only regression is caught by the post-merge smoke
on main, not on the PR — an accepted trade-off; the release path still requires a green main run
(`version.yml`). Cold build profile is identical on both runners: `bevy_*` crates take 56–58% of
the rustc time (`bevy_feathers` 141 s, `bevy_render` 137 s, `bevy_ui` and `bevy_sprite_render`
121 s each on Linux), so the job time hinges on dependency compilation, not on own code
(`kn-render` compiles in 39 s at opt-level 1).

Dependency compilation is cached with `Swatinem/rust-cache@v2` (`workspaces: crates`): only
dependencies are cached, workspace crates rebuild every run by design; the job id is part of the
cache key by default, so the three jobs keep separate caches without extra inputs — measured
≈1.1 GB (`dev`), ≈0.8 GB (`dev-runtime` Linux), ≈0.7 GB (`dev-runtime` Windows), restore 23–34 s
per job, and no re-save on an exact key hit. Both build jobs run `--timings` and upload
`cargo-timing.html` as a run artifact, so every regression in build time is attributable per
crate. There is no nightly schedule: pushes to main happen several times a day and every green
main run refreshes the caches; revisit if the cadence drops (GitHub evicts caches after 7 days
of disuse, and the repo-wide limit is 10 GB).

## Запуск

```bash
just viewer 17                        # карта 17; без номера — первая карта источника
just viewer 4 tutorial                # карта обучения (есть только в ENG)
just check-maps [tutorial]            # то же без окна, GPU и рендерера — используют CI и агенты
just game                             # заготовка re-konung на стартовой карте новой игры
just viewer-release [N] [source]      # то же окно в release: бенчмарки и диагностика
just game-release                     # то же в release
```

Каталог игры — `original/k1/<издание>/game` (как распаковать — [k1/editions.md](k1/editions.md)); по умолчанию RUS, переопределяется переменной:

```bash
GAME_DIR=original/k1/eng/game just viewer 17
```

Рецепты — обёртка над cargo-командами, их можно расписать и вручную:

```bash
cargo run --profile dev-runtime --manifest-path crates/Cargo.toml -p kn-viewer \
    --features kn-fe-viewer/dynamic -- \
    --game-dir original/k1/rus/game [--map N] [--source tutorial]

cargo run --release --manifest-path crates/Cargo.toml -p kn-viewer -- \
    --game-dir original/k1/rus/game check --map 1
… check --all [--format json]

cargo run --profile dev-runtime --manifest-path crates/Cargo.toml -p kn-player \
    --features kn-fe-player/dynamic -- \
    --game-dir original/k1/rus/game
```

Код возврата ненулевой, если карта не собралась или её текстура не декодировалась. Оригинальные файлы только читаются.

### UI showcase (`kn-ui`)

`just ui-showcase` opens a live catalog of the design system: the palette,
spacing and typography tokens, the konung theme on every Feathers control, the
bundled PT Sans faces and the `widgets` helpers. It is the quickest check of
what a change to the theme or the tokens does — the viewer and the player run
the same `KonungUiPlugin`. The recipe runs the `showcase` example of `kn-ui`
on the `dev-runtime` profile (`cargo run --profile dev-runtime
--manifest-path crates/Cargo.toml -p kn-ui --example showcase`), without the
`dynamic` feature: the example links Bevy statically.

### kn-ui: fonts and theme

Two decisions that are not visible from the outside:

- Fonts are not themeable in Feathers 0.20 — `ThemeProps` carries colors
  only, and every control hardcodes `InheritableFont` with the embedded
  FiraSans path. So `KonungUiPlugin` embeds PT Sans (full Cyrillic and Latin,
  `assets/fonts/OFL.txt`), the `widgets` helpers patch `InheritableFont`
  over each control root they build, and the plugin's normalization system
  rewrites every added `InheritableFont` handle to PT Sans — every
  `InheritableFont` is a `PropagateOver<TextFont>` barrier, so composites'
  interiors (the select's trigger and options, the slider's readout) are out
  of reach of root patches and are covered by that system instead.
- The theme is `create_dark_theme()` with the semantic colors remapped onto
  the konung palette; identity beyond colors — radii, row heights, paddings —
  is not fightable inside Feathers and comes from the kn-ui tokens applied in
  our own BSN styles.

### Player application flow (`kn-app`, `kn-fe-player`)

The player is an MVU loop over a pure core. `kn-app::AppController` holds the
whole application state (`Screen`, applied settings, the settings draft, the
last session error) and reduces queued typed `Action`s into it, returning
`Effect`s; the frontend (`flow.rs`) wires that into `Update` as one chained
pipeline per frame: `app_flush` (drain actions → effects) → `apply_effects`
(the only executor: session build, apply-and-persist settings, exit) →
`app_flush` again (result actions; the reducer guarantees they emit no
effects, so the chain cannot loop) → `sync_screens` (the view of the new
screen). A session is never built at startup: the menu loads nothing, and
«New game» is `StartSession`, executed as `session::init` — the old startup
loader, now all-or-nothing — with `session_watch` reporting `Succeeded` only
when the simulation exists, the request is consumed and the map tree is in
the world (`CreatureMotion::request` is a queue, not a build). A failure
tears the half-built session down (`session::teardown`, idempotent) and the
menu shows the typed reason until the next attempt. Both long effects —
`StartSession` and `ApplyAndPersist` — execute synchronously on the main
thread inside `apply_effects`: the loading screen is a static line shown
between frames, not a progress indicator, and no async machinery exists
behind it; making the build asynchronous is a candidate future issue, not a
plan. `kn-player` boots
settings-first (typed load errors, a bad file left untouched), then the game
and the hero, and `--new-game` seeds the flow before the first frame. The
idle pipeline is two exclusive world systems over an empty queue — the flush
allocates only when effects exist; no per-frame cost is claimed beyond that
qualitative statement, and a measurement lands when a screen does work worth
measuring.

## Проверки

`crates/` проверяются и локально, и в CI (job `rust` в `.github/workflows/ci.yml`: fmt, clippy, тесты). С появлением Bevy job `rust` компилирует его дерево зависимостей — это минуты, кэш в CI не настроен. На Linux сборке Bevy нужны системные `-dev` пакеты (wayland, x11, ALSA, udev) — в CI их ставит отдельный шаг, список взят из `docs/linux_dependencies.md` в крейте `bevy`; на macOS они не нужны.

```bash
just cargo-test   # тесты: синтетические всегда, на данных — если есть original/
just cargo-lint   # cargo fmt --check + clippy -D warnings
```

The movement engine tests (`crates/game/kn-game/tests/movement.rs`, `movement_grid.rs`) are synthetic: they need neither game data nor Bevy — the grid, routes, a step per tick, a crowd of creatures and determinism are checked on invented walkability fields. `movement_grid.rs` is the only place in the module that knows about `k1-data`: a comparison of the grid formulas with the Python reference and with `game_state` over all 51 200 cells, not a layer dependency. The cost measurement (`movement_cost.rs`) is hidden under `#[ignore]` and is computed only by hand in release (see "Engine cost"); `removal_cost.rs` works the same way for removals.

A window and a GPU are not needed for the tests: `kn-game` and `k1-game` use no Bevy at all, while `kn-render`, `kn-fe-viewer`, `kn-fe-player` and `kn-viewer` check pure logic on a `MinimalPlugins` app — piece coordinates and z-order, zoom steps, overlay toggles, CLI parsing, the error report, and for animation the frame chosen by ticks, the fractional remainder, pause/off and the per-frame centre recomputation. The player's tests (`crates/frontend/bevy/player/tests/creatures.rs`) run the app the window runs minus the window, and assert that the creatures of the hero's start map reach the screen and that a state-4 creature's special is what its sprite shows until the machine reaches idle; they skip when `original/` is absent. Сквозная проверка без окна — `check` (см. «Запуск»): на ENG все 43 карты игры и обе карты обучения собираются без ошибок, в release это ~5 с; в строке отчёта, кроме кусков и текстур, видно число анимированных кусков, их кадров и видимых в базовом кадре.

Запись (`serialize`, `serialize_lights`) проверяет точную геометрию сеток до записи, а не только итоговый размер: строки 81 и 79 клеток дают верное число байтов, но сместили бы границы строк. Та же проверка добавлена в Python-эталон (PR #18) — на валидных картах реализации идентичны побайтно.

Тесты на данных (`crates/game/k1/k1-data/tests/original.rs`, `original_game_state.rs`, `original_game_edit.rs` и `original_graphics.rs`) повторяют `tests/k1/test_maps.py`, `test_game_state.py`, `test_game_edit.py` и `test_graphics.py`: parse → serialize побайтно для всех `Konung.N`, `Game.N`/`TGame.N` и `LIGHTS.N`/`TLIGHTS.N` обоих изданий, `Tutorial.4/5` (ENG) с единственной сырой декорацией в слоте 994; ссылки между блоками состояния, стартовая клетка героя и независимая сверка смещений полей персонажа (`0x2A69A`); плотная укладка до и после правок состава, рост резерва, копия NPC со своим отрядом и своими предметами, атомарность правки без места; GRAPH.RES разбирается до последнего байта, спрайты упираются в границы блоков, простые и динамические объекты на месте, таблицы EXE находятся однозначно, кадры огней совпадают с анимациями, текстовые таблицы разложены по строению и сверены с эталоном дайджестом, у каждого расставленного на картах персонажа есть имя, каждый персонаж на каждой карте рисуется. Тесты на данных без `original/` пропускаются (пустой список изданий с сообщением в stderr), но если `original/k1` существует и ни одно издание не нашлось — тесты падают: иначе они «проходили бы», ничего не проверив. На CI `original/` нет, там корректный пропуск.

Графические форматы read-only: побайтного round-trip для них нет, поэтому порт сверялся с Python-эталоном кросс-языковыми дайджестами — RGBA-байты тайлов, кадров спрайтов и банка цветов GRAPH.RES совпали на `original/`.

Текстовые таблицы EXE сверяются тем же дайджестом, но не руками, а тестом: `text_tables_match_the_python_reference` запускает Python (`KONUNG_PYTHON`, затем `.venv` проекта, затем `uv run python`, затем `python3`), и тот печатает число записей и FNV-1a 64 по UTF-8 каждой из четырёх таблиц и дайджест имён всех персонажей из `Game.N`. Rust считает то же по своим байтам и сравнивает. Так в репозиторий не попадает ни одна строка таблицы и ни одна постоянная, из неё вычисленная: в коде теста есть только алгоритм дайджеста. Без `original/` тест не запускает ни одного процесса — список изданий пуст.

Тесты сцены (`crates/game/k1/k1-game/tests/original_scene.rs`, `crates/game/kn-game/tests/original_facade.rs`): позиции объектов совпадают с формулой эталона, куски тайлов покрывают все непустые клетки и несъеденные декорации, оверлеи следуют данным карты (проходимость, свет), список отрисовки детерминирован, все текстуры сцены декодируются, `check` не даёт ошибок и его статистика воспроизводима. Группы кусков: `group_filter_matches_a_scene_without_the_layer` сверяет на всех картах фильтр по группам с сборкой только с включёнными слоями (чистая карта — каждая карта, каждый слой поодиночке — там, где он что-то рисует), `pieces_carry_their_group` — что фасад не теряет группу куска. По анимациям: у анимированного куска базовый кадр — та же картинка в том же углу, что и кусок до анимаций, и именно поэтому число **видимых** кусков с появлением анимаций не изменилось (скрытые добавляются только для слоёв, которых нет в базовом кадре); кадры огня покрывают диапазон вида из EXE подряд и длятся по тику; у анимированного объекта кадров ровно `frame_count`, а длительность кадра — `frame_time` тиков; все кадры одного куска принадлежат одному слою (ключ текстуры не меняется внутри последовательности) — проверка на то, что слои персонажа не перепутались бы позицией в списке. Синтетика в `k1-game::scene` держит правила `layer_track`/`pad_frames`: дырка в кадре остаётся дыркой, один кадр не даёт анимации, слой без картинок не становится куском.

Слой отображения проверяется на живом мире Bevy без окна (`crates/frontend/bevy/render/tests/original_sprites.rs`): у карты со всеми слоями — по узлу на каждую группу в порядке `PieceGroup::ALL`, спрайты стоят на z своих номеров в общем списке (проверяется по `GlobalTransform`: именно по нему рендер сортирует спрайты), кусок с картинкой наследует видимость группы, а `Hidden` на узле гасит только его спрайты — за это отвечает штатная `VisibilityPlugin`, и тест вызывает её, как это делает настоящий рендер. Скрытая группа не теряет `Animator`, а границы видимых групп совпадают с `scene_bounds` тех же слоёв (`group_bounds_cover_the_pieces_a_layer_shows`) — на этом держится пересчёт `CameraBounds` при переключении.

Округление половинок в `character_cell`/`rect_cells` — к чётному (`round_ties_even`), как `round()` в Python: обычный Rust-`round` расходился бы на `.5`.

Проверка записи из AGENTS.md для Rust та же: все файлы обоих изданий после «прочитать → записать» совпадают с оригиналом побайтно.
