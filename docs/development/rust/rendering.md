<a id="bevy-scene-rendering"></a>

# Отрисовка сцены в Bevy

Bevy 0.20 отображает общую сцену kn-game. Окно и GPU нужны для интерактивного рендеринга; проверки логики работают без них. Данные и правила оригинала приходят через фасад; границы описывает [архитектура](../../../ARCHITECTURE.md).

<a id="static-sprites-and-animation"></a>

## Статические спрайты и анимация

- У пикселей карты начало координат слева сверху, y растёт вниз; у спрайтов Bevy координаты задают центр, y растёт вверх. Элемент `(x,y,w,h)` получает центр `(x+w/2, −(y+h/2))` (`piece_transform`). Отрицательные координаты допустимы, как в Python render.blit.
- Прозрачные спрайты сортируются по возрастанию z (`bevy_sprite_render` использует FloatOrd(transform.z)); элемент n получает `n · Z_STEP`.
- TextureCache создаёт один Image на TextureId. Примерно 6000–13000 элементов карты используют около 250–300 базовых изображений, до 500 с кадрами анимации. Фильтрация Nearest сохраняет пиксельную графику.
- Анимация использует Sprite/Handle<Image> с Sequence, а не отражаемые кривые свойств bevy_animation. В примере спрайтов Bevy кадры тоже переключаются вручную. Один Clock на карту считает тики; sub_tick хранит дробную часть времени кадра, необходимую при тике 60 мс и дисплее 120 Гц.
- Playing продвигает часы, Paused удерживает, Off обнуляет и показывает базовое изображение. Поля Sprite меняются только при смене кадра. piece_center вычисляет центр нового изображения по той же формуле: кадры K1 разного размера не должны сдвигать привязку к левому верхнему углу.

<a id="visibility-and-bounds"></a>

## Видимость и границы

Иерархия: MapRoot → SceneGroup → спрайты. Корни и группы должны иметь Transform и Visibility: преобразования родителей доходят до детей только по цепочке с корневым Transform; sync_simple_transforms пропускает сущности с ChildOf. Без преобразований сущности существуют, но не рисуются.

Спрайты наследуют видимость, кроме пустых кадров с Hidden. Аниматоры восстанавливают Inherited, а не Visible, который переопределил бы скрытую группу. Просмотрщик один раз строит SceneOptions::ALL и меняет видимость групп; Rebuild нужен только при смене карты. Фасад задаёт соответствие групп диагностическим слоям; соседство в дереве не заменяет порядок z в списке отрисовки.

SceneGroup хранит границы всех кадров анимации. sync_groups пересчитывает CameraBounds по видимым группам: диагностическая мелкая сетка выходит за нарисованные тайлы. Позиция и масштаб камеры сохраняются; frame_scene вызывается только для новой карты. Границы групп и scene_bounds используют piece_bounds; контракт проверяет `group_bounds_cover_the_pieces_a_layer_shows`.

<a id="camera-and-input-seam"></a>

## Камера и граница ввода

ZOOM_LEVELS задаёт дискретные масштабы, scale = 1 / zoom; целые масштабы сохраняют выравнивание пикселей. kn-input владеет pan_direction, drag_delta, wheel_steps и ZoomScroll; kn-ui — pointer_over_ui. Каждый фронтенд связывает ввод в CameraInput; плагин рендеринга выполняет CameraApply после него: apply_pending_frame, затем apply_zoom. Связка занимает около 20 строк; вычисления и константы имеют одного владельца.

Один шаг колеса или около 60 пикселей жеста трекпада меняют масштаб на ступень. ZoomScroll.push накапливает ввод; take выдаёт не больше MAX_ZOOM_STEPS_PER_FRAME, остаток переносится на следующие кадры. ZOOM_ACCUM_LIMIT ограничивает избыток. Над интерфейсом очередь целиком очищается, чтобы жест не изменил масштаб после ухода указателя с панели. Захват указателя читает булево значение Hovered, а не наличие компонента; корни панелей тоже несут его, покрывая промежутки между элементами.

