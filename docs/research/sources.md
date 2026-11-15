<a id="external-sources"></a>

# Внешние источники

Исторический поиск зафиксирован **2026-10-28**: GitHub, игровые форумы,
ZenHAX/XeNTaX и PCGamingWiki. Доступность и активность проектов ниже относятся
к тому поиску, не к текущей сетевой проверке. Точные ссылки и уверенность
сохраняются на тематических страницах; каталог не превращает сведения
только по источнику в подтверждённый факт.

<a id="code"></a>

## Код

| Проект | Что это | Применение и ограничения |
|---|---|---|
| [Marisa-Chan/Slavik](https://github.com/Marisa-Chan/Slavik) | Восстановленная по дизассемблеру реализация K1 на C++/SDL2; остались имена FUN_00xxxxxx; наблюдалась активность в 2024–2025 | Основной структурный источник: resources.cpp — GRAPH/OBJECTS/FLAMES/LIGHTS/SOUNDS/MUSICS.RES; game.cpp — LoadGameMap, KONUNG.N/LIGHTS.N и отрисовка; game_state.cpp — состояние и CP866. **Нет лицензии: только чтение, копировать код нельзя.** Локальный клон ../../Marisa-Chan/Slavik (Windows E:\github\Marisa-Chan\Slavik). |
| [rtsk_konung_mus_unpacker](https://github.com/dopedealer/rtsk_konung_mus_unpacker) | Распаковщик MUSICS.RES→OGG на C | Формат музыки коллекционного издания |
| [konung_map_creator](https://github.com/Ovsyanka/konung_map_creator) | Карты локаций, склеенные из снимков экрана | Эталон для локального сравнения рендера |
| [konung-2-save-editor](https://github.com/Benzeliden/konung-2-save-editor) | Редактор сохранений K2 | Возможный источник исследования K2 |
| [konung3-mods](https://github.com/Hawk-Games/konung3-mods) | Модификации K3 | Вне проекта; только контекст |

<a id="community-technical-discussions"></a>

## Технические обсуждения сообщества

Ссылки ведут на обсуждения и документацию, а не на распространение игры.

- [Страница серии gospitaliers](https://gospitaliers.narod.ru/knyaz_opisanie.htm):
  описания, упоминания патчей/редакторов и форум сообщества.
- [konung.topbb.ru](https://konung.topbb.ru/) и
  [тема редактора](https://konung.topbb.ru/viewtopic.php?id=135): смещения
  konung.sa1…sa9 / Cheat Engine; описания форматов ресурсов не найдено.
- [Обсуждение HD-патча](https://www.old-games.ru/forum/threads/knjaz-legendy-lesnoj-strany-aka-konung-legends-of-the-north.27570/page-4):
  совместимый с GOG патч Morendil и пути Konung/Video/*.flm.
- [HEX-заметки ag.ru](https://ag.ru/games/konung-legends-of-the-north/cheats/10659):
  смещения сохранений.

<a id="edition-patches"></a>

## Патчи изданий

- [PCGamingWiki](https://www.pcgamingwiki.com/wiki/Konung:_Legends_of_the_North)
  и [Resolution Fix](https://community.pcgamingwiki.com/files/file/1652-konung-legends-of-the-north-resolution-fix/):
  широкоформатный патч EXE, заявленный для RUS CD и коллекционного/GOG изданий.
- [Каталог патчей Gamepressure](https://www.gamepressure.com/games/konung-legends-of-the-north/mods-trainers/z8c14):
  те же широкоформатные исправления v2.

<a id="series-history"></a>

## История серии

- [История DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz).
- [Wikipedia](https://en.wikipedia.org/wiki/Konung:_Legend_of_the_North).
- [Cyclowiki K2](https://www.cyclowiki.org/wiki/%D0%9A%D0%BD%D1%8F%D0%B7%D1%8C_2:_%D0%9A%D1%80%D0%BE%D0%B2%D1%8C_%D0%A2%D0%B8%D1%82%D0%B0%D0%BD%D0%BE%D0%B2).
- [StopGame K2.5](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy).
- [История Snowball Studios](https://www.old-games.ru/wiki/Snowball_Studios).

[Сравнение механик](comparison.md#sources-for-the-comparison) содержит
отдельный, более поздний список источников и ограничения их доступности.

<a id="general-format-research"></a>

## Общие исследования форматов

Проверены [awesome-game-file-format-reversing](https://github.com/VelocityRa/awesome-game-file-format-reversing),
[ZenHAX](https://zenhax.com/) и архив XeNTaX; материалов именно о K1 не найдено.
Кроме Slavik, тот поиск не обнаружил публичного распаковщика GRAPH.RES/OBJECTS.RES,
редактора карт или описания форматов. Это результат ограниченного поиска,
а не утверждение, что таких материалов не может существовать.
