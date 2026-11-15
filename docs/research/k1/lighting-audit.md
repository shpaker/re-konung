# K1 map lighting audit

Audit date: 2026-11-15. Scope: the first game's map lighting in the Rust model,
Player and Viewer, compared with the owner's RUS, ENG and tutorial data and EXEs.
This audit does not change map/state formats, original files, or the parallel
character-creation work. It does not claim comparison with a running original game.
The canonical rule description remains [lighting.md](lighting.md) and [time.md](time.md).

## Results and corrections

| Finding | Severity / confidence | Before | Correction and evidence |
|---|---|---|---|
| Wrong color domain | High / confirmed | sRGB textures were decoded by Bevy, then original palette formulas ran on linear RGB. Nights were too bright and warm transitions too strong. | Convert sampled linear RGB to display-encoded RGB before lighting, then back before tint/compositing. Original palette tables operate directly on encoded 5/6-bit components (RUS `0x439B08..0x439DDE`). A synthetic GPU gray sample now produces `(51,38,38)` under `(-0.6,-0.7,-0.7)` rather than the much brighter linear-domain result. |
| Missing constant-map lighting | High / confirmed | Every map followed the outdoor cycle; the day/night switch cleared even fixed colors. | Execute the activation flag lookup and apply the fixed color before either option gate. All 56 slots checked in every binary; flags 42..55 are set. Maps 42, 44 and 45 have special colors; other flagged slots really apply `(-1,-1,-1)`, not an absence sentinel. Both frontends now pass the selected map number. |
| Dawn green division | Medium / confirmed | Window 12 evaluated `u/5` as a float and truncated after subtracting 70. Green brightened one step early for 80 ticks per palette. | Divide as an integer before subtraction, matching RUS `0x42947B..0x42948B`. All 43,200 packed states now match for each binary. |
| Approximate ramp coefficients | Low / confirmed | The model used `0.33`, `0.66` and `0.5988`, and exposed fractional channels before later packing. | Read the actual binary64 pool and evaluate its products before truncating to packed bytes. The old approximations happened to produce the same packed bytes outside the dawn-green defect; this prevents a naive replacement by f32 thirds from introducing new boundary errors. At tick 8154 blue is −67, not −68. |
| Lit pixels with nonnegative ambient | Medium / confirmed | Red/green light was ignored when ambient was zero or positive, following Slavik's shader. | The original lit path brightens first, then uses only the negative part of `ambient + mask`. Positive ambient is not applied there. Confirmed by RUS setup `0x43E7FA` and blit `0x43CC49`; GPU cases cover zero, positive and negative ambient. |
| Fractional ground-light levels | Low / confirmed | The ground used a floating half of the random level, then division by 100. | The usual original path integer-halves levels first, then selects the positive table factor. Odd levels no longer introduce an invented half step. RUS `0x4254E6..0x42553C`; the conditional full-strength path is an open question below. |
| Missing separate light-effects gate | Medium / confirmed | Only the day/night gate was represented in the model. | `params_for_map(tick, seed, map, day_night, light_effects)` keeps ambient when local light effects are disabled. Both frontends currently supply `true`; no decorative menu control was added. |
| Contradictory evidence labels and comments | Low / confirmed | The render header claimed terrain fades only at night; the source page both located and called unknown the effects global, and denied an existing absolute map-flag writer. | Reconciled the rule page and code comments. Terrain/decorations already used the common ambient correctly; that stale comment was not a runtime defect. |

The public changes are additive: `constant_map_color` and `params_for_map`.
`params` and `params_gated` remain the map-independent model. `ambient_value_with`
retains its float tuple interface but now returns the packed whole-byte values.
The generic renderer still receives only lighting parameters and mask/fine-grid data.

## Original evidence and reproduction

