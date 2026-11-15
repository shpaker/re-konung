---
name: konung-reverse-format
description: >
  Investigate a Konung file format or game rule through hypotheses, checks over
  every available edition, a parser and tests, research documentation and Tiled
  verification. Use to identify unknown structures, refine fields or recover
  a fact from the read-only Slavik reference.
---

# Reverse a format

1. **Existing evidence.** Read `docs/research/k1/README.md` for format status,
   then the topic in `docs/research/k1/formats/`. Inspect Slavik with `rg`
   (`../../Marisa-Chan/Slavik`, Windows `E:\github\Marisa-Chan\Slavik`);
   never copy its unlicensed code.
2. **Hypothesis on data.** Use a short scratch script or `uv run python -`
   over original/k1/*/game. Start with total block sizes, then ranges and
   whole-file statistics (Counter). Compare RUS and ENG and every other available edition.
3. **Image.** If the format draws anything, export through
   `uv run konung k1 tiles|map …` or a temporary script into out/ and inspect it.
4. **Parser.** Implement pure parse(data: bytes) in konung/k1, preserve unknown
   bytes in unknown fields. Add a CLI command when a person needs the result.
5. **Test.** In tests/k1 verify EOF consumption and cross-file references;
   use synthetic cases for algorithms. Written formats must round-trip every file.
6. **Documentation.** Add the field table (offset/type/name/meaning), confidence
   and source to `docs/research/k1/formats/<format>.md`; update the K1 research
   index. Game rules belong in the corresponding research topic, decisions/costs
   in development, commands in guides.
7. **Tiled.** For a map-affecting format mirror Python in tiled/konung/lib/k1.mjs,
   run just tiled-install, verify loading through step 4 of
   skills/konung-tiled-install, then compare render_map/map.toImage() against
   the Python PNG. See `docs/development/tiled.md` for representation and checks.
