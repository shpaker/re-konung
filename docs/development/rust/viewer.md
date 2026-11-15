<a id="viewer-implementation-and-frame-measurements"></a>

# Устройство просмотрщика и замеры кадров

[Управление](../../guides/viewer.md) · [Рендеринг](rendering.md) · [Симуляция](simulation.md). Ниже сохранены исторические замеры с исходным оборудованием, версиями и условиями. Перестройка документации не добавляет новых результатов. Исправленная атрибуция важнее ранних гипотез; прежние числа при этом сохраняются.

<a id="viewer-state-and-diagnostics"></a>

## Состояние и диагностика

kn-fe-viewer отделяет состояние Viewer от элементов Feathers. Обработчики меняют состояние, системы синхронизации применяют его. Rebuild возникает только при смене карты: строит SceneOptions::ALL и кадрирует камеру. Просмотрщик не выбирает героя (SceneOptions.hero = None); источник карты выбирает Game.N либо TGame.N.

Переключатели слоёв меняют Visibility групп, не спрайты или текстуры. Панель показывает отношение числа спрайтов с истинным InheritedVisibility к числу собранных; учитываются скрытые группы и пустые кадры. Сумма числа детей завысила бы результат. TextureCache::load сохраняет текст ошибки декодирования; лог и счётчики показывают недостающие изображения. Неверный номер карты в CLI завершает запуск неуспешно.

Checkbox Bevy сообщает !Has<Checked>, но сам не записывает Checked; kn-ui::sync_checked отражает действительное состояние после мыши и клавиатуры. Корень панели тоже имеет Hovered, чтобы камера не получала ввод в промежутках между элементами. Подписи английские: это инструмент разработки. KonungUiPlugin применяет общую тему и PT Sans, включая внутренние части составных элементов Feathers. Characters включён по умолчанию, флажок соответствует состоянию.

Метки один раз на карту строит k1_game::creatures::labels из имён EXE и индексов класса/имени/прозвища записи; доступны через Simulation::labels. Просмотрщик не импортирует таблицы имён K1. Метка — #<id> и собранное имя, либо только #<id>. Синтетические тесты не содержат строк игры. PT Sans поддерживает кириллицу; игровой шрифт и пользовательский путь к шрифту не нужны. LABEL_Z=500 находится над элементами карты (наибольший записанный z около 172) и внутри диапазона 2D-камеры ±1000; прежнее z=9999 отсекалось. Мировой y направлен вверх, положительное смещение метки помещает её над существом.

<a id="playback-and-lighting-controls"></a>

## Воспроизведение и освещение

Одно состояние Play управляет Clock.ticks для кадров и симуляции: Space переключает Playing/Paused, . и , делают шаг. Пауза удерживает оба процесса; флажок никогда не выбирает Off (`play_toggle_never_stops_the_animation_behind_the_simulation_s_back`). apply_playback меняет часы, не Rebuild; sync_playboxes отражает состояние.

Reset positions реагирует только на Activate своего маркера ResetPositions, не селектора карты. reset_creature_sim пересоздаёт Game::simulation с тем же SIM_SEED, что загрузка карты и игра, фиксирует эпоху на текущем Clock.ticks и обнуляет относительные sim_ticks. Слои, камера, карта, пауза, кеш текстур, узлы и метки сохраняются; позиции синхронизируются заново. Без фиксации эпохи новая симуляция догоняла бы старую историю.

Reset animation включает Off, обнуляет часы и пересоздаёт/проигрывает состояние до нуля. Следующий Play запускает особые анимации состояния 4 с кадра 0. Paused → Play продолжает удерживаемую стадию; сброс позиций сохраняет часы. Шаг назад пересоздаёт и проигрывает мир, не обращает движок; порядок и стоимость длинного повтора описывает [рендеринг](rendering.md).

Viewer::constant_day — настройка сеанса; меняется флажком либо N, отражается sync_daynight_boxes. update_lighting_state передаёт params_gated(tick,seed,!constant_day), не двигая Clock и не перестраивая сущности. Настройка переживает смену карты, при перезапуске получает DEFAULT_CONSTANT_DAY=false: цикл включён. Оригинальное начальное значение неизвестно. --constant-day и --light-tick фиксируют условия снимка/замера. В игровом приложении пока нет такого переключателя. Условия оригинала и отличия: [исследование освещения](../../research/k1/lighting.md).

<a id="recorded-lighting-render-cost"></a>

## Историческая стоимость отображения света

