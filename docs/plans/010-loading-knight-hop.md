# 010 — Let the loading knight hop again

- **Status**: DONE — landed in `72e1310c`
- **Commit**: `778375c9`
- **Issue**: #580 (finding 7; review comment P2 acceptance)
- **Severity**: HIGH
- **Category**: Missed opportunity (a rare, allowed delight moment rendered still)
- **Estimated scope**: 1 changed file, 1 new test file (~30 lines), optional Storybook story

## Problem

Both dashboard tabs wait behind `DashboardLoadingCard`, whose knight does not
move:

```tsx
/* apps/central-host/src/daily-coaching/DashboardLoadingCard.tsx:13-22 — current */
export function DashboardLoadingCard() {
  return (
    <WatercolorNotice
      glyph={<ChenKnightMark size="sm" thinking="trace" />}
      heading="Loading…"
      role="status"
      xstyle={styles.card}
    />
  )
}
```

`thinking="trace"` selects `knightMarkStyles.knightStill`, which is explicit
about it:

```ts
/* packages/ui/src/components/ChenKnightMark.styles.ts:87-95 — current */
  /** Trace holds the knight still; only the ring moves. */
  knightStill: {
    transformBox: "fill-box",
    transformOrigin: "50% 92%",
    animationName: { default: null, [reduceMotion]: knightQuiet },
    ...
```

The Luxo hop (`knightHop`, `:11-48`, selected by `thinking="hop"` or
`thinking`) is referenced by no application code — only
`packages/ui/stories/knight-mark.stories.tsx`. The existing
`ChenKnightMark.test.tsx` proves both modes render; nothing protects the
product call site that chose the still one.

## Target

```tsx
/* DashboardLoadingCard.tsx — target */
      glyph={<ChenKnightMark size="sm" thinking="hop" />}
```

The knight hops (2.8 s loop, `knightHop`); the painted ring stays still (no
trace mask). Under `prefers-reduced-motion: reduce` the knight breathes
(`knightQuiet`, `:50-54`) — already wired by `knightAnimated`. The notice
stays `role="status"` with text `Loading…`.

`DailyCoachingDigest.tsx:172` (the *preparing* digest status) also uses
`thinking="trace"`; it is a different moment (a ring being drawn while the
Coach works) and is **out of scope**.

## Repo conventions to follow

- Tests: vitest + `@testing-library/react`, `// @vitest-environment jsdom` header, `afterEach(cleanup)`, role-based queries — exemplar `apps/central-host/src/daily-coaching/DailyCoachingDigest.test.tsx:1-21` ("says only that the digest is loading").
- The mark exposes `data-thinking` on its `<svg>` — exemplar assertion `packages/ui/src/components/ChenKnightMark.test.tsx` (`mark.getAttribute("data-thinking")`).

## Steps

1. **`apps/central-host/src/daily-coaching/DashboardLoadingCard.tsx:16`**: `thinking="trace"` → `thinking="hop"`. In the doc comment (`:9`) change "The knight is doing the waiting, which is the part worth showing" to "The knight is doing the waiting — hopping, which is the part worth showing".

2. **New `apps/central-host/src/daily-coaching/DashboardLoadingCard.test.tsx`**:

   ```tsx
   // @vitest-environment jsdom

   import { cleanup, render, screen } from "@testing-library/react"
   import { afterEach, expect, test } from "vitest"

   import { DashboardLoadingCard } from "./DashboardLoadingCard"

   afterEach(cleanup)

   test("waits behind a hopping knight and says only that it is loading", () => {
     render(<DashboardLoadingCard />)

     const status = screen.getByRole("status")
     expect(status.textContent).toBe("Loading…")
     const mark = status.querySelector("svg[data-thinking]")
     expect(mark?.getAttribute("data-thinking")).toBe("hop")
     expect(mark?.getAttribute("data-size")).toBe("sm")
     // Hop moves the knight layer only; the painted ring is never masked.
     expect(mark?.querySelector("mask")).toBeNull()
   })
   ```

3. **Storybook**: if no story in `apps/central-host/stories/daily-coaching.stories.tsx` renders `DashboardLoadingCard`, add one named `Loading` that renders `<DashboardLoadingCard />` inside the file's existing page/theme decorator. Do not add a new stories file.

## Boundaries

- Do NOT change `ChenKnightMark`, its styles, or `DailyCoachingDigest.tsx:172`.
- Do NOT alter the notice's role, heading, or `styles.card`.
- If `DashboardLoadingCard.tsx` no longer matches the excerpt, STOP.

## Verification

- **Mechanical**: `bun run --cwd apps/central-host test -- DashboardLoadingCard` passes (the new test red on `778375c9` with `"trace"`, green after step 1); `bun run --cwd apps/central-host typecheck` and `lint` exit 0.
- **Feel check** (Storybook: `bun run --cwd apps/central-host storybook`, story from step 3 — or `brand-knight-mark--thinking-hop` for the motion alone):
  - The knight leans, squashes, launches, lands and settles every 2.8 s; the ring does not move.
  - Apex clipping: the hop rises to `translate(-2%, -26%)` of the knight's box. In DevTools, pause the animation at ~48% (Animations panel, or `document.querySelector('[data-layer="knight"]').getAnimations()[0].currentTime = 1344`) and confirm the ears are not cut by the notice or the card — `knightMarkStyles.mark` is `overflow: visible` and `noticeStyles.body` sets no overflow, so nothing should clip; if something does, report it, do not restyle.
  - DevTools Rendering → `prefers-reduced-motion: reduce`: the knight rocks −4° and breathes to 0.72 opacity; no travel.
  - In the real dashboard (`bun run --cwd apps/central-host dev`, signed-in Daily Coaching tab, throttle the network to see it): the loading card hops until the digest's ink wash lands over it.
- **Done when**: the test is green, the feel check shows an unclipped hop, and reduced motion breathes. Update Status here and in `docs/plans/README.md`.
