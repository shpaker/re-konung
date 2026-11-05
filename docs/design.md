# Product design principles

How the player-facing experience of re-konung is decided: what each source of
inspiration is authoritative for, and what is deliberately not borrowed. This is
the canonical statement of those principles — other documents link here instead of
repeating them. Structure and component boundaries are [../ARCHITECTURE.md](../ARCHITECTURE.md);
rules of work are [../AGENTS.md](../AGENTS.md).

These are project decisions, not findings about the original, so the confidence
markers of [the reverse-engineering loop](../AGENTS.md) do not apply to them. Where
a principle quotes what the original does, that fact keeps its own marker in the
document that recovered it (for example [k1/creatures.md](k1/creatures.md)).

## The two references

Two sources, two non-overlapping roles:

- **The original «Князь» is the source of the game.** Gameplay rules, content,
  atmosphere and visual identity come from the original files and from behavior
  recovered by the reverse-engineering loop. A rule that has not been confirmed is
  marked unknown in `docs/` and left unimplemented, never replaced by an invented
  one — the rules of [../AGENTS.md](../AGENTS.md), "Reverse engineering", apply in
  full.
- **The modern CRPGs of Larian Studios are the reference for the experience.**
  First of all **Baldur's Gate 3** and **Divinity: Original Sin 2**. They define
  what "good" means for how the game is controlled, how the world is presented and
  how an interface behaves — the bar a design decision is measured against, not a
  thing to reproduce.

The goal follows from both: a modern, responsive interface a player understands at
once, keeping the character of the original. The interface is not a preservation
project, and neither is the engine — the same stance as
[../AGENTS.md](../AGENTS.md), "Priority: modern experience".

## What the Larian reference covers

A design decision about any of these is evaluated against the reference rather than
against what was possible on the hardware of 1999:

| Area | The question the reference answers |
|---|---|
| Controls | Which input does what, and how a player discovers it: which device leads, what each button and modifier means, whether actions can be rebound — patterns weighed against the reference, none of them fixed here |
| Camera | Placement, edge behavior, zoom that stays readable at a pixel-art scale, follow and return to the party |
| World interaction | Hovering a cell or object, click-to-move, reachable-vs-unreachable feedback, cursor state under load |
| Selection | Single and group selection, selection feedback that survives a busy scene, what a right-click means when several things are under the pointer |
| HUD | What is on screen at all, where it lives, what is always visible and what is summoned |
| Inventory | Screens and flows for equipping, moving, stacking and using items, comparison of gear |
| Dialogues | Reading pace, who speaks, choices and their legibility |
| Journals | Quests, known characters and places, anything the player needs to recall later |
| Tooltips | What a tooltip promises, when it appears, how much it holds |
| Feedback | Whether every action says immediately that it was heard: cursor, sound, animation, the marker that something is in progress |

## What is not borrowed

A reference is not a specification, and nothing here is a requirement to implement
everything Baldur's Gate 3 has:

- **No copies.** No interface layouts, icons, sprites, fonts, sounds, texts or any
  other asset of another game — the repository already distributes no game content
  at all ([../AGENTS.md](../AGENTS.md), "Copyrighted content").
- **No foreign presentation.** «Князь» stays 2D pixel art with its isometric
  look; the reference games are 3D and that part of them says nothing to us.
- **No foreign systems.** D&D rules, turn-based combat, classes, dice and the rest
  of another game's design are not mechanics to port. Combat, inventory and
  dialogues come from the original, once they are recovered; until then they stay
  `[ ]` in [../ARCHITECTURE.md](../ARCHITECTURE.md).
- **No invented original behavior.** "A modern CRPG does it this way" never becomes
  evidence that the original did.

## Where the principles apply

- **The player program first: `kn-player` with its frontend `kn-fe-player`.** The
  player-facing game is what these principles govern — the camera, input, HUD,
  inventory, dialogues and journals, built on the shared mechanisms `kn-render`,
  `kn-input` and `kn-ui` (layers 01–03 of [../ARCHITECTURE.md](../ARCHITECTURE.md)).
- **The viewer — `kn-viewer` with `kn-fe-viewer` — stays a development tool.** Its
  read-only status names what it never does: it writes no game file and is not a
  smaller player, so it gains no HUD, inventory or dialogue UI
  ([../ARCHITECTURE.md](../ARCHITECTURE.md)). It is not static: it drives the
  creature simulation, animates creature actions and behavior and toggles overlays
  exactly so a person can see what the data means. It shares the camera, input and
  visual-feedback principles, so that both programs feel like one engine — the
  stepwise, pixel-exact wheel zoom of the viewer is the existing example of that
  shared behavior.

## Applying them

When a new game interface or control is designed, name the reference the decision
was measured against, in the ticket or in the document that records it. Reproducing
a 1999 UI limitation — a control that hides a state, a click that does nothing
visible, an action whose cost is invisible — needs a reason, not inertia; the
original's artificial caps are not reproduced as a matter of policy. Where the
result deliberately differs from the original, the divergence is written into
`docs/` beside the fact about the original, as [../AGENTS.md](../AGENTS.md)
requires, so that it reads as intentional.
