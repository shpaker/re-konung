# Издания первой части

## Откуда данные

Свои образы дисков (RAR с MDS/MDF) кладутся в `original/k1/rar/` (в git не попадают).

## Как распаковать

Нужны 7-Zip и uv. Один рецепт на издание:

```bash
just k1-unpack rus original/k1/rar/rus.rar
```

```bash
just k1-unpack eng original/k1/rar/eng.rar
```

Получается `original/k1/<издание>/`:
- `image/` — MDS/MDF из архива;
- `disc/` — `track01.iso` и `track02…11.wav` (музыка игры — CD-аудио), см. [../disc-images.md](../disc-images.md);
- `files/` — содержимое диска;
- `game/` — **установленная игра**: распакованный `inst/setup.dat`. Это обычный ZIP (7-Zip предупреждает о 18 лишних байтах в конце, файлы целы). С этой папкой работают `konung k1 …`, тесты и расширение Tiled.

## Отличия

На диске обоих изданий лежат установщик, `GRAPH.RES`, `LIGHTS.RES` и ролики `Video/*.flm` (игра читает их с диска). Остальная игра — в `inst/setup.dat`.

Разница в размере архивов складывается из содержимого диска, а не из игры:
- в RUS есть ролик `poster.flm` на 130 МБ (в ENG — 20 МБ) и демоверсии «Аллоды 3» и WAR (`Demo/`, 48 МБ);
- в ENG есть ролики `prolog.flm` (69 МБ) и `tutor.flm`, DirectX 7 в `inst/dx/`;
- у ENG дата-трек в формате CD-XA, образ снят с субканалами.

Сама игра (`game/`):

| | RUS | ENG |
|---|---|---|
| Файлов | 119 | 134 |
| Карты `Konung.N` | 43 | 43, **побайтно те же** |
| `LIGHTS.N` | 40 | 40, те же |
| `Objects.res`, `FLAMES.RES`, `LIGHTS.RES` | | те же |
| `GRAPH.RES` | отличается от ENG одним курсором (+22 байта); на диске RUS лежит вариант ENG | |
| Интерфейс | `*R.RES` — русский | `*E.RES` — английский, `*T.RES` — третий язык (`MENU`, `INTR`, `QUEST`) |
| Только в ENG | | туториал (`TUTORIAL.EXE`, `tutorial.4/5`, `TLIGHTS.4/5`, `TGame.1`), сеть (`LAN_KONG.EXE`, `Lanusers.dat`, `SERVER`, `Servers.cfg`), `SAVITEMS.DAT`, `KONUNG.EXE` под своим именем |
| Только в RUS | `CLIENT.EXE`, `konung.res` (это PE-файл, похоже на переименованный `konung.exe`) | |

Вывод: для карт издания равноценны. ENG полнее — в нём есть туториал и сетевая игра. RUS — оригинал 1999 года.

## Чего нет

- Коллекционного издания 1С и GOG. По сторонним данным, там музыка в `MUSICS.RES` (OGG) вместо CD-аудио, а HD-патчи рассчитаны на него.
- Патчей 01/02 для RUS (есть на [gospitaliers.narod.ru](https://gospitaliers.narod.ru/knyaz_files2.htm)).