frame_scene берёт границы содержимого, а не всей сетки 80×160, выбирает fitting_zoom и откладывает действие в PendingFrame до появления камеры. Применение перед масштабированием достигает проекции в том же обновлении (`a_framed_scene_reaches_the_projection_on_the_same_frame`). Перемещение — WASD/стрелками либо левой кнопкой вне интерфейса. Основания описаны в [принципах продукта](../design.md), управление — в [руководстве просмотрщика](../../guides/viewer.md).

<a id="creature-nodes"></a>

## Узлы существ

Существо представлено одним постоянным kn_render::CreatureNode под CreatureRoot. Узел владеет мировой позицией; тело, экипировка и тень — локальными смещениями и приращениями кадров. Только sync_creature_nodes записывает мировую позицию. Второе независимое перемещение спрайта учитывало бы движение дважды; поэтому прежний путь CreaturePiece/update_shadow_depth удалён.

- **Общий конвейер принадлежит плагину `kn-render`; оба окна подключают его.** CreatureAnimationPlugin владеет ресурсами CreatureSim, CreatureAnim, CreatureSpawns, CreatureDepth, CreatureMotion, CreatureAnimReport, CreatureNodesVisible, пятью системами и их порядком относительно часов сцены. Фронтенд вызывает `CreatureMotion::request(map, seed, scene)` и предоставляет GameResource, TextureCache, Assets<Image> и Clock из AnimPlugin; отдельных часов подсистема не создаёт. Метки `#id` и диагностика принадлежат просмотрщику и подключаются после CreatureSet::Nodes. Игровой фронтенд делает один запрос kn_fe_player::request_start_creatures с тем же seed, что и Reset positions просмотрщика: одна карта размещается одинаково в обоих окнах. До появления запроса игра не показывала существ: spawn_scene пропускает элементы с creature_id, а составные узлы никто не создавал. Регрессия покрыта `crates/frontend/bevy/player/tests/creatures.rs`.
- **Узел владеет миром, дочерние спрайты — кадром.** Узел получает `x, −y` из симуляции и глубину z; apply_animators записывает центр каждого дочернего спрайта относительно его базового смещения (Animator::with_base), никогда — мировой центр. Узел не имеет Animator: смена кадра меняет размер и положение спрайта внутри существа, но не двигает его по миру (`an_animation_frame_change_does_not_move_the_node_world_position`, `body_equipment_and_shadow_ride_the_node`). Корень и узел имеют Transform для propagate_parent_transforms.
- **Запросы узлов, спрайтов и меток не пересекаются.** sync_creature_nodes меняет узел и тень через `&mut Transform`; запрос узлов содержит Without<CreatureShadow> и Without<CreatureSprite>, поскольку преобразования спрайтов пишет анимация. Запрос sync_creature_labels содержит Without<CreatureSprite> и Without<CreatureNode>. Метки следуют изображению тела, а не ячейке. Без фильтров Bevy завершал просмотрщик паникой B0001 ещё до движения (`creature_node_and_label_queries_are_disjoint`).
- **Слой сохраняет идентичность через LayerKey, а не индекс.** kn_game::CreatureClips возвращает `CreatureLayer { base, animation, corner, key }`: базовое изображение, кадры, абсолютный угол сборки и часть спрайта. Pose::layer_keys в k1-game передаёт ключ из сборки позы. Внутри одной позы допустим порядок списка: элементы сцены и клипы покоя получены одной сборкой (`the_rest_clips_are_anchored_where_the_scene_draws_the_layer`). Между позами сопоставление выполняется по key (pose_matched_layers): порядок первого появления дорожек зависит от позы; соединение по индексу присваивало ходьбу другой части (`walk_layers_join_to_their_part_not_their_position`). Локальный z тела и экипировки сохраняет порядок позы сцены (`body_layers_keep_their_scene_order_depth`).
- **Смена клипа переносит привязку на собственный базовый угол клипа.** dx/dy каждого кадра отсчитываются от базового угла именно этого клипа (layer_track в k1-game); углы покоя и ходьбы различаются. Смена Sequence при старой привязке Animator давала скачок изображения на разницу углов при смене направления или покоя/ходьбы. Animator::play_at меняет привязку вместе с последовательностью; apply_clip переводит абсолютный угол в локальные координаты существа. Это одна атомарная запись (`play_at_reanchors_the_piece_on_the_new_clip_corner`, `a_clip_swap_reanchors_the_image_and_never_moves_the_node`, `apply_clip_reanchors_on_the_clips_own_corner`). Узел при смене клипа не меняется.
- **Однокадровый слой покоя мог бы блокировать анимированную ходьбу; в данных такого случая нет.** layer_track создаёт animation только для многокадровых дорожек, clip_seq требует его. Однокадровый слой появляется без Animator и недоступен sync_creature_animation, запрашивающему `&mut Animator`. Проверены все карты обоих изданий: ни у тела, ни у тени нет статического слоя покоя с анимированной ходьбой того же LayerKey. Изменения без подтверждённого случая не внесены; ограничение сохранено для будущих данных.
- **Глубину задают только стопки статических объектов; тень занимает слот существа.** DepthMap строится по нижним границам PieceGroup::Objects (is_depth_relevant принимает только Objects), без персонажей, земли и маркеров. Огни исключены: они рисуются вместе с объектом, а их более высокая нижняя граница ошибочно выводила бы существо перед горящим объектом; на RUS-карте 1 был ровно один такой случай. Одинаковые нижние границы сворачиваются к наибольшему ключу сортировки. Вклад ниже предыдущей границы ничего не добавляет и не уменьшает её: поиск и интерполяция требуют монотонного z (`depth_map_uses_only_static_object_stacks`, `equal_bottoms_collapse_to_the_highest_sort_key`, `depth_boundaries_stay_monotonic_on_every_map`, `flame_pieces_are_recognised_as_part_of_their_object`). DepthMap::creature_slot помещает существо между двумя границами: тень над статикой под ногами и строго под телом. Оригинал рисует их вместе; прежняя фиксированная полоса под всей картой скрывала тень под землёй. Интерполированная база резервирует место всей стопке: тело и каждый слой остаются ниже следующей границы при любом перемещении, без перезаписи локальных смещений (`the_slot_orders_shadow_body_and_layers_inside_the_boundaries`, `a_narrow_slot_keeps_every_layer_under_the_next_static`, `production_spawn_places_the_shadow_in_its_slot`, `a_reset_returns_nodes_animators_shadows_and_labels_home`).
- **Визуальный шаг сдвинут на тик, логический — нет.** Шаг начинается при step_elapsed == 1: по Slavik счётчик кадра увеличивается до первой отрисовки, первый видимый кадр имеет индекс 1. Этот логический контракт сохранён для движка и тестов. Ошибка была в интерполяции: она считала единицу уже прошедшим тиком и мгновенно сдвигала изображение. smooth_pixel использует `(step_elapsed − 1 + sub_tick)`, ограниченное `[0, step_ticks]`; обе границы непрерывны (`one_tick_into_a_four_tick_step_sits_a_quarter_across`, проверка шагов в 1/2/4 тика `movement_is_continuous_across_step_boundaries`).
- **Направление симуляции одно на весь шаг; дрожание возникало в отображении.** Прежнее чередование влево/вправо вызвала указанная интерполяция. Для диагностики KN_CREATURE_DEBUG=1 пишет по id тик, ячейку, начало/конец шага, step_elapsed, интерполированный пиксель, позицию узла и GlobalTransform. sync_creature_labels просмотрщика добавляет выбранный клип каждого спрайта (clip_identity), базовый угол, привязку, фазу, показанный кадр, локальное смещение и итоговую мировую позицию. Сравнение мировых позиций спрайта и узла отделяет скачок картинки от скачка узла. Логирование принадлежит просмотрщику; общая подсистема не пишет лог и не передаёт эту диагностику игре.
- **Порядок конвейера задаёт плагин.** CreatureAnimationPlugin связывает CreatureSet::Prepare → Advance → Spawn → Swap → Nodes. Первые четыре идут after(AnimSet::Clock); AnimSet::Sprites расположен между Swap и Nodes. Сначала устанавливаются часы, затем догоняет симуляция, выбираются клипы, обновляются спрайты и размещаются узлы. Фронтенд не перечисляет системы заново. Просмотрщик лишь ставит свою цепочку `.after(AnimSet::Clock).before(CreatureSet::Prepare)`, а метки — после CreatureSet::Nodes.
- **Смена карты и сброс фиксируют эпоху; Step назад пересоздаёт и проигрывает симуляцию.** rebuild_scene через CreatureMotion::request и reset_positions через reset_creature_sim фиксируют CreatureSim::epoch на текущем Clock.ticks, чтобы новая симуляция не проигрывала всю историю за кадр (`map_change_pins_the_epoch_so_history_is_not_replayed`). Обратного тика нет: advance_creature_sim пересоздаёт карту и проигрывает её вперёд до цели относительно эпохи. После этого эпоха фиксируется на clock.ticks; прежний сброс к нулю запускал ещё clock.ticks шагов и уводил существ дальше цели (`a_backward_step_rewinds_the_simulation_to_the_clock_not_beyond_it`, `replay_simulation_matches_forward_stepping`). Автомат состояния обновляется один раз за tick внутри симуляции; после перемотки, включая середину особой анимации состояния 4, виден точный целевой тик (`rewinding_the_clock_replays_to_the_positions_a_forward_run_reached`). Исторический замер худшего случая — 30 000 тиков, 30 игровых минут, настоящая первая карта, полные правила и движок на каждом тике, Apple M-series: 306 мс release (10,2 мкс/тик), 4,3 с debug. Проверка `replaying_a_long_session_stays_a_bargain`, release запускался через `cargo test --profile release`. Это разовая цена нажатия, не каждого кадра; после сброса эпохи короткая перемотка почти ничего не проигрывает.
- **Поза пересобирается при смене состояния, не каждого кадра.** Автомат состояния 4 k1_game::creature_anim::Machine принадлежит игровому слою и не имеет часов. Подсистема читает стадию каждый кадр, но sync_creature_animation пересобирает позу только при смене вида Special ↔ Idle, а не счётчика кадров внутри Special (`the_stage_snapshot_rebuilds_only_on_a_state_kind_flip`, `counted_run_shows_one_rebuild_per_state_change_and_none_per_frame`). Исторический замер на всех Game.0–Game.2 обоих изданий: 404 записи состояния 4 (249 Skeleton, 152 Eechetik, 3 Ion); загрузка карты и тики до завершения всех последовательностей дают ровно две пересборки на существо — вход в Special и выход в Idle. Ещё тысяча тиков покоя не даёт ни одной (`pose_rebuilds_count_one_per_state_four_creature_and_no_more`). Стоимость на существо — O(смен состояний), а не O(тиков).

