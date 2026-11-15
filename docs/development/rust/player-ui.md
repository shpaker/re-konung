# Player application and UI

## UI showcase (`kn-ui`)

`just ui-showcase` opens a live catalog of the design system: the palette,
spacing and typography tokens, the konung theme on every Feathers control, the
bundled PT Sans faces and the `widgets` helpers. It is the quickest check of
what a change to the theme or the tokens does — the viewer and the player run
the same `KonungUiPlugin`. The recipe runs the `showcase` example of `kn-ui`
on the `dev-runtime` profile (`cargo run --profile dev-runtime
--manifest-path crates/Cargo.toml -p kn-ui --example showcase`), without the
`dynamic` feature: the example links Bevy statically.

## kn-ui: fonts and theme

Two decisions that are not visible from the outside:

- Fonts are not themeable in Feathers 0.20 — `ThemeProps` carries colors
  only, and every control hardcodes `InheritableFont` with the embedded
  FiraSans path. So `KonungUiPlugin` embeds PT Sans (full Cyrillic and Latin,
  `assets/fonts/OFL.txt`), the `widgets` helpers patch `InheritableFont`
  over each control root they build, and the plugin's normalization system
  rewrites every added `InheritableFont` handle to PT Sans — every
  `InheritableFont` is a `PropagateOver<TextFont>` barrier, so composites'
  interiors (the select's trigger and options, the slider's readout) are out
  of reach of root patches and are covered by that system instead.
- The theme is `create_dark_theme()` with the semantic colors remapped onto
  the konung palette; identity beyond colors — radii, row heights, paddings —
  is not fightable inside Feathers and comes from the kn-ui tokens applied in
  our own BSN styles.

## Player application flow (`kn-app`, `kn-fe-player`)

The [product principles](../design.md) govern the UI. Original creation rules
are [research](../../research/k1/new-game.md); this page owns runtime contracts.

The player is an MVU loop over a pure core. `kn-app::AppController` holds the
whole application state (`Screen`, applied settings, the settings draft, the
last session error) and reduces queued typed `Action`s into it, returning
`Effect`s; the frontend (`flow.rs`) wires that into `Update` as one chained
pipeline per frame: `app_flush` (drain actions → effects) → `apply_effects`
(the only executor: session build, apply-and-persist settings, exit) →
`app_flush` again (result actions; the reducer guarantees they emit no
effects, so the chain cannot loop) → `sync_screens` (the view of the new
screen). A session is never built at startup: the menu loads nothing, and
New game opens HeroSelect; Begin emits `StartSession`, executed as
`session::init` with the validated draft as an all-or-nothing build. `session_watch` reports `Succeeded` only
when the simulation exists, the request is consumed and the map tree is in
the world (`CreatureMotion::request` is a queue, not a build). A failure
tears the half-built session down (`session::teardown`, idempotent) and the
menu shows the typed reason until the next attempt. Both long effects —
`StartSession` and `ApplyAndPersist` — execute synchronously on the main
thread inside `apply_effects`: the loading screen is a static line shown
between frames, not a progress indicator, and no async machinery exists
behind it; making the build asynchronous is a candidate future issue, not a
plan. `kn-player` boots
settings-first (typed load errors, a bad file left untouched), then the game
and the hero, and `--new-game` seeds the flow before the first frame. The
idle pipeline is two exclusive world systems over an empty queue — the flush
allocates only when effects exist; no per-frame cost is claimed beyond that
qualitative statement, and a measurement lands when a screen does work worth
measuring.

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
class reset and keyboard focus restoration across view rebuilds. The centered
page has three columns: portrait selection; model, biography and constructor
choices; then attributes and all derived statistics. Cards reserve their full portrait and caption height;
the bundled Cyrillic font is also applied inside the card. Neutral buttons use
light text and outlined raised surfaces; only amber primary buttons use dark
text. The separate footer keeps Back left and Begin right. Escape cancels;
Enter/Space activate the focused control. Scroll position survives edits, and
narrow windows wrap columns inside the scrollable body.

## Allocation UX

**Intentional presentation difference:** compact rows expose both actions and
prices without hiding information in hover text. Buttons contain only minus or
plus; adjacent localized captions name the spending/refund and explain minimum,
maximum or insufficient funds. `Allocation::change` quotes the existing K1 cost
function even when an increase is unaffordable; the frontend never duplicates
price bands or trusts its own balance. Occupation controls the cost bands;
combat and secondary specializations do not change them in the confirmed rules.

All four rows remain together. Their current value shares a line with its name;
changed rows show a quiet highlight and the current occupation's initial value.
Free points appear above the rows and in the fixed Back/Begin footer, together
with the original unspent-point behavior. A class-change warning is always
visible, and an actual reset displays a brief inline result, without a dialog.
No separate reset button is added: selecting the current occupation retains the
existing original reset behavior.

The creation root supplies a Bevy tab group. Attribute and statistic descriptions
can receive keyboard focus; focused controls expose the same original hints as
hovered controls. Popup bounds reserve the footer area. Tests exercise Bevy tab
navigation, focus restoration, visible prices, reset feedback and rejected
unaffordable activation, alongside the conserved-budget tests.

## Reusable attribute view

`kn-fe-player::widgets::attributes` owns the shared four-row presentation.
`AttributeMode::ReadOnly` takes runtime values without allocation controls,
baselines, costs or change observers; `Editable` adds caller-supplied action
scenes and class baselines. The creation screen adapts its validated draft to
this widget, while a future in-game character panel can supply current values
directly. The widget imports neither the creation reducer nor K1 allocation
rules, and its inspection test intentionally uses values above creation limits.
This extracts a reusable view; it does not add an in-game character panel yet.

Manual acceptance and outstanding platform checks: [macOS testing](../macos-testing.md).
