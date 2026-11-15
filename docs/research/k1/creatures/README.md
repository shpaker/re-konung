# Creature research

K1 characters, monsters and villagers: what is confirmed, source-only,
hypothetical or unknown. Record layouts live in [Game.N](../formats/game-state.md),
images in [GRAPH.RES](../formats/graph-res.md) and
[OBJECTS.RES](../formats/objects-res.md), names in [EXE tables](../formats/executable.md).

| Topic | Content |
|---|---|
| [Animation](animation.md) | States 0–9, frame engine, timing and the two meanings of state 4 |
| [Behavior](behavior.md) | Orders, squad state, villager decisions, monster returns and names |
| [Placement](placement.md) | Roaming scatter on load, refusals and observed goal reaches |
| [Combat](combat.md) | Hit/death/melee paths, health, measured sequences and unresolved rules |

The original census used ENG Game.0–2: 3179 filled characters and 572 filled
groups. RUS is also unpacked; a section claiming both editions checked each
separately. A code citation alone is **by Slavik**. **Confirmed** requires our
own file, executable or visual check; hypotheses/unknowns stay explicit. The
read-only reference is pinned to cf5a332 where a permalink is given.

Our implementation choices and costs belong in
[simulation](../../../development/rust/simulation.md) and
[rendering](../../../development/rust/rendering.md). Deliberate differences
remain noted beside their source facts. Unknown combat/village rules are left
unimplemented rather than replaced with guesses.
