# 008 — One hover gesture per control: the frame never shares the box with the stroke

- **Status**: DONE — executed 2026-09-02, landed in `72e1310c`; measured and strip-checked; residual: one ~40 ms frame on hover-in where the last two glyphs are mid-flip over uncovered paper, inherent to a single-colour label under a moving front
- **Commit**: `778375c9`
- **Issue**: #580 (findings 1, 2, 3, 6; review comments P1 ordering, P1 state matrix, P2 exceptions, P2 docs)
- **Severity**: HIGH
- **Category**: Physicality & origin / Purpose & frequency
- **Estimated scope**: 4 files (`watercolor.styles.ts`, `watercolor.tsx`, `WATERCOLOR.md`, `chenTokens.css`), ~80 changed lines, no new files

This plan is the **atomic shared-control change** for frame and clip state.
Plan 009 (card bloom geometry/timing/strength) depends on it and must not
start until this plan is DONE.

## Problem

Every `WatercolorButton` / `WatercolorButtonLink` hovers by sweeping a
dry-brush slab across itself (`--watercolor-hover-sweep`, 0% → 165% over
420 ms). That sweep is intact (measured on `778375c9`: 0.9% → 24% → 103% →
145% → 165% at ~16/76/156/256/376 ms). What is wrong is the *frame* each
control wears underneath it.

**1. A pale control shows the box and the brush at once.** The resting
four-strip frame on `secondary` / `outline` is only clipped away *behind*
the stroke front:

```ts
/* packages/ui/src/components/watercolor.styles.ts:196-200 — current (inside buttonStyles.base["::before"]) */
      /* The resting fill is cut away exactly where the stroke has already
         painted, so the control is never wearing both at once: left of the
         boundary is brushed ink, right of it is what the button looked like a
         moment ago, and the seam is hidden under the stroke's ragged edge. */
      clipPath: "inset(0 0 0 min(100%, var(--watercolor-hover-sweep, 0%)))",
```

For the whole 420 ms the right of the front is a crisp hairline box and the
left is an ink slab (2× hover strip at 50 ms and 110 ms). The seam does not
hide under the ragged edge, because the frame strips are 0.22 rem hairlines
and the slab edge is tall.

**2. `quiet` paints a box *in* on hover, then wipes it out.**

```ts
/* watercolor.styles.ts:297 — current (inside buttonStyles.quiet) */
    "--watercolor-button-stroke-opacity": { default: "0", ":hover": "1" },
```

The frame fades in over 160 ms (`::before` `opacity 160ms ease`) while the
base clip erases it from the left as the sweep advances: a rectangular
outline flashes in at ~50 ms and is gone by ~200 ms. This is the "outlined
box" on every quiet button and on every Imported Games row —
`apps/central-host/src/daily-coaching/ReviewedGameCard.tsx:59-63` is a
`variant="quiet" hoverWash="bloom"` link inside a framed `WatercolorCard`, so
the row shows its own frame, a second frame flashing inside it, and a splash.

**3. The secondary label bleaches before the ink reaches it.**

```ts
/* watercolor.styles.ts:173-177 — current (buttonStyles.base) */
    transition: {
      default:
        "color 160ms ease, background-color 160ms ease, box-shadow 160ms ease, transform 160ms ease, --watercolor-hover-sweep 420ms cubic-bezier(0.32, 0, 0.24, 1), --watercolor-hover-bloom 260ms cubic-bezier(0.16, 1, 0.3, 1)",
      [reduceMotion]: "none",
    },
```

`color` flips to paper over 160 ms from t=0; the ink reaches the first glyph
(~25% of the width) at ~76 ms and covers the label at ~120 ms. From ~30 ms to
~90 ms the label is pale on a half-opacity slab. The same interval recurs in
reverse on hover-out: `color` returns to navy in 160 ms while the slab takes
420 ms to retreat, so the label goes navy-on-navy.

**4. Bloom controls run the stroke's clip.** The clip above lives in `base`,
so a `hoverWash="bloom"` control (Review Moment card, Imported Game row) still
has its `::before` frame erased from the left on hover. A *current* Review
Moment (`momentCardStyles.current`, stroke opacity `1`) loses its tone frame
at ~80 ms on hover (measured), and a non-current one paints a half-opacity
frame in that the clip then wipes:

```ts
/* watercolor.styles.ts:1730 — current (inside momentCardStyles.card) */
    "--watercolor-button-stroke-opacity": { default: "0", ":hover": "0.5" },
```

