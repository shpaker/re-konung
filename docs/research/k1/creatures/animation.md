# Creature animation states

[Research and confidence](README.md). Character byte 0x00's low four bits are
the animation state; class byte 0x04 bit 0x80 marks a dead record.

## States 0–9

| Action | Weapons in hands | Weapons put away | Meaning |
|---|---|---|---|
| Idle | 0 | 6 | Standing |
| Walk | 1 | 7 | Following a route |
| Hit reaction | 2 | 8 | Stagger |
| Death | 3 | 9 | Dying/dead sequence |
| Human shot / dynamic special | 4 | 4 | Two distinct uses, described below |
| Melee | 5 | 5 | Attack sequence |

Byte 0x03 bit 0x04 selects the paired variants. GRAPH.RES holds all ten
sequences; dynamic OBJECTS.RES sprites hold six and map 6–9 to 0–3
(Slavik game_character.cpp:144-150). State names CHSTATE_0–9 are game.h:327-336.

Sources for selection: idle game_play.cpp:305-308 and 2465-2468; walk 1964-1967
and 3944-3947; death/hit 492-495, 504-507 and 1659-1662. Shooting needs equipment
slot 1 plus arrows (2421-2429). Melee enters when a step reaches an enemy cell
(game.cpp:2898-2913), a neighboring enemy attacks (game_play.cpp:4210-4243), or
ambush classes 0x44/0x57 decide (3200-3220). Lines 914-930 handle the state,
not entry. Exact hit/death/melee paths: [combat](combat.md).

**Our measurement on both editions:** per Game.0–2 triple, states are only
0 (2143), 4 (404), 6 (632). Combat states 2/8,3/9,5 never occur. All 404 state-4
records are dynamic classes 0x46/0x4C/0x55, Frame=0, no equipment/quiver;
none is a human shot. The weapon-in-hands bit is set in exactly states 0/4
(2547), clear in state 6 (632). All state-6 records are humans of kinds
1,2,3,5 without dynamic-class bit 0x40; every dynamic record holds the bit.

## Walk skip mask — byte 0x11

**By Slavik:** states 1/7 advance only if
`mask == 0 || (counter & mask) == mask` (game_play.cpp:3503; game.h:460,
game_state.cpp:24). **Our measurement:** zero for every filled starting record,
so they advance every tick. Preserve the field when writing; its separate
mask-based scheduling is not reproduced as an engine limit.

## Frame engine

**Where the machine runs.** The animation state is byte 0x00 of a character
record. Once per game tick — 60 ms, [`game.h:30`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.h#L30)
— the game walks the squads of the current map in the group order of the file and
runs one of three copies of the same ten-state switch: `FUN_004123bc` for roaming
squads
([`game_play.cpp:3468-3726`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3468-L3726)),
`FUN_00416058` for villager squads
([`game_play.cpp:2824-3029`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2824-L3029))
and `FUN_00421da4` for everyone else — the player's squads and lone NPCs
([`game_play.cpp:737-990`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L737-L990)).
The copies differ in what a state ends in, which is why the table below names the
pass. The speed option runs extra passes per frame
([`game_play.cpp:122-135`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L122-L135)),
so a combat state is measured in ticks, not in screen frames.

**The frame engine.** Every state change goes through one setter, `FUN_004143dc`
([`game.cpp:2766-2915`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L2766-L2915)):
it loads the sequence of the pair (state, direction), puts `Frame = 0` and plays
the state's sound (states 2/8 and 3/9 have their own, a human shot plays a bow
sound, and a dynamic-object creature gets none for state 4 — by Slavik). One tick
advances exactly one frame: `FUN_00413de4`
([`game_play.cpp:3727-3786`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3727-L3786))
raises `Frame`, and when the frame would pass the end it steps back and returns
false — that false is the only signal a caller gets that the sequence is over.
Nothing loops on its own: idle and walk restart the same state, the hit reaction
and the shot hand the creature to the idle chooser, death holds its last frame
forever. A sequence of `N` frames shows frames `0 … N-1` over `N` ticks and the
caller leaves the state on the next tick.

| State | Entry | Play mode | Length, our measurement | Exit | The beat that matters |
|---|---|---|---|---|---|
| 2/8 reaction to a hit | a blow that lands and does not kill: an arrow result 0, a melee result 0 (confirmed) | one-shot (confirmed) | human 7 frames = 420 ms; monsters 4–17, three sprites have none | idle chooser; a hit monster first puts its whole squad into combat mode | a second hit re-enters and restarts frame 0 (by Slavik) |
| 3/9 death | the result code 1 of a hit, or poison draining health to zero (confirmed) | hold (confirmed) | human 12 = 720 ms; monsters 7–16, one sprite has none | none — the last frame is held | the corpse is committed at the second-to-last frame, not at the last |
| 4 — the shot of a human | the shoot loop: a bow in equipment slot 1 and a quiver (confirmed) | one-shot; the loop re-enters it while its flag stands (confirmed) | human models 12 = 720 ms; no record of a new game reaches it — nobody carries arrows | idle chooser, or the shoot loop re-enters | the arrow is created at frame `FrameCount − 6`, and the state turns to face while `Frame == 0` |
| 4 — a dynamic creature's own animation | the creature's appearance or transformation: a skeleton rising instead of dying (confirmed); the 404 records of classes 0x46, 0x4c, 0x55 (confirmed) | one-shot (confirmed) | the five dynamic objects that carry the sequence: 5 → 13, 11 → 17, 17, 21, 22 → 12 | idle chooser | it is not a shot: those records hold no bow and no quiver, and the clone's monster branch creates no projectile there (unknown) |
| 5 melee | a walk step into the cell of an enemy, a counter-attack against an adjacent attacker, the ambush classes 0x44/0x57 (confirmed) | loops while the enemy stands (confirmed) | human 12; monsters 10–16, three sprites have none | the enemy died or became unreachable | health is written when the swing has fully played; the victim's squad is marked at frame 2 |

