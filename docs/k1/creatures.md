# Поведение существ в «Князе: Легенды Лесной страны»

Что известно о том, как игра ведёт персонажей, монстров и жителей: состояния,
приказы, движение и состояние ИИ отряда. Данные — записи персонажей и отрядов из
`Game.N` ([game-state.md](formats/game-state.md)), картинки —
[graph-res.md](formats/graph-res.md) и [objects-res.md](formats/objects-res.md),
названия — [executable.md](formats/executable.md).

Разделен по правилу AGENTS.md: подтверждённое, гипотезы, неподтверждённое, вне
объёма. Подтверждённое — это ссылка на код Slavik **и** проверка на наших файлах
или рендером. Источник без проверки на наших файлах помечен «по Slavik».

Measurements: the numbers of the sections below were taken from the ENG copy
(`original/k1/eng/game`, `Game.0`–`Game.2`: 3179 filled character records and 572
filled squad records). The RUS copy is unpacked here too
(`original/k1/rus/game`); a section that says «both editions» measured each of them
and got the same numbers.

## Подтверждённое

### Состояния анимации 0–9

Младшие 4 бита байта 0x00 — состояние анимации; старший бит того же класса
записи (0x80 в байте 0x04) — мёртв. Пары состояний отличаются тем, держит ли
персонаж оружие в руках (бит 0x04 байта 0x03):

