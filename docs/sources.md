# Внешние источники

Что уже сделано другими и где искать сведения. Поиск сделан 2026-10-28: GitHub, форумы old-games.ru, konung.topbb.ru, ZenHAX/XeNTaX, PCGamingWiki.

## Код

| Проект | Что это | Чем полезен |
|---|---|---|
| [Marisa-Chan/Slavik](https://github.com/Marisa-Chan/Slavik) | Реимплементация движка первой части на C++/SDL2, восстановленная по дизассемблированию (остались имена `FUN_00xxxxxx`). Активна в 2024–2025. **Лицензии нет.** | Главный источник по форматам: `resources.cpp` (GRAPH/OBJECTS/FLAMES/LIGHTS/SOUNDS/MUSICS.RES), `game.cpp` (`LoadGameMap` — `KONUNG.N` и `LIGHTS.N`, отрисовка), `game_state.cpp` (сохранения, строки CP866). Клон для чтения — `E:\github\Marisa-Chan\Slavik`, код не копируем. |
| [dopedealer/rtsk_konung_mus_unpacker](https://github.com/dopedealer/rtsk_konung_mus_unpacker) | Распаковщик `MUSICS.RES` → OGG (C) | Формат музыки коллекционного издания |
| [Ovsyanka/konung_map_creator](https://github.com/Ovsyanka/konung_map_creator) | Склейка карт локаций из скриншотов | Эталон для сверки рендера |
| [Benzeliden/konung-2-save-editor](https://github.com/Benzeliden/konung-2-save-editor) | Редактор сохранений «Князя 2» | Пригодится на этапе второй части |
| [Hawk-Games/konung3-mods](https://github.com/Hawk-Games/konung3-mods) | Моды «Князя 3» | Вне рамок, для справки |

## Фан-сайты и форумы

- [gospitaliers.narod.ru](https://gospitaliers.narod.ru/knyaz_opisanie.htm) — описание серии, [файлы и утилиты](https://gospitaliers.narod.ru/knyaz_files2.htm): патчи 01/02, HD-патчи, редакторы сохранений; ссылки на «Народный форум Князя».
- [konung.topbb.ru](https://konung.topbb.ru/) — форум фанатов. Например, [читы и редакторы](https://konung.topbb.ru/viewtopic.php?id=135): смещения в сохранениях `konung.sa1`…`sa9`, Cheat Engine. О ресурсах ничего нет.
- [old-games.ru, тема игры](https://www.old-games.ru/forum/threads/knjaz-legendy-lesnoj-strany-aka-konung-legends-of-the-north.27570/page-4) — HD-патч Morendil, совместимый с GOG; ролики `Konung\Video\*.flm`.
- [ag.ru, HEX-коды](https://ag.ru/games/konung-legends-of-the-north/cheats/10659) — смещения в сохранениях.

## Патчи изданий

- [PCGamingWiki: Konung](https://www.pcgamingwiki.com/wiki/Konung:_Legends_of_the_North) и [Resolution Fix](https://community.pcgamingwiki.com/files/file/1652-konung-legends-of-the-north-resolution-fix/) — widescreen-патч, только для RUS (CD и коллекционное/GOG). Правит `konung.exe`.
- [gamepressure: моды](https://www.gamepressure.com/games/konung-legends-of-the-north/mods-trainers/z8c14) — те же widescreen-фиксы v2.

## Об играх

- [DTF: история серии](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz)
- [Wikipedia: Konung: Legend of the North](https://en.wikipedia.org/wiki/Konung:_Legend_of_the_North)
- [Cyclowiki: «Князь 2: Кровь Титанов»](https://www.cyclowiki.org/wiki/%D0%9A%D0%BD%D1%8F%D0%B7%D1%8C_2:_%D0%9A%D1%80%D0%BE%D0%B2%D1%8C_%D0%A2%D0%B8%D1%82%D0%B0%D0%BD%D0%BE%D0%B2)
- [StopGame: «Князь 2.5: Продолжение легенды»](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy)
- [old-games.ru: Snowball Studios](https://www.old-games.ru/wiki/Snowball_Studios)

## Общие ресурсы по реверсу игровых форматов

- [awesome-game-file-format-reversing](https://github.com/VelocityRa/awesome-game-file-format-reversing)
- [ZenHAX](https://zenhax.com/) и архив XeNTaX — о «Князе» ничего не нашлось.

## Чего нет

Публичных описаний форматов «Князя», кроме кода Slavik, не нашлось. Нет ни распаковщиков `GRAPH.RES`/`OBJECTS.RES`, ни редакторов карт.
