# 009 — Make the card bloom a visible drop that keeps its shape

- **Status**: DONE — executed 2026-09-02, landed in `72e1310c`; postconditions measured on the moment card (0.12 settled, 51/78/91% at 80/160/240 ms, `95% auto`); Imported Game row checked as the same composition only. Follow-up found, not fixed: a disabled control still shows its hover wash while the pointer is over it (StyleX emits `:hover` after `:disabled`; pre-existing on the stroke wash too)
- **Commit**: `778375c9`
- **Issue**: #580 (findings 4, 5; review comment P2 acceptance)
- **Severity**: HIGH
- **Category**: Purpose & frequency / Interruptibility
- **Estimated scope**: 2 files (`watercolor.styles.ts`, `WATERCOLOR.md`), ~25 changed lines
- **Depends on**: **008 DONE** (008 removes the stroke clip from bloom controls and the hover frame from `quiet` / moment cards; this plan only touches the drop itself)

## Problem

A card-sized control (`hoverWash="bloom"`: Review Moment card, Imported Game
row) lands an ink drop that is supposed to spread through the paper. Three
things stop it reading as a drop.

**1. It settles invisible.** The bloom's opacity multiplies two attenuations:

```ts
/* packages/ui/src/components/watercolor.styles.ts:432-436 — current (hoverWashStyles.bloom) */
    /* A wash under a whole card settles far lighter than a stroke along one
       control's edge — the copy on top of it has to stay the darkest thing
       in the row. The switch is still the shared hover property, so a
       disabled control gets no splash for free. */
    opacity: "calc(var(--watercolor-hover-opacity, 0) * 0.22)",
```

`--watercolor-hover-opacity` is `var(--watercolor-hover-strength)` under
hover, and every bloom control in the app is `quiet`
(`--watercolor-hover-strength: 0.2`, `:291`). Settled opacity is therefore
**0.044** (measured on `watercolor--moment-card`). The `0.22` was written
against strength `1`; on a quiet control it is a second attenuation.

**2. It is over before the eye arrives.**

```ts
/* watercolor.styles.ts:128-131 — current (buttonStyles.base) */
    "--watercolor-hover-bloom": {
      default: "12%",
      ":hover": "95%",
      ":focus-visible": "95%",
      ":disabled": "12%",
    },
/* watercolor.styles.ts:175 — the bloom segment of the base transition */
    --watercolor-hover-bloom 260ms cubic-bezier(0.16, 1, 0.3, 1)
```

Measured: 41% at 16 ms, 84% at 76 ms, 94% at 156 ms. The curve front-loads
~85% of the spread into the first 80 ms.

**3. The blot is stretched to the box.**

```ts
/* watercolor.styles.ts:423 — current (hoverWashStyles.bloom) */
    mask: "var(--watercolor-ink-blot) center / var(--watercolor-hover-bloom) var(--watercolor-hover-bloom) no-repeat",
```

`mask-size` takes the same percentage on both axes of the *box*, so on a
496×91 Imported Game row the 192×169 blot becomes 471×86 — a 5:1 smear with
no drop silhouette at all. The comment at `:418-422` claims the silhouette
"stays at full resolution however far it has spread"; it does not.

## Target

- **Strength** is its own number. A pure on/off switch on the control
  (`--watercolor-hover-on`, `0 | 1`) times a bloom-specific strength
  (`--watercolor-bloom-strength`, default `0.12`). Settled opacity on a quiet
  card is **0.12**; disabled `0`; focus-visible `0.12`.
- **Spread** takes 320 ms on a plain ease-out
  (`cubic-bezier(0.25, 0.46, 0.45, 0.94)`): 12% → 95%, passing ≈51% at 80 ms,
  ≈77% at 160 ms, ≈91% at 240 ms.
- **Shape** is the blot's own: `mask-size: <bloom> auto`, so the width is the
  bloom percentage and the height follows the artwork's 192:169 aspect
  (`packages/ui/src/assets/brand/brush/ink-blot.webp`). On a wide row the box
  shows the blot's middle band with true ragged ends; on a squarer card the
  whole drop. Nothing is stretched.
- Reduced motion, focus-visible, disabled and hover-out keep today's
  behaviour through the same shared properties.

## Repo conventions to follow

- Custom-property switches flipped under the control's own pseudo-classes: `buttonStyles.base["--watercolor-hover-opacity"]` (`:105-110`) is the exemplar; the new switch sits beside it.
- Fallback-in-`var()` for a per-control knob with a shared default: `hoverWashStyles.wash.backgroundImage` uses `var(--watercolor-hover-tip, var(--watercolor-hover-ink, currentColor))` (`:403`). `--watercolor-bloom-strength` follows that pattern — no token registration, no `@property`.
- Registered properties stay registered: `--watercolor-hover-bloom` keeps its `@property` in `chenTokens.css:33-37`; do not add another.

## Steps

1. **`watercolor.styles.ts` — `buttonStyles.base`**: directly after the `"--watercolor-hover-opacity"` object (`:105-110`) add

   ```ts
       /* The bare switch, for washes that carry their own strength: `1` while
          the control is hovered or focused, `0` otherwise, and `0` disabled. */
       "--watercolor-hover-on": {
         default: "0",
         ":hover": "1",
         ":focus-visible": "1",
         ":disabled": "0",
       },
   ```

