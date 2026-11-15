# External sources

Historical search recorded on **2026-10-28**: GitHub, game-community forums,
ZenHAX/XeNTaX and PCGamingWiki. Availability and project activity below describe
that research, not a current network check. Topic pages retain precise source
citations and confidence; this catalogue does not promote a source-only claim
to a confirmed fact.

## Code

| Project | What it is | Use and limitations |
|---|---|---|
| [Marisa-Chan/Slavik](https://github.com/Marisa-Chan/Slavik) | C++/SDL2 K1 reimplementation reconstructed from disassembly; FUN_00xxxxxx names remain; activity observed in 2024–2025 | Main structural reference: resources.cpp for GRAPH/OBJECTS/FLAMES/LIGHTS/SOUNDS/MUSICS.RES; game.cpp for LoadGameMap, KONUNG.N/LIGHTS.N and drawing; game_state.cpp for state and CP866. **No license; read only, never copy code.** Local clone ../../Marisa-Chan/Slavik (Windows E:\github\Marisa-Chan\Slavik). |
| [rtsk_konung_mus_unpacker](https://github.com/dopedealer/rtsk_konung_mus_unpacker) | C MUSICS.RES→OGG extractor | Collector's-edition music format |
| [konung_map_creator](https://github.com/Ovsyanka/konung_map_creator) | Location maps stitched from screenshots | Local rendering comparison reference |
| [konung-2-save-editor](https://github.com/Benzeliden/konung-2-save-editor) | K2 save editor | Potential K2 research reference |
| [konung3-mods](https://github.com/Hawk-Games/konung3-mods) | K3 modifications | Outside scope; background only |

## Community technical discussions

These links cite discussions and documentation, not game distribution sources.

- [gospitaliers series page](https://gospitaliers.narod.ru/knyaz_opisanie.htm):
  descriptions, patch/editor references, and the community forum.
- [konung.topbb.ru](https://konung.topbb.ru/) and its
  [editor thread](https://konung.topbb.ru/viewtopic.php?id=135): save offsets
  for konung.sa1…sa9 / Cheat Engine; no resource-format description found.
- [HD-patch discussion](https://www.old-games.ru/forum/threads/knjaz-legendy-lesnoj-strany-aka-konung-legends-of-the-north.27570/page-4):
  Morendil's GOG-compatible patch and Konung/Video/*.flm paths.
- [ag.ru HEX notes](https://ag.ru/games/konung-legends-of-the-north/cheats/10659): save offsets.

## Edition patches

- [PCGamingWiki](https://www.pcgamingwiki.com/wiki/Konung:_Legends_of_the_North)
  and its [Resolution Fix](https://community.pcgamingwiki.com/files/file/1652-konung-legends-of-the-north-resolution-fix/):
  a widescreen EXE patch reported for RUS CD and collector's/GOG editions.
- [Gamepressure patch catalogue](https://www.gamepressure.com/games/konung-legends-of-the-north/mods-trainers/z8c14):
  the same v2 widescreen fixes.

## Series history

- [DTF history](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz).
- [Wikipedia](https://en.wikipedia.org/wiki/Konung:_Legend_of_the_North).
- [Cyclowiki K2](https://www.cyclowiki.org/wiki/%D0%9A%D0%BD%D1%8F%D0%B7%D1%8C_2:_%D0%9A%D1%80%D0%BE%D0%B2%D1%8C_%D0%A2%D0%B8%D1%82%D0%B0%D0%BD%D0%BE%D0%B2).
- [StopGame K2.5](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy).
- [Snowball Studios history](https://www.old-games.ru/wiki/Snowball_Studios).

The [gameplay comparison](comparison.md#sources-for-the-comparison) records
its own later source census and reachability limitations.

## General format research

[awesome-game-file-format-reversing](https://github.com/VelocityRa/awesome-game-file-format-reversing),
[ZenHAX](https://zenhax.com/) and the XeNTaX archive were searched; no K1-specific
material was found there. Apart from Slavik, the historical search found no
public GRAPH.RES/OBJECTS.RES extractor, map editor or format description.
This is a bounded search result, not a claim that none can exist.
