# Поведение существ в «Князе: Легенды Лесной страны»

Что известно о том, как игра ведёт персонажей, монстров и жителей: состояния,
приказы, движение и состояние ИИ отряда. Данные — записи персонажей и отрядов из
`Game.N` ([game-state.md](formats/game-state.md)), картинки —
[graph-res.md](formats/graph-res.md) и [objects-res.md](formats/objects-res.md),
названия — [executable.md](formats/executable.md).

Разделен по правилу AGENTS.md: подтверждённое, гипотезы, неподтверждённое, вне
объёма. Подтверждённое — это ссылка на код Slavik **и** проверка на наших файлах
или рендером. Источник без проверки на наших файлах помечен «по Slavik».

Замер на файлах — ENG-издание (`original/k1/eng/game`, `Game.0`–`Game.2`, 3179
заполненных записей персонажа и 572 заполненных записи отрядов). RUS локально не
перепроверялся: он распакован на другой машине, и там те же замеры ещё не
делались.

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
| выстрел | 4 | 4 | отдельное, только с оружием в руках |
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
  (`game_play.cpp:2421-2429`), ближняя атака — когда он добрался до цели
  (`game_play.cpp:914-930`);
- имена состояний `CHSTATE_0`–`CHSTATE_9` — `game.h:327-336`.

**Наш замер**: в начальных состояниях обоих изданий встречается только 0, 4 и 6
(2143, 404 и 632 записи); 4 и 5 в файлах новой игры нет — это уже боевые действия.

**Наш замер**: бит 0x04 байта 0x03 выставлен ровно у записей с состоянием 0 или 4
и снят ровно у записей с состоянием 6 — 2950 и 632 записи. Отдельно: 632 записи с
состоянием 6 — это люди без 0x40 в классе (виды 1, 2, 3 и 5), а все записи с
динамическим спрайтом держат оружие в руках. Это и есть подтверждение пары
«0–3 с оружием, 6–9 без него» на наших данных, а не только по Slavik.

### Флаги приказов и решений (байты 0x02 и 0x03)

Байт 0x02 — приказы игрока и отметка, что персонажу нужно решение поведения:

| Бит | Смысл | Источник |
|---|---|---|
| 0x01 | приказ атаковать | `game_play.cpp:550`, `game_play.cpp:2489`, `game_play.cpp:3289`; снимается в `game_play.cpp:3335` |
| 0x02 | следовать за ведущим | ставится членам отряда при приказе в `game_input.cpp:395-397`, снимается `game_play.cpp:969`, `game_play.cpp:2994` |
| 0x04 | персонаж стреляет (лука и стрел достаточно) | `game_play.cpp:2421-2431`, `game_play.cpp:3918-3954`; снимается `game_play.cpp:2953`, `game_play.cpp:3912`. Смысл bit-в-байту-0x02 — **гипотеза**: код Slavik его не называет |
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
records carry: every member, in file order, is re-placed near the centre of
the squad's spawn area (`game.cpp:647-656`, `game_character.cpp:328-360`):

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
  us the answer does not depend on the order groups are visited;
- a member may not share a cell with anybody, living or dead (see above);
- the generator is ours (`kn_game::movement::Rng`, splitmix64), asked with
  the original's draw order, so a run with a fixed seed replays.

**Our measurement**, both editions: 379 squads, 2284 members across the 41
maps and `Game.0`–`Game.2` — every member finds a free cell inside its area
within the 100 attempts and no squad of the shipped data is ever trimmed; the
same seed places the same map twice (`original_rules.rs`). The re-placement
also explains the file cells: they are the squad's park position, not where
the game stands it (see the measurement under «The monster decision»).

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

- **Бит 0x04 байта 0x02 — «персонаж стреляет»**. Код Slavik называет состоянием
  (`CHSTATE_4`) и проверяет лук со стрелами, но сам бит не именован
  (`game_play.cpp:2421-2431`, `game_play.cpp:3918-3954`).
- **Отряд-цель (байт 0x1A) и `ai_state` связаны**: `game_play.cpp:1038` пишет в
  `unk3` номер отряда игрока вместе с `unk4 = 1` — похоже, это «кого преследуем».
  Как это читается дальше, не проверялось.
- **Прямоугольник погони (0x0A–0x16 записи отряда)** ограничивает, как далеко отряд
  уходит за игроком: `game_play.cpp:1026` берёт `spwnTlRect` для проверки, а
  `someRect` в этом коде не используется.
- **Слот оружия в руках (байт 0x12)** перебирается при атаке от 0 к 1 и 2 и
  выбирается случайно при появлении (`game_play.cpp:3521-3531`); что именно в
  нём 0xFF — не проверялось.

## Не подтверждено

- Combat, once the squad state becomes 1: which monster is picked as a target
  and how the attack resolves (`game_play.cpp:3179-3283`). The return on 0x20
  is confirmed now — «The monster decision» above.
- Whether a villager squad ever enters combat mode: the elder squad's state
  byte is set from the village flags and the player's weapons
  (`FUN_00416700`, `game_play.cpp:1516-1562`), part of the village rules not
  yet recovered. Until then a villager squad is treated as calm and our
  villagers never turn hostile — an open question, not confirmed behavior.
- Как выбирается цель атаки из нескольких (`game_play.cpp:2441`, `FUN_004202a8`) и
  как считается, стоит ли преследовать (`game_play.cpp:550-560`).
- Как часто и по какому правилу игра берёт «дорогие» решения поведения
  (`SkipBecauseOfHardCompute`, `game_play.cpp:770`, `game_play.cpp:3485`).
- Смысл 0x80 в байте 0x1D (см. выше) и младших 4 бит вида отряда (0x1E) — кроме
  сторон деревни, разобранных в [game-state.md](formats/game-state.md).
- Правила деревенской жизни: знахарь, кузнец, воевода, дань — таймеры в записи
  деревни есть, их поведение не разбиралось.
- Значения `CHSTATE_2`/`8` и `CHSTATE_5` в бою: сколько кадров длятся и чем
  заканчиваются.

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
