# 011 — Keep the landing's ink scrub under reduced motion

- **Status**: DONE — landed in `72e1310c`
- **Commit**: `778375c9`
- **Issue**: #580 (finding 8; reporter verified `prefers-reduced-motion: reduce` is on in the browser that shows the fade)
- **Severity**: MEDIUM
- **Category**: Accessibility
- **Estimated scope**: 1 file (`apps/central-host/src/landingPage.styles.ts`), ~20 deleted lines, 1 comment rewritten

## Problem

The landing paints each showcase visual in with a 25-frame ink-spread sprite,
scrubbed by the reader's own scrolling. Under `prefers-reduced-motion: reduce`
that scrub is swapped for an opacity cross-fade:

```ts
/* apps/central-host/src/landingPage.styles.ts:193-197 — current */
/* Reduced motion never moves the ink — the wash cross-fades in instead. */
const washFade = stylex.keyframes({
  from: { opacity: 0 },
  to: { opacity: 1 },
})
```

```ts
/* landingPage.styles.ts:322-328, 341-354, 365-372 — current (washStyles.inkReveal) */
    maskImage: {
      default: null,
      [supportsScrollTimeline]: {
        default: "var(--chen-ink-sprite)",
        [reduceMotion]: "none",
      },
    },
    ...
    animationName: {
      default: null,
      [supportsScrollTimeline]: {
        default: inkSequenceRun,
        [reduceMotion]: washFade,
      },
    },
    animationTimingFunction: {
      default: null,
      [supportsScrollTimeline]: {
        default: "steps(24)",
        [reduceMotion]: "linear",
      },
    },
    ...
    animationRange: {
      default: null,
      [supportsScrollTimeline]: {
        default: "entry 0% entry 100%",
        /* The cross-fade stays short. Held across a whole viewport it would
           leave the card half-transparent for a screen of scrolling, which is
           the opposite of what asking for less motion is asking for. */
        [reduceMotion]: "entry 0% entry 30%",
      },
    },
```

Measured on staging (`778375c9` build, Chrome 152, reduce emulated): every
reveal sits at `mask-position: 100% 0px` and its `opacity` climbs 0 → 0.52 →
1 across `entry 0%–30%`. With no preference, the same page scrubs the sprite
`0%` → `33%` → `92%` across 900 px of scroll (Chrome 152 and Comet 151).

Reduced motion asks for fewer and gentler animations, not the removal of
comprehension aids; what it removes is autonomous movement and position
change. The ink scrub is neither: it is a mask, it translates nothing, and it
advances only as far as the reader scrolls — stop scrolling and it stops. The
cross-fade it was swapped for is *also* scroll-driven, so the swap removed the
brand's transition without removing any motion. This plan keeps the scrub
under reduced motion. (The alternative — leave the fade and treat the
reporter's browser setting as the explanation — is a valid no-change; the
product owner chose to bring the scrub back.)

What stays dropped under reduced motion elsewhere is right and is **not**
touched by this plan: the 420 ms hover sweep on every watercolor control
(`buttonStyles.base.transition` → `none`), the bloom's travel, the knight's hop
(`knightQuiet` breath instead). Those are autonomous motion.

## Target

`washStyles.inkReveal` behaves identically with and without
`prefers-reduced-motion: reduce`: sprite mask, `steps(24)`, the beat's
`entry 0% entry 100%` (or the hero's `entry 0% entry 30%` via `selfArrival`).
`washFade` no longer exists.

## Repo conventions to follow

- Conditional StyleX values nest `[reduceMotion]` inside `[supportsScrollTimeline]`; removing the reduce branch leaves a one-key object, which this file writes as `{ default: null, [supportsScrollTimeline]: value }` — exemplar `animationFillMode` (`:340`) and `animationTimeline` (`:381-384`).
- Comments explain the *why* in one short paragraph; do not leave a rationale for a branch that no longer exists.

## Steps

1. **`landingPage.styles.ts:193-197`**: delete the `washFade` keyframes and the comment above it.
2. **`:305-311`** (the `washStyles` header comment): replace the final clause "engines without scroll timelines never mask (the visual is simply there), and reduced motion cross-fades instead." with "engines without scroll timelines never mask (the visual is simply there). Reduced motion keeps the scrub: it is a mask the reader drives with their own scrolling — nothing translates and nothing plays on its own — so it is comprehension, not motion, and the cross-fade it was once swapped for was scroll-driven too."
3. **`:317-328`** `maskImage`: delete the four-line comment beginning "`none` rather than `null` under reduced motion" and reduce the value to
   ```ts
       maskImage: { default: null, [supportsScrollTimeline]: "var(--chen-ink-sprite)" },
   ```
4. **`:341-347`** `animationName` →
   ```ts
       animationName: { default: null, [supportsScrollTimeline]: inkSequenceRun },
   ```
5. **`:348-354`** `animationTimingFunction` →
   ```ts
       animationTimingFunction: { default: null, [supportsScrollTimeline]: "steps(24)" },
   ```
6. **`:365-372`** `animationRange` → delete the inner comment about the cross-fade and reduce to
   ```ts
       animationRange: { default: null, [supportsScrollTimeline]: "entry 0% entry 100%" },
   ```
7. If `reduceMotion` (`:170`) is now unreferenced in the file, delete its declaration; otherwise leave it.

## Boundaries

- Do NOT touch `selfArrival`, `showcaseSectionStyles.section`, `inkSequenceRun`, the sprite, `maskSize`, or `LandingInkReveal.tsx`.
- Do NOT change reduced-motion handling anywhere else (controls, knight, wash panels).
- If any excerpt does not match the code at `778375c9`, STOP and report.

## Verification

- **Mechanical**: `bun run --cwd apps/central-host typecheck`, `lint`, `format` (fix from inside the package) exit 0; `bun run --cwd apps/central-host build` exits 0 (runs `verify:public-build`).
- **Measured** (Playwright, `astro preview` of `apps/central-host/dist` at `http://127.0.0.1:4174/`, viewport 1440×900, context `reducedMotion: "reduce"`; for each element whose computed `animation-timeline` is not `auto`/`none`, read `mask-position`, `opacity`, `getAnimations()[0].playState`):
  - At scroll 0: board beat `mask-position: 0% 0px`, `opacity: 1`, `running`; chat beat the same.
  - At scroll 400: board `33.3333% 0px`; at 900: `91.6667% 0px`; at 1400: `100% 0px`. `opacity` reads `1` at every sample.
  - The same run with `reducedMotion: "no-preference"` produces identical numbers.
  - `document.styleSheets` contains no keyframes animating `opacity` on the reveal (grep the built CSS for the `washFade` hash is enough: the keyframe name that appeared next to `mask-position` keyframes at `778375c9` is gone).
- **Feel check**: with macOS Reduce Motion on (or DevTools Rendering → reduce), scroll the landing on staging or the preview: the board and chat card paint in as ink as they rise, and hold still the moment scrolling stops. Nothing fades.
- **Done when**: the four measured checks pass in both preference states and the feel check reads as ink. Update Status here and in `docs/plans/README.md`.
