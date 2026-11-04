# Игры серии «Князь»

Цель проекта — работать с частями, сделанными на одном движке. По всей видимости, это первая часть, вторая и «2.5». Это **гипотеза**: проверка — сравнить форматы файлов, когда будут файлы второй части. Третья часть («Новая династия», 2009) трёхмерная и почти наверняка на другом движке, она вне рамок проекта.

| Часть | Год | Разработчик | Издатели | Движок | В проекте |
|---|---|---|---|---|---|
| «Князь: Легенды Лесной страны» / *Konung: Legends of the North* | 1999 (RU), 2000 (EN) | Snowball Interactive, 1С | 1С; Strategy First (EN) | свой, изометрия 2D, 640×480 и 800×600 | этап 1, есть RUS и ENG |
| «Князь 2: Кровь Титанов» / *Konung 2: Blood of Titans* | 2003 (RU), 2004 (EN) | Snowball Interactive, 1С | 1С; Got Game Entertainment (US) | гипотеза: развитие движка первой части | нужна копия игры |
| «Князь 2.5: Продолжение легенды» | 2004–2005 | команда фанатов на основе «Князя 2» | 1С | гипотеза: движок «Князя 2» | нужна копия игры |
| «Князь 3: Новая династия» | 2009 | другая команда | 1С | 3D, другой движок | нет |