`uv run tools/audit_k1_lighting.py` loads the owner's binaries into an isolated x86
emulator. It executes the lighting procedure at every cycle tick for each palette,
compares with the Rust `lighting_trace` example, checks thresholds, tests the option
bypass and all constant-color slots, and executes the map-flag writer for every slot.
The script dependencies are isolated and pinned; nothing is added to game dependencies.
No executable bytes or full traces are stored in the repository.

| Build | lighting body VA | body exit VA | flag store VA | effective flag byte base |
|---|---|---|---|---|
| RUS `konung.res` | `0x428FEC` | `0x42952A` | `0x43AE0A` | `0x45AE2F` |
| ENG `KONUNG.EXE` | `0x429164` | `0x4296A2` | `0x43B052` | `0x45AE67` |
| ENG `TUTORIAL.EXE` | `0x429164` | `0x4296A2` | `0x43A422` | `0x45AE73` |

**Confirmed:** the three traces agree. Before the corrections each had 160 mismatches
out of 43,200 palette/tick states; afterwards each has zero. The complete trace
(R, G, B as signed bytes, then ground flag, Deep before Green) has SHA-256
`3fe6c2dafcb46ed813a7311c3fd38022d79094b1756c13ee6a0335a03c62d40b`.
The ordinary Rust unit test pins separate FNV-1a checksums for both complete cycles,
so CI covers every tick without distributing fixtures derived from game resources.

**Emulation limitation:** random selection is replaced with a controlled result to
exercise both branches. RUS runs its original rounding helper at `0x43EAAE`.
The protected ENG/TUTORIAL helpers are not executable as stored on disk; their calls
are intercepted with the RUS-confirmed chop semantics, letting the original `fistp`
perform truncation. Those runs verify the intact light routines and pools under that
explicit rounding assumption, not the protected loaders or a complete game session.
The routines have matching control flow, not byte-identical per-build operands.

The RUS 565 palette-table builder is also executed independently and its 25,600
entries compared with the documented f32-accumulation/truncation arithmetic. The
lit-path evidence comes from the original's pointer-table setup and pixel blit:
nonzero mask bytes select the negative ambient table after the positive light table;
zero-mask pixels and blue use the ordinary ambient transform. This is independent
of the reference reimplementation's fragment shader.