## State 4 — two meanings under one number

One state value carries two things
that share nothing but the number, and the data never lets one be read as the
other: the shot of a human who shoots a bow, and the appearance or transformation
animation a dynamic creature plays for itself. Which of the two a creature uses
follows from what it is, not from a decision, which is why the table above gives
them two rows and the text below keeps their two paths apart.

*The shot of a human.* The state is only reachable by a creature that holds a
weapon in hands: the human shooting branch requires a bow in equipment slot 1 and
a quiver
([`game_play.cpp:2421-2431`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2421-L2431),
[`game_play.cpp:3808-3824`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3808-L3824)),
and it is what byte 0x02 bit 0x04 gates — `FUN_0041ed8c` runs the shoot loop while
that bit is set, re-targets, and re-enters the state as long as the bow, the
arrows and the target hold, clearing the bit the moment one of them is gone
([`game_play.cpp:3952-4016`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3952-L4016)).
The *name* of this bit the clone never gives, but its use is exactly the shooting
flag, so we treat it as **confirmed behavior** ([order flag 0x04](behavior.md#order-and-presentation-flags)).
While `Frame == 0` the shooter turns toward the target one
direction per tick and does not advance the animation; at `Frame == FrameCount − 6`
the arrow is created and the quiver count drops
([`game_play.cpp:3622-3648`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3622-L3648),
and the same shape in the player and villager passes at
[`game_play.cpp:885-912`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L885-L912),
[`game_play.cpp:2927-2957`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2927-L2957)).
`FUN_0041bff8` is the only place a projectile is created — at most 100 in the air,
and it carries the bow's damage and the quiver's poison
([`game_play.cpp:2568-2623`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2568-L2623)).
An arrow is a trace, not a homing shot: each tick it steps eight times, dies on
impassable terrain (and, if it was aimed at a village building, sets it alight with
one roll of 21%), and damages whoever's cell it passes over **except the shooter
himself** — the shot has no friendly-fire filter
([`game_play.cpp:394-526`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L394-L526)).
The state ends into the idle chooser; the shoot loop may immediately re-enter it.
This is the human-shot path; it must remain distinct from a dynamic special.

*The appearance or transformation animation of a dynamic creature.* A dynamic
sprite enters the same numeric state 4 for something that has nothing to do with
shooting: a sequence its class plays to come onto the board or to change shape. The
skeleton (class 0x4c) is the confirmed case — instead of finishing its death it
restores health, steps its weapon slot down and re-enters state 4 to rise again
([`game_play.cpp:3586-3594`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3586-L3594))
— and it is what the shipped files hold: the only 404 state-4 records of a new game
belong to the three dynamic classes 0x46 (152 records, dynamic object 5), 0x4c
(249, object 11) and 0x55 (3, object 22); none of them is human, every one carries
`Frame = 0`, a weapon slot above 0, no equipment and an empty quiver ([combat measurements](combat.md#our-measurements)). The two uses are disjoint by data, not by convention: the human
branch above needs a bow in slot 1 and a quiver, and these records hold neither, so
not one of them can be a shot. What each sequence shows — rising from the ground,
jumping out of a burrow, a human↔beast change — is the content of the
dynamic-object sequences: **only the human path launches a confirmed shot;
a dynamic state-4 sequence must never be routed through that shoot loop.** Whether the monster branch that plays
state 4 for these classes also releases an arrow is **unknown** («What stays
unknown» in [combat](combat.md#what-stays-unknown)), so that branch remains unimplemented.

## Deliberate play-mode difference

Slavik holds a dynamic special at Frame=0 until its monster decision releases it
([game_play.cpp:3226-3232](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3226-L3232),
[3312-3317](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3312-L3317)).
Our map-load event starts it explicitly so an unrecovered decision cannot hold
a creature forever. The original restarts the special on map entry
([game.cpp:692](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L692));
its frame counter clamps rather than loops
([game_play.cpp:3727-3735](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3727-L3735)).
Both our windows play once on load; the viewer can replay. Implementation:
[one-shot state machine](../../../development/rust/simulation.md#one-shot-specials).