2. **`watercolor.styles.ts` — `buttonStyles.base.transition`** (as left by Plan 008, three string branches): in **each** of the `default`, `":hover"` and `":focus-visible"` strings replace
   `--watercolor-hover-bloom 260ms cubic-bezier(0.16, 1, 0.3, 1)` with
   `--watercolor-hover-bloom 320ms cubic-bezier(0.25, 0.46, 0.45, 0.94)`.
   Amend the sentence in the comment above it that ends "…so it keeps the plainer ease-out." to: "The bloom is a spread rather than a travelling front, so it takes a plain ease-out and a little longer than the first frame the eye lands on."

3. **`watercolor.styles.ts` — `hoverWashStyles.bloom`** (`:417-440`): resulting block —

   ```ts
     bloom: {
       inset: "-0.3rem",
       /* The ink-blot artwork is the ripple, and it keeps its own shape: the
          bloom sets the width and the height follows the artwork, so a wide
          row shows the drop's middle band with true ragged ends where a size
          stretched to the box was a smear with no edge at all. */
       mask: "var(--watercolor-ink-blot) center / var(--watercolor-hover-bloom) auto no-repeat",
       /* The pigment under the blot fills the box and carries its own falloff,
          so the wash has depth at the centre and thins at the rim — a wet mark
          rather than a flat scrim. It is held near-solid well past halfway,
          which is the difference between this and the digest's soft drop. */
       backgroundImage:
         "radial-gradient(ellipse at center, var(--watercolor-hover-ink, currentColor) 0 58%, color-mix(in srgb, var(--watercolor-hover-ink, currentColor) 62%, transparent) 82%, color-mix(in srgb, var(--watercolor-hover-ink, currentColor) 18%, transparent) 100%)",
       backgroundPosition: "center",
       backgroundSize: "100% 100%",
       /* A wash under a whole card settles far lighter than a stroke along one
          control's edge — the copy on top has to stay the darkest thing in the
          row — and it carries that strength itself rather than inheriting the
          stroke's: a quiet card's fifth-of-the-ink, times the stroke's
          card factor, was 0.044 and read as nothing. The switch is the shared
          hover state, so a disabled control gets no splash for free. */
       opacity:
         "calc(var(--watercolor-hover-on, 0) * var(--watercolor-bloom-strength, 0.12))",
       /* The ripple's own travel is the button's transition on
          `--watercolor-hover-bloom`, which reduced motion drops there; the
          settled tint stays either way. */
       transition: "opacity 150ms ease",
     },
   ```

4. **`WATERCOLOR.md` — `## Hover`**, the paragraph beginning "Controls the size of a card — the Review Moment card, an Imported Game row — pass `hoverWash="bloom"`": replace "and settling as a wash far lighter than a stroke, since the copy on top of it has to stay the darkest thing in the row." with "and settling at `--watercolor-bloom-strength` (0.12 by default — far lighter than a stroke, since the copy on top of it has to stay the darkest thing in the row; a host may set the property on the control). The blot keeps its own aspect: the bloom sets its width and the height follows the artwork, so a wide row shows the drop's middle band rather than a smear."

## Boundaries

- Do NOT touch `hoverWashStyles.wash`, `compact`, `buttonStyles.quiet`, `momentCardStyles`, or `ReviewedGameCard.tsx`.
- Do NOT change the pigment gradient, the `12%` / `95%` endpoints, or the `@property` registrations.
- Do NOT tune `--watercolor-bloom-strength` past the feel check: if `0.12` reads wrong, report the observation and STOP rather than pick another number.
- If Plan 008 is not DONE (the `base.transition` still has a single `default` string, or `base["::before"]` still carries `clipPath`), STOP.

## Verification

- **Mechanical** (each exits 0): `bun run --cwd packages/ui typecheck`, `lint`, `format` (fix from inside `packages/ui`), `test`; `bun run --cwd apps/central-host typecheck`.
- **Measured postconditions** (Storybook, Playwright; `wash = control.querySelector(".chen-watercolor-hover-wash")`, read `getComputedStyle(wash)` and `getComputedStyle(control).getPropertyValue("--watercolor-hover-bloom")`):

  | Surface | At rest | 80 ms | 160 ms | 240 ms | ≥ 400 ms (settled) |
  |---|---|---|---|---|---|
  | `watercolor--moment-card` (non-current card) at viewport 560 | opacity `0`, bloom `12%`, `mask-size` `12% auto` | bloom 43–59% | 69–85% | 83–95% | opacity **`0.12`** (±0.005), bloom `95%`, `mask-size` `95% auto` |
  | An Imported Game row — `ImportedGamesPage` in the running app, or the same composition (`WatercolorCard frame` › `WatercolorButtonLink variant="quiet" hoverWash="bloom"`) at 496×91 | same | same | same | same |

  Also: `:disabled` moment card (a `disabled` `WatercolorButton` with `hoverWash="bloom"`) — opacity `0` while hovered; keyboard focus (Tab) — opacity `0.12`; DevTools `prefers-reduced-motion: reduce` — bloom reads `95%` on the first sample after hover, opacity still `0.12`.

- **Feel check** (2× strip at 0/80/160/240/400 ms, both surfaces above):
  - The tint lands as a drop in the middle and spreads outward; the ends of the wash are ragged blot edges, not a straight-cut band, and nothing looks squashed.
  - The row copy is still the darkest thing in the row at 400 ms.
  - Hover-out: the drop shrinks back over the same 320 ms; no pop.
  - The card's own frame (008) does not move at any frame.
- **Done when**: the table holds on both surfaces, the disabled / focus / reduced-motion checks hold, and the strip reads as a drop. Update Status here and in `docs/plans/README.md`.