Сведения о годах и разработчиках собраны из [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz), [Cyclowiki](https://www.cyclowiki.org/wiki/%D0%9A%D0%BD%D1%8F%D0%B7%D1%8C_2:_%D0%9A%D1%80%D0%BE%D0%B2%D1%8C_%D0%A2%D0%B8%D1%82%D0%B0%D0%BD%D0%BE%D0%B2), [StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy) и [Wikipedia](https://en.wikipedia.org/wiki/Konung:_Legend_of_the_North). Источники местами расходятся: на Wikidata у первой части указан Infinity Engine, но без подтверждения, и это почти наверняка ошибка.

## Что известно о движке первой части

- Изометрическая карта со сдвигом каждой второй строки, ромб тайла 114×64, шаг 116×32 ([k1/formats/map.md](k1/formats/map.md)).
- Графика — 8-битные спрайты с построчным RLE и палитрами из общего банка RGB555 ([k1/formats/rle.md](k1/formats/rle.md)).
- Звук — PCM 22050 Гц в `SOUNDS.RES`, музыка — CD-аудио на диске; в коллекционном издании — OGG в `MUSICS.RES` (по [rtsk_konung_mus_unpacker](https://github.com/dopedealer/rtsk_konung_mus_unpacker)).
- Ролики `*.flm` проигрывает `playavi.dll`.
- Интерфейс хранится в файлах под каждое разрешение и язык: `MENU800R.RES` — меню, 800×600, русский; `E` — английский, `T` — третий язык ENG-издания (какой — неизвестно).

## Издания первой части

| Издание | У нас | Заметки |
|---|---|---|
| RUS, CD 1999 | `original/k1/rus` | CD + 10 аудиодорожек, демо «Аллоды 3» и WAR — [k1/editions.md](k1/editions.md) |
| ENG, CD 2000 (Strategy First) | `original/k1/eng` | новее: туториал, сетевая игра, третий язык — [k1/editions.md](k1/editions.md) |
| Коллекционное издание 1С (переиздание) | нет | по сторонним данным, музыка в OGG (`MUSICS.RES`) |
| GOG | нет | под него есть HD- и widescreen-патчи (PCGamingWiki) |

## Как проверить гипотезу об общем движке

1. Взять свои копии «Князя 2» и «2.5» и подготовить файлы по той же схеме ([k1/editions.md](k1/editions.md)).
2. Сравнить имена и размеры файлов с первой частью: есть ли `GRAPH.RES`, `OBJECTS.RES`, `KONUNG.N`.
3. Прогнать парсеры `konung/k1` на файлах второй части и посмотреть, где они ломаются.
4. Записать вывод сюда. Если совпадение есть, общий код вынести из `konung/k1` в корень пакета, отличия — в `konung/k2`.

## Mechanic comparison: «Князь 1» / «Князь 2» / «Князь 2.5»

The tables below compare gameplay mechanics across the three 2D parts. They describe
**game mechanics and content**, not engine internals: nothing here says anything about
whether the parts share an engine, which stays a hypothesis (see the section «Как
проверить гипотезу об общем движке» above and the closing note of this page).

Every row is graded, and the grade is part of the claim:

| Mark | Meaning |
|---|---|
| **Да / Yes** | A cited source states it directly. |
| **≈** | Tentative: an inference, or continuity assumed because «Князь 2.5» builds on «Князь 2». Cited, but not stated for this part. |
| **?** | Needs confirmation: no usable source found, see [Requires verification](#requires-verification). |
| **—** | Not applicable to this part. |

`≈ none` means: no source describes the mechanic for this part. Absence itself is *not*
confirmed — a mechanic nobody bothered to mention reads the same as one that does not
exist. Rows that measure content rather than presence (new monsters, new locations,
separate campaigns) carry a description instead of a mark. Where a cell rests on our own
files, it says so; those facts come from
`original/k1/` (never committed), from our parsers, and from the read-only reference
[Marisa-Chan/Slavik](https://github.com/Marisa-Chan/Slavik) (cited by file and line as of
the pinned commit [`cf5a332`](https://github.com/Marisa-Chan/Slavik/tree/cf5a3320dc93859e3dc51f37ee823fed30f399b6),
its code is not copied). Sources are attached per table; the same list with reachability
status is at the bottom.

### 1. Characters and progression

| Mechanic | «Князь 1» | «Князь 2» | «Князь 2.5» |
|---|---|---|---|
| Selectable heroes | Да — 3: Волк, Драгомир, Михаил ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)); our own data: one starting-state file per hero, `Game.0`/`Game.1`/`Game.2` ([game-state.md](k1/formats/game-state.md)) | Да — 6, two per people ([SoftClub](https://www.softclub.ru/games/pc/19815-knyaz-2), [Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | Да — 4: the usual three plus an eastern nomad ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz), [gospitaliers series page](https://gospitaliers.narod.ru/knyaz_opisanie.htm)) |
| Female heroes | Да — none: the three named protagonists are male ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) | Да — 3 (Велиславна, Хельга, Анастасия): [Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [gospitaliers guide](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm) names the same three, [PandoraKvest](https://pandorakvest.ru/text/49) counts "three valkyries" | ? — 4 heroes, the gender split of the added one is not reported |
| Starting classes | Да — 4: leader, warrior, hunter, merchant, each with a primary attribute ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz); our own observation of the readme shipped with the ENG edition) | Да — classes removed, every attribute and skill is open to every hero, only the starting values differ ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) | ≈ — built on 2; 2.5 ships a changed character-development system without saying how ([StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy)) |
| Primary attributes | ≈ — the parsed character record exposes five: strength, stamina, dexterity, charisma, health ([game-state.md](k1/formats/game-state.md)); the press names four as class primaries | Да — 6: strength, stamina, dexterity, intelligence, learnability, charisma ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [gospitaliers guide](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ — as in 2, plus reputation as an added hero characteristic ([SoftClub 2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy)) |
| Progression model | Да — classes plus experience; the record carries level, experience and unspent points ([game-state.md](k1/formats/game-state.md)) | Да — free skills plus ranks; one skill per weapon type plus social skills ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) | ≈ — based on 2 ([StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy)) |
| Skills rise from use | ? | Да — points at level-up **and** automatic growth through use, e.g. brewing raises the mixing skill, fighting with a one-handed sword raises that weapon skill ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [gospitaliers guide](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm) on healing and mixing). The 2003 review describes point allocation only ([PandoraKvest](https://pandorakvest.ru/text/49)) — the conflict is unresolved | ≈ |
| Maximum character level | ? | ? — the attribute cap of 150 is reported, no level cap is | ? |
| Ranks derived from attributes | — (the class system plays that role) | Да — the rank is composed from the two highest attributes, tiers at 1–30 / 31–60 / 61–90 / 91–120 / 121–150, and is descriptive only ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ — differs, unconfirmed |
| Reputation as a separate mechanic | ≈ none | ≈ none — the 2 guide never mentions it ([gospitaliers guide](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | Да — a new hero characteristic that moves with the deeds committed ([SoftClub 2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy), [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) |
| Reputation changes dialogue and quests | ≈ none | ≈ none | Да — it changes dialogues, the way some quest tasks can be solved, and how other characters treat the hero ([SoftClub 2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy)) |

Sources for this group: [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz),
[SoftClub «Князь 2»](https://www.softclub.ru/games/pc/19815-knyaz-2),
[SoftClub «Князь 2.5»](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy),
[Steam guide to «Князь 2»](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179),
[gospitaliers guide to «Князь 2»](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm),
[PandoraKvest on «Князь 2»](https://pandorakvest.ru/text/49),
[StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy),
our own data ([game-state.md](k1/formats/game-state.md)).

### 2. Party and companion AI

| Mechanic | «Князь 1» | «Князь 2» | «Князь 2.5» |
|---|---|---|---|
| Largest reported party, hero included | ≈ 10 — the leader class, whose primary attribute is charisma, reaches the largest retinue, "up to ten" ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)); whether the hero is one of the ten is unconfirmed | ≈ 7 — the documented rows end at 6 companions at charisma 100 on top of the hero; the guide leaves the higher rows "still being determined" ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ? — no maximum is reported |
| Charisma sets the party size | Да ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) | Да ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | Да — same mechanism, lower thresholds ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) |
| Charisma threshold per companion | ? | ≈ 16 → 1, 25–35 → 2, 50 → 3, 65 → 4, 80 → 5, 100 → 6 (incomplete table, [Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ 14 → 1, ~28 → 2, 30–40 → 3 ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) |
| Hiring gated per hero and per quest | Да — the three starting states differ in parties, caches and exits, while maps, transitions and villages match ([game-state.md](k1/formats/game-state.md)); quest NPCs cannot be recruited ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) | Да — companions are recruited from volunteers in a village ([PandoraKvest](https://pandorakvest.ru/text/49)); some joins need a high level and charisma ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ |
| Orders to companions | Да — party-wide attack and follow, group and single selection (our own observation of the readme shipped with the ENG edition; command bits in the record, [creatures.md](k1/creatures.md)) | Да — three companion settings: automatic weapon swap, guarding the hero, drinking a healing flask when badly hurt ([PandoraKvest](https://pandorakvest.ru/text/49)); "all to me" on the world map | ≈ |
| Autonomous companion behaviour | ≈ — roaming groups with an AI state recomputed every 16 ticks, and village workers, are in the data; what the states mean is only partly recovered ([game-state.md](k1/formats/game-state.md), [creatures.md](k1/creatures.md)) | Да — companions act on the settings above; civilians withdraw at night ([gospitaliers guide](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ |
| AI differences between the parts | ? | ? | ? |

**On the caps.** A clean set of party caps — 10, 9 and 7, one per part, hero included —
is **not** confirmed by the sources. What they give is: 10 for 1 (a press description,
hero inclusion unstated), and for 2 a table that excludes the hero explicitly
("retinue size, not counting the main hero", [Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179))
and stops at 6 companions. For 2.5 no maximum is stated at all. Guides also describe NPCs
who join for one task and then leave; no source says whether such a temporary companion
occupies a slot — the only exclusion stated explicitly is the hero's. See
[Requires verification](#requires-verification).

Sources for this group: [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz),
[Steam guide to «Князь 2»](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179),
[gospitaliers guide to «Князь 2»](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm),
[PandoraKvest on «Князь 2»](https://pandorakvest.ru/text/49),
our own data ([game-state.md](k1/formats/game-state.md), [creatures.md](k1/creatures.md)).

### 3. World and movement

| Mechanic | «Князь 1» | «Князь 2» | «Князь 2.5» |
|---|---|---|---|
| Isometric 2D world | Да ([gospitaliers series page](https://gospitaliers.narod.ru/knyaz_opisanie.htm), [StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy) lists top-down/isometric) | Да | Да |
| Walking | Да | Да | ≈ |
| Running | ≈ none — the first part gave no running and substituted a changeable game speed ([PandoraKvest](https://pandorakvest.ru/text/49)); the readme shipped with the ENG edition binds movement to a left click only, with no run key | Да — double click switches to a run ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)); "running was finally added in the second part" ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) | ≈ |
| Player-controlled world map | ? — 2-era sources present it as new in part 2; nothing first-hand says 1 had none | Да — travel moves an icon over the world map, opened with `M`, compared to the early *Fallout* ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [gospitaliers guide](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm), [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) | ≈ |
| Transitions between locations | Да — a transitions block in the starting state, portals confirmed on the picture ([game-state.md](k1/formats/game-state.md)) | Да | ≈ |
| Random encounters on the world map | ? | Да — monsters on the world map interrupt the march; a high tracking skill means fewer encounters ([gospitaliers guide](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm), [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) | ≈ |
| Ships | ≈ none — published as an addition of part 2 ([SoftClub](https://www.softclub.ru/games/pc/19815-knyaz-2): travel on water by ship, new islands) | Да — buy a ship, sail, look for other islands ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [gospitaliers guide](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ |
| Naval battles | ≈ none | Да — battles at sea appeared in the second part ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)); random sea skirmishes with raiders and the Lord's warriors ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ |
| Building interiors | ? | ≈ — guides describe going inside buildings (a storekeeper's premises, barracks with duel places, the forge) ([gospitaliers guide](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ |
| Caves and underground locations | ? | ? | ? |
| Hidden items and caches | Да — a cache record exists, and a cache with a negative gold value is hidden from view; chests are the same records ([game-state.md](k1/formats/game-state.md)) | ≈ — guides describe buried and planted items that stay where left | ≈ |
| Dropped items survive leaving the location | ≈ — the readme shipped with the ENG edition lists item persistence after the party leaves a location as a difference of the network mode, which implies the single-player default removes them | Да — plant roots, leave the location, come back later to find the plant grown, permanently ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ |
| Game speed control | Да — numpad `+`/`-` change a speed multiplier that runs extra update passes per frame (our own observation of the shipped readme; [Slavik `game_input.cpp:183-190`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_input.cpp#L183-L190), [`game_play.cpp:122-135`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L122-L135)) | ≈ none — "the speed is always the same" ([PandoraKvest](https://pandorakvest.ru/text/49)) | ? |

**K2-era additions.** Running, the world map with its random encounters, ships and naval
battles are all documented as things the second part introduced, not as things the first
part had and 2.5 inherited. Conversely, transitions between locations, hidden caches and
village life are already in the 1999 data and are **not** new in part 2.

Sources for this group: [SoftClub «Князь 2»](https://www.softclub.ru/games/pc/19815-knyaz-2),
[Steam guide to «Князь 2»](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179),
[gospitaliers guide to «Князь 2»](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm),
[PandoraKvest on «Князь 2»](https://pandorakvest.ru/text/49),
[DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz),
[gospitaliers series page](https://gospitaliers.narod.ru/knyaz_opisanie.htm),
[StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy),
our own data ([game-state.md](k1/formats/game-state.md), [movement.md](k1/movement.md)).

### 4. Combat

| Mechanic | «Князь 1» | «Князь 2» | «Князь 2.5» |
|---|---|---|---|
| Real-time combat | Да | Да | Да |
| Tactical pause with orders | ? — no order-giving pause is documented; the shipped readme maps `Esc` to the main menu | ≈ none — attacks come from all sides and a retreat order has to be given to each character separately ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)); the 2003 review complains that the game still has no pause for issuing orders and reaching the inventory ([PandoraKvest](https://pandorakvest.ru/text/49)) | Да — a publisher page describes a pause mode in which combat orders can be reviewed and changed and ammunition actions performed ([SoftClub 2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy)); the same is described as an active pause in combat ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) |
| A weapon in each hand | ≈ none | Да — new combat possibilities include fighting armed in two hands ([SoftClub](https://www.softclub.ru/games/pc/19815-knyaz-2)), and it is a skill of its own ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ |
| Additional strikes | ? | Да — "additional strikes" among the new combat possibilities ([SoftClub](https://www.softclub.ru/games/pc/19815-knyaz-2)) | ≈ |
| Crossbows | ≈ none — listed as a new weapon of part 2 ([SoftClub](https://www.softclub.ru/games/pc/19815-knyaz-2)); 1's parsed skill list has marksmanship but no crossbow ([game-state.md](k1/formats/game-state.md)) | Да — a new weapon brought by the Byzantines, with a skill of its own; crossbows outrange and outdamage bows ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [gospitaliers guide](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ |
| Additional heavy weapons | ? | Да — knuckles, two-handed swords, axes and clubs are named as new weapon types ([SoftClub](https://www.softclub.ru/games/pc/19815-knyaz-2)) | ≈ |
| Magic upgrades of items | ? — the item record carries a bonus, a poison and a concentration value, which says little by itself ([game-state.md](k1/formats/game-state.md)) | Да — the magic-casting skill raises the values of gems set into weapons and armour; per item the cap is +6 to an attribute and +48 to armour or damage, and a stone cannot push past a cap already reached ([gospitaliers guide](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ |
| Alchemy | ≈ — elixirs, oils and poisons exist (a poison field and a concentration field in the item record, a village healer's wares), but the mixing itself is not documented for part 1 | Да — recipes combine an ingredient with a jar (balm, oil, poison, antidote, brew, elixir), and the mixing skill sets their concentration ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ |
| New monster types | baseline set | ≈ — an extended set (guides name desert and mountain foes, pirates, the Lord's warriors) | Да — a new faction, the clan of the Yellow Dogs of the desert ([SoftClub 2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy), [StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy)) |

**Tactical pause is not the same as a pause menu.** The cell for 2.5 is graded on the
publisher's own wording: while paused, combat orders can be reviewed and changed and
ammunition handled — that is an order-giving pause, not a menu that merely stops the
game. Two independent sources say part 2 had no such mode, and nothing published says
part 1 had one (its readme only maps `Esc` to the main menu).

**Not new in part 2.** Alchemy, the retinue, capturing and running villages, and
construction all have a footing in the 1999 game — see the next table for what the
starting-state data of part 1 actually carries. Do not describe them as 2's inventions.

Sources for this group: [SoftClub «Князь 2»](https://www.softclub.ru/games/pc/19815-knyaz-2),
[SoftClub «Князь 2.5»](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy),
[Steam guide to «Князь 2»](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179),
[gospitaliers guide to «Князь 2»](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm),
[PandoraKvest on «Князь 2»](https://pandorakvest.ru/text/49),
[DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz),
[StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy).

### 5. Settlements and economy

The first part's column comes mostly from our own files: the presence of the structures
is confirmed by the parser and by byte-exact round-trips, the *meaning* of the fields is
read from the Slavik reference and stays at `≈` there. Details in
[game-state.md](k1/formats/game-state.md).

| Mechanic | «Князь 1» | «Князь 2» | «Князь 2.5» |
|---|---|---|---|
| Villages under the player | ≈ — 15 villages stand in the 1999 starting state, on the same 15 maps in all three heroes' files (of 20 record slots), each with an elder's party and staffed workplaces; that the player can take one over is not something the data states | Да — take villages under your care and rule them, which in practice means collecting tribute and repelling attacks ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ |
| Village management | ≈ — five workplaces (elder, healer, merchant, blacksmith, voivode), each pointing at a character; most villages start with all five staffed, one starts with the elder alone ([game-state.md](k1/formats/game-state.md)) | Да — a village-management skill decides how much tribute the village yields ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ |
| Construction | ≈ — 119 building slots per village with a kind, a state, a map-object index and a countdown, and a declared count of two kinds of buildings (in the starting state: 2 to 11 of one kind, 0 to 100 of the other); whether the player commissions buildings is not established | Да — new buildings are raised through the elder and masters from the retinue move into them ([PandoraKvest](https://pandorakvest.ru/text/49)); a good builder is needed to repair burned buildings and raise new ones ([gospitaliers guide](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)); a bridge can be commissioned | ≈ |
| Assigning specialists | ≈ — the five village workplaces above | Да — builder, healer, blacksmith and hunter fill named roles ([gospitaliers guide](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ |
| Producing goods | ≈ — the healer's potions and the merchant's and blacksmith's wares are recorded per village | ≈ — a blacksmith in the village lets part of the tribute be spent on buying weapons ([gospitaliers guide](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ? |
| Collecting tribute | ≈ — a tribute amount and a tribute deadline are recorded per village | Да — tribute has levels, the starting level is low, and a high level is untested by the guide's author ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [gospitaliers guide](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ |
| Production runs on a timer | ≈ — timer fields per village for the healer, the blacksmith and the voivode; all five village timers read zero in a fresh start, so they only move during play ([game-state.md](k1/formats/game-state.md)) | ? | ? |
| Tribute is due on a timer | ≈ — a deadline field, meaning per Slavik, zero in a fresh start | ? | ? |

Sources for this group: [Steam guide to «Князь 2»](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179),
[gospitaliers guide to «Князь 2»](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm),
[PandoraKvest on «Князь 2»](https://pandorakvest.ru/text/49),
our own data ([game-state.md](k1/formats/game-state.md)).

### 6. Time and world simulation

| Mechanic | «Князь 1» | «Князь 2» | «Князь 2.5» |
|---|---|---|---|
| Day/night cycle | Да — a day counter runs through light phases; a full cycle is 21600 ticks, about 21.6 minutes at the 60 ms tick ([Slavik `game.cpp:1764-1890`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1764-L1890), [`game.h:28`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.h#L28), [movement.md](k1/movement.md) for the tick) | Да — a day/night mode that can be switched on and off ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [gospitaliers guide](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ |
| Lighting changes with the hour | Да — light is added to ground tiles from the evening phases of the cycle ([lights-res.md](k1/formats/lights-res.md)) | Да — guides complain about pitch darkness at night ([PandoraKvest](https://pandorakvest.ru/text/49)) | ≈ |
| Permanent-day option | ≈ — a configuration flag read from the saved options gates the lighting computation ([Slavik `game.cpp:120`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L120), [`game.cpp:1776`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1776), [`game.h:1464`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.h#L1464)) | Да — turning the day/night change off in the options keeps it daytime all the time ([gospitaliers guide](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ |
| Night affects ordinary NPCs | ? | Да — civilians refuse to talk late at night, while the main characters are ready to talk at any hour ([PandoraKvest](https://pandorakvest.ru/text/49), [gospitaliers guide](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm)) | ≈ |
| Traders unavailable at night | ? | Да — part of the population is unreachable at night (the same civilian restriction) | ≈ |
| Creatures or spirits appear at night | ? | Да — a spirit by the grave appears only at night, and only with the day/night mode switched on ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ |
| Quests conditioned on the hour of day | ? | Да — a quest is solved by waiting for night so that a warehouse keeper goes to bed and returns in the morning ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)) | ≈ |
| NPCs go to sleep and change availability | ? | Да — the same guide evidence as the two rows above | ≈ |
| Production timers | ≈ — the village timer fields ([game-state.md](k1/formats/game-state.md)) | ? | ? |
| Economic timers | ≈ — the tribute amount and deadline fields | ? | ? |
| Monster attributes change at night | ? | ? | ? |
| Permanent day stops the game clock | ≈ no — in the Slavik reconstruction the day counter keeps advancing and the flag is read only by the lighting computation, so the option fixes the light phase rather than freezing time; the reading comes from a third-party reconstruction, so it is not raised to a fact | ? | ? |

What each part is evidenced by: part 1 has the cycle in code and the timers in data;
part 2 has first-hand guide evidence for the toggle, sleeping civilians, a night-only
spirit and a quest that waits for dark; for 2.5 the day/night claims are inherited from
part 2 and no first-hand description was found.

Sources for this group: [Steam guide to «Князь 2»](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179),
[gospitaliers guide to «Князь 2»](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm),
[PandoraKvest on «Князь 2»](https://pandorakvest.ru/text/49),
[Slavik](https://github.com/Marisa-Chan/Slavik) (read-only reference),
our own data ([game-state.md](k1/formats/game-state.md), [lights-res.md](k1/formats/lights-res.md),
[movement.md](k1/movement.md)).

### 7. Narrative and quests

| Mechanic | «Князь 1» | «Князь 2» | «Князь 2.5» |
|---|---|---|---|
| Plot differs per hero | ≈ — the starting states differ per hero in our own data: own starting map, parties, caches and exits ([game-state.md](k1/formats/game-state.md)); a plot-level difference beyond the opening is not stated by a cited source | ≈ — six heroes across three peoples ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)); one hero's party admits only men, another's only women — a distinct plot per hero is not stated by a cited source | ≈ — 4 heroes with their own lines ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) |
| Quest variety | ≈ — quests live outside the starting state, in `QUEST*.RES`, and dialogues are chosen by a record byte ([game-state.md](k1/formats/game-state.md)); the variety of quest types itself is unconfirmed | Да — main and side quests, "not always of one type" ([PandoraKvest](https://pandorakvest.ru/text/49)) | ≈ — inherited from 2; the advertised more than 100 locations and more than 1000 game scenes are a publisher content-scale claim and do not speak to quest diversity ([SoftClub 2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy), [StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy)) |
| Reputation changes NPCs and dialogue | ≈ none | ≈ none | Да ([SoftClub 2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy), [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) |
| Several endings | ? | ? | Да — several different endings ([SoftClub 2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy)) |
| Voiced dialogue | ? | ? | ? |
| Dialogue conditions on the time of day | ? | Да — civilians only talk during the day; night-gated quest steps ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179), [PandoraKvest](https://pandorakvest.ru/text/49)) | ≈ |

Sources for this group: [SoftClub «Князь 2.5»](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy),
[Steam guide to «Князь 2»](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179),
[PandoraKvest on «Князь 2»](https://pandorakvest.ru/text/49),
[DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz),
[StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy),
our own data ([game-state.md](k1/formats/game-state.md)).

### 8. Technical and content differences

| Feature | «Князь 1» | «Князь 2» | «Князь 2.5» |
|---|---|---|---|
| Network mode | Да — a LAN/DirectPlay mode shipped with the ENG edition (`LAN_KONG.EXE`, `SERVER`, `Servers.cfg`); the RUS edition carries only `CLIENT.EXE`, so the mode is edition-specific (our own observation, [editions.md](k1/editions.md)); up to 16 players over three peoples in an amulet-carry mode, with its own party cap of two companions and persistent dropped items (our own observation of the shipped readme); official servers closed long ago ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) | ≈ none — the series table marks network play as absent ([gospitaliers series page](https://gospitaliers.narod.ru/knyaz_opisanie.htm)) | ≈ none — same table; the store card lists it as single-player ([StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy)) |
| Supported resolutions | Да — 640×480 and 800×600: interface resources exist per language for both sizes (`MENU640R/E/T.RES`, `MENU800R/E/T.RES`, and the same for `INTO`, `INTR`, `ME`, `NEW`, `NEWH`, `PLAY`, `SAVE`), and the shipped options menu offers a resolution setting (our own observation); the original topped out at 800×600 ([DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)) | ≈ 1024×768 — the publisher's minimum hardware line asks for a card supporting 1024×768 at 16 bit ([SoftClub](https://www.softclub.ru/games/pc/19815-knyaz-2)); the review only says the resolution went up ([PandoraKvest](https://pandorakvest.ru/text/49)) | ≈ 1024×768 — assumed from the base game, unconfirmed |
| Separate story campaigns | ≈ — three heroes with distinct starting states ([game-state.md](k1/formats/game-state.md)); the hero count alone does not establish separate campaigns | ≈ — six heroes ([Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179)); the hero count alone does not establish separate campaigns | ≈ — four heroes ([gospitaliers series page](https://gospitaliers.narod.ru/knyaz_opisanie.htm)); the hero count alone does not establish separate campaigns |
| Shared game base with «Князь 2» | — | the base part | Да — a standalone add-on made by a fan group on the second game's base, published by 1C, filling the gap between part 1 and part 2 ([gospitaliers series page](https://gospitaliers.narod.ru/knyaz_opisanie.htm), [StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy)) |
| Save compatibility 2 / 2.5 | — | not confirmed | not confirmed |
| Binary format compatibility | not confirmed | not confirmed | not confirmed |
| New locations and content | the original game | new content | extra content: over 100 locations, new buildings, trees and landscape elements for the southern lands, a new faction ([SoftClub 2.5](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy)) |

Sources for this group: [gospitaliers series page](https://gospitaliers.narod.ru/knyaz_opisanie.htm),
[SoftClub «Князь 2»](https://www.softclub.ru/games/pc/19815-knyaz-2),
[SoftClub «Князь 2.5»](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy),
[PandoraKvest on «Князь 2»](https://pandorakvest.ru/text/49),
[DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz),
[StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy),
our own observation of the unpacked editions ([editions.md](k1/editions.md)).

## Requires verification

Cells left at `?`, and the specific question each still needs answered:

1. **Party caps.** Whether the hero counts toward the "up to ten" of part 1; the real
   maximum beyond 6 companions in part 2 (the guide's table stops there); the maximum in
   2.5. Whether temporary companions who join for a single task occupy a slot.
2. **Charisma thresholds.** The unfilled rows of the companion table for both parts 2
   and 2.5, and any per-hero hiring limits.
3. **Maximum level.** For all three parts. The 150 reported for 2 is an attribute cap,
   not a level cap; the figure "up to 13" for part 1 has no source found and is not
   repeated above.
4. **Ranks in 2.5.** Whether the rank system of part 2 is kept, and how it differs.
5. **Game speed control in 2.5.** Part 1 has it, part 2 is reported not to.
6. **A world map in part 1.** Only 2-era sources frame it as new; no first-hand
   statement about part 1.
7. **Random encounters in part 1**, and whether any world-map travel existed there at
   all.
8. **Building interiors, caves and underground locations in part 1** — our map data does
   not name maps, so the question is open for all three parts.
9. **A tactical pause in part 1.** The shipped readme knows only `Esc` for the main
   menu; absence is unconfirmed.
10. **Item magic upgrades and potion mixing in part 1.** The fields exist in the item
    record; the gameplay around them is not documented.
11. **Production and tribute timers in parts 2 and 2.5** — whether they tick on the game
    clock, and at what interval.
12. **Night effects in part 1** (ordinary NPCs, traders, night-only creatures) and
    **night behaviour in 2.5** — everything claimed for 2.5 is inherited from 2 by
    assumption.
13. **Whether a permanent-day option freezes the world clock** in parts 2 and 2.5. For
    part 1 the reconstruction of the day counter and the option flag argues "only the
    lighting", but that is a third-party reading and stays at `≈`.
14. **Monster attributes at night** — no source for any part.
15. **Voiced dialogue** — no source for any part.
16. **Multiple endings in parts 1 and 2.**
17. **Female heroes in 2.5** — the count of heroes is four, their gender split is not
    reported.
18. **Resolutions of 2 and 2.5** — only a minimum hardware line and a reviewer's "the
    resolution went up"; no mode list.
19. **Network play in 2 and 2.5** — a third-party series table and a single-player store
    card, no first-party statement of absence.
20. **Save and binary format compatibility** between 2 and 2.5, and between all three
    parts. See «Как проверить гипотезу об общем движке» above: this is the same open
    question, and the tables above do not settle it.
21. **Separate story campaigns and a plot that differs per hero** — the hero counts and
    the differing starting states are confirmed; no cited source states separate
    campaigns or hero-specific plots for any part.
22. **Quest variety in 2.5** — the location and scene counts are a publisher content
    figure; a source that describes the quest types themselves is missing.

## Sources for the comparison

Checked 2026-11-04.

| Source | Used for | Status |
|---|---|---|
| [SoftClub: «Князь 2»](https://www.softclub.ru/games/pc/19815-knyaz-2) | publisher's feature list for 2: heroes, skills, ships, two-handed fighting, new weapons, settlement management; the 1024×768 hardware line | reachable |
| [SoftClub: «Князь 2.5»](https://www.softclub.ru/games/pc/19816-knyaz-2-prodolzhenie-legendy) | publisher's feature list for 2.5: reputation, the pause mode with combat orders and ammunition, over 100 locations, the new faction, several endings | reachable |
| [Steam guide to «Князь 2»](https://steamcommunity.com/sharedfiles/filedetails/?id=1506357179) | heroes and genders, six attributes, skill growth, ranks and the 150 cap, the charisma table for 2 **and** 2.5, running, world map, ships, night content, alchemy recipes, villages | reachable |
| [gospitaliers guide to «Князь 2»](https://gospitaliers.narod.ru/knaz/k2_bigguide.htm) | hero roster, attributes, skills, tribute levels, builders, gem-set item upgrades, the permanent-day option and its effect | reachable |
| [gospitaliers series page](https://gospitaliers.narod.ru/knyaz_opisanie.htm) | series table: hero counts 3 / 6 / 4, network play per part, dates, 2.5 as a fan-made standalone add-on published by 1C | reachable |
| [DTF: history of the series](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz) | part 1 heroes and classes, the retinue of ten, classes removed in 2, running and the world map and naval battles as additions of 2, reputation and the active pause in 2.5, the 800×600 ceiling of part 1, the network mode of part 1 | reachable |
| [PandoraKvest review of «Князь 2»](https://pandorakvest.ru/text/49) | a contemporary 2003 comparison with part 1: running versus game speed, no order-giving pause, night civilians, village building and masters, companion settings | reachable |
| [StopGame: «Князь 2.5»](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy) | 2.5 as a fan-group production published by 1C, its premise, the new faction, scale figures, "somewhat changed character development" | reachable |
| [Marisa-Chan/Slavik](https://github.com/Marisa-Chan/Slavik) | read-only reference for part 1: the day counter and light phases, the day/night option flag, the speed multiplier. Cited by file and line as of the pinned commit [`cf5a332`](https://github.com/Marisa-Chan/Slavik/tree/cf5a3320dc93859e3dc51f37ee823fed30f399b6), its code is never copied | reachable |
| [MobyGames: *Legend of the North: Konung*](https://www.mobygames.com/game/3325/legend-of-the-north-konung/) | intended for part 1 technical metadata | **not reachable** (403), nothing above relies on it |
| [Cyclowiki: «Князь 2.5»](https://www.cyclowiki.org/wiki/) | intended for the 2.5 hero roster | **not reachable** (404), the hero count is taken from DTF and gospitaliers instead |
| ag.ru catalogue page for «Князь 2.5» | intended for 2.5 heroes and requirements | **not reachable** (redirects to 404), nothing above relies on it |

Our own observations are limited to `original/k1/` (never committed) and to what our
parsers report: file names and sizes, record layout, counts, and the readme shipped with
the ENG edition. Game texts, sprites and renders stay out of the repository.

**Engine identity is not claimed.** Nothing above says that «Князь 1», «Князь 2» and
«Князь 2.5» share an engine; the shared base of 2.5 with 2 is a published statement about
the product, and the relation of part 1 to part 2 remains the hypothesis described at the
top of this page.
