# New-game constructor

The product reference is [../design.md](../design.md). Rules live in
`k1-game::new_game`; the facade prepares a session, the application stores a
constructor draft, and the frontend only displays it and submits actions.

## Attribute allocation

**Confirmed by our experiment on both executables:** the original upgrade
routine is at VA `0x410B08`. Its 13 price bands begin at file offset `0x411B0`
in RUS `konung.res`, `0x423E8` in ENG `KONUNG.EXE`. Attribute progression begins
at `0x412AC` / `0x424E4`, in class-major order: 104 bytes per class, 26 per
attribute. The four attributes are agility, charisma, strength and endurance.

The menu uses level 10. The maximum lookup is `table + 2 * level - 1`, while
the next-level minimum uses `table + 2 * level`. Thus the creation limits are
95 for the class's main attribute and 37 for the other three, including the
merchant's secondary attributes. Primary attributes: strength for warrior,
agility for hunter, endurance for merchant, charisma for chief.

**Confirmed by emulation:** `tools/verify_new_game_rules.py` executes that
original function on synthetic character records, without launching the game.
It checks all four classes, four attributes, values 0–100 and five available
point balances: **8080 calls per edition, 16160 total**, all agree with our
price and limit rules. Reproduce from the repository root:

```sh
uv run tools/verify_new_game_rules.py
```

**Reference behavior:** the menu minus/plus branches in Slavik
[`game_newgame.cpp:472–599`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_newgame.cpp#L472)
refund the upgrade price at the decreased value, allow reduction to zero,
and spend the price at the current value when incrementing. Selecting an
occupation resets its attributes and free points; hero and specialization
changes preserve allocation. Unspent points carry into the game.

Our implementation computes remaining points from the difference between the
initial and current cumulative costs. This preserves the original outcomes
without reproducing its temporary addition of 1000 points for a refund check.
Invalid values and an overspent budget are rejected again when starting the
session. The UI disables unavailable changes, displays each available change's
cost/refund, and recomputes carrying capacity and party capacity immediately.

### Disagreement with Slavik

**Confirmed external discrepancy:**
[`game_character.cpp:12–141`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_character.cpp#L12)
uses `[Level][1]` instead of the original `[Level - 1][1]` maximum and has a
medium progression row for the merchant's secondary attributes that is absent
from both original executables. Compare the table test and emulator above to
that function. The original data and executed routine are authoritative for
our implementation; the reference repository remains unmodified.

## Equipment and preview

**Confirmed over all six Game.N files:** the player record's equipment is
`[0, 0, 1, 0, 0, 0]`, with the weapon reference in slot 2. The item's identity,
properties, inventory, accessories and wielding state remain those of the
selected file. No demonstration items are inserted or substituted.

**Reference behavior:** Slavik
[`game.cpp:2433–2456`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L2433)
loads Game.N, then copies the chosen attributes, skills, free points, name,
model, palette and class, setting the playable level to 1. It does not copy the
menu's demonstration equipment or item table. Previously our preview applied
demonstration slot numbers to the unrelated Game.N item table, showing equipment
that belonged to neither the original menu nor the playable character.

**Intentional presentation difference:** our preview shows the actual starting
equipment, using the same composition path as the configured session. It shows
playable level 1 rather than the internal allocation level 10. This makes the
preview truthful without changing the starting loadout. Both editions' previews
are rendered locally by:

```sh
cargo run --release --manifest-path crates/Cargo.toml -p kn-game --example inspect_new_game
```

Outputs are local RGBA files and sizes under `out/new-game/`. No game-derived
image is distributed. Our local inspection covered all six previews; the test
compares each preview's pixels with the configured session's pixels.

**Confirmed representation:** a character's palette is a byte offset into
256-color palettes. Reference indices 74, 33, 39 need multiplication by 512,
not by two. Sources: `k1-data/src/characters.rs`,
[formats/graph-res.md](formats/graph-res.md), and
Slavik [`resources.cpp:13`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/resources.cpp#L13).

## Runtime and interface contracts

The session effect carries the complete draft. The prepared state is shared by
scene assembly, animation loading and simulation; leaving or failing the session
removes it. Tests cover 144 hero/class/specialization combinations across both
editions, nondefault allocation, the remaining balance, equipment identity,
unchanged NPCs and original item bytes, and the absence of added placement
failures. Disk files are never written. Item comparisons use serialized bytes
because unused floating fields can contain NaN.

The entire portrait card is a button, with original selected/unselected images.
Tests activate the actual controls and check attribute refunds, spending,
class reset and keyboard focus restoration across view rebuilds. The page
uses shared text colors, wrapping content and a scrollable body; the footer
stays separate, Back on the left and Begin on the right. Escape cancels;
Enter/Space activate the focused control. Scroll position survives edits.

Real pointer acceptance and final window/DPI visual inspection remain unverified:
`unified-computer-use` returned `Invalid app` for the executable name and absolute
path and omitted the running game from its inventory. This limits the evidence
for presentation; it does not leave the allocation or equipment implementation
unfinished. Headless activation tests and sprite inspection are not represented
as real mouse testing.
