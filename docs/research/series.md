# Games and editions

The project targets the 2D family, beginning with K1. A shared engine between
K1, K2 and K2.5 is a **hypothesis** until their files are compared. K3 is 3D
and outside the project. Gameplay similarities alone do not establish binary
compatibility; see the [mechanic comparison](comparison.md).

| Game | Year | Developer | Publishers | Engine / project status |
|---|---|---|---|---|
| «Князь: Легенды Лесной страны» / Konung: Legends of the North | 1999 RU, 2000 EN | Snowball Interactive, 1C | 1C; Strategy First EN | Custom 2D isometric; 640×480 and 800×600. RUS and ENG available locally. |
| «Князь 2: Кровь Титанов» / Konung 2: Blood of Titans | 2003 RU, 2004 EN | Snowball Interactive, 1C | 1C; Got Game Entertainment US | **Hypothesis:** evolution of K1's engine. No copy examined. |
| «Князь 2.5: Продолжение легенды» | 2004–2005 | Fan team building on K2 | 1C | **Hypothesis:** K2 engine. No copy examined. |
| «Князь 3: Новая династия» | 2009 | Different team | 1C | 3D; outside scope. |

Dates and teams came from [DTF](https://dtf.ru/games/233716-pervaya-russkaya-rpg-istoriya-serii-knyaz),
[Cyclowiki](https://www.cyclowiki.org/wiki/%D0%9A%D0%BD%D1%8F%D0%B7%D1%8C_2:_%D0%9A%D1%80%D0%BE%D0%B2%D1%8C_%D0%A2%D0%B8%D1%82%D0%B0%D0%BD%D0%BE%D0%B2),
[StopGame](https://stopgame.ru/game/knjaz_2_prodolzhenie_legendy) and
[Wikipedia](https://en.wikipedia.org/wiki/Konung:_Legend_of_the_North).
Sources disagree: an Infinity Engine attribution found on Wikidata was
unsupported and is not adopted here. This is a record of research, not a fresh
verification of external catalogues.

## K1 engine evidence

- [Maps](k1/formats/map.md): staggered isometric grid, tile rhombus 114×64,
  placement pitch 116×32.
- [Graphics](k1/formats/rle.md): 8-bit row-compressed sprites and an RGB555 color bank.
- **By external source:** SOUNDS.RES uses 22050 Hz PCM; disc music is CD audio.
  The collector's edition reportedly uses MUSICS.RES OGG
  ([unpacker](https://github.com/dopedealer/rtsk_konung_mus_unpacker)).
- Video `*.flm` is handled by `playavi.dll`.
- Interface files vary by resolution and language: MENU800R.RES is the Russian
  800×600 menu; E is English, T an **unknown** third language in ENG.

## K1 editions

| Edition | Local status | Evidence |
|---|---|---|
| RUS CD 1999 | `original/k1/rus` | CD plus 10 audio tracks; Allods 3 and WAR demos |
| ENG CD 2000, Strategy First | `original/k1/eng` | Tutorial, network play and a third interface language |
| 1C collector's reissue | Not examined | Reported OGG music in MUSICS.RES |
| GOG | Not examined | HD/widescreen patch references in PCGamingWiki |

Exact observed differences are in [editions](k1/editions.md); extraction steps
are in [game-file preparation](../guides/game-files.md).

## Verify the shared-engine hypothesis

1. Prepare the owner's copies of K2 and K2.5.
2. Compare file names and sizes: GRAPH.RES, OBJECTS.RES, KONUNG.N and others.
3. Try the K1 parsers and investigate every mismatch.
4. Record results here. Extract shared code only after commonality is demonstrated;
   keep differences in the relevant game package. Movement's existing shared
   implementation is the explicit exception described in the architecture.
