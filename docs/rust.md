# Rust-реализация

Cargo-workspace в `crates/` (edition 2024). Повторяет Python-пакет `konung/` слой за слоем; Python — эталон, и при расхождении прав Python (AGENTS.md). Пока Rust ничего не заменяет: Python-CLI и расширение Tiled остаются как есть.

The game-layer crates are grouped under `crates/game/` (`game/k1/k1-data`, `game/k1/k1-game`, `game/kn-game`); the display crates and the binaries move under `crates/frontend/` and `crates/apps/` in the following steps of the layout refactor. Directory grouping changes no package name and no dependency.

## Крейты

| Крейт | Что внутри | Статус |
|---|---|---|
| `k1-data` | данные игры K1: репозиторий (`repo`), форматы карт и состояния (`map`, `game_state`), правки состава (`game_edit`) и графика (`rle`, `palette`, `graph_res`, `objects_res`, `lights_res`, `flames_res`, `executable`, `characters`) | `Konung.N`, `LIGHTS.N`, `Game.N`: разбор и побайтно точная запись; добавление/удаление персонажей сохраняют укладку; `GRAPH.RES`, `Objects.res`, `LIGHTS.RES`, `FLAMES.RES` и таблицы из EXE читаются; кадры персонажей собираются; тесты на `original/` |
| `k1-game` | специфика K1: ресурсы (`resources`), ключи текстур и RGBA-декодирование (`textures`), список отрисовки карты с группами кусков (`scene`) | сцена строится по эталону `konung/k1/render.py` (порядок, сортировка по нижнему краю, тени, огни, персонажи, оверлеи); каждый кусок помечен группой, какой слой её переключает; кадры анимаций собираются из `OBJECTS.RES`, таблицы огней EXE и последовательностей персонажа; headless, без Bevy; тесты на `original/` |
| `kn-game` | the shared facade: `Game` (open a game directory, maps, scene, textures, headless `check`), the Bevy-independent model `Scene`/`Piece`/`TextureId`, `Animation`/`AnimationFrame`, `PieceGroup` and `group_is_shown`; the shared movement engine `movement` (grid, walkability, routes, a step per tick) | hides K1 behind a common API; `movement` knows neither K1 nor Bevy and reads no game files — the part's knowledge arrives through the `Passage` and `Pace` traits; K1 and K2 movement mechanics are presumably nearly identical ([AGENTS.md](../AGENTS.md), "Priority: modern experience"); `CheckReport` serializes to JSON |
| `kn-input` | frontend input mechanisms: `pan_direction` (keyboard), `drag_delta` (mouse drag), `ZoomScroll`/`wheel_steps` (wheel accumulation) | `bevy` only — no camera types, no UI, no game rules; the bindings that wire the mechanisms to a camera live in the apps (the `CameraInput` set of `kn-render`) |
| `kn-ui` | shared Bevy UI mechanics: the Feathers dark theme (`dark_theme`), checkbox mirroring (`sync_checked`), the pointer-capture check (`pointer_over_ui`) | `bevy` only (with `bevy_feathers`) — no rendering, no input bindings, no game rules; app-specific panels and widget state stay in the frontends |
| `kn-render` | Bevy: кэш текстур (`textures`), куски сцены → спрайты по узлам групп (`scene`), покадровая анимация (`animation`), камера без ввода (`camera`) | Bevy 0.20, окно и GPU нужны только для запуска, не для тестов; оверлей — `Visibility` узла группы, а не пересборка карты; `animation` не знает ни о K1, ни о форматах |
| `kn-fe-viewer` | Bevy frontend of the viewer: state (`Viewer`), the Feathers panel, overlays, playback, diagnostics, the camera binding and the App assembly; receives a `ViewerConfig` and knows no CLI | the map is built with all layers at once; toggling an overlay changes group visibility, not a rebuild |
| `kn-viewer` | app `re-konung-viewer`: CLI, startup resolution, headless `check` | `check` creates no Bevy App, window or GPU context and returns a nonzero code on failure; bevy is linked only transitively |
| `kn-fe-player` | Bevy frontend of the player: window, camera binding, start-map loading; receives a `PlayerConfig` | no CLI, no game rules — only the display pipeline and the App assembly |
| `kn-player` | app `re-konung`: CLI, game opening, hero resolution | thin `main`: opens `Game`, resolves the hero, calls `run_player` |

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
kn-ui — the shared UI mechanics — stands beside them: bevy-only as well (with
bevy_feathers), no rendering and no input bindings; the frontends take the
Feathers theme, checkbox mirroring and the pointer-capture check from it
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
- **Интерфейс просмотрщика** — Bevy Feathers (фича `bevy_feathers`): состояние (`Viewer`) отделено от виджетов, обработчики правят только его. `Rebuild` остаётся для одного случая — смены карты: сцена пересобирается, камера наводится заново. Переключение оверлея запрос не взводит: система `sync_groups` переносит флаги `Viewer` в видимость узлов групп (как `sync_playboxes` переносит состояние воспроизведения в чекбоксы), и кадр не обходится в despawn/spawn тысяч спрайтов. Героя просмотрщик не выбирает: `SceneOptions.hero` остаётся `None`, и состояние берётся по источнику карты. `Checkbox` в Bevy считает новое значение как `!Has<Checked>` и сам `Checked` не пишет, поэтому обработчик синхронизирует его вручную (как `checkbox_self_update`) — иначе каждый клик шлёт `true`. Подписи на английском: у встроенного шрифта Feathers нет гарантии кириллицы. Персонажи включены по умолчанию (`DEFAULT_OVERLAYS`), и `Checked` у этого чекбокса должен этому соответствовать. Колесо над панелью карту не масштабирует — по тому же принципу, что и перетаскивание; чтобы блокировка действовала и над пустыми промежутками панели, `Hovered` висит и на её корневом `Node` (Bevy обновляет компонент только у тех, у кого он уже есть, а Feathers кладёт его лишь на свои контролы). The mirroring helper `sync_checked` now lives in `kn-ui` — a single implementation for all frontends.
- **Диагностика считает спрайты в мире.** Панель показывает `видимых/собраных` кусков: карта собирается со всеми слоями, и одно число без другого сказало бы не то, что на экране. Видимым считается спрайт с истинной `InheritedVisibility` — в неё уже входит и скрытая группа, и кадр анимации без картинки, поэтому счёт по детям групп завышал бы первое.
- **Playback and the simulation share one clock.** A single `Play` checkbox (`DEFAULT_PLAYBACK`) drives creature movement; `Space` toggles play/pause and `.`/`,` step one tick. There is no second "animation off" checkbox: the old pair (`Animation` + `Paused`) let turning off the *visual* sprite cycling silently freeze movement, because both the sprite frames (`apply_animators`) and the simulation (`advance_simulation`) read the same `Clock.ticks`. With one control, pausing freezes frames and movement together (an unambiguous pause), and `playback_for`/`toggle_playback` never reach `PlayState::Off` from the UI (`play_toggle_never_stops_the_animation_behind_the_simulation_s_back`). Like an overlay toggle it is state-only: no `Rebuild`, no scene rebuild — `apply_playback` just moves `Clock` to match `Viewer.playback`. `sync_playboxes` mirrors `Viewer.playback == Playing` into the checkbox, because a Bevy `Checkbox` never writes `Checked` itself and the state is also driven by the hotkeys.
- **Reset positions rebuilds the simulation, not the map.** The button is a `FeathersButton`; a click emits `bevy_ui_widgets::Activate`, and `reset_positions` filters its own by the `ResetPositions` marker (the map `FeathersSelect` fires `Activate` too). It recreates the `Simulation` via `Game::simulation(map_ref, SIM_SEED)` — so routes, walk states and the RNG are fresh — and pins `SimTickTracker` to the current `Clock.ticks`, otherwise the fresh simulation would replay the ticks the old one had already burned and lurch forward. It keeps the overlays, selected map, camera/zoom and the pause state, and never touches `Rebuild` or the texture cache: no scene rebuild, no texture reload. Labels are kept — `sync_creature_nodes` repositions them from the simulation each frame, so they settle over the spawn points on their own. The seed is shared with `rebuild_scene` so a reset reproduces the exact first spawn.
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
- **Layers are matched by position in a self-contained list, not a global index (P2 #7).**
  `kn_game::CreatureClips` hands each layer as `CreatureLayer { base, animation }`, a
  layer carrying its resting image and its frames together, so the viewer builds the
  node by the layer's place in the list instead of joining animation to scene pieces by
  an index that shifts when a frame omits a layer. Body and equipment keep the scene's
  pose order in their local z, so equipment reads over the body
  (`body_layers_keep_their_scene_order_depth`).
- **Depth comes from the static stacks only, and the shadow is its own layer (P1 #4, P2 #6).**
  `DepthMap` is built from `PieceGroup::Objects` bottoms (`is_depth_relevant` matches only
  `Objects`), never from characters, terrain or markers — those must not decide what a
  creature stands behind. Equal bottoms collapse to the highest sort key, so two statics
  sharing an edge resolve to the one drawn on top rather than a z between them
  (`depth_map_uses_only_static_object_stacks`, `equal_bottoms_collapse_to_the_highest_sort_key`).
  The shadow sits at a constant local z below every body layer, so it stays under the
  body and rides the node together with it (`the_shadow_sits_below_the_body_layers`).
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
  the sim tick, cell, step from/to, `step_elapsed`, the interpolated pixel and the rendered
  position.
- **A map change and a reset both pin the tracker; a backward `Step` rebuilds and replays (P1 #3, P2 #8).**
  `rebuild_scene` and `reset_simulation` pin `SimTickTracker` to the current `Clock.ticks`
  so a fresh simulation does not re-run the whole tick history in one frame
  (`map_change_pins_the_tracker_so_history_is_not_replayed`). The engine has no reverse
  tick, so `advance_simulation` rewinds by rebuilding the map's simulation and replaying it
  forward to the rewound clock tick; the replay already lands at `clock.ticks`, so the
  tracker is pinned there — pinning it to 0 let the catch-up loop tick the fresh simulation
  a second `clock.ticks` times and every creature overshot
  (`a_backward_step_rewinds_the_simulation_to_the_clock_not_beyond_it`,
  `replay_simulation_matches_forward_stepping`).
- **Spawn refusals and the Characters overlay (P2 #9).** `Simulation::spawn_failures`
  carries every creature the world refused to place with its `WorldError` reason, and the
  headless report prints it instead of dropping it silently. Creature labels hang on their
  own `LabelRoot`, spawned `Hidden`; `sync_labels` shows them only while
  `Overlay::Characters` is on, so turning creatures off also hides their names.
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

## Запуск

```bash
just viewer 17                        # карта 17; без номера — первая карта источника
just viewer 4 tutorial                # карта обучения (есть только в ENG)
just check-maps [tutorial]            # то же без окна, GPU и рендерера — используют CI и агенты
just game                             # заготовка re-konung на стартовой карте новой игры
```

Каталог игры — `original/k1/<издание>/game` (как распаковать — [k1/editions.md](k1/editions.md)); по умолчанию RUS, переопределяется переменной:

```bash
GAME_DIR=original/k1/eng/game just viewer 17
```

Рецепты — обёртка над cargo-командами, их можно расписать и вручную:

```bash
cargo run --release --manifest-path crates/Cargo.toml -p kn-viewer -- \
    --game-dir original/k1/rus/game [--map N] [--source tutorial]

cargo run --release --manifest-path crates/Cargo.toml -p kn-viewer -- \
    --game-dir original/k1/rus/game check --map 1
… check --all [--format json]

cargo run --release --manifest-path crates/Cargo.toml -p kn-player -- \
    --game-dir original/k1/rus/game
```

Код возврата ненулевой, если карта не собралась или её текстура не декодировалась. Оригинальные файлы только читаются.

## Проверки

`crates/` проверяются и локально, и в CI (job `rust` в `.github/workflows/ci.yml`: fmt, clippy, тесты). С появлением Bevy job `rust` компилирует его дерево зависимостей — это минуты, кэш в CI не настроен. На Linux сборке Bevy нужны системные `-dev` пакеты (wayland, x11, ALSA, udev) — в CI их ставит отдельный шаг, список взят из `docs/linux_dependencies.md` в крейте `bevy`; на macOS они не нужны.

```bash
just cargo-test   # тесты: синтетические всегда, на данных — если есть original/
just cargo-lint   # cargo fmt --check + clippy -D warnings
```

The movement engine tests (`crates/game/kn-game/tests/movement.rs`, `movement_grid.rs`) are synthetic: they need neither game data nor Bevy — the grid, routes, a step per tick, a crowd of creatures and determinism are checked on invented walkability fields. `movement_grid.rs` is the only place in the module that knows about `k1-data`: a comparison of the grid formulas with the Python reference and with `game_state` over all 51 200 cells, not a layer dependency. The cost measurement (`movement_cost.rs`) is hidden under `#[ignore]` and is computed only by hand in release (see "Engine cost").

Окно и GPU для тестов не нужны: `kn-game` и `k1-game` Bevy не используют, а `kn-render`, `kn-fe-viewer` и `kn-viewer` проверяют чистую логику — координаты и z-порядок кусков, ступени масштаба, переключатели оверлеев, разбор CLI, отчёт об ошибке, а у анимации — выбор кадра по тикам, накопление дробного остатка, пауза/выключение и пересчёт центра кадра. Сквозная проверка без окна — `check` (см. «Запуск»): на ENG все 43 карты игры и обе карты обучения собираются без ошибок, в release это ~5 с; в строке отчёта, кроме кусков и текстур, видно число анимированных кусков, их кадров и видимых в базовом кадре.

Запись (`serialize`, `serialize_lights`) проверяет точную геометрию сеток до записи, а не только итоговый размер: строки 81 и 79 клеток дают верное число байтов, но сместили бы границы строк. Та же проверка добавлена в Python-эталон (PR #18) — на валидных картах реализации идентичны побайтно.

Тесты на данных (`crates/game/k1/k1-data/tests/original.rs`, `original_game_state.rs`, `original_game_edit.rs` и `original_graphics.rs`) повторяют `tests/k1/test_maps.py`, `test_game_state.py`, `test_game_edit.py` и `test_graphics.py`: parse → serialize побайтно для всех `Konung.N`, `Game.N`/`TGame.N` и `LIGHTS.N`/`TLIGHTS.N` обоих изданий, `Tutorial.4/5` (ENG) с единственной сырой декорацией в слоте 994; ссылки между блоками состояния, стартовая клетка героя и независимая сверка смещений полей персонажа (`0x2A69A`); плотная укладка до и после правок состава, рост резерва, копия NPC со своим отрядом и своими предметами, атомарность правки без места; GRAPH.RES разбирается до последнего байта, спрайты упираются в границы блоков, простые и динамические объекты на месте, таблицы EXE находятся однозначно, кадры огней совпадают с анимациями, текстовые таблицы разложены по строению и сверены с эталоном дайджестом, у каждого расставленного на картах персонажа есть имя, каждый персонаж на каждой карте рисуется. Тесты на данных без `original/` пропускаются (пустой список изданий с сообщением в stderr), но если `original/k1` существует и ни одно издание не нашлось — тесты падают: иначе они «проходили бы», ничего не проверив. На CI `original/` нет, там корректный пропуск.

Графические форматы read-only: побайтного round-trip для них нет, поэтому порт сверялся с Python-эталоном кросс-языковыми дайджестами — RGBA-байты тайлов, кадров спрайтов и банка цветов GRAPH.RES совпали на `original/`.

Текстовые таблицы EXE сверяются тем же дайджестом, но не руками, а тестом: `text_tables_match_the_python_reference` запускает Python (`KONUNG_PYTHON`, затем `.venv` проекта, затем `uv run python`, затем `python3`), и тот печатает число записей и FNV-1a 64 по UTF-8 каждой из четырёх таблиц и дайджест имён всех персонажей из `Game.N`. Rust считает то же по своим байтам и сравнивает. Так в репозиторий не попадает ни одна строка таблицы и ни одна постоянная, из неё вычисленная: в коде теста есть только алгоритм дайджеста. Без `original/` тест не запускает ни одного процесса — список изданий пуст.

Тесты сцены (`crates/game/k1/k1-game/tests/original_scene.rs`, `crates/game/kn-game/tests/original_facade.rs`): позиции объектов совпадают с формулой эталона, куски тайлов покрывают все непустые клетки и несъеденные декорации, оверлеи следуют данным карты (проходимость, свет), список отрисовки детерминирован, все текстуры сцены декодируются, `check` не даёт ошибок и его статистика воспроизводима. Группы кусков: `group_filter_matches_a_scene_without_the_layer` сверяет на всех картах фильтр по группам с сборкой только с включёнными слоями (чистая карта — каждая карта, каждый слой поодиночке — там, где он что-то рисует), `pieces_carry_their_group` — что фасад не теряет группу куска. По анимациям: у анимированного куска базовый кадр — та же картинка в том же углу, что и кусок до анимаций, и именно поэтому число **видимых** кусков с появлением анимаций не изменилось (скрытые добавляются только для слоёв, которых нет в базовом кадре); кадры огня покрывают диапазон вида из EXE подряд и длятся по тику; у анимированного объекта кадров ровно `frame_count`, а длительность кадра — `frame_time` тиков; все кадры одного куска принадлежат одному слою (ключ текстуры не меняется внутри последовательности) — проверка на то, что слои персонажа не перепутались бы позицией в списке. Синтетика в `k1-game::scene` держит правила `layer_track`/`pad_frames`: дырка в кадре остаётся дыркой, один кадр не даёт анимации, слой без картинок не становится куском.

Слой отображения проверяется на живом мире Bevy без окна (`crates/frontend/bevy/render/tests/original_sprites.rs`): у карты со всеми слоями — по узлу на каждую группу в порядке `PieceGroup::ALL`, спрайты стоят на z своих номеров в общем списке (проверяется по `GlobalTransform`: именно по нему рендер сортирует спрайты), кусок с картинкой наследует видимость группы, а `Hidden` на узле гасит только его спрайты — за это отвечает штатная `VisibilityPlugin`, и тест вызывает её, как это делает настоящий рендер. Скрытая группа не теряет `Animator`, а границы видимых групп совпадают с `scene_bounds` тех же слоёв (`group_bounds_cover_the_pieces_a_layer_shows`) — на этом держится пересчёт `CameraBounds` при переключении.

Округление половинок в `character_cell`/`rect_cells` — к чётному (`round_ties_even`), как `round()` в Python: обычный Rust-`round` расходился бы на `.5`.

Проверка записи из AGENTS.md для Rust та же: все файлы обоих изданий после «прочитать → записать» совпадают с оригиналом побайтно.
