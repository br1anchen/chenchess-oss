# Issue 418 web UI revamp acceptance

Date: 2026-08-30

Revision measured: `main` at `96ea12f2`

Every child of #418 was on `main` before this run. This record covers the
epic's own five **Done when** assertions, which are a separate set from the
children and were re-measured here rather than inherited. #430 (`a2187083`)
moved 212 spacing declarations onto the 4px grid and migrated nine
breakpoints, including the move-nav cut's neighbours, so a pass taken before
it proves nothing about what ships.

## What changed about the procedure

#418 D4 settled that deterministic layout assertions would block. The harness
that implemented it (#420, `6fca430b`) was deleted four days later in
`ab5a2397`. The reason is in that commit: its all-pairs `text-rect-overlap`
check reported six invisible failures, because content clipped by a scroll
container still reports a full rect, so a hidden section "overlapped" the one
below it. `docs/architecture/packages.md` records the position that followed —
layout and styling carry no automated gate.

So this run does not restore a gate, and `text-rect-overlap` and CLS are not
re-measured. They are dropped, not deferred. What replaces them asserts only
what one element's own box can prove, which a clip cannot fake:

| Assertion | What it catches |
| --- | --- |
| `zero-width-text` | a text leaf that renders, and renders at 0px wide |
| `page-horizontal-overflow` | the document scrolling sideways |
| `container-horizontal-overflow` | a box marked `data-layout-single-row` or `data-layout-name` whose content is wider than it is |
| `single-row-wrap` | a no-wrap control whose children landed on more than one row |

The sweep is `apps/central-host/scripts/verifySurfaceLayout.ts`, run manually:

```
bun run --cwd apps/coach-app scripts/buildArtifacts.ts
DEPLOYMENT_ENVIRONMENT=staging bun run --cwd apps/central-host build
bun run --cwd apps/central-host verify:surface-layout
```

It starts Storybook and `astro preview` itself if they are not already
answering, enumerates every story from Storybook's `index.json` rather than
from a hand-written allowlist — the #420 harness listed 22 stories by hand and
a new story was invisible to it — and adds the four marketing pages from
`src/siteSurfaces.ts`. Output, including a screenshot of every failing
surface, lands in `.layout-verify/surfaces/`.

Widths: 375, 390 and 1280 for every surface. Any surface that mounts a
`data-layout-single-row` control is additionally measured at 320, 360, 414,
480, 519, 520, 521, 640, 768 and 1024 — D1 mandates an icon-only move nav
below 520px, so the rungs either side of the cut are where a wrap would show.

Screenshots remain evidence, not a gate (D2, D14). Nothing here is wired into
turbo.

### Which surfaces carry evidence

Not every story is a page. A story titled `Pages/…` composes a surface a Player
meets; a component showcase hand-assembles a composition no app surface renders,
so a clean reading there proves nothing and a dirty one indicts nothing. The
sweep measures both and reports both, but only the page tier decides the exit
code. Two `Pages/` titles are excluded from that tier on purpose:

- **`Pages/Review Session/Before`** is the frozen pre-#427 baseline, kept so the
  redesign can be read against what it replaced. It is *supposed* to fail.
- **`Pages/Daily Coaching`** renders the digest cards bare, with no page shell.
  Its own docstring says the connected dashboard renders whole under
  `Pages/Player Dashboard`, which is the surface that carries the evidence.

The SPA roots (`/app/`, `/dashboard/`, `/login/`, `/join/`) are not measured as
URLs: built, they are empty auth shells. Their content is measured through the
`Pages/…` stories that render it with mocked data, which is what D4 asked for.

## Results

153 surfaces, 916 measurements, 0 skipped.

| Tier | Result |
| --- | --- |
| Enforced — `Pages/…` stories and the four marketing pages | **712 ok, 0 failures** |
| Reported — frozen `before` baseline, bare digest showcase, 3 component showcases | 204 noted |

Enforced surfaces, all clean at 375 / 390 / 1280: `pages-player-dashboard`,
`pages-landing`, `pages-auth` (123 measurements), `pages-review-session-shell`
(211), `pages-review-session-conversation-panel`, `pages-coaching-board` (104),
and `/`, `/privacy/`, `/support/`, `/terms/`. The landing additionally measured
clean across the full 320–1280 ladder, because it mounts a move nav.

### One defect found and fixed

The Coaching Board scrolled 4px sideways on every width from 320 to 1024 — 96
failing measurements across all 8 of its stories.

The cause is brand craft meeting a viewport edge. Watercolor frames bleed by
design: the page-title plaque paints its brush `::before` at 383px against a
351px plaque, 16px proud on each side. Against a phone edge that bleed reached
the viewport and became a real scrollport — confirmed with a wheel gesture, not
inferred from `scrollWidth`: `window.scrollX` reached 4.

Fixed in the primitive rather than at the call site, which is what #418 asks
for: `studioStyles.studio` in `packages/ui/src/components/watercolor.styles.ts`
now carries `overflow-x: clip`. The shell owns the viewport, so the shell clips
the bleed. `clip` and not `hidden`, because `hidden` would make the shell a
scroll container and break the `position: sticky` chrome inside it. Verified
after the change with the same wheel gesture: horizontal `scrollX` stays 0,
vertical scrolling still covers the full 469px range.

### What the `noted` tier says

The 27 `zero-width-text` findings are all in `Pages/Review Session/Before`:
crushed coach commentary, which is defect A2 from the original audit. The
shipped `Pages/Review Session/Shell` stories that replaced them are clean at
every width. That contrast is the clearest single piece of evidence that this
epic did what it set out to do.

`watercolor--move-nav`, `watercolor--moment-card` and
`watercolor-position--board-and-evaluation` overflow their showcase canvas
because they render a bare control against the viewport with no page around it.
`watercolor-position--board-and-evaluation` is the story called out on #418 as
a composition no app surface renders.

## Two false-positive classes, and why the assertions are shaped as they are

Both were hit during this run and both are the reason a naive geometry check
cannot be a gate here.

1. **Pseudo-element ink inflates `scrollWidth`.** The move nav reads as 2px
   over its own box at every width purely from a button's brush `::before`.
   `getBoundingClientRect` excludes pseudo-elements and `scrollWidth` does not,
   so `container-content-overflow` compares descendant *rects* against the
   container's rect. Nothing is clipped and no Player sees a break, so nothing
   is reported.
2. **The same ink at page level is not a false positive.** Where the bleed
   reaches the viewport it creates a scrollport the Player can actually drag,
   which is the Coaching Board defect above. `page-horizontal-overflow`
   therefore keeps using `scrollWidth` on `documentElement`.

The distinction is whether anything clips the bleed before it reaches the
viewport. That is also why `text-rect-overlap` cannot be rehabilitated: it
cannot see clipping at all.

## Assertions

| #418 **Done when** | Verdict |
| --- | --- |
| At 375px and 390px no text renders at 0px width and nothing overflows, on every story and every static page | **Met**, with `text-rect-overlap` dropped rather than deferred. 0 failures over the enforced tier. |
| The move nav renders on one row at every width; the Critical Moment Selector header fits a 343px widget with a long opening name | **Met.** No `single-row-wrap` and no `container-content-overflow` on any surface, across 320–1280. The 520px cut is held to `theme/breakpoints.ts` by `tooling/scripts/foundation-breakpoints.test.ts`. |
| `--chen-*` is the only colour system; no shadcn bridge token remains | **Met**, reworded to the post-#422/#477 contract. No bridge token is defined in source; neither `tailwind` nor `shadcn` appears in any manifest; the 22 remaining raw hex literals are all three documented exception classes plus one `#219` issue reference in a comment. |
| The landing leads with the daily digest from real, drift-gated components, and presents the web session as live | **Met.** `generateLandingReviewSession.ts --check` gates the fixture in both `turbo test` and `turbo build`; `landingComposition.test.ts` now asserts the digest is the *first* beat, not merely present; no "coming soon" string exists in the product. |
| A Player who has connected Claude or ChatGPT does not see the installation cards expanded | **Met.** `PlayerDashboard.test.tsx` covers the collapse and its restoration on revocation; in `turbo test` and the push gate. |

## Validation

`turbo run test lint check` over `@chenchess/ui`, `@chenchess/central-host`,
`@chenchess/coach-app` and `@chenchess/scripts`: 18 tasks green, including 902
central-host tests. The layout sweep is not among them and is not meant to be.