Slavik remains a read-only, unlicensed secondary reference. Its
[light calculation](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L1762)
and [shader](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/gfx.cpp#L56)
are not authoritative for integer division, encoded color, or positive-ambient lit
pixels. These discrepancies were reported in the chat; no change was made upstream
and no reference code was copied.

## Buildings, masks and surfaces

**Confirmed on local data:** each edition has 43 game maps, 19 tile masks, 463 simple
object definitions and 74 flame frames. Each edition's map light grids contain
4,586 nonzero tile entries. Maps 6, 8 and 46 have no `LIGHTS.N`; absence means no
local tile mask, not daylight. The two ENG tutorial maps have 173 lit tiles in total.
Every referenced mask exists, every fine block matches the tile-derived grid, and
every parsed light file writes back byte-exactly. Existing Python map and Rust
original-graphics tests cover those relationships and object/flame references.

The three distinct mechanisms must not be conflated:

- **Ground light:** the map supplies the tile's 1-based mask index; zero means no
  mask. The renderer samples that mask over the tile and its upper layer. The
  original gates it at ticks `[8100,18900)` and with the effects option. No dynamic
  light source is inferred from a bright window in an object's image.
- **Object/flame appearance:** objects with at least two flame-list entries skip
  ambient; flames and shadows also skip it. This rule remains **by Slavik**, not
  promoted to binary-confirmed by this audit. All 74 qualifying object definitions
  in both editions have active entries, so list-length and active-count interpretations
  coincide on the available files. Flame placement follows object offsets and EXE
  animation ranges; it does not generate or reposition ground masks.
- **Creature appearance:** the existing rule uses zero/nonzero fine-light cells to
  select ambient or unchanged color, independently of the tile-light interval.
  That behavioral interpretation remains **by Slavik**. The grid geometry and
  values are confirmed from every available map; movement/material integration is
  covered by scene tests. This audit does not invent building occupancy schedules.

**Confirmed current behavior:** the simulation owns the clock; pause holds it and
scrub derives lighting from the requested tick. The viewer switch changes only
lighting, preserves the tick and survives viewer map replacement. Viewer map
replacement starts an independent simulation. Player inter-map travel/save restore
is not implemented here; its documented continuous-time target is not a tested
completed capability.

## Render and regression checks

The synthetic probe exercises the actual material, shader, sRGB texture decoding
and output encoding without game content:

```sh
cargo run --profile dev-runtime --manifest-path crates/Cargo.toml -p kn-render \
  --example lighting_probe -- out/lighting-audit/probe.png
uv run python tools/check_lighting_probe.py out/lighting-audit/probe.png
```

All six RGB cases passed, within one 8-bit output step: unchanged gray, negative
ambient, positive ambient, masked night, masked positive red, and masked neutral
ambient. Expected/observed pixels are `(128,128,128)`, `(51,38,38)`, `(143,136,128)`,
`(83,67,38)`, `(138,133,64)`, `(138,133,128)`. Alpha is passed through the lighting
shader to Bevy's standard tint/discard/compositing path.

Real-map before/after captures use identical map/tick/camera inputs and are kept only
in `out/lighting-audit/{before,after}`. The capture command is:

```sh
cargo run --profile dev-runtime --manifest-path crates/Cargo.toml -p kn-viewer -- \
  --game-dir original/k1/rus/game --map 17 --light-tick 12000 \
  --screenshot out/lighting-audit/after/rus-12000.png
```

The matrix includes both editions, map 17 through neutral/warm/dusk/local-light/
dawn states, buildings on map 1, local light on map 19, fixed-color maps 42/44/45,
and map 46 without a mask file. With seed 42, tick 12000 is the Green palette;
tick 55200 is the Deep palette (day 2). Tick 33600 is another Green day, not a
second palette. Live creatures/flames and diagnostic text are not frozen by
`--light-tick`, so full-screen image equality is not an acceptance criterion.

Rust tests cover the complete packed trace, boundary cases, separate gates,
constant-map precedence, viewer map changes, material assignment and moving
creatures. `just check` remains the complete repository gate. Performance measurements
and their limitations are recorded in [rendering.md](../../development/rust/rendering.md#lighting-audit-measurements).

## Remaining limits and intentional differences

- **Unknown applicability:** RUS `0x4254A9..0x425514` chooses full-strength ground light
  when a nonnull record at `[0x81D580]` selects a group whose final byte equals 1
  (`[group * 32 + 0x67C0A6] >> 24`). Otherwise levels are integer-halved. The group
  field is still unknown in the format description, and the active-record lifecycle
  is not recovered. The normal half-strength path is implemented; no invented
  meaning or guessed player bonus is attached to this exception.
- **By Slavik:** per-draw random light ranges, and the object/flame/shadow/creature
  exemptions above still need independent behavioral or binary confirmation.
- **Unknown:** shipped option defaults, the narrative meaning of the two night
  variants, exact save/map-activation lifecycle, and nighttime NPC schedules.
- **Intentional:** per-day palette selection and flicker are deterministic functions
  of seed/tick; flicker changes every 16 ticks. The clock is one simulation counter,
  not the original per-frame speed multiplier. UI hour/phase names are conventions.
- **Intentional precision differences:** display-encoded float lighting avoids the
  original intermediate 5/6-bit truncation. Masks retain smooth sampling and an
  8-bit normalized texture representation; filtered values below one percent use
  the plain palette path. Bevy blends alpha in linear space. The earlier claim
  that every resulting pixel differs by at most one original quantization step
  was not established and is withdrawn, especially for the two-stage lit path.
  The GPU probe verifies the chosen formulas, not universal screenshot identity
  with the original framebuffer.