All four are the one defect: the control wears two hover gestures — a frame
event and a stroke event — that were authored as one and never were.

**These behaviours are documented as intended** in
`packages/ui/src/components/WATERCOLOR.md:39-72`, `watercolor.styles.ts:196-200,
231-242, 283-288, 362-385`, and `packages/ui/src/theme/chenTokens.css:9-18`.
The product owner has rejected the result (#580). This plan reverses that
decision and updates the durable contract so nothing re-instructs it.

## Target

One rule: **a control's hover is one gesture.** Stroke controls get the slab;
the resting frame leaves as the slab arrives and returns as it retreats, never
sharing the box with it. Bloom controls get the drop; nothing else on them
moves.

### State matrix (secondary and outline, button and link)

| Transition | Frame (`::before` opacity) | Slab (`--watercolor-hover-sweep`) | Label `color` | Inner paper |
|---|---|---|---|---|
| rest → `:hover` | 1 → 0 over **120ms ease**, and still clipped behind the front | 0% → 165% over 420ms `cubic-bezier(0.32, 0, 0.24, 1)` | navy → paper over **60ms ease, delayed 80ms** (80→140ms — the slab crosses the label, 25%→75% of the width, at 76→125ms) | tint → transparent 180ms |
| `:hover` → rest | 0 → 1 over 120ms ease, uncovered as the clip releases | 165% → 0% over **160ms ease** — the exit is the fast phase: the wash element fades its own opacity out in 140ms (`hoverWashStyles.wash`), so a 420ms retreat under it is invisible and only delays the frame | paper → navy over **100ms ease, delayed 20ms** — in step with the wash fade | transparent → tint 180ms |
| rest → `:focus-visible` | same as hover | same as hover | same as hover | same as hover |
| `:focus-visible` → blur | same as hover → rest | same | same | same |
| `:disabled` (button only) | unchanged from today: hover opacity 0, sweep 0%, `disabledCraft` | no travel | no flip | — |
| `prefers-reduced-motion: reduce` | `transition: none` on the control — every value snaps; the frame is simply absent while hovered/focused | snaps to 165% | snaps | snaps |

`--watercolor-button-shadow`, `--watercolor-button-frame-scale` and the hover
branch of `--watercolor-button-fill` on `secondary` act on the `::before`
layer, which is now hidden while hovered; their hover branches are removed as
dead.

### Exceptions that survive the reversal (explicit)

| Control | Frame at rest | Frame on hover | Why |
|---|---|---|---|
| `quiet` button / link (incl. `ReviewedGameCard` link) | none | **none** | the wash is the whole gesture |
| `moveNavStyles.jump` (quiet + `size="icon"`, `watercolor.styles.ts:534-538`) | 0.46 | 0.82 | a *resting* light edge that deepens — an identity, not a hover-only box; composed after `quiet`, unaffected by this plan |
| Review Moment card, not current | none | **none** (was 0.5) | same as quiet |
| Review Moment card, `current` | 1 | 1 | selected-state identity; with the clip scoped away from bloom controls it now survives hover |
| `primary` / `danger` | filled slab | filled slab, second pull over the first (unchanged) | filled controls keep both passes by design |

### Clip ownership

The sweep clip moves out of `base` into a style applied **only when
`hoverWash === "stroke"`**, composed *before* the variant so `primary` /
`danger` keep their existing `clipPath: "none"` override.

## Repo conventions to follow

- Craft is StyleX in `packages/ui/src/components/watercolor.styles.ts`, applied through `craft(...)` in `watercolor.tsx`. Parent-state craft rides on custom properties flipped under the control's own pseudo-classes (file header, `watercolor.styles.ts:18-21`).
- Pseudo-class + at-rule conditions on one property: see `buttonStyles.base.transition` (`:173-177`) — `{ default, [reduceMotion]: "none" }`. Extend that object; do not add a second `transition` key.
- Exemplar for a later-composed override that wins on the same property: `momentCardStyles.current` (`:1751-1754`) after `momentCardStyles.card`.
- Comments in this file explain *why*, in the brand's voice, one short paragraph per decision. Match that; do not leave the old rationale beside the new value.

## Steps

1. **`watercolor.styles.ts` — `buttonStyles.base`**: replace the `transition` object (`:173-177`) with:

   ```ts
       /* The control's own answer — frame, lift, paper — lands in 160ms
          whatever the brush is doing. The label crosses with the ink: it
          turns to paper once the slab has reached the first glyph (about a
          quarter of the way in, ~80ms on the sweep's curve) and is paper by
          the time the slab has crossed it. The exit is the fast phase: the
          wash fades out in 140ms, so the sweep and the label come back with
          it rather than trailing a retreat nobody can see. The stroke's
          curve carries speed through the middle and eases out at the wet
          tip. */
       transition: {
         default:
           "color 100ms ease 20ms, background-color 160ms ease, box-shadow 160ms ease, transform 160ms ease, --watercolor-hover-sweep 160ms ease, --watercolor-hover-bloom 260ms cubic-bezier(0.16, 1, 0.3, 1)",
         ":hover":
           "color 60ms ease 80ms, background-color 160ms ease, box-shadow 160ms ease, transform 160ms ease, --watercolor-hover-sweep 420ms cubic-bezier(0.32, 0, 0.24, 1), --watercolor-hover-bloom 260ms cubic-bezier(0.16, 1, 0.3, 1)",
         ":focus-visible":
           "color 60ms ease 80ms, background-color 160ms ease, box-shadow 160ms ease, transform 160ms ease, --watercolor-hover-sweep 420ms cubic-bezier(0.32, 0, 0.24, 1), --watercolor-hover-bloom 260ms cubic-bezier(0.16, 1, 0.3, 1)",
         [reduceMotion]: "none",
       },
   ```

   (Plan 009 later retimes the `--watercolor-hover-bloom` segment; leave it as-is here.)

2. **`watercolor.styles.ts` — `buttonStyles.base["::before"]`**: delete the four-line comment and the `clipPath` line (`:196-200`). Change the `::before` `transition` (`:193-194`) to
   `"filter 160ms ease, opacity 120ms ease, background-color 180ms ease, transform 180ms cubic-bezier(0.23, 1, 0.32, 1)"`.

3. **`watercolor.styles.ts` — new style after `buttonStyles.base`** (before `primary`):

   ```ts
     /* Stroke controls only (`hoverWash="stroke"`, composed before the
        variant so a filled control's `clipPath: none` still wins). Where the
        slab has landed the resting frame is cut away, so even while it is
        fading out it is never underneath brushed ink. A bloom control never
        takes this: its frame is its identity and the drop lands over it. */
     strokeClip: {
       "::before": {
         clipPath: "inset(0 0 0 min(100%, var(--watercolor-hover-sweep, 0%)))",
       },
     },
   ```

4. **`watercolor.styles.ts` — `buttonStyles.primary`**: replace the comment above `"::before": { clipPath: "none" }` (`:231-235`) with
   `/* A filled control wears both passes at once: the clip belongs to the pale stroke variants, whose frame leaves as the slab lands. */`. Keep the `clipPath: "none"` line. Same edit on `danger` (`:320`), whose comment already points at primary — leave that comment.

5. **`watercolor.styles.ts` — `buttonStyles.secondary`** (`:237-281`): resulting block —

   ```ts
     secondary: {
       /* Paper button: its identity is the frame. On hover the brush is
          loaded over it and the frame leaves as the slab lands — one gesture,
          so the control is never wearing the box and the brush together. The
          label crosses to paper as the ink reaches it (timing in `base`), and
          the frame comes back only where the retreating front has uncovered
          it. */
       "--watercolor-hover-ink":
         "color-mix(in srgb, var(--color-text-primary) 74%, var(--color-ink-deep))",
       "--watercolor-hover-strength": "1",
       "--watercolor-brush-weight": "0.22rem",
       "--watercolor-button-inner-inset": "0.08rem 0.12rem",
       "--watercolor-button-fill":
         "linear-gradient(color-mix(in srgb, var(--color-text-primary) 84%, black), color-mix(in srgb, var(--color-text-primary) 84%, black))",
       "--watercolor-button-stroke-opacity": {
         default: "1",
         ":hover": "0",
         ":focus-visible": "0",
       },
       /* The paper clears as the stroke lands: a tint left under full-strength
          ink is what turned the control grey rather than inked. */
       "--watercolor-button-inner": {
         default: "color-mix(in srgb, var(--color-paper-raised) 62%, transparent)",
         ":hover": "transparent",
         ":focus-visible": "transparent",
       },
       color: {
         default: "var(--color-text-primary)",
         ":hover": "var(--color-background-surface)",
         ":focus-visible": "var(--color-background-surface)",
       },
       "::before": {
         inset: "-0.04rem -0.12rem",
         mask: "var(--watercolor-brush-frame)",
         maskSize: "var(--watercolor-brush-sizes)",
       },
       "::after": {
         borderRadius: "0.24rem 0.3rem 0.22rem 0.28rem",
       },
     },
   ```

   i.e. remove the `:hover` branch of `--watercolor-button-fill`, and remove `--watercolor-button-shadow` and `--watercolor-button-frame-scale` entirely (base supplies `none` / `scale(1)`).

6. **`watercolor.styles.ts` — `buttonStyles.quiet`** (`:283-307`): replace the leading comment and the stroke-opacity line:

   ```ts
     quiet: {
       /* Quiet owns no edge, at rest or on hover: the wash travelling under
          the label at a fifth of the ink is the whole gesture, and the label
          stays the darkest thing on it the whole way across. The frame mask
          and fill stay declared for the controls that compose a resting edge
          on top of quiet (`moveNavStyles.jump`). */
       ...
       "--watercolor-button-stroke-opacity": "0",
   ```

   Everything else in `quiet` stays.

7. **`watercolor.styles.ts` — `momentCardStyles.card`** (`:1730`): `"--watercolor-button-stroke-opacity": "0",` with the comment `/* A moment's frame is its selected state (`current`), never a hover. */`. Leave `momentCardStyles.current` (`:1751-1754`) as is.

8. **`watercolor.tsx` — `WatercolorButton`** (`:178-192`): insert `hoverWash === "stroke" && buttonStyles.strokeClip,` immediately after `buttonStyles.base,` in the `craft(...)` call. Same insertion in `WatercolorButtonLink` (`:240-254`).

9. **`watercolor.styles.ts` — `hoverWashStyles` header comment** (`:362-385`): replace the second and third paragraphs ("The stroke repaints whatever carries…" through "…whichever part of the control means something.") with:

   ```
    * A filled button has its identity in its fill, so the wash lays a deeper
    * pull of that same ink over it — a second pass of the brush, not a
    * different colour. A pale button (secondary, outline) has its identity in
    * its frame, so the frame leaves as the slab lands (`buttonStyles.secondary`)
    * and the control is never wearing both; `buttonStyles.strokeClip` cuts the
    * frame away under the ink while it fades. Quiet has no edge at all and
    * takes the travelling stroke at a fifth of the ink. One gesture per control.
   ```

10. **`chenTokens.css:9-18`**: change "and the `clip-path: inset(...)` that cuts the resting `::before` fill away behind it" to "and, on pale stroke controls, the `clip-path: inset(...)` that cuts the resting frame away under it while it fades".

11. **`WATERCOLOR.md` — `## Hover`**: replace the paragraph beginning "On a **pale** control the resting frame is clipped away…" with:

    > On a **pale** control (secondary, outline) the frame is the control's identity, and it leaves as the slab lands: it fades over 120 ms and is clipped away under the ink where the stroke has already painted (`buttonStyles.strokeClip`), so the control is never wearing the box and the brush at once. The label crosses to paper only once the ink has reached it, and back to navy only once the retreating front has uncovered it. A **filled** control keeps both — the brush artwork carries partial alpha, so a stroke that replaced the slab would read _lighter_ than the resting one and the button would look rubbed out. It wears the second pull over the first, a shade deeper.

    and replace the paragraph beginning "The stroke repaints whatever carries that control's identity…" with:

    > The stroke repaints whatever carries that control's identity, and nothing else on the control moves. Filled buttons (primary, danger) keep their exact colour — `danger` stays unmistakably destructive at the moment of commitment. Quiet owns no edge at rest or on hover: the same stroke travels under its label at a fifth of the ink and the label stays the darkest thing on it. Two quiet controls keep a *resting* edge on purpose — the move-nav jump buttons (0.46, deepening to 0.82 on hover) and a `current` Review Moment (its selected state) — and neither is a hover decoration. Card-sized controls (`hoverWash="bloom"`) never take the stroke's clip: their frame is their identity and the drop lands over it.

## Boundaries

- Do NOT touch `hoverWashStyles.bloom`, `--watercolor-hover-bloom`, or its transition segment — that is Plan 009.
- Do NOT change `moveNavStyles.jump`, `momentCardStyles.current`, `primary`, `danger`, or any component markup beyond the two `craft(...)` insertions in step 8.
- Do NOT add tokens, dependencies, or new files.
- Do NOT regenerate `packages/ui/src/theme/generated/*`.
- If any excerpt above does not match the code at `778375c9` (drift), STOP and report which step.

## Verification

- **Mechanical** (each must exit 0):
  - `bun run --cwd packages/ui typecheck`
  - `bun run --cwd packages/ui lint`
  - `bun run --cwd packages/ui format` — if it fails, run `oxfmt` **from inside `packages/ui`** on the changed files (never on a path from the root) and re-check.
  - `bun run --cwd packages/ui test`
  - `bun run --cwd apps/central-host typecheck`
- **Measured** (Storybook: `bun run --cwd apps/central-host storybook`, then Playwright from the repo root against `http://127.0.0.1:6006/iframe.html?id=watercolor-controls--buttons&viewMode=story`; sample `getComputedStyle` on the control and on `getComputedStyle(control, "::before")` while hovered):
  - Secondary (index 1): `::before` `opacity` reads `< 0.05` by 130 ms after hover and `1` by 160 ms after un-hover; `--watercolor-hover-sweep` still reaches `165%` at ~420 ms on the way in.
  - Secondary: `color` is still `var(--color-text-primary)`'s RGB at 60 ms after hover, and is paper's RGB by 150 ms; on un-hover it is navy by 140 ms, and `--watercolor-hover-sweep` reads `0%` by 200 ms.
  - Quiet (index 2): `::before` `opacity` reads `0` at every sample, hovered or not.
  - `watercolor--moment-card` story, the `current` card: `::before` `clip-path` reads `none` while hovered and its `opacity` stays `1`.
  - `watercolor-controls--buttons`, primary (index 0): `::before` `clip-path` reads `none` while hovered.
- **Feel check** (2× DPR strip at 0/50/110/180/300/600 ms, hover-in then hover-out):
  - Hover-out on secondary: the slab fades, the label is navy by the time it has, and the frame is whole within ~160 ms — no interval with an empty paper box.
  - Secondary and outline, `md` and `sm`, `block`, and a `WatercolorButtonLink` (landing "Join the private beta" via `astro preview` on `apps/central-host`): at no frame is a hairline box visible to the right of the slab front. The label is legible in every frame (dark on paper, or paper on ink — never pale on paper or navy on navy). Widths differ; confirm the ink reaches the first glyph before the label has turned.
  - Quiet: no rectangle at any frame; only the light slab travels.
  - Imported Games row (`ImportedGamesPage` in the running app, or `watercolor--moment-card` as the same composition): the card's own outer frame stays; nothing rectangular appears inside it on hover.
  - Keyboard: Tab onto a secondary button — same choreography as hover; Tab away — same as hover-out.
  - DevTools Rendering → emulate `prefers-reduced-motion: reduce`: hover snaps to the inked slab with no frame and no travel; label readable.
- **Done when**: all mechanical checks pass, the five measured assertions hold, and the feel-check strips show one gesture per control. Update this plan's Status and the row in `docs/plans/README.md`.

## Appendix — probe

```js
// run from the repo root: node probe.mjs 1   (index into [data-watercolor-control="button"])
import { chromium } from "playwright"
const browser = await chromium.launch({ channel: "chrome", headless: false })
const page = await browser.newPage({ viewport: { width: 1200, height: 900 }, deviceScaleFactor: 2 })
await page.goto("http://127.0.0.1:6006/iframe.html?id=watercolor-controls--buttons&viewMode=story", { waitUntil: "networkidle" })
await page.waitForTimeout(800)
const i = Number(process.argv[2])
const read = () => page.evaluate((i) => {
  const el = document.querySelectorAll('[data-watercolor-control="button"]')[i]
  const cs = getComputedStyle(el), b = getComputedStyle(el, "::before")
  return { sweep: cs.getPropertyValue("--watercolor-hover-sweep").trim(), color: cs.color, frameOpacity: b.opacity, frameClip: b.clipPath }
}, i)
const el = page.locator('[data-watercolor-control="button"]').nth(i)
await el.hover()
for (const t of [30, 80, 130, 180, 300, 440]) { await page.waitForTimeout(t - (globalThis.p ?? 0)); globalThis.p = t; console.log("in", t, await read()) }
await page.mouse.move(2, 2); globalThis.p = 0
for (const t of [30, 130, 250, 440, 600]) { await page.waitForTimeout(t - globalThis.p); globalThis.p = t; console.log("out", t, await read()) }
await browser.close()
```