| Состояние | Без 0x04 | Смысл |
|---|---|---|
| покой | 0 | 6 | стоит |
| ходьба | 1 | 7 | идёт по маршруту |
| реакция на удар | 2 | 8 | |
| смерть | 3 | 9 | |
| the shot of a human — for a dynamic creature the same number is its own appearance or transformation animation (#70) | 4 | 4 | отдельное, только с оружием в руках |
| ближняя атака | 5 | 5 | отдельное, только с оружием в руках |

То есть состояния 0–3 — это те же 6–9, но с оружием в руках: `GRAPH.RES` содержит
все десять последовательностей, а динамические объекты (`OBJECTS.RES`, монстры,
женщины и дети) — шесть, и игра сводит 6–9 к 0–3
(`Marisa-Chan/Slavik/game_character.cpp:144-150`: `if (State > CHSTATE_5) State -= 6`).

Выбор варианта делает сама игра по биту 0x04 байта 0x03:

- покой: `game_play.cpp:305-308` (`FUN_00414ab4`) и `game_play.cpp:2465-2468`;
- ходьба: `game_play.cpp:1964-1967` (`FUN_004142d4`) и `game_play.cpp:3944-3947`;
- смерть и реакция на удар: `game_play.cpp:492-495` и `game_play.cpp:504-507`,
  `game_play.cpp:1659-1662`;
- выстрел ставится, когда у персонажа есть второй слот оружия и стрелы
  (`game_play.cpp:2421-2429`);
- ближняя атака — не «дошёл до цели» решением, а следствие шага: персонаж входит в
  неё, когда шаг маршрута выводит его на клетку врага (`game.cpp:2898-2913`), либо
  когда соседний враг уже атакует его (ответный удар, `game_play.cpp:4210-4243`),
  либо для классов засады 0x44 и 0x57 (`game_play.cpp:3200-3220`);
  `game_play.cpp:914-930` — обработчик состояния, а не вход в него;
- имена состояний `CHSTATE_0`–`CHSTATE_9` — `game.h:327-336`.

**Our measurement**, both editions: the recorded animation states are only 0, 4
and 6 (2143, 404 and 632 records). States 2/8, 3/9 and 5 never occur in a file of
a new game — they are what a fight does. The 404 records of state 4 are all dynamic
sprites of the three classes 0x46, 0x4c and 0x55, every one of them with `Frame =
0`, no equipment and an empty quiver, so none of them is a shot: state 4 carries
two different things, and these records belong to the second one («State 4 — two
meanings under one number» under «The combat states»).

**Our measurement**: the weapons-in-hands bit 0x04 of byte 0x03 is set in exactly
the records with state 0 or 4 and cleared in exactly the records with state 6 —
2547 and 632 records. Separately: the 632 records in state 6 are humans without
0x40 in the class (kinds 1, 2, 3 and 5), while every record with a dynamic sprite
holds its weapon in hands. This is the confirmation of the pair «0–3 with a weapon,
6–9 without it» on our own data, not only by Slavik.

Entry, play mode, duration and exit of 2/8, 3/9, 4 and 5 — «The combat states»
below, with the frame counts of our own sprite tables.

### Флаги приказов и решений (байты 0x02 и 0x03)

Байт 0x02 — приказы игрока и отметка, что персонажу нужно решение поведения:

| Бит | Смысл | Источник |
|---|---|---|
| 0x01 | приказ атаковать | `game_play.cpp:550`, `game_play.cpp:2489`, `game_play.cpp:3289`; снимается в `game_play.cpp:3335` |
| 0x02 | следовать за ведущим | ставится членам отряда при приказе в `game_input.cpp:395-397`, снимается `game_play.cpp:969`, `game_play.cpp:2994` |
| 0x04 | the character is shooting (a bow and arrows are enough) | `game_play.cpp:2421-2431`, `game_play.cpp:3918-3954`; cleared in `game_play.cpp:2953`, `game_play.cpp:3912`. **Confirmed behavior**: the clone never gives the bit a name, but only the shooting branch sets and reads it — «State 4 — two meanings under one number» below |
| 0x08 | приказ переместиться | `game_input.cpp:395` и `game_play.cpp:1943` ставят 0x48, снимается `game_play.cpp:826`, `game_play.cpp:3550` |
| 0x20 | ведомый член отряда игрока | ставится, если персонаж не главный, в одном отряде с игроком и без приказов (`game_play.cpp:302-303`) |
| 0x40 | нужно решение поведения | ставится почти в каждой точке, где состояние могло измениться (`game_play.cpp:300`, `game_play.cpp:831`, `game_play.cpp:2458`, `game_play.cpp:3051`, `game_play.cpp:3149`), снимается, когда решение принято (`game_play.cpp:1962`, `game_play.cpp:3949`) |

Младшие 4 бита — сами приказы: «приказов нет» игра проверяет как
`(поле & 0xf) == 0` (`game_play.cpp:302`, `game_play.cpp:774`,
`game_pathfind.cpp:37`), а все они снимаются разом маской `&= 0xf0`
(`game_play.cpp:220`, `game_play.cpp:269`).

Байт 0x03 — про оружие в руках и про то, что уже сделано с картинкой:

| Бит | Смысл | Источник |
|---|---|---|
| 0x04 | оружие в руках | выбирает пары состояний (см. выше) и набор спрайтов снаряжения: без него снаряжение рисуется по порядку последних 8 строк таблицы EXE (`game_character.cpp:167-169`, `EqLookUp1`) |
| 0x40 | чистое перемещение: дойдя до клетки, персонаж не атакует и не подбирает | `game_play.cpp:812-824` |
| 0x80 | спрайт динамического объекта ещё не загружен | ставится при высадке монстра (`game.cpp:687`, `game.cpp:744`), снимается, когда объект загрузился, и кадр пересчитывается (`game.cpp:1036-1042`); пока бит стоит, картинка считается однопиксельной (`game_character.cpp:157-160`) |

**Наш замер**: в начальных состояниях байт 0x02 равен 0x40 (3176 записей) или
0x60 (3 записи), байт 0x03 — 0x04 (2547) или 0x00 (632). Битов 0x01, 0x02, 0x04 и
0x08 в байте 0x02, а также 0x40 и 0x80 в байте 0x03 в файлах новой игры нет: это
всё появляется в бою.

### Маска пропуска тиков при ходьбе (байт 0x11)

Шаг маршрута персонаж делает не каждый тик: в ветке состояний 1 и 7 он
обрабатывается, только если `маска == 0 || (счётчик & маска) == маска`
(`game_play.cpp:3503`, поле `field15_0x11` — `game.h:460`, читается из файла в
`game_state.cpp:24`). Это число движок оригинала использует, чтобы медленные
существа шли реже остальных.

**Наш замер**: во всех заполненных записях персонажей `Game.0`–`Game.2` байт 0x11
равен 0, то есть каждый тик идёт без пропуска.

Значит правило «идти каждые N тиков» в наших данных сводится к «идти каждый тик»,
и реализовывать маску отдельно смысла нет — но поле надо сохранять при записи.

### Состояние ИИ отряда (байт 0x1D)

**По Slavik**: игра читает байт из файла (`game_state.cpp:230`), но для
бродячих отрядов (вид с битом 0x40, 0x41–0x50) пересчитывает его каждые 16 тиков
(`game_play.cpp:1623-1638`, `_counter & 0xf == 0`) в один из трёх смыслов
(`game_play.cpp:1024-1062`): 1 — кто-то из отряда игрока внутри прямоугольника
появления, 0x20 — член отряда вне его, 0 — спокойно. Отряду старосты деревни то же
поле ставят в деревенском проходе, на тике, сдвинутом на 7 (`game_play.cpp:1696`,
`game_play.cpp:1533-1561`). Остальные отряды своего значения не пересчитывают, а
читают из него только младший бит: `unk4 & 1` — по нему панель персонажа показывает
работу жителя или его подсказки (`game_character.cpp:755`, `game_character.cpp:757`).

**Наш замер**: в начальных состояниях поле равно 0 (3 отряда) или 0x80 (569
отрядов). Ни один путь в коде Slavik 0x80 не ставит, и в наших файлах это значение
встречается только у отрядов жителей (виды 0x80–0x82) и NPC (0x53), которых игра
не пересчитывает. Что оно значит — **неизвестно**; по младшему биту (`& 1`) оно
ведёт себя как 0, то есть «не в режиме».

Поэтому: значение из файла — это состояние игры на старте, а не настройка, и
полагаться на него нельзя: у бродячих отрядов первые же 16 тиков его перепишут.

### The villager decision (confirmed 2026-11-03)

Confirmed against the reference and implemented with tests
(`k1-game/src/rules.rs`, `villager_decision`; synthetic cases and both
editions of `Game.0`–`Game.2`, `k1-game/tests/original_rules.rs`). The game
calls the peaceful decision for members of a villager squad — group kind
0x80–0x82, dispatch at `Marisa-Chan/Slavik/game_play.cpp:737-757` — through
`FUN_00416934` (`game_play.cpp:2288-2385`), and only when the character needs
a decision: flag 0x40 of byte 0x02 is set on arrival and cleared once a
decision was made (`game_play.cpp:2834-2846`, `3477-3489`). A dead character
(class bit 0x80, state 3 or 9) and a walking one take no decision. The order
of the branches matters:

1. **Woman (class 0x51) and child (0x52)**: one roll of `rand() % 100`; below
   75 the creature stands, otherwise it walks (branch 5) — a quarter walks
   (`game_play.cpp:2294-2309`).
2. **Static work from bits 3–5 of the class**: `job = (class >> 3) & 7` names
   a village slot; a village has exactly five slots and index 0 is the elder
   (`game.h:702`, and our check: every village in the files carries five job
   records, `docs/k1/formats/game-state.md`). An empty slot
   (`Jobs[job].CharID == 0`) is passed over; the original also clears the work
   bits in memory (`game_play.cpp:2340`), we do not touch the record — the
   branch depends only on data that never changes mid-run, so the next tick
   takes the same path (our difference). A filled slot sends every *other*
   creature of the village to it (`game_play.cpp:2323-2338`): unless the
   creature stands on one of the eight neighbours of the place's cell (the
   direction lookup `FUN_0041165c`, `game_play.cpp:1973-1983`; standing on the
   place itself is not adjacency), it is sent to the first free neighbour of
   the place in direction order 0..7 (`FUN_00410ffc`,
   `game_play.cpp:1985-1992`); once adjacent, it faces the place's direction
   byte, and once facing — it stands. The branch never falls through.
   - **Our measurement**: no creature of the shipped starting data reaches
     this branch — of the villager classes 1, 2, 3, 5, 0x45, 0x51, 0x52 the
     first five carry zero work bits and 0x51/0x52 leave through the woman and
     child branch first. The branch is covered by synthetic tests only.
   - Bits naming slot 5, 6 or 7 read past the five-slot array in the original
     (`std::array<Job, 5>`); with us they are no static work either way (our
     difference).
   - When no neighbour of the place is free, `FUN_00410ffc` returns `-1`,
     which the original treats as success and walks the creature to an
     uninitialized point (`game_play.cpp:2327-2331`); we give no order (our
     difference).
   - The original's free-neighbour test checks terrain only; we also require
     that nobody stands there (our difference).
   - The original turns by one direction per tick (`FUN_004391bc`,
     `game_play.cpp:1995-2013`, listed under «Вне объёма»); our
     `Decision::Turn` faces the creature in one step.
