<a id="gameplay-comparison-across-the-series"></a>

# Сравнение механик серии

Таблицы сравнивают **механики и содержимое** трёх двумерных частей, не устройство движка. Общий движок остаётся гипотезой; см. [план проверки](series.md#verify-the-shared-engine-hypothesis). Доступность внешних источников зафиксирована 2026-11-04; ячейки K1 также учитывают более поздние локальные исследования форматов и исполняемых файлов по указанным ссылкам.

Оценка уверенности — часть каждого утверждения:

| Метка | Значение |
|---|---|
| **Да** | Указанный источник прямо сообщает факт. |
| **≈** | Предварительно: вывод либо предполагаемая преемственность K2.5 от K2. Источник указан, но не утверждает это для данной части. |
| **?** | Нужна проверка, подходящий источник не найден; см. [открытые вопросы](#requires-verification). |
| **—** | К этой части неприменимо. |

`≈ нет` означает отсутствие описания в найденных источниках, **не подтверждённое отсутствие механики**. Количественные строки о монстрах, локациях и кампаниях содержат описание вместо метки. Собственные факты помечены: они получены из original/k1/ (не коммитится), наших парсеров и доступного только для чтения [Marisa-Chan/Slavik](https://github.com/Marisa-Chan/Slavik) со ссылками на строки коммита [cf5a332](https://github.com/Marisa-Chan/Slavik/tree/cf5a3320dc93859e3dc51f37ee823fed30f399b6). Код источника не копируется. Источники указаны у таблиц и сведены со статусом доступности в конце.

<a id="1-characters-and-progression"></a>

### 1. Персонажи и развитие

| Механика | K1 | K2 | K2.5 |
|---|---|---|---|
| Выбираемые герои | Да — 3: Волк, Драгомир, Михаил ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)); собственные данные: Game.0/Game.1/Game.2, по файлу начального состояния на героя ([формат состояния](k1/formats/game-state.md)) | Да — 6, по два на народ ([SoftClub](https://www.softclub.ru/games/pc/19815-knyaz-2), [руководство Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | Да — 4: обычные три и восточный кочевник ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz), [обзор серии gospitaliers](https://gospitaliers.narod.ru/knyaz_opisanie.htm)) |
| Героини | Да — нет: все три названных героя мужчины ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) | Да — 3: Велиславна, Хельга, Анастасия ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), те же имена у [gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm); [PandoraKvest](https://pandorakvest.ru/text/49) считает трёх валькирий) | ? — героев четыре, пол добавленного не указан |
| Начальные классы | Да — 4: вождь, воин, охотник, купец, у каждого основной параметр ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz) и собственное чтение readme ENG) | Да — классы убраны, все параметры и навыки доступны всем, различаются начальные значения ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) | ≈ — основана на K2; заявлено изменение развития без подробностей ([StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy)) |
| Основные характеристики | Да — четыре распределяемых: ловкость, харизма, сила, выносливость; подтверждены обоими EXE и процедурой улучшения. Здоровье — отдельное поле ([правила новой игры](k1/new-game.md#attribute-allocation), [запись персонажа](k1/formats/game-state.md#character--256-bytes)) | Да — 6: сила, выносливость, ловкость, интеллект, обучаемость, харизма ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ — как K2, плюс репутация как новая характеристика ([SoftClub K2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy)) |
| Модель развития | Да — классы и опыт; запись хранит уровень, опыт и свободные очки ([формат состояния](k1/formats/game-state.md)) | Да — свободные навыки и ранги; навык на каждый вид оружия и социальные навыки ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) | ≈ — на основе K2 ([StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy)) |
| Рост навыков от использования | ? | Да — очки на уровне и рост от практики: смешивание повышает алхимию, одноручный меч — соответствующий навык ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm) о лечении и смешивании). Рецензия 2003 года описывает только распределение очков ([PandoraKvest](https://pandorakvest.ru/text/49)); противоречие не разрешено | ≈ |
| Максимальный уровень | ? | ? — сообщается предел характеристики 150, не уровня | ? |
| Ранги по характеристикам | —, эту роль играет система классов | Да — по двум максимальным характеристикам, ступени 1–30 / 31–60 / 61–90 / 91–120 / 121–150; только описание ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ — отличается, не подтверждено |
| Репутация как отдельная механика | ≈ нет | ≈ нет — руководство K2 её не упоминает ([gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | Да — новая характеристика, меняется от поступков ([SoftClub K2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy), [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) |
| Репутация влияет на диалоги и задания | ≈ нет | ≈ нет | Да — диалоги, способы решения отдельных заданий и отношение персонажей ([SoftClub K2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy)) |

Источники: [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz), [SoftClub K2](https://www.softclub.ru/games/pc/19815-knyaz-2), [SoftClub K2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy), [руководство Steam по K2](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [руководство gospitaliers по K2](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm), [PandoraKvest о K2](https://pandorakvest.ru/text/49), [StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy), [собственные данные](k1/formats/game-state.md).

<a id="2-party-and-companion-ai"></a>

### 2. Отряд и поведение спутников

| Механика | K1 | K2 | K2.5 |
|---|---|---|---|
| Наибольший описанный отряд вместе с героем | ≈ 10 — вождь с основной харизмой набирает дружину до десяти ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)); входит ли герой, не подтверждено | ≈ 7 — таблица заканчивается на 6 спутниках сверх героя при харизме 100; следующие строки автор ещё уточняет ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ? — максимум не указан |
| Размер отряда зависит от харизмы | Да ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) | Да ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | Да — тот же механизм, ниже пороги ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) |
| Порог харизмы на спутника | ? | ≈ 16 → 1, 25–35 → 2, 50 → 3, 65 → 4, 80 → 5, 100 → 6; неполная таблица ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ 14 → 1, около 28 → 2, 30–40 → 3 ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) |
| Ограничения найма по герою и заданиям | Да — начальные состояния различаются отрядами, тайниками и выходами, при одинаковых картах, переходах и деревнях ([формат состояния](k1/formats/game-state.md)); квестовые NPC не нанимаются ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) | Да — добровольцы в деревне ([PandoraKvest](https://pandorakvest.ru/text/49)); отдельным нужны высокий уровень и харизма ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ |
| Приказы спутникам | Да — общие атака/следование, выбор группы и одного персонажа (readme ENG, собственное чтение; биты команд в [исследовании существ](k1/creatures/README.md)) | Да — автоматическая смена оружия, защита героя, лечебная склянка при сильном ранении ([PandoraKvest](https://pandorakvest.ru/text/49)); общий сбор на мировой карте | ≈ |
| Самостоятельное поведение | ≈ — данные содержат бродячие группы с пересчётом состояния каждые 16 тиков и работников деревень; значения состояний восстановлены частично ([состояние](k1/formats/game-state.md), [существа](k1/creatures/README.md)) | Да — спутники следуют настройкам, мирные жители уходят ночью ([gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ |
| Различия алгоритмов между частями | ? | ? | ? |

**Пределы отряда.** Набор максимумов 10, 9 и 7 с включённым героем **не подтверждён**. Для K1 есть журналистское описание десяти без уточнения героя; для K2 — таблица, явно исключающая героя и заканчивающаяся шестью спутниками ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)). Для K2.5 максимум не указан. Источники описывают временных спутников на одно задание, но не говорят, занимают ли они слот; явно исключён только герой. См. [открытые вопросы](#requires-verification).

Источники: [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz), [Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm), [PandoraKvest](https://pandorakvest.ru/text/49), собственные [данные](k1/formats/game-state.md) и [исследование существ](k1/creatures/README.md).

<a id="3-world-and-movement"></a>

### 3. Мир и перемещение

| Механика | K1 | K2 | K2.5 |
|---|---|---|---|
| Изометрический двумерный мир | Да ([gospitaliers](https://gospitaliers.narod.ru/knyaz_opisanie.htm), [StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy) указывает вид сверху/изометрию) | Да | Да |
| Ходьба | Да | Да | ≈ |
| Бег | ≈ нет — вместо бега менялась скорость игры ([PandoraKvest](https://pandorakvest.ru/text/49)); readme ENG знает только движение левым щелчком, без клавиши бега | Да — двойной щелчок ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)); добавление во второй части отмечает [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz) | ≈ |
| Управляемая игроком мировая карта | ? — источники эпохи K2 называют её новой, прямого свидетельства отсутствия в K1 нет | Да — значок перемещается по карте, открываемой M, сравнение с ранней Fallout ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm), [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) | ≈ |
| Переходы между локациями | Да — блок переходов начального состояния, порталы подтверждены изображением ([формат состояния](k1/formats/game-state.md)) | Да | ≈ |
| Случайные встречи на мировой карте | ? | Да — монстры прерывают путь, высокий навык следопыта уменьшает встречи ([gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm), [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) | ≈ |
| Корабли | ≈ нет — заявлены как добавление K2, с водными путешествиями и островами ([SoftClub](https://www.softclub.ru/games/pc/19815-knyaz-2)) | Да — покупка корабля, плавание, поиск островов ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ |
| Морские сражения | ≈ нет | Да — добавлены в K2 ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)); случайные столкновения с налётчиками и воинами Владыки ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ |
| Интерьеры зданий | ? | ≈ — описан вход к кладовщику, в казармы с поединками, кузницу ([gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ |
| Пещеры и подземелья | ? | ? | ? |
| Скрытые предметы и тайники | Да — запись тайника; отрицательное золото скрывает его, сундуки — те же записи ([формат состояния](k1/formats/game-state.md)) | ≈ — руководства описывают зарытые и посаженные предметы, остающиеся на месте | ≈ |
| Предметы остаются после ухода с карты | ≈ — readme ENG называет сохранение брошенных вещей отличием сетевого режима, косвенно предполагая удаление в одиночном | Да — посаженные корни после возвращения становятся постоянным растением ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ |
| Управление скоростью игры | Да — numpad +/- меняют множитель проходов обновления за кадр (readme ENG; [Slavik game_input.cpp:183-190](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_input.cpp#L183-L190), [game_play.cpp:122-135](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L122-L135)) | ≈ нет — рецензия сообщает постоянную скорость ([PandoraKvest](https://pandorakvest.ru/text/49)) | ? |

**Добавления эпохи K2.** Бег, мировая карта со случайными встречами, корабли и морские сражения описываются как новшества второй части. Переходы, тайники и жизнь деревень уже есть в данных 1999 года; их нельзя считать новшествами K2.

Источники: [SoftClub K2](https://www.softclub.ru/games/pc/19815-knyaz-2), [Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm), [PandoraKvest](https://pandorakvest.ru/text/49), [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz), [обзор серии gospitaliers](https://gospitaliers.narod.ru/knyaz_opisanie.htm), [StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy), собственные [данные состояния](k1/formats/game-state.md) и [перемещения](k1/movement.md).

<a id="4-combat"></a>

### 4. Бой

| Механика | K1 | K2 | K2.5 |
|---|---|---|---|
| Бой в реальном времени | Да | Да | Да |
| Тактическая пауза с приказами | ? — не описана; readme назначает Esc главному меню | ≈ нет — приходится отдельно отдавать приказ отступления каждому ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)); рецензия 2003 года отмечает отсутствие паузы для приказов и инвентаря ([PandoraKvest](https://pandorakvest.ru/text/49)) | Да — издатель описывает просмотр/изменение боевых приказов и действия с амуницией на паузе ([SoftClub K2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy)); активную паузу отмечает [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz) |
| Оружие в каждой руке | ≈ нет | Да — новая возможность ([SoftClub](https://www.softclub.ru/games/pc/19815-knyaz-2)) и отдельный навык ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ |
| Дополнительные удары | ? | Да — перечислены среди новых возможностей ([SoftClub](https://www.softclub.ru/games/pc/19815-knyaz-2)) | ≈ |
| Арбалеты | ≈ нет — новое оружие K2 ([SoftClub](https://www.softclub.ru/games/pc/19815-knyaz-2)); в разобранных навыках K1 есть стрельба, отдельного арбалета нет ([формат состояния](k1/formats/game-state.md)) | Да — оружие византийцев с отдельным навыком, большей дальностью и уроном, чем лук ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ |
| Дополнительное тяжёлое оружие | ? | Да — названы кастеты, двуручные мечи, топоры и дубины ([SoftClub](https://www.softclub.ru/games/pc/19815-knyaz-2)) | ≈ |
| Магическое улучшение предметов | ? — бонус, яд и концентрация в записи сами по себе не доказывают механику ([формат состояния](k1/formats/game-state.md)) | Да — навык колдовства повышает значения вставленных камней; предел предмета +6 к параметру и +48 к броне/урону, камень не превышает достигнутый предел ([gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ |
| Алхимия | ≈ — эликсиры, масла и яды существуют: поля яда/концентрации и товары лекаря; само смешивание для K1 не описано | Да — ингредиент и сосуд дают бальзам, масло, яд, противоядие, брагу, эликсир; навык определяет концентрацию ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ |
| Новые монстры | Исходный набор | ≈ — расширен: пустынные и горные враги, пираты, воины Владыки | Да — клан Жёлтых Псов пустыни ([SoftClub K2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy), [StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy)) |

**Тактическая пауза отличается от меню паузы.** Издатель K2.5 прямо описывает изменение приказов и работу с амуницией при остановленном бое. Два независимых источника сообщают отсутствие такой возможности в K2; опубликованного свидетельства для K1 нет, readme знает только главное меню по Esc.

**Не новшества второй части.** Алхимические предметы, дружина и структуры управления/строительства деревень имеют основания уже в игре 1999 года. Данные ниже подтверждают наличие структур; доступность конкретных действий игроку требует отдельного доказательства.

Источники: [SoftClub K2](https://www.softclub.ru/games/pc/19815-knyaz-2), [SoftClub K2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy), [Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm), [PandoraKvest](https://pandorakvest.ru/text/49), [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz), [StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy).

<a id="5-settlements-and-economy"></a>

### 5. Поселения и экономика

Колонка K1 в основном основана на собственных файлах. Наличие структур подтверждено парсером и побайтным циклом чтения/записи; **смысл** полей по Slavik остаётся ≈. Подробности: [формат состояния](k1/formats/game-state.md).

| Механика | K1 | K2 | K2.5 |
|---|---|---|---|
| Деревни игрока | ≈ — 15 деревень на одинаковых 15 картах во всех трёх начальных состояниях, из 20 слотов; у каждой отряд старосты и работники. Возможность захвата сами данные не доказывают | Да — опека деревень, дань и защита ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ |
| Управление деревней | ≈ — пять рабочих мест со ссылками на персонажей: староста, лекарь, торговец, кузнец, воевода. Обычно заняты все, в одной деревне только староста ([формат состояния](k1/formats/game-state.md)) | Да — навык управления определяет дань ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ |
| Строительство | ≈ — 119 слотов зданий на деревню: вид, состояние, индекс объекта карты, отсчёт; объявлены два числа зданий, в старте 2–11 одного вида и 0–100 другого. Заказ игроком не установлен | Да — строительство через старосту, заселение мастерами дружины ([PandoraKvest](https://pandorakvest.ru/text/49)); нужен строитель для сгоревших и новых зданий ([gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)); можно заказать мост | ≈ |
| Назначение специалистов | ≈ — пять мест выше | Да — строитель, лекарь, кузнец и охотник ([gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ |
| Производство товаров | ≈ — зелья лекаря, товары торговца и кузнеца записаны на деревню | ≈ — кузнец позволяет тратить часть дани на оружие ([gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ? |
| Сбор дани | ≈ — сумма и срок в записи деревни | Да — уровни дани, начальный низкий; высокий автор руководства не проверял ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ |
| Производство по таймеру | ≈ — поля лекаря, кузнеца, воеводы; все пять таймеров деревни в новом старте нулевые, ненулевые значения должны возникать при игре ([формат состояния](k1/formats/game-state.md)) | ? | ? |
| Срок дани по таймеру | ≈ — поле срока по Slavik, в старте ноль | ? | ? |

Источники: [Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm), [PandoraKvest](https://pandorakvest.ru/text/49), [собственные данные](k1/formats/game-state.md).

<a id="6-time-and-world-simulation"></a>

### 6. Время и симуляция мира

| Механика | K1 | K2 | K2.5 |
|---|---|---|---|
| Смена дня и ночи | Да — цикл 21600 тиков, 21,6 минуты при тике 60 мс и скорости 0; ускорение добавляет проходы счётчика за кадр. Делитель, 13 порогов int16 и условие настройки прочитаны из своих файлов ([игровое время](k1/time.md); [Slavik game.cpp:1764-1890](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1764-L1890), [game.h:28](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.h#L28), [тик перемещения](k1/movement.md)) | Да — переключаемый режим суток ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ |
| Освещение меняется со временем | Да — фоновый цвет действует на землю, объекты и существ с отдельными условиями; маски LIGHTS.N освещают землю только в интервале внутри более длинного тёмного участка ([освещение](k1/lighting.md), [LIGHTS.RES](k1/formats/lights-res.md)) | Да — жалобы на непроглядную ночь ([PandoraKvest](https://pandorakvest.ru/text/49)) | ≈ |
| Постоянный день | Да — настройка ограничивает **только** вычисление света, счётчик продолжается; обе точки увеличения вне ветвей настройки. Собственное чтение; постоянный свет карты проверяется раньше и остаётся исключением ([время](k1/time.md), [Slavik game.cpp:120](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L120), [game.cpp:1776](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1776), [game.h:1464](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.h#L1464)) | Да — отключение смены суток оставляет день ([gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ |
| Ночь влияет на обычных NPC | ? | Да — мирные жители не разговаривают поздно ночью, главные персонажи доступны всегда ([PandoraKvest](https://pandorakvest.ru/text/49), [gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ |
| Торговцы недоступны ночью | ? | Да — часть жителей недоступна по тому же правилу | ≈ |
| Ночные существа/духи | ? | Да — дух у могилы появляется ночью, только при включённой смене суток ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ |
| Задания зависят от времени суток | ? | Да — ожидание сна кладовщика ночью и возвращения утром ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ |
| NPC спят и меняют доступность | ? | Да — те же свидетельства выше | ≈ |
| Производственные таймеры | ≈ — поля деревень ([формат состояния](k1/formats/game-state.md)) | ? | ? |
| Экономические таймеры | ≈ — сумма и срок дани | ? | ? |
| Характеристики монстров меняются ночью | ? | ? | ? |
| Постоянный день останавливает часы мира | Да — **не останавливает**: оба увеличения счётчика вне ветвей настройки в обоих изданиях и tutorial ([счётчик](k1/time.md#the-day-counter-and-the-length-of-a-cycle), [порядок проверок](k1/lighting.md#the-order-of-the-checks)) | ? | ? |

Для K1 есть цикл в коде и таймеры в данных; для K2 — руководства о переключателе, спящих жителях, ночном духе и задании с ожиданием темноты. Для K2.5 преемственность предполагается, прямого описания не найдено.

Источники: [Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [gospitaliers](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm), [PandoraKvest](https://pandorakvest.ru/text/49), [Slavik](https://github.com/Marisa-Chan/Slavik) только для чтения, собственные [данные состояния](k1/formats/game-state.md), [света](k1/formats/lights-res.md), [перемещения](k1/movement.md).

<a id="7-narrative-and-quests"></a>

### 7. Сюжет и задания

| Механика | K1 | K2 | K2.5 |
|---|---|---|---|
| Разный сюжет у героев | ≈ — различаются стартовая карта, отряды, тайники, выходы ([формат состояния](k1/formats/game-state.md)), но сюжет после вступления источником не подтверждён | ≈ — шесть героев трёх народов ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)); у одного только мужчины в отряде, у другого женщины, но отдельный сюжет не заявлен | ≈ — четыре героя со своими линиями ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) |
| Разнообразие заданий | ≈ — QUEST*.RES вне начального состояния, диалоги выбирает байт записи ([формат состояния](k1/formats/game-state.md)); разнообразие типов не подтверждено | Да — основные и побочные, не всегда однотипные ([PandoraKvest](https://pandorakvest.ru/text/49)) | ≈ — преемственность K2; более 100 локаций и 1000 игровых сцен — масштаб контента от издателя, не доказательство типов заданий ([SoftClub K2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy), [StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy)) |
| Репутация меняет NPC и диалоги | ≈ нет | ≈ нет | Да ([SoftClub K2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy), [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) |
| Несколько концовок | ? | ? | Да ([SoftClub K2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy)) |
| Озвученные диалоги | ? | ? | ? |
| Диалоги зависят от суток | ? | Да — мирные жители говорят днём, есть ночные шаги заданий ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [PandoraKvest](https://pandorakvest.ru/text/49)) | ≈ |

Источники: [SoftClub K2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy), [Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [PandoraKvest](https://pandorakvest.ru/text/49), [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz), [StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy), [собственные данные](k1/formats/game-state.md).

<a id="8-technical-and-content-differences"></a>

### 8. Техника и содержимое

| Возможность | K1 | K2 | K2.5 |
|---|---|---|---|
| Сетевой режим | Да — ENG содержит LAN/DirectPlay: LAN_KONG.EXE, SERVER, Servers.cfg; RUS только CLIENT.EXE, состав зависит от издания ([издания](k1/editions.md)). Собственное readme: до 16 игроков трёх народов, перенос амулета, отдельный предел двух спутников, сохранение брошенных вещей. По историческому обзору официальные серверы давно закрыты ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) | ≈ нет — отсутствует в таблице серии ([gospitaliers](https://gospitaliers.narod.ru/knyaz_opisanie.htm)) | ≈ нет — та же таблица, карточка указывает одиночную игру ([StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy)) |
| Разрешения | Да — 640×480, 800×600: MENU640R/E/T.RES, MENU800R/E/T.RES и аналогичные INTO, INTR, ME, NEW, NEWH, PLAY, SAVE; настройка разрешения в исходном меню, собственное наблюдение. Потолок 800×600 отмечает [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz) | ≈ 1024×768 — минимум требует видеокарту с этим режимом и 16 бит ([SoftClub](https://www.softclub.ru/games/pc/19815-knyaz-2)); рецензия говорит лишь о росте разрешения ([PandoraKvest](https://pandorakvest.ru/text/49)) | ≈ 1024×768 — предполагается по базе, не подтверждено |
| Отдельные сюжетные кампании | ≈ — три героя с разными стартами ([формат состояния](k1/formats/game-state.md)); число героев не доказывает отдельные кампании | ≈ — шесть героев ([Steam](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)), тот же предел вывода | ≈ — четыре героя ([gospitaliers](https://gospitaliers.narod.ru/knyaz_opisanie.htm)), тот же предел вывода |
| Общая игровая база с K2 | — | Базовая часть | Да — самостоятельное дополнение фанатской группы на базе второй игры, издано 1С, заполняет промежуток между K1 и K2 ([gospitaliers](https://gospitaliers.narod.ru/knyaz_opisanie.htm), [StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy)) |
| Совместимость сохранений K2/K2.5 | — | Не подтверждена | Не подтверждена |
| Совместимость бинарных форматов | Не подтверждена | Не подтверждена | Не подтверждена |
| Новые локации и содержимое | Исходная игра | Новое содержимое | Более 100 локаций, новые здания, деревья и элементы южного ландшафта, новая фракция ([SoftClub K2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy)) |

Источники: [gospitaliers](https://gospitaliers.narod.ru/knyaz_opisanie.htm), [SoftClub K2](https://www.softclub.ru/games/pc/19815-knyaz-2), [SoftClub K2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy), [PandoraKvest](https://pandorakvest.ru/text/49), [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz), [StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy), собственное наблюдение [распакованных изданий](k1/editions.md).

<a id="requires-verification"></a>

## Требуется проверка

1. **Пределы отряда:** входит ли герой в десять K1; максимум K2 после шести спутников; максимум K2.5; занимают ли слот временные спутники.
2. **Пороги харизмы:** недостающие строки K2/K2.5 и ограничения найма по герою.
3. **Максимальный уровень** всех частей. 150 в K2 — предел характеристики. Для утверждения «до 13» в K1 источник не найден, оно не включено в таблицу.
4. **Ранги K2.5:** сохранение и отличия системы K2.
5. **Скорость игры K2.5:** в K1 управление есть, для K2 сообщается отсутствие.
6. **Мировая карта K1:** новшеством её называют только источники эпохи K2, прямого свидетельства для K1 нет.
7. **Случайные встречи K1** и существование путешествия по мировой карте.
8. **Интерьеры, пещеры, подземелья:** прежде всего K1; карты не содержат названий. Пещеры и подземелья требуют проверки во всех частях, интерьеры K2 пока косвенны.
9. **Тактическая пауза K1:** readme знает лишь Esc для главного меню; отсутствие не подтверждено.
10. **Магическое улучшение и смешивание зелий K1:** поля предметов есть, действия вокруг них не описаны.
11. **Таймеры производства и дани K2/K2.5:** связь с часами и интервалы.
12. **Ночь K1:** обычные NPC, торговцы, ночные существа. **Поведение ночью K2.5:** все утверждения пока перенесены по предположению из K2.
13. **Останавливает ли постоянный день часы K2/K2.5.** Для K1 ответ установлен чтением EXE: только свет, счётчик продолжается ([время](k1/time.md), [освещение](k1/lighting.md)).
14. **Характеристики монстров ночью:** нет источников для всех частей.
15. **Озвученные диалоги:** нет источников для всех частей.
16. **Несколько концовок K1/K2.**
17. **Героини K2.5:** известно число четыре, не распределение по полу.
18. **Разрешения K2/K2.5:** есть минимальные требования и замечание о росте разрешения, нет списка режимов.
19. **Сетевая игра K2/K2.5:** сторонняя таблица и карточка одиночной игры, нет прямого заявления разработчика об отсутствии.
20. **Совместимость сохранений и форматов:** K2/K2.5 и всех трёх частей; см. [проверку движка](series.md#verify-the-shared-engine-hypothesis). Эти таблицы вопроса не решают.
21. **Отдельные кампании и сюжет героя:** число героев и стартовые различия подтверждены, отдельные кампании и полный сюжет по герою прямо не установлены источниками.
22. **Разнообразие заданий K2.5:** числа локаций/сцен отражают масштаб; нужен источник о типах заданий.

<a id="sources-for-the-comparison"></a>

## Источники сравнения

Доступность проверялась 2026-11-04; это исторический срез.

| Источник | Использование | Статус на дату проверки |
|---|---|---|
| [SoftClub: K2](https://www.softclub.ru/games/pc/19815-knyaz-2) | Список издателя: герои, навыки, корабли, оружие в двух руках, новые виды оружия, поселения; требование 1024×768 | Доступен |
| [SoftClub: K2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy) | Репутация, пауза с приказами/амуницией, более 100 локаций, фракция, несколько концовок | Доступен |
| [Руководство Steam по K2](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179) | Герои и пол, шесть параметров, рост навыков, ранги и предел 150, харизма K2 и K2.5, бег, мировая карта, корабли, ночь, алхимия, деревни | Доступен |
| [Руководство gospitaliers по K2](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm) | Герои, параметры, навыки, дань, строители, камни улучшений, постоянный день | Доступен |
| [Обзор серии gospitaliers](https://gospitaliers.narod.ru/knyaz_opisanie.htm) | Число героев 3/6/4, сеть, даты, самостоятельное фанатское дополнение K2.5 от 1С | Доступен |
| [DTF: история серии](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz) | Герои и классы K1, дружина десяти, отказ от классов в K2, бег/карта/море как новшества K2, репутация и активная пауза K2.5, предел 800×600 и сеть K1 | Доступен |
| [PandoraKvest: рецензия K2](https://pandorakvest.ru/text/49) | Сравнение 2003 года с K1: бег/скорость, отсутствие паузы приказов, ночь, строительство/мастера, настройки спутников | Доступен |
| [StopGame: K2.5](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy) | Фанатская разработка, издатель 1С, завязка, фракция, масштаб, изменение развития | Доступен |
| [Marisa-Chan/Slavik](https://github.com/Marisa-Chan/Slavik) | Только чтение: счётчик суток, фазы, настройка смены суток, множитель скорости K1; ссылки закреплены на [cf5a332](https://github.com/Marisa-Chan/Slavik/tree/cf5a3320dc93859e3dc51f37ee823fed30f399b6), код не копируется | Доступен |
| [MobyGames: Legend of the North: Konung](https://www.mobygames.com/game/3325/legend-of-the-north-konung/) | Планировались технические сведения K1 | **Недоступен**, 403; утверждения выше на него не опираются |
| [Cyclowiki: K2.5](https://www.cyclowiki.org/wiki/) | Планировался список героев K2.5 | **Недоступен**, 404; число героев взято из DTF/gospitaliers |
| Страница каталога ag.ru о K2.5 | Планировались герои и требования | **Недоступна**, перенаправляет на 404; не использована |

Собственные наблюдения ограничены original/k1/ и результатами парсеров: имена/размеры файлов, структура записей, количества и readme ENG. Игровые тексты, спрайты и рендеры не попадают в репозиторий.

**Общий движок не утверждается.** Общая база K2.5 и K2 — опубликованное свойство продукта. Отношение движков K1 и K2 остаётся гипотезой из [обзора серии](series.md).