Узлы существ находятся под CreatureRoot общей подсистемы и сразу получают запрошенную фронтендом CreatureNodesVisible. Метки просмотрщика находятся под его LabelRoot. sync_creature_visibility управляет обоими корнями по видимости слоя Characters; оба создаются с этой видимостью. Фиксированный Visible раньше показывал существ на один кадр после смены карты, до синхронизации (`roots_are_born_hidden_when_characters_are_off`). Отключение слоя скрывает и существ, и имена.

<a id="creature-light-material-lifetime"></a>

## Материал существ на границе освещения

В [#119](https://github.com/shpaker/re-konung/issues/119) зафиксированы два
прерывания GPU-рендера на Windows/Vulkan: карта 19 RUS, `--benchmark-secs 60`,
`KN_VIEWER_LABELS=0`, примерно на 14-й и 38-й секундах. Ошибка wgpu указывала
на несовместимость bind group `KonungLightMaterial` с pipeline
`SpriteMeshMaterial`. В Bevy 0.20.1 смена типа проходит через
`bevy_sprite_render/src/sprite_mesh/sprite_extended_material.rs`
(`add_material`, `SpriteMaterial::on_remove`); сохранённая фаза и текущая
таблица материалов читаются в `src/mesh2d/material.rs`. Сбой находится на границе нашего переключения материалов и
[Bevy](https://github.com/bevyengine/bevy). Точный путь пропуска специализации
остаётся гипотезой; детерминированного воспроизведения исходного падения пока нет.

Наше исправление сохраняет `SpriteMaterial<KonungLightMaterial>` на теле
и экипировке с момента создания. В тёмной клетке выбирается `ambient()`,
в освещённой — `neutral()`: меняется только handle, без удаления компонента.
Тени продолжают использовать обычный материал спрайта. Пул добавляет один
нейтральный материал, всего 22 при 19 масках; он исключён из обновления
uniforms. Нулевые blend и light_gain возвращают стандартный результат
спрайта до преобразования цветового пространства, сохраняя tint и alpha.
В неизменившейся клетке компонент не помечается изменённым.

Оба фронтенда устанавливают `LightingMaterials` до создания существ.
Потребитель без пула может создавать спрайты без освещения; включение пула
после такого создания не поддерживается. Замена уже установленного пула
сохраняет тип компонента и обновляет handle. Смена позы меняет кадр и
аниматор существующего слоя, поэтому не возвращает обычный тип материала.

Синтетические тесты проверяют тело, статическую экипировку, тень и скрытый
корень на повторных переходах свет/тьма; тест просмотрщика проходит реальный
маршрут через выделенную освещённую клетку. GPU-пример `lighting_probe`
дополнительно переключает handle 240 раз и сравнивает нейтральный спрайт
с обычным при одинаковых tint и alpha:

```sh
cargo run --release --manifest-path crates/Cargo.toml -p kn-render \
    --example lighting_probe -- out/issue-119/probe.png
uv run python tools/check_lighting_probe.py out/issue-119/probe.png
```

Проверка 2026-11-15: macOS 27.0.1, Apple M4 Pro (24 ГБ), Metal,
Rust/Cargo 1.99.1, Bevy 0.20.1, release. GPU-пример прошёл 240 переключений:
нейтральный и стандартный спрайты дали одинаковый RGBA `(61,79,98,255)`;
шесть прежних палитровых случаев также прошли. Снимки исходного вида карты
19 RUS при остановленной симуляции, без подписей, с `--light-tick 0` и
`11000` до/после совпали пиксель в пиксель (2560×1600).

<a id="layer-switch-cost"></a>

## Стоимость переключения слоёв

Исторический замер: ENG, карта 17, release, Apple M4 Pro; 12501 элемент со всеми слоями, 7180 без диагностических. Измеряется ECS и распространение видимости, без рендерера.

| Операция | Время |
|---|---|
| Разбор списка отрисовки, все слои / чистая карта | 26 мс / 18 мс |
| Удаление корня и создание 12501 спрайта, без разбора | 6,4 мс, максимум 6,5 мс |
| Смена видимости одного узла для 5131 спрайта | 0,15 мс, максимум 0,20 мс |
| Подсчёт панелью 12501 значений InheritedVisibility | 0,5 мкс/кадр |

Прежнее переключение заново разбирало и строило сцену: около 30 мс за обновление (38–43 мс с рендерингом в #30). Сейчас переключение этого не делает; взамен открытие карты 17 стоит на 8 мс больше и сохраняет 12501 сущность вместо 7180. У скрытых групп ViewVisibility = false, они не извлекаются для рендеринга, но остаются в ECS. Панель считает спрайты, а не детей групп, чтобы исключить пустые кадры анимации; 0,5 мкс подсчёта дешевле переключения за 150 мкс.

## Lighting audit measurements

The [K1 lighting audit](../../research/k1/lighting.md) corrects palette-space rendering,
packed ramp arithmetic, integer ground-light levels and constant-map selection.
Measurements on the same local Apple Silicon machine, 2026-11-15, using the existing
release `lighting_cost` and `time_of_day_cost` tests (nanoseconds per call):

| Operation | Before | After |
|---|---:|---:|
| Ambient triple over complete cycles | 2.20 | 2.29 |
| Complete game-time readout | 5.35 | 5.36 |
| Parameters plus three-channel application | 17.48 | 21.78 |
| Night-palette selection | 1.08 | 1.03 |

The added integer-half table factors cost about 4.3 ns in the combined CPU harness;
this is about four fifths of one game-time readout, once per lighting update, not
per sprite. The ramp correction adds no measurable whole-clock cost in this run.

The actual GPU path was exercised with a synthetic pixel probe and before/after
map captures. Two 600-frame `dev-runtime` viewer measurements per day/night case
on RUS map 17 kept 12,487 sprites and 21 materials. Before: average 18.95–19.07 ms,
p95 19.58–19.71 ms. After: average 3.28–19.06 ms, p95 17.49–19.70 ms. The large
within-run/platform scheduling variation makes a GPU-cost delta **inconclusive**;
these are diagnostic local measurements, not a replacement for the established
release frame-time gate or evidence of a speedup. Raw logs are local under
`out/lighting-audit`; the parallel session's unrelated changes are not attributed
to this audit.