3. **The dynamic workplace**: the village slot whose `CharID` equals the
   creature's record number — `GetVillageCharacterJob`
   (`game_village.cpp:522-536`) scans the five slots in order and returns the
   first match as `i + 1`. Standing on the place's cell: face the place's
   direction when the facing differs, else stand; anywhere else: move exactly
   onto the place's cell (`game_play.cpp:2354-2381`).
   - **Our measurement**: `Job.character` is the 0-based record number
     (the village of map 19 points at records 181–185, and group 19 starts at
     character 181), and `0` marks the empty slot — the very marker the rules
     test — so the player's hero, record 0, never owns an open slot. In the
     starting data every job holder stands exactly on its place's cell, in
     state 6, and already faces the place's direction: over 41 maps ×
     `Game.0`–`Game.2` of both editions the rules turn nobody
     (`k1-game/tests/original_rules.rs`), so the turn path is covered by the
     synthetic tests alone.
4. **No work at all**: one roll of `rand() % 100`; below 50 the creature
   stands — half walk (`game_play.cpp:2346-2348`).
5. **The walk**: a free cell strictly inside the squad's spawn area
   (`left < x < right`, `top < y < bottom`), up to ten attempts; no attempt
   lands free, or the area has no interior, and there is no order.
   - The original does not pick inside the area at all: it offsets from the
     character's own cell by at most `ViewTiles * 2`, and `ViewTiles` is the
     viewport size in tiles (`game_play.cpp:1889-1949`, `game.cpp:921-922`).
     Behavior must not depend on the window size, so we pick uniformly inside
     the spawn area, drawing `x` and then `y` per attempt (our difference).
6. The original's first decision for a villager with a weapon in hands is
   putting it away (bit 0x04 of byte 0x03 → state 6, `game_play.cpp:2312-2317`).
   That is an animation-state change, not a movement; the rules layer owns no
   animation state, and we do not implement it.

**Our measurement** of the village shape, both editions: one village per map,
15 villages on 15 distinct maps; `village.elder_group` is the squad slot, and
it is the map's only villager squad — every villager of a map is a member of
it (map 19 → squad slot 19, kind 0x80, 19 members). The earlier note that
among workplaces `character` is filled only for the elder is wrong: map 19
fills all five slots, map 14 only the elder.

### The group state of a roaming squad (confirmed 2026-11-03)

Confirmed and implemented with tests (`k1-game/src/rules.rs`, `group_states`;
synthetic cases and both editions). For a roaming squad (group kind 0x41–0x50,
`FUN_00412c30`, `game_play.cpp:1024-1062`):

1. any alive member of the player squad inside the squad's spawn area, bounds
   included (`game_play.cpp:1028-1042`) — state 1: combat mode; the game also
   writes the player squad's number into the squad record at the same moment
   (`game_play.cpp:1038`, byte 0x1A);
2. else any alive member of the squad itself standing outside the area —
   state 0x20: the strayed members return, «The monster decision» below;
3. else state 0, calm.

«Alive» is the original's own test: class bit 0x80 clear, state neither 3 nor
9. Every other squad — villagers, the player squad, lone NPCs — takes no
recomputed state and reads calm. The byte stored in the file (0x1D,
`ai_state`) is never read: the game zeroes it for roaming squads on map load
(`game.cpp:645`), and in the shipped files it is the unknown 0x80 for every
squad («Состояние ИИ отряда» above).

Deliberate differences from the original:

- the state is recomputed every tick, not every 16 (`game_play.cpp:1623-1638`
  — a cost cap of 1999, listed under «Вне объёма»);
- the original's "outside" test for its own members is tangled by `&&`/`||`
  precedence into `y < top` alone (`game_play.cpp:1050-1052`); we use the
  plain negation of the inclusive inside test;
- the elder squad's byte is set in the village pass from village flags and the
  player's weapons (`FUN_00416700`, `game_play.cpp:1516-1562`, `4451`), not
  from positions. Until the village rules are recovered, a villager squad is
  calm with us — our villagers never turn hostile; this is an open question,
  not confirmed behavior (see «Не подтверждено»).

### The monster decision (the return is confirmed; combat is not 2026-11-03)

Confirmed and implemented for the return — `FUN_00412e00`
(`game_play.cpp:3156-3181`, `3286`, `3337-3392`), tested at the synthetic and
real-data level. The dispatch gives it to members of a roaming squad (group
kind bit 0x40, `game_play.cpp:737-745`); classes 0x51, 0x52 and 0x53 return
from the decision untouched (`game_play.cpp:3163-3164`). Dead and walking
characters take no decision, as with villagers. By the squad state:

- **Calm (0)**: stand. The original only resets the animation state
  (`game_play.cpp:3166-3177`).
- **Player inside (1)**: combat — target choice, attack and shooting are not
  reverse-engineered, so no order is given rather than one invented
  (`game_play.cpp:3179-3283`).
- **Strayed (0x20)**: a creature already inside the spawn area, bounds
  included, stands. Outside, it returns: take the centre
  `((left + right) / 2, (top + bottom) / 2)`, with `rh = bottom - top - 1`
  and `rw = right - left - 1`; one coin `rand() % 2` picks *subtract* (0) or
  *add* (1) for both axes together; then `rand() % (rh / 4)` to `y`, then
  `rand() % (rw / 4)` to `x`, each drawn only when its divisor reaches two
  (`game_play.cpp:3353-3375` — the draw order is the coin, then `y`, then
  `x`, and we keep it for reproducibility). From that point the area is
  scanned over at most `rh * rw` cells: the first cell with empty foot
  terrain becomes the target; otherwise one step east with wraps — past
  `right - 1` the row restarts at `left + 1`, past `bottom - 1` at `top + 1`
  (`game_play.cpp:3377-3391`). Nothing free — stand.
- Our difference: the original tests "inside the area" with `x >= r.top`
  where `r.left` is meant — the reference itself marks it as an error
  (`game_play.cpp:3339`); we use `r.left`. The original's scan checks terrain
  only; our `Ground` also requires that nobody stands in the cell.

**Our measurement**, both editions: the cells of roaming members in the file
are not where the game puts them — the map load re-places the squad («Roaming
squad placement on map load» below) — so recomputing from the file cells flags
`MemberOutside` for thousands of squad members (2 123 across the runs of
`original_rules.rs` per edition), and every return order the rules give lands
strictly inside the squad's spawn area and on a free cell; the test asserts
that on all 41 maps per edition that carry creatures.