Измерялось **окно**, не CPU-модель. Release, --benchmark 600: 40 кадров стабилизации и 600 измеряемых кадров RUS-карты 17, 12 487 спрайтов, слой существ включён. Выборки — время кадра из DiagnosticsStore, записываемое FrameTimeDiagnosticsPlugin; процентили nearest-rank по 600 значениям, каждая строка охватывает три последовательных запуска. Только режим замера использует PresentMode::AutoNoVsync, интерактивный — обычный Fifo. С vsync готовый кадр ждал бы квант дисплея 16,67 мс, и сравнение дня/ночи измеряло бы экран.

Оборудование: AMD Ryzen 9 9950X, NVIDIA GeForce RTX 2070 SUPER, backend wgpu Vulkan по логу адаптера. Счётчик вызовов рисования недоступен в bevy_render 0.20 без дополнительных возможностей; вместо него считались ScenePiece и материалы пула света.

| Версия | Свет | Среднее, мс | p95, мс | p99, мс | Спрайтов | Материалов света |
|---|---|---|---|---|---|---|
| a95db9f, историческая локальная main, все стандартные возможности Bevy | Нет | 1,49–1,52 | 1,86–1,94 | 2,06–2,10 | 12 487 | 0 |
| ba22cda, база исследования света, профили сборки #105 | Нет | 1,28–1,29 | 1,64–1,68 | 1,86–1,93 | 12 487 | 0 |
| Прототип 3838657, материал только на земле, свет закреплён CLI | День, --light-tick 0 | 3,00–3,01 | 3,46–3,47 | 3,71–3,77 | 12 487 | 20 |
| Прототип 3838657 | Ночь, --light-tick 11000 | 2,96–3,02 | 3,38–3,43 | 3,64–3,67 | 12 487 | 20 |
| Версия света 16e4371 | День, часы на тике 0 | 3,30–3,33 | 3,82–3,86 | 4,03–4,20 | 12 487 | 21 |
| Версия света 16e4371 | Ночь, --light-tick 11000 | 3,27–3,30 | 3,76–3,80 | 3,97–4,06 | 12 487 | 21 |

Локальная main a95db9f тогда отставала от origin/main ba22cda на один инфраструктурный коммит #105, сокративший набор возможностей Bevy и инструментарий. Уже он меняет среднее примерно на 0,2 мс (1,49 → 1,28). Свет разрабатывался поверх ba22cda; сравнение ведётся с ней:

- **Материалы на всех поверхностях днём:** +2,0 мс среднего (1,28 → 3,30–3,33), p95 1,64 → 3,82–3,86. Материал получают земля, декорации, объекты и существа в тёмных ячейках: 21 материал, 19 масок LIGHTS.RES и два обычных варианта.
- **Ночная формула относительно дня:** различие не разрешено разбросом: 3,27–3,30 против 3,30–3,33 мс. Тройка меняет uniforms, не граф материалов; нулевой дневной цвет идёт по пустой ветви шейдера. [CPU-модель](simulation.md#lighting-cost) стоит наносекунды.
- **Расширение прототипа за пределы земли:** +0,3 мс среднего (3,00 → 3,30–3,33), p95 3,46 → 3,82–3,86. Материал только земли стоил +1,7 мс к базе (1,28 → 3,00); декорации, объекты и замены материалов существ добавили около трети миллисекунды и один материал (20 → 21).

**Выделение памяти ещё не измерено отдельно.** update_creature_lighting читает Simulation::states(), как и системы существ/меток; стоимость Vec состояния для этих популяций не изолирована. Карта затемнения существ переиспользует Local<HashMap>. Утверждать незначительность выделения памяти до профилирования нельзя.

**Догоняющая симуляция после ползунка.** Отпускание ползунка догоняет часы за одно обновление: вперёд обычными тиками, назад детерминированным повтором seed. Исторический benchmark_scrub_catchup_cost в kn-fe-viewer (#[ignore], release, нужен original/) измерял целиком app.update() на самой населённой симуляции RUS: карта 36, 65 существ, три свежих запуска на переход.

| Переход | Среднее время обновления |
|---|---|
| 0 → 18960, вперёд на 18 960 тиков | 54–76 мс, максимум шести запусков 75,8 |
| 18960 → 900, назад с повтором 900 тиков | 44–48 мс |

Оба перехода укладываются в десятую секунды; это задержка одного обновления, которая может пропустить несколько кадров дисплея. Обратный переход стабильно дешевле. Сам этот замер не потребовал оптимизации; критерий времени кадра описан ниже.

<a id="viewer-costs"></a>

## Стоимость просмотрщика

Исторические замеры окна с симуляцией и отображением (#83): карта 17 с бродячими группами (28 существ, ни одного шага за 600 тиков) и деревня 19 (20 существ, 14 идут), оба издания.

**Методика.** Release, Windows 11, AMD Ryzen 9 9950X, 64 GiB RAM, NVIDIA GeForce RTX 2070 SUPER по адаптеру wgpu; встроенный GPU простаивает. Окно 1280×800. --benchmark 8000: 40 кадров стабилизации и 8000 измеряемых, при примерно 2,4 мс деревни это 300–350 тиков, несколько циклов шага каждого пешехода. AutoNoVsync исключает квант дисплея 16,67 мс. Выборки — реальное время DiagnosticsStore из FrameTimeDiagnosticsPlugin, как на панели; процентили nearest-rank по 8000 значениям.

Строка BENCH содержит max_ms, ticks только за измеряемое окно, creatures, walking и labels. Соотношение `ticks · 60 ms ≈ frames · avg_ms` проверяет согласованность. Запуски с нагрузкой рабочего стола отбрасывались: p95 одновременно уходил к 16–17 мс во всех конфигурациях, включая паузу. Они сохраняли равенство, но не изолировали сцену. Чистые серии двух изданий согласовались в пределах 0,1–0,3 мс.

**Инструментирование.** Переменные окружения не требуют флагов сборки и без установки не меняют поведение. KN_VIEWER_LABELS=0 не создаёт метки id; стандартно они создаются. KN_VIEWER_PAUSE=1 начинает на паузе, удерживая симуляцию и клипы ходьбы: это база стоимости отображения. knob_value выключает фактор при 0, сохраняет стандартное значение при отсутствии переменной, включает при любом другом значении (`the_benchmark_knobs_default_to_the_shipped_behavior`). Изолированные тесты ниже помечены #[ignore]; данные остаются в original/, доступны через GAME_DIR.

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

**Активная сцена.** Стандартные настройки, по 8000 кадров, разброс чистых серий:

| Карта | Воспроизведение | Пауза, база отображения | Тиков | p95 / p99 / максимум, мс, воспроизведение |
|---|---|---|---|---|
| 17 RUS, 28 существ, 0 идут | 3,19–3,57 мс | 3,24–3,78 мс | 425–476 | 3,70–4,28 / 4,28–5,12 / 10,6–12,8 |
| 17 ENG, 28 существ, 0 идут | 3,15–3,20 мс | 3,06–3,15 мс | 420–426 | 3,56–3,66 / 3,95–4,12 / 10,4 |
| 19 RUS, 20 существ, 14 идут | 2,34–2,62 мс | 2,24–2,61 мс | 311–349 | 2,73–3,23 / 3,12–3,84 / 9,3–13,4 |
| 19 ENG, 20 существ, 14 идут | 2,33 мс | 2,23 мс | 310 | 2,75 / 3,13 / 9,1 |

Сравнение — воспроизведение минус пауза на одной карте в одной серии. Для деревни: +0,09/+0,10/+0,10 мс (RUS серии 2–3, ENG серия 1), воспроизводимые около +0,1 мс. Спокойная карта даёт −0,59…+0,33 мс RUS, но +0,05…+0,10 мс во всех трёх ENG-сериях: несколько процентов кадра.

Из +0,1 мс деревни логика симуляции оценивается примерно в 1,5 мкс на кадр в среднем: тик примерно каждый 24-й кадр, полный тик движка 18–22 мкс ([стоимость правил](simulation.md#rules-tick-cost)), пакет смен клипов около 10 мкс и глубина не больше 0,5 мкс. Это на три порядка ниже кадра. Воспроизводимая часть разницы согласуется с обновлением до 85 анимированных элементов деревни и 37 спокойной карты каждый тик, но отдельная причинная доля рендеринга здесь не изолирована. Чистый расчёт решений карты 17 за 1,2–1,6 мкс/тик в окне неразличим на фоне разброса.

**Одна смена позы.** benchmark_pose_swap_cost запускает общую подсистему существ без окна через MinimalPlugins на карте 19: один тик на обновление, 400 обновлений. База — обновление с продвижением симуляции без смены поз; кадры с ровно одной сменой изолируют добавочную цену. По три запуска на издание: +10,2–13,7 мкс RUS и +6,9–12,2 мкс ENG к медиане базы. Смена переустанавливает 3,7–3,8 клипа тела и тени, то есть около 2–4 мкс на установку и перезапись спрайта. Для сравнения: полный тик 18–22 мкс, [сборка позы в слое данных](data-scene.md#pose-cost) 57–72 мкс один раз на смену состояния/направления, вызов глубины около 8–19 нс. Здесь измерена смена уже собранных клипов. При примерно 1,1 начала шага за тик среди жителей она даёт около 0,2 мкс на кадр; предел — одна пересборка на смену состояния, ни одной просто за кадр.

**Пересчёт глубины движения.** benchmark_depth_slot_cost измеряет DepthMap::creature_z: двоичный поиск статических стопок, интерполяцию и слот id. sync_creature_nodes делает поиск для тени каждого существа каждый кадр и для тела при смене нижней границы. Используются реальные карты глубины и позиции нижних границ идущих существ (текущий ряд плюс высота тела). Шесть запусков × оба издания × обе карты: **8–19 нс на вызов**, **0,2–0,5 мкс на всю популяцию за кадр** — 28 либо 20 существ. Один вызов примерно в тысячу раз дешевле пакета смен клипов около 10 мкс; вся глубина занимает около 1/5000–1/12 000 кадра деревни. Кеш или ограничение частоты для этих популяций не оправданы.

**Метки существ.** Сравнение KN_VIEWER_LABELS=0 с воспроизведением той же серии чаще ниже межсерийного разброса; максимум +0,16 мс для 20 меток деревни RUS и +0,15 мс для 28 меток бродячей карты RUS. Чистые пары дали +0,01–0,16 мс; исходная оценка порядка нескольких микросекунд на метку, включая перемещение готового текстового узла и его долю рендеринга. Это сопоставимо с частью смены позы; 28 меток занимают менее 1% бюджета 60 мс. На этих картах замеры не обосновывают отдельную оптимизацию видимости меток.

**Исторические максимумы и границы атрибуции.** В спокойных условиях деревня давала максимумы 9,1–9,7 мс при воспроизведении и 3,5–3,7 мс на паузе (RUS серии 2–3 и ENG серия 1), включая 9,17 мс без меток. Спокойная карта имела около 10 мс даже на паузе: 9,5–10,6 мс RUS. Ранняя гипотеза связывала пики с обновлением спрайтов на тике; положительная причина не измерялась. Поздняя проверка исправила сопоставление интервала времени с тиком и установила: худшие интервалы не исполняли тик симуляции. Нагрузочные серии с p95 16–17 мс исключены; планировщик и драйвер остаются гипотезами. Изолированная CPU-работа на тиковом кадре была около 35 мкс максимум, намного ниже пиков.

<a id="frame-time-gate"></a>

## Критерий времени кадра

Условие #98: среднее и p95 не выше 33,3 мс (30 FPS) на каждой опорной карте. Проверка проводилась после внедрения освещения и постоянного дня, со светом, то есть на итоговом рендеринге. Превышение требует отдельной задачи оптимизации, не скрытой оптимизации внутри задачи замера.

**Протокол.** Release, Windows 11, AMD Ryzen 9 9950X, 64 GiB RAM, NVIDIA GeForce RTX 2070 SUPER, Vulkan; та же машина, что в таблице света. Окно 1280×800, ступень масштаба 3 (scale 1.0, начальная), камера и сцена не меняются. Воспроизведение включено, все существа активны, смена суток включена и следует тику, --constant-day выключен. Карты 17 и 19, оба издания, по 60 секунд:

```bash
just viewer-gate
```

Это четыре запуска cargo run --release … -p kn-viewer с `--game-dir original/k1/<rus|eng>/game --map <17|19> --benchmark-secs 60`. Данные не коммитятся. Каждый запуск печатает BENCH … и `GATE map=.. pass=.. avg_ms=.. p95_ms=.. limit_ms=33.3`.

Запрошенный бенчмарк считается завершённым только после обеих строк.
`run_viewer_app` хранит признак завершения вне мира Bevy: если оконный runner
вернул успех до результатов, процесс завершится с ошибкой и диагностикой
`interrupted before BENCH/GATE results were produced` (#119). Уже полученный
ненулевой код сохраняется; обычное закрытие интерактивного окна не меняется.
Тесты охватывают оба ограничения замера, остановку до и после прогрева,
отсутствие выборок и успешный/проваленный порог.

**Проверка #119, 2026-11-15.** macOS 27.0.1, Apple M4 Pro (24 ГБ),
Metal, Rust/Cargo 1.99.1, Bevy 0.20.1, release, окно 1280×800 логических
пикселей. Восемь последовательных запусков по `--benchmark-secs 60`,
симуляция и цикл суток включены, `KN_VIEWER_LABELS=0` или `1`:

| Издание и карта | Подписи | Среднее, мс | p95, мс | Тиков |
|---|---|---:|---:|---:|
| RUS 19 | выкл. | 5.72 | 15.55 | 1000 |
| RUS 17 | выкл. | 16.92 | 29.79 | 990 |
| ENG 19 | выкл. | 8.11 | 18.87 | 1000 |
| ENG 17 | выкл. | 8.38 | 17.22 | 1000 |
| RUS 19 | вкл. | 5.18 | 16.38 | 1000 |
| RUS 17 | вкл. | 7.18 | 17.11 | 1000 |
| ENG 19 | вкл. | 3.99 | 8.18 | 1000 |
| ENG 17 | вкл. | 6.09 | 16.59 | 1000 |

Во всех восьми запусках код выхода 0, ровно по одной строке BENCH/GATE,
`pass=true`, 22 материала. До изменения один запуск RUS 19 без подписей
также прошёл: 6,69 мс среднего, 17,50 мс p95, 21 материал. После изменения
та же конфигурация дала 5,72/15,55 мс; это не доказательство ускорения:
локальный разброс велик. В RUS 17 без подписей максимальный кадр достиг
734,20 мс, число тиков составило 990 вместо примерно 1000. Причина выброса
не установлена; p99 и максимум по принятому протоколу не определяют GATE.
Эта серия подтверждает прохождение проверки на Metal, но не заменяет
исходный Windows/Vulkan-сценарий и не устанавливает причину гонки Bevy.
Логи, таблица `benchmarks.json` и снимки сохранены локально в `out/issue-119`.

**Граница кадра.** AutoNoVsync, сырые последние измерения FrameTimeDiagnosticsPlugin в DiagnosticsStore, не сглаженные; первые 40 кадров исключены, процентили nearest-rank. Система замера выполняется после записи времени и цепочки существ. Сама дельта начинается в First и относится к **предыдущей итерации**: bevy_time 0.20.1 time_system вычисляет её там, bevy_diagnostic 0.20.1 diagnostic_system пишет в Update из Time<Real>. Поэтому max_on_tick сопоставляет выборку с числом тиков предыдущей итерации; первая выборка окна относится ещё к стабилизации и получает 0. Это фиксирует обе стороны сопоставления.

--benchmark-secs закрывает окно, когда сумма выборок пересекает 60 000 мс. Количество frames — результат, не заданный параметр. Строка содержит ticks за окно (в записанных запусках ровно 1000 при TICK_MS=60 из k1-data::objects_res), конечные creatures/walking и labels. Все четыре запуска ниже выполняют `ticks · 60 ms ≈ frames · avg_ms` с погрешностью до 0,2%.

**Воспроизводимость.** До интеграции выполнены три последовательных прохода: два командами, один рецептом just viewer-gate. Среднее и p95 воспроизвелись в пределах 0,04 мс, p99 — 0,21 мс, максимум — 0,41 мс. Допуск для последовательных запусков на стабилизированной машине: ±0,05 мс для среднего и p95. Более крупный сдвиг между днями или режимами питания требует проверки условий, сам по себе не устанавливает причину.

Между состояниями машины максимальный попарный разброс записанного дерева был 0,06 мс среднего и 0,11 мс p95; между проходами наблюдались разные режимы простоя GPU. Отброшенный проход сразу после игровой сессии давал на 0,5–0,9 мс больше во всех конфигурациях; связь с послесеансовым режимом драйвера — гипотеза. Также исключены серии с нагрузочным признаком p95 16–18 мс. Они не усреднялись с основной таблицей.

Ниже историческая повторная проверка дерева после #115–#117, по одному проходу на конфигурацию, включая покадровый рендеринг имён #116:

| Карта | Среднее, мс | p95, мс | p99, мс | Максимум, мс | Кадров | Популяция |
|---|---|---|---|---|---|---|
| 17 RUS | 3,08 | 3,41 | 3,90 | 10,15 | 19 478 | 28 существ, 0 идут, метки включены |
| 19 RUS | 2,28 | 2,65 | 3,01 | 8,87 | 26 281 | 20 существ, 14 идут, метки включены |
| 17 ENG | 3,11 | 3,49 | 3,94 | 10,17 | 19 319 | 28 существ, 0 идут, метки включены |
| 19 ENG | 2,33 | 2,76 | 3,15 | 9,77 | 25 715 | 20 существ, 14 идут, метки включены |

Каждое окно охватило ровно 1000 тиков.

**Результат: все четыре конфигурации прошли, запас около 9×.** Худший p95 — 3,49 мс при пределе 33,3. Среднее 2,28–3,11 мс попадает в прежний диапазон критерия 2,2–3,3 мс; спокойную карту сверху ограничивают прошлые 3,15–3,57 мс. Добавление покадровых имён #116 не выделяется над разбросом машины при прежнем числе спрайтов и материалов.

Пара деревни 19 RUS без/с метками: среднее 2,30 → 2,28 мс, p95 2,71 → 2,65 мс; стоимость не разрешается воспроизводимостью серии. Более ранняя, заменённая интеграционная таблица давала около 0,2 мс: среднее 3,40 → 3,22, p95 3,98 → 3,77. Это сохранённый разрешённый замер; условия, при которых пара разрешает разницу, не установлены. Разброс изданий на одной карте здесь 0,03–0,05 мс против прежних 0,8–1,0 мс; предположение о большей ширине английских имён не подтверждено причинно. Деревня быстрее спокойной карты в обоих изданиях: 2,28 против 3,08 RUS и 2,33 против 3,11 ENG.

**p99 и худший кадр.** p99 — 3,01–3,94 мс; отдельный класс остановок этим не установлен. Максимум 8,9–10,2 мс имеет max_on_tick=0 во всех четырёх окнах при исправленном сопоставлении: ни один худший интервал не исполнял тик. Тик приходится примерно на каждый 19–26-й кадр, около 4–5% окна; его изолированная цена — 18–22 мкс движка и около 10 мкс смены позы, на три порядка меньше пика. Исключение тиков как причины измерено; положительная причина — планировщик или драйвер — неизвестна. max_at меняется: 14799 и 15615 на спокойной карте, 6999 и 13854 в деревне. Это согласуется с шумом, но не доказывает его. Все процентили на порядок ниже предела; оснований для задачи оптимизации эти результаты не дали.

<a id="the-hour-we-display--our-convention-not-a-fact-about-k1"></a>

## Показываемые часы — наша условность

Подтверждённых часов оригинала нет. Для показаний, потребовавшихся в #87, принята явная схема:

- 21600 тиков цикла соответствуют 24:00: 15 тиков на минуту, 900 на час.
- Тик цикла 0 показывает 06:00: окно 1 становится утром, не полуночью.
- Часы — чистая функция тика симуляции; отдельного времени и накопителя просмотрщик не имеет.
- Фазы и свет **не читают показанные часы**: они вычисляются из тиков и [таблицы исследованных интервалов](../../research/k1/time.md#what-the-source-gives-intervals-not-day-phases); часы лишь подписывают тот же тик.

При этом сдвиге: светло 03:08 → 12:00, тёплая полоса 08:00 → 10:00, затемнение с 12:00, свет земли 15:00 → 03:00, нейтральный цвет снова в 03:08. Ночь длиннее дня из-за длины исходного тёмного интервала; другой сдвиг перенесёт, но не устранит асимметрию. Эти часы не связаны с делителем 1440 в коде оригинала ([исследование](../../research/k1/time.md#a-unit-of-1440-ticks)).

<a id="camera-presets-ours"></a>

## Наши предустановки просмотра

Предустановки называют интервал, в котором располагают наблюдение, а не условные четыре названия фаз интерфейса. Иначе подпись вроде «сумерки в 09:00» могла бы противоречить классификации. Предустановки **не являются границами**, тесты не должны на них опираться.

| Предустановка | Тик | Окно | Изображение |
|---|---|---|---|
| light | 900 | 1 | Нейтральный фон, земля без света; 07:00 наших часов |
| warm band | 2700 | 3 | Тёплое плато, земля без света; середина полосы, 09:00 |
| ground-lit plateau | 11000 | 7 | Фон (0, −50, −70), свет тайлов включён, до перехода окна 8 |
| final ramp | 18960 | 13 | Внутри перехода длиной 121 тик к нейтральному цвету, земля без света |

Названия фаз отображения классифицируют интервалы таблицы; слово фазы не вводит дополнительной временной границы.

## Workspace inspection costs

Local synthetic measurements on an Apple M4 Pro, `dev-runtime` (workspace
opt-level 1, dependencies opt-level 3), using `Instant` and `black_box`; each
number is the minimum per-operation average of five batches of 1,000 operations.
These are CPU microbenchmarks, not a frame-time budget or measurements of original
maps. The fixtures contain 500 visible sprite layers and, separately, 200 idle
lone NPCs on an open grid.

| Operation | Time | Neighboring operation |
|---|---:|---|
| Build a 96×128 one-bit alpha mask | 21.57 µs | Copy the same RGBA buffer: 0.52 µs |
| Pick among 500 sprite layers | 11.81 µs | Find one selected creature's bounds among those layers: 1.33 µs |
| Inspect one record, worst-case scan of 200 records | 0.095 µs | Collect 200 movement states: 2.58 µs |
| Build all 200 inspection projections, including label copies | 8.80 µs | Tick those 200 idle NPCs: 4.00 µs |

Mask construction is about 41 times the upload-buffer copy, paid once per unique
creature body/equipment image during decode. Its pixel payload is 1,536 bytes,
versus 49,152 bytes of RGBA (32 times smaller); map/hash allocation overhead is
excluded. Terrain and shadow images retain no mask, and the RGBA upload remains
render-world-only. A pick is about nine times the bounds query in this fixture;
a direct single-record lookup is substantially cheaper than allocating all live
states. Full inspection enumeration costs about twice this deliberately simple
idle tick. These are facade API costs: the current workspace builds the complete
read projection each frame, including the JSON and scene scans measured below. These comparisons do not estimate route
search or texture decode costs.

Reproduce without game files:

```sh
cargo test --manifest-path crates/Cargo.toml --profile dev-runtime \
  -p kn-game -p kn-render --lib workspace_inspection_costs -- \
  --ignored --nocapture --test-threads=1
```

The shared picker quantizes the current displayed frame into half-open local
sprite pixels before applying flips, excludes transparent texels and all shadow
layers, and resolves overlapping opaque body/equipment pixels by their actual
world z. Selection bounds use those same current quads. Read these mechanisms
after transform and inherited-visibility propagation. The Inspector's squad
snapshot is the exact pre-movement value consumed by the last decision pass;
tick-zero and position-reset snapshots are computed from the initial placement
and marked unevaluated. It never substitutes the stored, unknown `ai_state` byte.

### Complete read projection

Measured separately on the same Apple M4 Pro in `dev-runtime` with the dynamic
feature: `state_value` includes inspection enumeration, JSON allocation and
cleanup, selected-layer JSON and both complete scene-visibility scans. After
100 warm-up calls, 2,000 calls gave:

| Local simulation | Creatures | Synthetic visible ScenePiece nodes | Median | p95 |
|---|---:|---:|---:|---:|
| RUS map 17 | 28 | 12,501 | 86.500 µs | 103.417 µs |
| RUS map 19 | 20 | 13,993 | 69.084 µs | 78.041 µs |

The simulation is loaded from local files, while the scene components and eight
selected static body-layer entries are synthetic. This exercises the actual
full read-projection path without window/GPU/UI-layout cost; it is not a
measurement of the whole Inspector, a complete frame, or animation-clip lookup.
The map with fewer scene nodes but more creature records costs more here, so
core inspection-only microbenchmarks cannot stand in for JSON/workspace costs.

```sh
GAME_DIR=original/k1/rus/game MAP=17 cargo test \
  --manifest-path crates/Cargo.toml --profile dev-runtime \
  -p kn-fe-viewer --features dynamic --lib \
  action::tests::measure_workspace_projection_cost -- --ignored --exact --nocapture
```

## Local workspace verification

Checked on macOS, Apple M4 Pro, 2026-11-15, at logical window sizes 1280×800
and 1024×720, plus a 960×640 stress layout. `check --all --format json` reports
were exactly equal to the preceding `main` for 43 RUS game maps, 43 ENG game
maps and two ENG tutorial maps.

A missing-map failure was reproduced in a local symlink fixture under `out/`.
Requesting map 1 retained displayed map 17 and selection 513, showed requested
map 1 and its load error, and left state reads available. Restoring only the
fixture symlink and Retry loaded map 1 and cleared selection; originals were
not changed. Selection alone preserved scene/cache facts and camera.

Native ENG map 19 scrubbing to tick 5,357 changed 14 creature positions and
reached ready state. Seeking to the same tick through control produced exactly
the same complete creature projection. UI scrubbing preserves the previous
play/pause preference, whereas control Seek pauses. Approximately one-second
samples advanced 17 ticks at 1× and 134 ticks at 8×, consistent with about eight
times the rate; this is a behavioral check, not frame-performance evidence.
Reset at clock 600 followed by a relative step to 599 also completed ready.

Workspace captures contained nonblack pixels, and paired JSON was exactly equal
to the capture response state. Capturing while playing restored playback after
save; write failure returned an error. Legacy batch `--screenshot` also exited
successfully and produced a visually checked, nonblack 2560×1600 PNG at 1:1.
Images, session tokens and response evidence stay local under
`out/viewer-workspace-verification/` and are not committed. An earlier black
baseline capture was not reproduced in the later check; no cause was established.

## Debug-player journal and render bridge

The development viewer opts into `DebugReplay`; ordinary renderer consumers
retain the existing simulation path. Each successful map load creates a new
map-local journal and resolves the actual source hero profile. Profile failure
only disables placement. The runtime actor registers through the simulation's
world actor API, so route requests, occupancy, group evaluation and directional
pace use the same rules as ordinary actors. It has a code-native marker rather
than a copied character sprite and is excluded from ordinary creature picking.

Commands are recorded after logical tick T and before T+1, including failed
attempts. Rewinding retains future entries until a new attempt branches history.
The renderer honors journal `caught_up`, including pending same-tick commands;
its per-frame work budget covers both commands and ticks. A replay mismatch is
latched and synchronizes wrapper progress to the actual reached logical tick
before reporting failure. It is never retried against a partially modified state.
Position resets are journal events rather than new history origins: relative
steps can cross them in either direction. They remove the debug actor while
preserving logical time and animation. Special-animation resets retain the actor
and ordinary walk/rest phases.

The bridge observes step, direction and special-stage changes to reconstruct
render animation origins. A rewind seeds the rebuilt tick-zero picture before
advancing, preserving first-tick transitions. Anchor lookup is built once per
creature sync and reused by all body/shadow layers. Synthetic production-system
regressions compare uninterrupted and replayed clip frames/transforms, natural
special completion, walking special-reset behavior, reset boundaries, and
mismatched replay progress.

The visual layers and shell state use the same scoped `DebugSnapshot`. Signed
raw group corners are retained; only the spawn rectangle receives a confirmed
role label. Geometry entities are pooled. A toggle-only update changes
visibility; cached frames do not reinsert sprite/text geometry. Missing simulation
hides the pool. Marker, overlay and selection geometry still synchronize during
capture after input and time are frozen, so saved pixels match the frozen state.
Empty or entirely black readback returns `capture_failed` before either output
file is written. Window focus/visibility and last observed occlusion are exposed
as diagnostics, without claiming they prove render availability.

### Local debug-player verification

The integrated runtime retained exact `check --all` JSON equality for 43 RUS
maps, 43 ENG maps and two ENG tutorial maps. Native ENG map 17 placement at
(44,48) followed by a move request to (42,46) produced a real route without a
teleport. While paused, group 162 preview changed before its used state; one
simulation tick applied that evaluation. Sixteen endpoint acceptance checks
compared live and headless snapshots across resets, rewinds, failed history
branches and the final state; all matched. A restarted native session produced
a visually inspected nonblack 2560×1600 workspace capture whose paired JSON
exactly matched the capture response state. The earlier session's
black readbacks remain an unresolved diagnostic observation, not an established
platform cause or a claimed restart-based product fix. Evidence remains local
under `out/debug-player-verification/`.

The debug workspace was checked at 1280×800, 1024×720 and 960×640. At the
smallest size, actual pointer clicks reached Scope All and Remove; removal kept
selection 1416 and tick 1 while showing used group state 1 against preview 0.
Opaque body picking selected creature 1416, while a transparent point inside its
quad, a label point and an outside point cleared selection. Capturing during
playback restored its prior play state, and a write failure returned a structured
error. These are behavioral checks rather than performance measurements.

### Debug-overlay CPU costs

Measured locally on Apple M4 Pro/macOS in optimized `dev-runtime`, using actual
ENG map 17 after 600 simulation ticks, with the temporary player present and
All scope (29 creature snapshots, seven group records). Each row is the mean of
2,000 calls in a headless ECS world; no GPU, font layout or window composition is
included. The pure geometry row rebuilds all overlay primitives, whereas cached
draw includes snapshot generation/comparison but reuses existing geometry.

| Operation | Mean per call |
|---|---:|
| Core `debug_snapshot` | 2.134 µs |
| Full scoped geometry build | 6.541 µs |
| Cached draw with player and overlays | 3.053 µs |
| Toggle-only draw, including visibility writes | 6.537 µs |
| Debug read-projection JSON | 36.073 µs |

Geometry construction cost was about three times the neighboring core snapshot
operation; JSON serialization cost more than either geometry path. These are
CPU microbenchmarks, not whole-frame or Inspector performance claims. Cached
sprite/text geometry is updated only when its facts or scale change. Animation
anchor lookup is linear in creature records plus rendered layers, rather than a
roster search per layer. Scene-build and texture-cache preservation on toggles
and actor removal is also covered by synthetic production-system tests.

```sh
GAME_DIR="$PWD/original/k1/eng/game" MAP=17 cargo test \
  --manifest-path crates/Cargo.toml --profile dev-runtime \
  -p kn-fe-viewer --features dynamic --lib \
  debug::tests::measure_actual_map_overlay_cost -- --ignored --exact --nocapture
```
