# Combat states and health

## Sources and confidence

Research recorded 2026-11-05 (#71).

Recovered over the read-only clone plus our own measurements. Every cite is a
permalink pinned to the commit
[`cf5a332`](https://github.com/Marisa-Chan/Slavik/tree/cf5a3320dc93859e3dc51f37ee823fed30f399b6);
the clone's code is never copied. Markers: **confirmed** — the clone's code and
our check of our own files agree; **by Slavik** — only the clone's code;
**hypothesis**; **unknown**.

## State 2/8 — the reaction to a hit

Four call sites, all of them the result
code of a blow, never a decision: an arrow that did not kill
([`game_play.cpp:504-508`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L504-L508))
and a melee blow that did not kill, one per pass
([`game_play.cpp:976-980`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L976-L980),
[`game_play.cpp:3000-3006`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3000-L3006),
[`game_play.cpp:3701-3706`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3701-L3706)).
The result codes come from the two damage helpers and they decide the state:
`1` kills, `0` staggers, `-1` is a blow so light (`hit < 48`, three displayed
points) that the victim keeps doing what it was doing, `-2` is a dodge or a block
and changes nothing at all
([`game_play.cpp:2699-2712`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2699-L2712),
[`game_arrows.cpp:42-55`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_arrows.cpp#L42-L55)).
The pair is chosen by the weapons-in-hands bit, so an armed creature takes 2 and
an unarmed one 8. The state re-enters through the same setter on every new hit,
which resets `Frame` to 0 — a stunned creature that is hit again starts its
stagger over (by Slavik). The exit is the idle chooser of the pass: `FUN_00414ab4`
for the player's squads
([`game_play.cpp:848-861`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L848-L861)),
`FUN_00410450` for a roaming squad — after that pass has set the squad state to 1,
so **a hit monster's stagger ends with its whole squad in combat mode**
([`game_play.cpp:3570-3579`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3570-L3579))
— and `FUN_00410728` for villagers, after they raised the village hostility flags
and aggroed the elder's squad
([`game_play.cpp:2897-2906`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2897-L2906)).
Marking a squad is its own helper: `FUN_004248a0` sets the squad byte to 1 and
cuts the route of every member that has no attack order, so the squad stops
walking where it stands
([`game_play.cpp:2555-2564`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2555-L2564)).

## State 3/9 — death

Entered only by a result code 1: a melee kill
([`game_play.cpp:955-973`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L955-L973),
[`game_play.cpp:2981-2998`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2981-L2998),
[`game_play.cpp:3679-3698`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3679-L3698)),
an arrow kill
([`game_play.cpp:483-499`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L483-L499))
or the per-tick poison drain that empties health
([`game_play.cpp:1332-1350`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L1332-L1350)).
The killer writes health 0 itself and clears the orders byte of the victim. While
the death sequence plays, the creature is still alive as the game counts it; the
corpse is committed when `Frame` reaches `FrameCount − 2` — the orders byte is
cleared, the dead bit of the class is set and the foot cell is released
([`game_play.cpp:863-883`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L863-L883),
[`game_play.cpp:2909-2925`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2909-L2925),
[`game_play.cpp:3581-3605`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3581-L3605)).
After the last frame nothing advances it: the frame engine refuses to pass the end
and the case refuses to call it, so a corpse holds its last frame and leaves the
cell free. The one exception is the skeleton (class 0x4c): at the same second-to-last
frame, if it still has a weapon in a slot above 0, health is restored to 1600, the
weapon slot steps down and the creature enters **state 4** instead of dying
([`game_play.cpp:3586-3594`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3586-L3594)).

## State 5 — the melee blow

Three entries, none of them an order. A character
walking a route that steps onto a cell held by an enemy it may hit takes the state
inside the state setter itself, which is also where it draws its weapons
([`game.cpp:2898-2913`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game.cpp#L2898-L2913));
a creature with no move or attack order that finds an enemy of another squad next
to it *and already attacking it* turns and swings back
([`game_play.cpp:4210-4243`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L4210-L4243));
and the ambush classes 0x44 and 0x57 enter it from the monster decision when a
player-squad member stands in one of the eight neighbours
([`game_play.cpp:3200-3220`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3200-L3220)).
Before the swing the attacker faces the target (`FUN_0043976c` picks the direction
that sees the enemy from the cell it stands on,
[`game_play.cpp:2625-2657`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2625-L2657)).
At `Frame == 2` the victim's squad is marked (and, if the victim belongs to a
village, its hostility flags go up); the health write happens only after the whole
swing has played, and then the attacker restarts the same state, so it keeps
swinging while the enemy stands
([`game_play.cpp:914-987`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L914-L987),
[`game_play.cpp:2959-3020`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2959-L3020),
[`game_play.cpp:3652-3723`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3652-L3723)).
It stops when the enemy is dead or the attacker can no longer reach it: the
per-pass guards `FUN_0041f258`, `FUN_00415cd0` and `FUN_00411fd0` drop
`EnemyCharID` and hand the creature to the idle chooser; for a monster the same
guard also abandons the chase when the target leaves the squad's leash rectangle
([`game_play.cpp:3088-3105`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3088-L3105))
— the pursuit itself is #72. Class 0x49 is the one attacker that steps away after
a blow instead of swinging again
([`game_play.cpp:3713-3720`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3713-L3720)).

## Orders, decisions and the budget

No order interrupts a playing combat
animation. A player order only writes the order byte and the route — the move
command sets `0x48` and truncates the path without touching the state
([`game_input.cpp:392-400`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_input.cpp#L392-L400))
— and the order is picked up after the animation ends, because all three exits of
the combat states set the needs-a-decision flag 0x40 of byte 0x02 on the way out
(`FUN_00414ab4`, `FUN_00410450`, `FUN_00410728`,
[`game_play.cpp:296-313`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L296-L313),
[`game_play.cpp:3049-3061`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3049-L3061),
[`game_play.cpp:2456-2470`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2456-L2470)).
The decision gate of the roaming and villager passes explicitly excludes the
combat states — states 2, 3, 9 for a monster and 2, 3, 8, 9 for a villager take no
decision at all
([`game_play.cpp:3474-3486`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3474-L3486),
[`game_play.cpp:2830-2843`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2830-L2843)).
That exclusion also has a cost side: once the budget of three expensive decisions
per tick is spent, a creature whose decision is pending and whose state is not idle
or walk is skipped *for the whole tick, animation included*
(`SkipBecauseOfHardCompute`,
[`game_play.cpp:3787-3792`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3787-L3792)),
which is one of the 1999 caps under the deliberate-limit differences rather than a rule to keep. State 4
is the one state the monster decision touches directly: for the classes 0x46, 0x4c
and 0x55 it advances the first frame itself, because the state-4 branch of the
monster pass — the clone names it a shot, one of the [two state-4 meanings](animation.md#state-4--two-meanings-under-one-number) — does
nothing while `Frame == 0`
([`game_play.cpp:3229-3236`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3229-L3236),
[`game_play.cpp:3607-3619`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3607-L3619)).

## Where health lives

Health is the s16 at 0x5C of a character record — the
fifth of the nine attributes that start at 0x54
([`game_state.cpp:47`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_state.cpp#L47),
our row 0x54 in [game-state.md](../formats/game-state.md)). The scale is 0…1600 with
100 displayed points: the interface prints `(HP + 15) / 16` and the health bar
splits at 400 and 800
([`game_character.cpp:715`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_character.cpp#L715),
[`game_play.cpp:4303-4350`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L4303-L4350)).
A blow writes health in exactly two places — the melee helper and the arrow helper
([`game_play.cpp:2701-2709`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2701-L2709),
[`game_arrows.cpp:44-52`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_arrows.cpp#L44-L52))
— and each of them refuses to write a negative value: the damage is applied only
while it stays below the current health, and the killing blow writes 0 from its
caller instead. Health is also an input to damage (a wounded creature strikes
weaker, `HP × CurrentSila × 0.04 × 0.0625`,
[`game_play.cpp:2045-2075`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2045-L2075))
and poison drains it outside combat
([`game_play.cpp:1336-1349`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L1336-L1349));
the formulas themselves are #74's, not this document's.

## Our measurements

Over `original/k1/{rus,eng}/game` — `GRAPH.RES`, `OBJECTS.RES` and the
`Game.0`–`Game.2` triples; every number below is identical in both editions
(3179 filled character records, 404 of them in state 4).

- **The shipped files contain no combat at all.** Recorded states are 0 (2143),
  4 (404) and 6 (632); 2/8, 3/9 and 5 never occur — those three are what a running
  fight does. State 4 belongs to exactly three dynamic classes, 0x46 (152 records,
  dynamic object 5), 0x4c (249, object 11) and 0x55 (3, object 22), and no human
  record is in it; all 404 carry `Frame = 0`, the skip-mask 0, the weapons-in-hands
  bit 0x04 of byte 0x03 and a weapon slot above 0, with no equipment and no quiver.
  So none of them goes through the human shooting branch, and each of them is the
  appearance or transformation animation of its class («State 4 — two meanings
  under one number»).
- **Durations, from our own sprite tables** (frames per sequence, equal over all
  eight directions and both editions — one tick per frame at 60 ms). The three
  human models of `GRAPH.RES`: state 2/8 is **7 frames** (420 ms), states 3/9, 4
  and 5 are **12 frames** (720 ms) — the same count as idle and walk, so only the
  stagger is visibly shorter. Of the 30 dynamic objects of `OBJECTS.RES`: state 2
  runs 4–17 frames, state 3 runs 7–16, state 5 runs 10–16, and state 4 is empty
  for 25 of them; the five that carry a state-4 sequence are objects 5, 11, 17, 21
  and 22 with 13, 17, 12, 12 and 12 frames — of these only 5, 11 and 22 belong to a
  class the files record in state 4, so the sequences of 17 and 21 are unreachable
  from the shipped data. Every non-empty state-4 sequence is at least 12 frames, so
  the `FrameCount − 6` release beat of the human shot is always reachable; no melee
  sequence is shorter than 10, so the frame-2 marking beat is too.
- **Health in the shipped files takes two values only:** 1600 for 3063 records and
  **1** for 116 — 110 children (class 0x52), 3 lone NPCs (0x53) and 3 of 0x56 —
  all of them in state 0. No record carries the dead bit of the class, and the
  hero (record 0) is at 1600. Nothing in the starting data is wounded or dead, so
  a test over the shipped files cannot exercise 2/8 or 3/9 without a synthetic
  record.
- **Nobody carries arrows.** Field 0xCC, the quiver reference, is 0 in all 3179
  records, while 626 records do carry equipment (classes 0x1–0x5, plus three of
  0x45 and three of 0x54). Since the shooting branch needs both a bow in slot 1 and
  a quiver, no creature of the starting data — the player's party included — can
  enter state 4 through that branch until arrows are picked up or bought. The state
  4 the files do hold is the other meaning of the number: the three dynamic classes
  of the previous bullet, none of them armed.
- **Three sprites have no combat frames at all.** Dynamic sprites 14, 15 and 16 —
  the children (0x52, 110 records), the women (0x51, 48) and the lone NPCs (0x53,
  50) — carry zero frames for state 2 and state 5, and sprite 15 also for state 3.
  These classes can never attack, and the code has no filter that would keep a hit
  away from them: an arrow that passes their cell calls the state setter anyway.
  What the original then shows is **unknown** — the frame engine of the clone would
  reset frame and count through its `FRM ERR` path
  ([`game_play.cpp:3737-3748`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3737-L3748)),
  which means such a creature never leaves the state; the frame arrays of the clone
  are fixed size, so nothing crashes and nothing completes. This is an inference
  from the clone's code plus our counts, not a measurement of the game: it needs a
  check in the original (shoot a woman or a child and watch) before it is a fact.

## What stays unknown

- **Whether the monster branch of state 4 releases an arrow.** It is a different
  path from the human shot and its answer does not change #77. The shot branch of
  the monster pass plays the animation and a sound and calls nothing that creates a
  projectile
  ([`game_play.cpp:3607-3619`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3607-L3619)),
  and `FUN_0041bff8` is created nowhere else; the human branch does create the
  arrow. So in the clone a class 0x46/0x4c/0x55 monster plays a shot and does no
  damage — while our files say the same classes hold no bow and no quiver either.
  Which of the two the original does, and from which line, is unverified, so this
  branch is not implemented and its animation is read as the creature's own
  ([state 4](animation.md#state-4--two-meanings-under-one-number)).
- **The result codes' thresholds as the original's.** The `-1` limit of 48, the
  dodge roll against half the agility and the facing table that lets a blow be
  blocked
  ([`game_play.cpp:2686-2692`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2686-L2692))
  are the clone's reconstruction; they drive state choices, so #74 has to confirm
  them against the game before they become rules.
- **Target choice and pursuit** — which enemy of the squad is picked and how far
  the chase runs: the combat branch of the monster decision
  ([`game_play.cpp:3179-3283`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L3179-L3283)),
  the nearest-target helper behind it
  ([`game_play.cpp:2441`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2441))
  and the keep-or-give-up test on a walking attacker
  ([`game_play.cpp:528-560`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L528-L560)).
  #72's material; this document only notes that the monster's own version of that
  test is what enforces the leash rectangle.
- **Whether a villager squad ever turns hostile** — its pass sets the hostility
  flags from a hit
  ([`game_play.cpp:2897-2906`](https://github.com/Marisa-Chan/Slavik/blob/cf5a3320dc93859e3dc51f37ee823fed30f399b6/game_play.cpp#L2897-L2906)),
  but who sets the elder squad's state byte is still the village rules, open since
  [squad state](behavior.md#the-group-state-of-a-roaming-squad-confirmed-2026-11-03).
- **What a hit does to a creature that is already swinging** — the clone restarts
  the stagger of the victim and leaves the attacker's swing running; only the game
  can tell whether the attacker's blow is really not interrupted.

## Implementation boundaries

The recorded findings support hit reactions, explicit health writes, one-shot
death/corpse transitions, melee state sequencing and the human-shot path.
They do not confirm damage thresholds, monster projectiles, target choice,
pursuit, hostile-village rules or empty combat sequences for sprites 14–16.
Do not infer these missing rules from the recovered animation states.
The two uses of [state 4](animation.md#state-4--two-meanings-under-one-number)
must stay separate; a dynamic appearance/transformation is not a human shot.