### Roaming squad placement on map load (confirmed 2026-11-04)

Confirmed against the reference and implemented with tests
(`k1-game/src/placement.rs`, `place_roaming`; synthetic cases in the module
and both editions in `k1-game/tests/original_rules.rs`,
`roaming_squads_are_re_placed_on_free_cells_inside_their_areas`). On map load
the game does not leave a roaming squad (kind 0x41–0x50) at the cells its
records carry. Squads are processed in `MapChar_ARRAY` order — the group
table as the file holds it, group slots ascending (`game.cpp:637`,
`game_state.cpp:364-368`) — and the members of one squad in file order, the
contiguous `Characters` slice at `CharacterIndex + j` (`game.cpp:649`); every
member is re-placed near the centre of the squad's spawn area
(`game.cpp:647-656`, `game_character.cpp:328-360`):

- the candidate: from the centre `((left + right + 1) / 2, (top + bottom + 1)
  / 2)` — the cell the static render already draws for the squad's first
  member — with the original's draw order: a `y` distance `rand() %
  ((bottom - top) / 2)` drawn only when that divisor reaches two, then a coin
  `rand() % 2` picking subtract or add, then the `x` distance and its coin
  (`game_character.cpp:334-354`); the spans are exclusive (`Width() = right -
  left`, `common.h:187`), so a candidate of a span of two or more never
  leaves `left + 1..right - 1` — arithmetic, not a measurement; the
  placement test asserts containment of every squad of both editions, and a
  degenerate point area can only hold its own cell (map 37 carries one);
- the check: up to 100 attempts; a candidate is accepted when its foot cell
  is free — `FootMap(Tile).ID == 0`, which counts cells taken by members
  placed earlier on this load (`game_character.cpp:356-357`);
- no free cell in 100 attempts: `GroupSize = j` and the squad stops placing —
  that member and every later one are out of the game (`game.cpp:654-655`);
- a dead record is placed like everyone else — the original places first and
  asks who is alive afterwards — but it marks the ground only when alive, so
  its cell can be given twice («по Slavik», `game.cpp:660-664`).

Our differences, all three deliberate:

- the cells of the creatures that are not re-placed (villagers, lone NPCs,
  the player squad) are held from the start, and so is every cell a squad has
  already been given; the original marks the ground group by group in file
  order, so an earlier squad can be placed onto a later one's villager — with
  us what stops depending on the visit order is where a creature that is not
  re-placed sits in it, not the order of the squads: one shared random stream
  and sequentially taken cells make the squad order visible, and we mirror
  the original's (`game.cpp:637`);
- a member may not share a cell with anybody, living or dead (see above);
- the generator is ours (`kn_game::movement::Rng`, splitmix64), asked with
  the original's draw order, so a run with a fixed seed replays.

**Our measurement**, both editions: 379 squads, 2284 members across the 41
maps and `Game.0`–`Game.2` — every member finds a free cell inside its area
within the 100 attempts and no squad of the shipped data is ever trimmed; the
same seed places the same map twice (`original_rules.rs`). The re-placement
also explains the file cells: they are the squad's park position, not where
the game stands it (see the measurement under «The monster decision»).

### Load-time refusals (confirmed 2026-11-05, #82)

**Our measurement** — after the load rule above stands every roaming squad,
`simulate --all` of the viewer still cannot put eleven records of `Game.0`
into the world, identically in both editions: map 7 record 433 with
`CellTaken` (a villager shares its cell with another record — the same fact
the harness test pins as `Denied::SharedCell`), and records 507 (map 10),
513 (17), 517 and 518 (25), 520 (30), 838 (33), 482 (37), 525 (42), 527
(44), 529 (47) with `ImpassableCell` — the file stands each of them in
blocked foot data. These are facts about the shipped files, not engine
faults, and the diagnosis of #82 says which of the three suspects they are
**not**: not a passage divergence (the façade's walkability is the confirmed
foot rule alone, `FootMap(Tile).ID == 0`, as the harness test's `Foot` is),
not an edition difference (the two lists are equal cell for cell), not a
placement gap — the eleven carry no roaming group and `PlaceMob` never
touches them (the load loop calls it under `!(kind & 0x80) && kind < 0x51`
only, `game.cpp:647-656`; `game_character.cpp:328-361` is the placement the
roaming members got instead). What the original does with a record whose
cell cannot hold it is **unknown**: the clone keeps such a creature at its
record cell in its own arrays and draws it there; the engine's own check we
have not recovered. We keep them out of the movement world, report each with
its reason, and give the renderer the record cell to draw them at — the
viewer's report prints them as data, and only a failed order or a build
failure moves the exit code (`apps/viewer/src/lib.rs`, the shipped test
`tests/original_simulate.rs` holds the exact list for both editions). A trim
(`GroupSize = j`, above) is reported the same way; the shipped data never
produces one, so the shipped test treats a trim as a regression of the load
rule, not as data.

### Goal reach (confirmed 2026-11-05, #82)

The report counts, per creature, how often a completed order landed the
creature exactly on the cell the rules had commanded — counters of movement
events (`Arrived` with the cell equal to the last accepted `MoveTo`
target), never a position log. Only targets of the confirmed rules are
counted, which are the villager's two (`docs/k1/creatures.md`, «The
villager decision», [cf5a332](https://github.com/Marisa-Chan/Slavik/tree/cf5a3320dc93859e3dc51f37ee823fed30f399b6)
`src/game/game_play.cpp:2288-2385`):

- **workplace** — arrival exactly on the place cell of a job slot the
  creature itself holds (the dynamic branch), or on a free neighbour of the
  place cell of the job its class bits 3–5 name while that slot is occupied
  (the static branch, `rules.rs::static_work`);
- **walk** — arrival on a cell strictly inside the creature's spawn area,
  the band `left + 1..=right - 1` the jobless half roll picks from
  (`rules.rs::walk_target`).

Anything else an order could aim at — a monster's return to the centre, say,
or the neighbour a blocked goal fell back to — earns no counter until its
rule is confirmed. **Our measurement**, `simulate --all` at seed 42 over
5000 ticks, both editions and identical there: eleven village maps produce
walk arrivals (11 to 20 per map — the jobless roll is the only order the
shipped data ever completes on them); workplace arrivals are rare because
every holder of a job slot stands on its place cell already at load — the
single exception across all maps is map 1 record 369, a class 0x52 villager
whose bits 3–5 name the merchant: its slot is occupied by record 357, who
stands on the place cell (30, 115) himself, and the arrival is the free
neighbour the order sent 369 to. `tests/original_simulate.rs` pins the map
19 counters and this arrival. The counters ride the seeded generators only
(placement draws on `Rng::new(seed)`, decisions on `Rng::new(seed ^
0xDEAD_BEEF)`), so the same seed replays them identically — the shipped test
asserts the equality of two full runs.

### The combat states (researched 2026-11-05, #71)

Recovered over the read-only clone plus our own measurements. Every cite is a
permalink pinned to the commit
[`cf5a332`](https://github.com/Marisa-Chan/Slavik/tree/cf5a3320dc93859e3dc51f37ee823fed30f399b6);
the clone's code is never copied. Markers: **confirmed** — the clone's code and
our check of our own files agree; **by Slavik** — only the clone's code;
**hypothesis**; **unknown**.

**Where the machine runs.** The animation state is byte 0x00 of a character
record. Once per game tick — 60 ms, [`game.h:30`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.h#L30)
— the game walks the squads of the current map in the group order of the file and
runs one of three copies of the same ten-state switch: `FUN_004123bc` for roaming
squads
([`game_play.cpp:3468-3726`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3468-L3726)),
`FUN_00416058` for villager squads
([`game_play.cpp:2824-3029`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2824-L3029))
and `FUN_00421da4` for everyone else — the player's squads and lone NPCs
([`game_play.cpp:737-990`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L737-L990)).
The copies differ in what a state ends in, which is why the table below names the
pass. The speed option runs extra passes per frame
([`game_play.cpp:122-135`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L122-L135)),
so a combat state is measured in ticks, not in screen frames.

**The frame engine.** Every state change goes through one setter, `FUN_004143dc`
([`game.cpp:2766-2915`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L2766-L2915)):
it loads the sequence of the pair (state, direction), puts `Frame = 0` and plays
the state's sound (states 2/8 and 3/9 have their own, a human shot plays a bow
sound, and a dynamic-object creature gets none for state 4 — by Slavik). One tick
advances exactly one frame: `FUN_00413de4`
([`game_play.cpp:3727-3786`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3727-L3786))
raises `Frame`, and when the frame would pass the end it steps back and returns
false — that false is the only signal a caller gets that the sequence is over.
Nothing loops on its own: idle and walk restart the same state, the hit reaction
and the shot hand the creature to the idle chooser, death holds its last frame
forever. A sequence of `N` frames shows frames `0 … N-1` over `N` ticks and the
caller leaves the state on the next tick.

| State | Entry | Play mode | Length, our measurement | Exit | The beat that matters |
|---|---|---|---|---|---|
| 2/8 reaction to a hit | a blow that lands and does not kill: an arrow result 0, a melee result 0 (confirmed) | one-shot (confirmed) | human 7 frames = 420 ms; monsters 4–17, three sprites have none | idle chooser; a hit monster first puts its whole squad into combat mode | a second hit re-enters and restarts frame 0 (by Slavik) |
| 3/9 death | the result code 1 of a hit, or poison draining health to zero (confirmed) | hold (confirmed) | human 12 = 720 ms; monsters 7–16, one sprite has none | none — the last frame is held | the corpse is committed at the second-to-last frame, not at the last |
| 4 — the shot of a human | the shoot loop: a bow in equipment slot 1 and a quiver (confirmed) | one-shot; the loop re-enters it while its flag stands (confirmed) | human models 12 = 720 ms; no record of a new game reaches it — nobody carries arrows | idle chooser, or the shoot loop re-enters | the arrow is created at frame `FrameCount − 6`, and the state turns to face while `Frame == 0` |
| 4 — a dynamic creature's own animation | the creature's appearance or transformation: a skeleton rising instead of dying (confirmed); the 404 records of classes 0x46, 0x4c, 0x55 (confirmed) | one-shot (confirmed) | the five dynamic objects that carry the sequence: 5 → 13, 11 → 17, 17, 21, 22 → 12 | idle chooser | it is not a shot: those records hold no bow and no quiver, and the clone's monster branch creates no projectile there (unknown) |
| 5 melee | a walk step into the cell of an enemy, a counter-attack against an adjacent attacker, the ambush classes 0x44/0x57 (confirmed) | loops while the enemy stands (confirmed) | human 12; monsters 10–16, three sprites have none | the enemy died or became unreachable | health is written when the swing has fully played; the victim's squad is marked at frame 2 |

**State 2/8 — the reaction to a hit.** Four call sites, all of them the result
code of a blow, never a decision: an arrow that did not kill
([`game_play.cpp:504-508`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L504-L508))
and a melee blow that did not kill, one per pass
([`game_play.cpp:976-980`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L976-L980),
[`game_play.cpp:3000-3006`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3000-L3006),
[`game_play.cpp:3701-3706`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3701-L3706)).
The result codes come from the two damage helpers and they decide the state:
`1` kills, `0` staggers, `-1` is a blow so light (`hit < 48`, three displayed
points) that the victim keeps doing what it was doing, `-2` is a dodge or a block
and changes nothing at all
([`game_play.cpp:2699-2712`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2699-L2712),
[`game_arrows.cpp:42-55`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_arrows.cpp#L42-L55)).
The pair is chosen by the weapons-in-hands bit, so an armed creature takes 2 and
an unarmed one 8. The state re-enters through the same setter on every new hit,
which resets `Frame` to 0 — a stunned creature that is hit again starts its
stagger over (by Slavik). The exit is the idle chooser of the pass: `FUN_00414ab4`
for the player's squads
([`game_play.cpp:848-861`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L848-L861)),
`FUN_00410450` for a roaming squad — after that pass has set the squad state to 1,
so **a hit monster's stagger ends with its whole squad in combat mode**
([`game_play.cpp:3570-3579`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3570-L3579))
— and `FUN_00410728` for villagers, after they raised the village hostility flags
and aggroed the elder's squad
([`game_play.cpp:2897-2906`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2897-L2906)).
Marking a squad is its own helper: `FUN_004248a0` sets the squad byte to 1 and
cuts the route of every member that has no attack order, so the squad stops
walking where it stands
([`game_play.cpp:2555-2564`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2555-L2564)).

**State 3/9 — death.** Entered only by a result code 1: a melee kill
([`game_play.cpp:955-973`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L955-L973),
[`game_play.cpp:2981-2998`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2981-L2998),
[`game_play.cpp:3679-3698`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3679-L3698)),
an arrow kill
([`game_play.cpp:483-499`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L483-L499))
or the per-tick poison drain that empties health
([`game_play.cpp:1332-1350`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L1332-L1350)).
The killer writes health 0 itself and clears the orders byte of the victim. While
the death sequence plays, the creature is still alive as the game counts it; the
corpse is committed when `Frame` reaches `FrameCount − 2` — the orders byte is
cleared, the dead bit of the class is set and the foot cell is released
([`game_play.cpp:863-883`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L863-L883),
[`game_play.cpp:2909-2925`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2909-L2925),
[`game_play.cpp:3581-3605`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3581-L3605)).
After the last frame nothing advances it: the frame engine refuses to pass the end
and the case refuses to call it, so a corpse holds its last frame and leaves the
cell free. The one exception is the skeleton (class 0x4c): at the same second-to-last
frame, if it still has a weapon in a slot above 0, health is restored to 1600, the
weapon slot steps down and the creature enters **state 4** instead of dying
([`game_play.cpp:3586-3594`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3586-L3594)).

**State 4 — two meanings under one number.** One state value carries two things
that share nothing but the number, and the data never lets one be read as the
other: the shot of a human who shoots a bow, and the appearance or transformation
animation a dynamic creature plays for itself. Which of the two a creature uses
follows from what it is, not from a decision, which is why the table above gives
them two rows and the text below keeps their two paths apart.

*The shot of a human.* The state is only reachable by a creature that holds a
weapon in hands: the human shooting branch requires a bow in equipment slot 1 and
a quiver
([`game_play.cpp:2421-2431`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2421-L2431),
[`game_play.cpp:3808-3824`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3808-L3824)),
and it is what byte 0x02 bit 0x04 gates — `FUN_0041ed8c` runs the shoot loop while
that bit is set, re-targets, and re-enters the state as long as the bow, the
arrows and the target hold, clearing the bit the moment one of them is gone
([`game_play.cpp:3952-4016`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3952-L4016)).
The *name* of this bit the clone never gives, but its use is exactly the shooting
flag, so we treat it as **confirmed behavior** (row 0x04 of byte 0x02 above).
While `Frame == 0` the shooter turns toward the target one
direction per tick and does not advance the animation; at `Frame == FrameCount − 6`
the arrow is created and the quiver count drops
([`game_play.cpp:3622-3648`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3622-L3648),
and the same shape in the player and villager passes at
[`game_play.cpp:885-912`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L885-L912),
[`game_play.cpp:2927-2957`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2927-L2957)).
`FUN_0041bff8` is the only place a projectile is created — at most 100 in the air,
and it carries the bow's damage and the quiver's poison
([`game_play.cpp:2568-2623`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2568-L2623)).
An arrow is a trace, not a homing shot: each tick it steps eight times, dies on
impassable terrain (and, if it was aimed at a village building, sets it alight with
one roll of 21%), and damages whoever's cell it passes over **except the shooter
himself** — the shot has no friendly-fire filter
([`game_play.cpp:394-526`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L394-L526)).
The state ends into the idle chooser; the shoot loop may immediately re-enter it.
This is the human-shot path, and the whole of what #77 (the launch of a shot) owns.

*The appearance or transformation animation of a dynamic creature.* A dynamic
sprite enters the same numeric state 4 for something that has nothing to do with
shooting: a sequence its class plays to come onto the board or to change shape. The
skeleton (class 0x4c) is the confirmed case — instead of finishing its death it
restores health, steps its weapon slot down and re-enters state 4 to rise again
([`game_play.cpp:3586-3594`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3586-L3594))
— and it is what the shipped files hold: the only 404 state-4 records of a new game
belong to the three dynamic classes 0x46 (152 records, dynamic object 5), 0x4c
(249, object 11) and 0x55 (3, object 22); none of them is human, every one carries
`Frame = 0`, a weapon slot above 0, no equipment and an empty quiver («Our
measurements»). The two uses are disjoint by data, not by convention: the human
branch above needs a bow in slot 1 and a quiver, and these records hold neither, so
not one of them can be a shot. What each sequence shows — rising from the ground,
jumping out of a burrow, a human↔beast change — is the content of the
dynamic-object sequences and belongs to the viewer's #70, not to combat: **#77
launches a shot on the human path only and must never route a dynamic creature's
state-4 sequence through the shoot loop.** Whether the monster branch that plays
state 4 for these classes also releases an arrow is **unknown** («What stays
unknown»), so that branch stays unimplemented rather than folded into #77.

**State 5 — the melee blow.** Three entries, none of them an order. A character
walking a route that steps onto a cell held by an enemy it may hit takes the state
inside the state setter itself, which is also where it draws its weapons
([`game.cpp:2898-2913`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L2898-L2913));
a creature with no move or attack order that finds an enemy of another squad next
to it *and already attacking it* turns and swings back
([`game_play.cpp:4210-4243`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L4210-L4243));
and the ambush classes 0x44 and 0x57 enter it from the monster decision when a
player-squad member stands in one of the eight neighbours
([`game_play.cpp:3200-3220`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3200-L3220)).
Before the swing the attacker faces the target (`FUN_0043976c` picks the direction
that sees the enemy from the cell it stands on,
[`game_play.cpp:2625-2657`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2625-L2657)).
At `Frame == 2` the victim's squad is marked (and, if the victim belongs to a
village, its hostility flags go up); the health write happens only after the whole
swing has played, and then the attacker restarts the same state, so it keeps
swinging while the enemy stands
([`game_play.cpp:914-987`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L914-L987),
[`game_play.cpp:2959-3020`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2959-L3020),
[`game_play.cpp:3652-3723`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3652-L3723)).
It stops when the enemy is dead or the attacker can no longer reach it: the
per-pass guards `FUN_0041f258`, `FUN_00415cd0` and `FUN_00411fd0` drop
`EnemyCharID` and hand the creature to the idle chooser; for a monster the same
guard also abandons the chase when the target leaves the squad's leash rectangle
([`game_play.cpp:3088-3105`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3088-L3105))
— the pursuit itself is #72. Class 0x49 is the one attacker that steps away after
a blow instead of swinging again
([`game_play.cpp:3713-3720`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3713-L3720)).

**Orders, decisions and the budget.** No order interrupts a playing combat
animation. A player order only writes the order byte and the route — the move
command sets `0x48` and truncates the path without touching the state
([`game_input.cpp:392-400`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_input.cpp#L392-L400))
— and the order is picked up after the animation ends, because all three exits of
the combat states set the needs-a-decision flag 0x40 of byte 0x02 on the way out
(`FUN_00414ab4`, `FUN_00410450`, `FUN_00410728`,
[`game_play.cpp:296-313`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L296-L313),
[`game_play.cpp:3049-3061`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3049-L3061),
[`game_play.cpp:2456-2470`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2456-L2470)).
The decision gate of the roaming and villager passes explicitly excludes the
combat states — states 2, 3, 9 for a monster and 2, 3, 8, 9 for a villager take no
decision at all
([`game_play.cpp:3474-3486`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3474-L3486),
[`game_play.cpp:2830-2843`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2830-L2843)).
That exclusion also has a cost side: once the budget of three expensive decisions
per tick is spent, a creature whose decision is pending and whose state is not idle
or walk is skipped *for the whole tick, animation included*
(`SkipBecauseOfHardCompute`,
[`game_play.cpp:3787-3792`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3787-L3792)),
which is one of the 1999 caps under «Вне объёма» rather than a rule to keep. State 4
is the one state the monster decision touches directly: for the classes 0x46, 0x4c
and 0x55 it advances the first frame itself, because the state-4 branch of the
monster pass — the clone names it a shot, one of the two meanings above — does
nothing while `Frame == 0`
([`game_play.cpp:3229-3236`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3229-L3236),
[`game_play.cpp:3607-3619`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3607-L3619)).

**Where health lives.** Health is the s16 at 0x5C of a character record — the
fifth of the nine attributes that start at 0x54
([`game_state.cpp:47`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_state.cpp#L47),
our row 0x54 in [game-state.md](formats/game-state.md)). The scale is 0…1600 with
100 displayed points: the interface prints `(HP + 15) / 16` and the health bar
splits at 400 and 800
([`game_character.cpp:715`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_character.cpp#L715),
[`game_play.cpp:4303-4350`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L4303-L4350)).
A blow writes health in exactly two places — the melee helper and the arrow helper
([`game_play.cpp:2701-2709`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2701-L2709),
[`game_arrows.cpp:44-52`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_arrows.cpp#L44-L52))
— and each of them refuses to write a negative value: the damage is applied only
while it stays below the current health, and the killing blow writes 0 from its
caller instead. Health is also an input to damage (a wounded creature strikes
weaker, `HP × CurrentSila × 0.04 × 0.0625`,
[`game_play.cpp:2045-2075`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2045-L2075))
and poison drains it outside combat
([`game_play.cpp:1336-1349`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L1336-L1349));
the formulas themselves are #74's, not this document's.

#### Our measurements

Over `original/k1/{rus,eng}/game` — `GRAPH.RES`, `OBJECTS.RES` and the
`Game.0`–`Game.2` triples; every number below is identical in both editions
(3179 filled character records, 404 of them in state 4).

- **The shipped files contain no combat at all.** Recorded states are 0 (2143),
  4 (404) and 6 (632); 2/8, 3/9 and 5 never occur — those three are what a running
  fight does. State 4 belongs to exactly three dynamic classes, 0x46 (152 records,
  dynamic object 5), 0x4c (249, object 11) and 0x55 (3, object 22), and no human
  record is in it; all 404 carry `Frame = 0`, the skip-mask 0, the weapons-in-hands
  bit 0x04 of byte 0x03 and a weapon slot above 0, with no equipment and no quiver.
  So none of them goes through the human shooting branch, and each of them is the
  appearance or transformation animation of its class («State 4 — two meanings
  under one number»).
- **Durations, from our own sprite tables** (frames per sequence, equal over all
  eight directions and both editions — one tick per frame at 60 ms). The three
  human models of `GRAPH.RES`: state 2/8 is **7 frames** (420 ms), states 3/9, 4
  and 5 are **12 frames** (720 ms) — the same count as idle and walk, so only the
  stagger is visibly shorter. Of the 30 dynamic objects of `OBJECTS.RES`: state 2
  runs 4–17 frames, state 3 runs 7–16, state 5 runs 10–16, and state 4 is empty
  for 25 of them; the five that carry a state-4 sequence are objects 5, 11, 17, 21
  and 22 with 13, 17, 12, 12 and 12 frames — of these only 5, 11 and 22 belong to a
  class the files record in state 4, so the sequences of 17 and 21 are unreachable
  from the shipped data. Every non-empty state-4 sequence is at least 12 frames, so
  the `FrameCount − 6` release beat of the human shot is always reachable; no melee
  sequence is shorter than 10, so the frame-2 marking beat is too.
- **Health in the shipped files takes two values only:** 1600 for 3063 records and
  **1** for 116 — 110 children (class 0x52), 3 lone NPCs (0x53) and 3 of 0x56 —
  all of them in state 0. No record carries the dead bit of the class, and the
  hero (record 0) is at 1600. Nothing in the starting data is wounded or dead, so
  a test over the shipped files cannot exercise 2/8 or 3/9 without a synthetic
  record.
- **Nobody carries arrows.** Field 0xCC, the quiver reference, is 0 in all 3179
  records, while 626 records do carry equipment (classes 0x1–0x5, plus three of
  0x45 and three of 0x54). Since the shooting branch needs both a bow in slot 1 and
  a quiver, no creature of the starting data — the player's party included — can
  enter state 4 through that branch until arrows are picked up or bought. The state
  4 the files do hold is the other meaning of the number: the three dynamic classes
  of the previous bullet, none of them armed.
- **Three sprites have no combat frames at all.** Dynamic sprites 14, 15 and 16 —
  the children (0x52, 110 records), the women (0x51, 48) and the lone NPCs (0x53,
  50) — carry zero frames for state 2 and state 5, and sprite 15 also for state 3.
  These classes can never attack, and the code has no filter that would keep a hit
  away from them: an arrow that passes their cell calls the state setter anyway.
  What the original then shows is **unknown** — the frame engine of the clone would
  reset frame and count through its `FRM ERR` path
  ([`game_play.cpp:3737-3748`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3737-L3748)),
  which means such a creature never leaves the state; the frame arrays of the clone
  are fixed size, so nothing crashes and nothing completes. This is an inference
  from the clone's code plus our counts, not a measurement of the game: it needs a
  check in the original (shoot a woman or a child and watch) before it is a fact.

#### What stays unknown

- **Whether the monster branch of state 4 releases an arrow.** It is a different
  path from the human shot and its answer does not change #77. The shot branch of
  the monster pass plays the animation and a sound and calls nothing that creates a
  projectile
  ([`game_play.cpp:3607-3619`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3607-L3619)),
  and `FUN_0041bff8` is created nowhere else; the human branch does create the
  arrow. So in the clone a class 0x46/0x4c/0x55 monster plays a shot and does no
  damage — while our files say the same classes hold no bow and no quiver either.
  Which of the two the original does, and from which line, is unverified, so this
  branch is not implemented and its animation is read as the creature's own
  («State 4 — two meanings under one number»).
- **The result codes' thresholds as the original's.** The `-1` limit of 48, the
  dodge roll against half the agility and the facing table that lets a blow be
  blocked
  ([`game_play.cpp:2686-2692`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2686-L2692))
  are the clone's reconstruction; they drive state choices, so #74 has to confirm
  them against the game before they become rules.
- **Target choice and pursuit** — which enemy of the squad is picked and how far
  the chase runs: the combat branch of the monster decision
  ([`game_play.cpp:3179-3283`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3179-L3283)),
  the nearest-target helper behind it
  ([`game_play.cpp:2441`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2441))
  and the keep-or-give-up test on a walking attacker
  ([`game_play.cpp:528-560`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L528-L560)).
  #72's material; this document only notes that the monster's own version of that
  test is what enforces the leash rectangle.
- **Whether a villager squad ever turns hostile** — its pass sets the hostility
  flags from a hit
  ([`game_play.cpp:2897-2906`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2897-L2906)),
  but who sets the elder squad's state byte is still the village rules, open since
  «The group state of a roaming squad».
- **What a hit does to a creature that is already swinging** — the clone restarts
  the stagger of the victim and leaves the attacker's swing running; only the game
  can tell whether the attacker's blow is really not interrupted.

#### Where the combat tickets can be cut

Implementable today on confirmed facts: **#73** (2/8 as a one-shot of the measured
length, restarted by a second hit, ending in the idle chooser of its pass),
**#74** (health as the s16 at 0x5C on the 1600 scale, written at the two sites, the
killer writing 0, the corpse state never entered from health alone), **#75** (3/9
playing once, committing the corpse at `FrameCount − 2` and holding the last
frame), **#76** (5 with its three entries, the frame-2 squad marking and the health
write at the end of the swing, restarting while the enemy stands) and **#77** on
the human path of state 4 only (the turn at frame 0, the release at `FrameCount − 6`,
the shoot loop as its own event) — the other meaning of state 4, a dynamic
creature's appearance or transformation, is #70's and must never be routed through
the shoot loop. What must stay marked unknown there rather than be invented: the
projectile of the monster branch of state 4, the damage thresholds, the empty
sequences of sprites 14–16, target choice and pursuit (#72) and the hostile village.

### Названия существ

Монстр называется по виду из таблицы имён exe: номер записи `(класс & 0x7F) −
0x41`, и заполнены первые 20 видов (`executable.md`, наш замер на ENG: у
расставленных на картах монстров название вида непустое). Люди, женщины и дети —
по байтам 0xD4 и 0xD5 записи персонажа, имя и прозвище через пробел; прозвище 0 —
только имя.

**Наш замер**: в каждом из `Game.0`–`Game.2` ровно один расставленный на карте
житель без имени и прозвища (класс 0x51, оба байта 0) — он стоит в отряде 31 на
карте 37. Так в данных, а не ошибка разбора.

## Гипотезы

- **Отряд-цель (байт 0x1A) и `ai_state` связаны**: `game_play.cpp:1038` пишет в
  `unk3` номер отряда игрока вместе с `unk4 = 1` — похоже, это «кого преследуем».
  Как это читается дальше, не проверялось.
- **The leash rectangle (0x0A–0x16 of a squad record)** is the chase limit, but not
  where the squad-state check reads it: `game_play.cpp:1026` uses `spwnTlRect`,
  while `someRect` is read by the monster's keep-or-give-up test on a walking
  attacker, which drops the target once the enemy leaves that rectangle
  (`game_play.cpp:3088-3105`) — by Slavik, and #72's to confirm.
- **Слот оружия в руках (байт 0x12)** перебирается при атаке от 0 к 1 и 2 и
  выбирается случайно при появлении (`game_play.cpp:3521-3531`); что именно в
  нём 0xFF — не проверялось.

## Не подтверждено

- Once the squad state becomes 1, **which** enemy a creature picks and how far the
  chase runs: the combat branch of the monster decision (`game_play.cpp:3179-3283`),
  the nearest-target helper behind it (`game_play.cpp:2441`, `FUN_004202a8`) and the
  keep-or-give-up test on a walking attacker (`game_play.cpp:528-560`). #72. How the
  attack itself resolves — entry, length and exit of the combat states — is
  recovered now under «The combat states»; the return on 0x20 since «The monster
  decision» above.
- The combat unknowns that stay open after #71 are listed with their line ranges in
  that section: whether the monster branch of state 4 releases a projectile at all,
  the damage thresholds that decide 2/8 versus 3/9, and what the game shows for the
  sprites that carry no combat frames (14, 15, 16).
- Whether a villager squad ever enters combat mode: the elder squad's state
  byte is set from the village flags and the player's weapons
  (`FUN_00416700`, `game_play.cpp:1516-1562`), part of the village rules not
  yet recovered. Until then a villager squad is treated as calm and our
  villagers never turn hostile — an open question, not confirmed behavior.
- Как часто и по какому правилу игра берёт «дорогие» решения поведения
  (`SkipBecauseOfHardCompute`, `game_play.cpp:770`, `game_play.cpp:3485`).
- Смысл 0x80 в байте 0x1D (см. выше) и младших 4 бит вида отряда (0x1E) — кроме
  сторон деревни, разобранных в [game-state.md](formats/game-state.md).
- Правила деревенской жизни: знахарь, кузнец, воевода, дань — таймеры в записи
  деревни есть, их поведение не разбиралось.

## Вне объёма

Это плата за железо 1999 года, а не правило игры — переносить не будем
(AGENTS.md, «Priority: modern experience»):

- маршрут не длиннее 32 направлений (поле 0x13 записи персонажа) и поворот на одно
  направление за тик (`game_play.cpp:3511-3515`);
- «дорогое» решение поведения не больше одного на персонажа за тик и очередь на
  одного такого персонажа на кадр (`HardComputeUnit`, `game_play.cpp:770-781`);
- пересчёт состояний отрядов раз в 16 тиков, а не каждый тик;
- пропуск тиков ходьбы маской в байте 0x11 (в наших файлах он везде 0, так что
  данные и не проигрывают).
