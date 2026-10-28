# Файлы первой части

Содержимое `inst/setup.dat` — установленная игра (`original/k1/<издание>/game`). Сравнение RUS и ENG по sha256: `=` — одинаковые, `≠` — различаются. Назначение без ссылки — догадка по имени или по Slavik, не проверено.

| Файл | RUS, байт | ENG, байт | | Назначение |
|---|---:|---:|---|---|
| `CLIENT.EXE` | 939008 | — |  | сетевой клиент? |
| `FLAMES.RES` | 1533070 | 1533070 | = | огни — [flames-res](formats/flames-res.md) |
| `Game.res` | 365509 | 365509 | ≠ | ? В начале — записи по 16 байт, похожие на предметы `Game.N`, но размер файлу не кратен; Slavik его не читает |
| `GRAPH.RES` | 14505371 | 14505349 | ≠ | палитры, курсоры, тайлы земли и декорации, спрайты персонажей — [graph-res](formats/graph-res.md) |
| `KONUNG.EXE` | — | 1884291 |  | игра; таблицы снаряжения — [executable](formats/executable.md) |
| `konung.ico` | 2238 | 2238 | = | иконка |
| `konung.res` | 1586688 | — |  | PE-файл (MZ) — вероятно, `konung.exe`, переименованный установщиком; те же таблицы, что в `KONUNG.EXE` ENG — [executable](formats/executable.md) |
| `LAN_KONG.EXE` | — | 4118147 |  | сетевая игра |
| `Lanusers.dat` | — | 1288192 |  | ? |
| `LIGHTS.RES` | 145472 | 145472 | = | маски света (по Slavik) |
| `ListFile.ini` | 1337 | — |  | список файлов установки |
| `Maps.res` | 3856000 | 3856000 | ≠ | ? Гипотеза: 20 записей по 192800 байт = `Konung.N` + `LIGHTS.N`; ни с одной картой дистрибутива не совпадают; Slavik файл не читает |
| `Objects.res` | 52522058 | 52522058 | = | объекты карты (деревья, дома) — [objects-res](formats/objects-res.md) |
| `playavi.dll` | 316416 | 328704 | ≠ | проигрыватель роликов `*.flm` |
| `readme.txt` | — | 8545 |  | readme |
| `SAVITEMS.DAT` | — | 5120000 |  | ? 10000 записей по 512 байт, все `0xFF`, кроме нуля в байте 42 — пустая таблица; Slavik не читает |
| `SERVER` | — | 91 |  | сетевая игра |
| `Servers.cfg` | — | 21 |  | сетевая игра |
| `SOUNDS.RES` | 21981240 | 21961483 | ≠ | звуки PCM 22050 Гц (по Slavik) |
| `Sounds_.res` | 13314264 | 13294507 | ≠ | звуки, второй набор? |
| `TUTORIAL.EXE` | — | 1880707 |  | обучение |
| `uninstall.dat` | — | 98402 |  | деинсталлятор |
| `Uninstall.exe` | 432128 | 535552 | ≠ | деинсталлятор |
| `Uninstall.ini` | 679 | — |  | деинсталлятор |

- `game.N`: RUS 3, ENG 3, одинаковых 0; размеры: [709822]; файлы: game.0, game.1, game.2
- `into…{r,e,t}.res`: RUS 2, ENG 2, одинаковых 0; размеры: [1066080, 1072233, 1302491]…; файлы: into640e.res, into640r.res, into800e.res, into800r.res
- `intr…{r,e,t}.res`: RUS 2, ENG 4, одинаковых 0; размеры: [1003654, 1009735, 1240065]…; файлы: intr640e.res, intr640r.res, intr640t.res, intr800e.res, intr800r.res, intr800t.res
- `konung.N`: RUS 43, ENG 43, одинаковых 43; размеры: [128800]; файлы: 43
- `lights.N`: RUS 40, ENG 40, одинаковых 40; размеры: [64000]; файлы: 40
- `me…{r,e,t}.res`: RUS 2, ENG 2, одинаковых 0; размеры: [1375748, 1398061, 2152854]…; файлы: me640e.res, me640r.res, me800e.res, me800r.res
- `menu…{r,e,t}.res`: RUS 2, ENG 4, одинаковых 0; размеры: [1312566, 1350410, 1381112]…; файлы: menu640e.res, menu640r.res, menu640t.res, menu800e.res, menu800r.res, menu800t.res
- `new…{r,e,t}.res`: RUS 2, ENG 2, одинаковых 0; размеры: [365223, 376113, 510531]…; файлы: new640e.res, new640r.res, new800e.res, new800r.res
- `newh…{r,e,t}.res`: RUS 2, ENG 2, одинаковых 0; размеры: [410382, 410492, 582193]…; файлы: newh640e.res, newh640r.res, newh800e.res, newh800r.res
- `play…{r,e,t}.res`: RUS 2, ENG 2, одинаковых 0; размеры: [1303963, 1505501, 2031104]…; файлы: play640e.res, play640r.res, play800e.res, play800r.res
- `quest…{r,e,t}.res`: RUS 2, ENG 3, одинаковых 0; размеры: [337800]; файлы: quest_e.res, quest_r.res, quest_t.res, queste.res, questr.res
- `save…{r,e,t}.res`: RUS 2, ENG 2, одинаковых 0; размеры: [1237464, 1932024]; файлы: save640e.res, save640r.res, save800e.res, save800r.res
- `tgame.N`: RUS 0, ENG 1, одинаковых 0; размеры: [709822]; файлы: tgame.1
- `tlights.N`: RUS 0, ENG 2, одинаковых 0; размеры: [64000]; файлы: tlights.4, tlights.5
- `tutorial.N`: RUS 0, ENG 2, одинаковых 0; размеры: [128800]; файлы: tutorial.4, tutorial.5

Группы:
- `Konung.N` — карты, [formats/map.md](formats/map.md). Номера 1–50 с пропусками: 9, 13, 18, 26, 31, 34, 41.
- `LIGHTS.N` — освещение карт, [formats/map.md](formats/map.md). Нет для карт 6, 8, 46.
- `Game.N` — начальное состояние новой игры за героя N (0 — Волк, 1 — Драгомир, 2 — Михаил), [formats/game-state.md](formats/game-state.md). `TGame.1` — то же для обучения.
- `<экран><разрешение><язык>.RES` — экраны интерфейса: `MENU` (меню), `INTR`/`INTO` (вступление?), `NEW`/`NEWH` (новая игра, выбор героя), `PLAY` (игровой экран), `SAVE` (сохранение), `ME` (?). Разрешение 640 или 800, язык `R`/`E`/`T`. По Slavik: в начале — таблица картинок RLE с палитрами из `GRAPH.RES`.
- `QUEST*.RES` — 337800 байт во всех языках, вероятно, тексты квестов фиксированной длины.

На диске, вне установки: `GRAPH.RES`, `LIGHTS.RES`, `Video/*.flm` (ролики, проигрывает `playavi.dll`).
