<a id="combat-states-and-health"></a>

# Боевые состояния и здоровье

<a id="sources-and-confidence"></a>

## Источники и уверенность

Исследование записано 2026-11-05 (#71). Основа — read-only клон и собственные
замеры. Ссылки закреплены на [`cf5a332`](https://github.com/Marisa-Chan/Slavik/tree/cf5a3320dc93859e3dc51f37ee823fed30f399b6), код не копируется.
**Подтверждено** — источник согласуется с нашей проверкой собственных файлов;
**по Slavik** — только чтение клона; **гипотеза** и **неизвестно** отмечаются отдельно.

<a id="state-28--the-reaction-to-a-hit"></a>

## Состояния 2/8 — реакция на удар

Четыре входа, каждый по результату удара, не решению поведения:
несмертельная стрела ([`game_play.cpp:504-508`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L504-L508)), несмертельный ближний удар в трёх проходах
([`game_play.cpp:976-980`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L976-L980), [`game_play.cpp:3000-3006`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3000-L3006), [`game_play.cpp:3701-3706`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3701-L3706)). Два помощника урона возвращают код:
1 — смерть, 0 — пошатывание, −1 — слишком слабый удар (`hit < 48`,
три отображаемые единицы) без смены действия, −2 — уклонение/блок без изменений
([`game_play.cpp:2699-2712`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2699-L2712), [`game_arrows.cpp:42-55`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_arrows.cpp#L42-L55)). Численные пороги как правила оригинала ещё не подтверждены.

Бит оружия выбирает 2 с оружием или 8 без него. Новый удар заново вызывает
setter, сбрасывающий Frame=0, то есть повторяет пошатывание (**по Slavik**).
Выход — выбор покоя: `FUN_00414ab4` для игрока ([`game_play.cpp:848-861`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L848-L861)),
`FUN_00410450` для бродячих после установки отряду 1 — **пошатывание монстра
заканчивается с целым отрядом в режиме боя** ([`game_play.cpp:3570-3579`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3570-L3579)), `FUN_00410728`
для жителей после флагов враждебности и привлечения отряда старосты ([`game_play.cpp:2897-2906`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2897-L2906)).
`FUN_004248a0` ставит группе 1 и обрезает путь участников без приказа атаки,
останавливая отряд на месте ([`game_play.cpp:2555-2564`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2555-L2564)).

<a id="state-39--death"></a>

## Состояния 3/9 — смерть

Вход по результату 1: ближний бой ([`game_play.cpp:955-973`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L955-L973), [`game_play.cpp:2981-2998`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2981-L2998), [`game_play.cpp:3679-3698`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3679-L3698)), стрела ([`game_play.cpp:483-499`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L483-L499))
или потиковое отравление до нуля ([`game_play.cpp:1332-1350`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L1332-L1350)). Убивающий код сам записывает
здоровье 0 и очищает приказы жертвы. Во время анимации игра ещё считает
существо живым. На `FrameCount − 2` фиксируется труп: очищаются приказы,
ставится мёртвый бит класса, освобождается foot-клетка ([`game_play.cpp:863-883`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L863-L883), [`game_play.cpp:2909-2925`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2909-L2925), [`game_play.cpp:3581-3605`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3581-L3605)).
Последний кадр удерживается: кадровый механизм не проходит конец,
обработчик перестаёт его вызывать; клетка свободна.

Исключение — скелет 0x4c: на предпоследнем кадре, если слот оружия >0,
здоровье восстанавливается до 1600, слот уменьшается и вместо смерти
включается **состояние 4** ([`game_play.cpp:3586-3594`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3586-L3594)).

<a id="state-5--the-melee-blow"></a>

## Состояние 5 — ближний удар

Три входа, ни один не является самим приказом:

- Шаг маршрута в клетку допустимого врага переводит в бой внутри setter,
  там же достаётся оружие ([`game.cpp:2898-2913`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L2898-L2913)).
- Без приказа движения/атаки, если соседний враг другого отряда *уже атакует*,
  существо поворачивается и отвечает ([`game_play.cpp:4210-4243`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L4210-L4243)).
- Засадные классы 0x44/0x57 входят из решения монстра, когда участник игрока
  находится в одном из восьми соседей ([`game_play.cpp:3200-3220`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3200-L3220)).

Перед взмахом `FUN_0043976c` выбирает направление на врага ([`game_play.cpp:2625-2657`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2625-L2657)).
На Frame=2 помечается отряд жертвы и враждебность её деревни.
Здоровье записывается лишь после полного взмаха, затем состояние
перезапускается, пока враг стоит ([`game_play.cpp:914-987`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L914-L987), [`game_play.cpp:2959-3020`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2959-L3020), [`game_play.cpp:3652-3723`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3652-L3723)).
Если враг умер или недостижим, проверки проходов `FUN_0041f258`,
`FUN_00415cd0`, `FUN_00411fd0` сбрасывают EnemyCharID и передают в покой.
У монстра также прекращается погоня за пределами прямоугольника удержания
([`game_play.cpp:3088-3105`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3088-L3105)); само преследование — отдельное неизвестное (#72).
Класс 0x49 после удара отступает вместо повторного взмаха ([`game_play.cpp:3713-3720`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3713-L3720)).

<a id="orders-decisions-and-the-budget"></a>

## Приказы, решения и вычислительный бюджет

Приказ не прерывает боевую анимацию: движение записывает 0x48 и обрезает
маршрут без изменения состояния ([`game_input.cpp:392-400`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_input.cpp#L392-L400)). Приказ подхватывается после
конца анимации: три выхода `FUN_00414ab4`, `FUN_00410450`, `FUN_00410728`
ставят запрос решения 0x40 байта 0x02 ([`game_play.cpp:296-313`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L296-L313), [`game_play.cpp:3049-3061`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3049-L3061), [`game_play.cpp:2456-2470`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2456-L2470)).
Проверка решения исключает у монстра 2,3,9, у жителя 2,3,8,9 ([`game_play.cpp:3474-3486`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3474-L3486), [`game_play.cpp:2830-2843`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2830-L2843)).

После исчерпания трёх дорогих решений за тик существо с ожидающим решением,
если оно не стоит и не идёт, пропускает **весь тик вместе с анимацией**
(`SkipBecauseOfHardCompute`, [`game_play.cpp:3787-3792`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3787-L3792)). Это старый вычислительный предел,
не правило для сохранения. Состояние 4 решение монстра затрагивает напрямую:
для 0x46/0x4c/0x55 оно продвигает первый кадр, поскольку обработчик
[динамического состояния 4](animation.md#state-4--two-meanings-under-one-number) ничего не делает при Frame=0 ([`game_play.cpp:3229-3236`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3229-L3236), [`game_play.cpp:3607-3619`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3607-L3619)).

<a id="where-health-lives"></a>

## Где хранится здоровье

Здоровье — s16 по 0x5C, пятая из девяти характеристик от 0x54
([`game_state.cpp:47`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_state.cpp#L47), [Game.N](../formats/game-state.md)). Шкала 0…1600 соответствует 100 отображаемым
единицам: `(HP + 15) / 16`, полоска делится по 400 и 800 ([`game_character.cpp:715`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_character.cpp#L715), [`game_play.cpp:4303-4350`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L4303-L4350)).
Удар пишет здоровье в двух помощниках — ближнего боя и стрелы ([`game_play.cpp:2701-2709`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2701-L2709), [`game_arrows.cpp:44-52`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_arrows.cpp#L44-L52)).
Отрицательное не записывается: урон применяется только ниже текущего здоровья,
смертельный случай вызывающий код заменяет нулём.
Здоровье участвует и в уроне: раненый бьёт слабее,
`HP × CurrentSila × 0.04 × 0.0625` ([`game_play.cpp:2045-2075`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2045-L2075)); яд уменьшает его вне боя ([`game_play.cpp:1336-1349`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L1336-L1349)).
Полная проверка формул относится к исследованию урона (#74).

<a id="our-measurements"></a>

## Наши замеры

`original/k1/{rus,eng}/game`: GRAPH.RES, OBJECTS.RES, тройки Game.0–Game.2.
Все числа одинаковы в обоих изданиях: 3179 заполненных записей,
из них 404 в состоянии 4.

- **На старте нет боевых состояний.** Только 0 (2143), 4 (404), 6 (632);
  2/8,3/9,5 не встречаются. Состояние 4: 0x46 — 152 записи, объект 5;
  0x4c — 249, объект 11; 0x55 — 3, объект 22. Ни одного человека.
  У всех Frame=0, маска пропуска 0, бит оружия 0x04 байта 0x03,
  слот оружия >0, но снаряжения/колчана нет. Это собственное появление/
  превращение класса, не человеческая стрельба.
- **Длины из наших таблиц спрайтов**, одинаковые по восьми направлениям
  и изданиям, один кадр за 60-мс тик: у трёх моделей GRAPH.RES
  2/8 — **7 кадров, 420 мс**, 3/9,4,5 — **12, 720 мс**, как покой/ходьба.
  У 30 динамических объектов состояние 2 — 4–17, 3 — 7–16, 5 — 10–16.
  У 25 нет состояния 4; у объектов 5,11,17,21,22 по 13,17,12,12,12 кадров.
  Только 5,11,22 соответствуют стартовым классам состояния 4,
  последовательности 17/21 из стартовых записей недостижимы.
  Каждая непустая 4 имеет ≥12 кадров, поэтому отметка FrameCount−6 достижима;
  ближняя атака ≥10, значит достижим и кадр 2.
- **Два значения здоровья:** 1600 у 3063, **1** у 116: 110 детей 0x52,
  три одиночных NPC 0x53 и три 0x56, все состояние 0. Мёртвого бита нет,
  герой 0 имеет 1600. На старте нет раненых/мёртвых состояний;
  проверка 2/8 или 3/9 требует синтетической записи.
- **Стрел нет ни у кого.** Ссылка колчана 0xCC нулевая во всех 3179 записях,
  хотя снаряжение есть у 626: классы 0x1–0x5, три 0x45 и три 0x54.
  Без лука слота 1 и колчана никто, включая игрока, не входит в человеческую
  стрельбу до получения стрел. Стартовая 4 — другая, динамическая анимация.
- **Три спрайта без боевых кадров:** динамические 14,15,16 — дети
  (0x52,110), женщины (0x51,48), одиночные NPC (0x53,50).
  Нет кадров 2/5, у 15 также 3. Эти последовательности не дают атаки,
  но код не отфильтровывает попадание: проходящая стрела всё равно вызывает setter.
  Показ оригинала **неизвестен**. Клон сбросил бы кадр/число через FRM ERR
  ([`game_play.cpp:3737-3748`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3737-L3748)) и не вышел из состояния; фиксированные массивы не падают,
  но ничего не завершается. Это вывод из клона и подсчёта, не замер игры.
  Нужна проверка попадания в женщину/ребёнка в оригинале.

<a id="what-stays-unknown"></a>

## Что остаётся неизвестным

- **Снаряд монстр-ветки состояния 4.** Она отлична от человеческой и
  не меняет отдельный запуск особой анимации (#77). Клон проигрывает
  анимацию/звук, но не вызывает создание снаряда ([`game_play.cpp:3607-3619`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3607-L3619));
  других вызовов FUN_0041bff8 для неё нет. В файлах тоже нет лука/колчана.
  Что делает оригинал и из какого места кода — не проверено;
  ветка не реализована, [состояние 4](animation.md#state-4--two-meanings-under-one-number) трактуется как собственная анимация.
- **Пороги результатов урона.** −1 при 48, уклонение по половине ловкости,
  таблица направления блока ([`game_play.cpp:2686-2692`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2686-L2692)) — реконструкция клона,
  требующая подтверждения оригиналом (#74).
- **Выбор цели и преследование:** боевая ветка ([`game_play.cpp:3179-3283`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3179-L3283)), помощник ближайшей
  цели ([`game_play.cpp:2441`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2441)), проверка продолжения/отказа идущего атакующего ([`game_play.cpp:528-560`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L528-L560)).
  Это материал #72; здесь установлена связь монстр-проверки с прямоугольником удержания.
- **Враждебность деревни:** удар ставит флаги ([`game_play.cpp:2897-2906`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2897-L2906)), но кто меняет
  состояние отряда старосты — невосстановленные деревенские правила
  ([состояние отряда](behavior.md#the-group-state-of-a-roaming-squad-confirmed-2026-11-03)).
- **Удар по уже атакующему:** клон перезапускает реакцию жертвы,
  оставляя взмах атакующего. Непрерываемость его удара требует проверки в игре.

<a id="implementation-boundaries"></a>

## Границы реализации

Записанные сведения описывают реакции, запись здоровья, однократную смерть/
фиксацию трупа, последовательность ближнего боя и человеческую стрельбу.
Они не подтверждают пороги урона, снаряды монстров, выбор цели, преследование,
враждебность деревни или пустые боевые последовательности 14–16.
Эти правила нельзя выводить из восстановленных состояний.
Два применения [состояния 4](animation.md#state-4--two-meanings-under-one-number) остаются раздельными: динамическое
появление/превращение не является человеческим выстрелом.
