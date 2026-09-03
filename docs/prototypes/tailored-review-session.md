# Tailored Review Session prototype

The prototype code (`apps/central-host/src/preview/tailored-review-session/`,
681 TSX + 491 CSS + fixtures) was removed on 2026-08-25 once the tailored web
Review Session shipped this direction. This file is the surviving record of
the study; the live surface is `apps/central-host/src/review-session/`.

Status: **accepted on 2026-08-10** as the reference direction for the tailored
web Review Session.

GitHub context:

- Wayfinder map: [Ship the tailored OpenRouter web Language Layer to beta](#229)
- Prototype ticket: [Prototype the tailored web Review Session](#235)

## Question answered

What concrete web Review Session experience makes the OpenRouter-backed coach
feel genuinely tailor-made — across profile-shaped comments, Move Intent and
Alternative Move coaching, feedback, progress and cancellation, budget or
provider fallback, and profile-change inspection — without exposing model brands
or internal evidence machinery?

## Accepted direction

Three variants were built and compared: **One thread** (conversation-first,
board a sidekick), **Workbench** (board-first three-column tool with a docked
Note / Ask panel), and **Briefing** (document-shaped scroll, one section per
moment). The accepted answer is the compact One thread shell with the
Workbench's separation between the note and the conversation:

- **Two columns, following the landing page.** A board column on the left
  carrying the position, evaluation, and ranked alternatives; one Review Session
  column on the right holding the moment picker, the pinned note, and the
  thread. No three-column workbench, no full-page briefing.
- **The page does not scroll; the Review Session column does.** Both columns fit
  the viewport, and the thread is the only scroll container, so the board and
  the note never leave the screen. Nothing needs to be sticky, because nothing
  moves behind it. Below 64rem the columns stack and the page scrolls normally.
- **The board takes the height its column leaves.** It is a flex item with a
  square aspect ratio, so its width follows from the space the heading,
  evaluation, candidates, and moment picker do not use. `vh` arithmetic was
  tried first and re-guessed wrong every time the column changed.
- **Moment navigation leads the board column.** The picker sits above the
  position it moves, leaving the right column to one job: the pinned coaching
  note with its learning plan, then the chat thread.
- **The board column scrolls if it has to.** The page still does not scroll,
  but a short viewport that cannot hold the picker, the line controls, and a
  square board gives the column its own scrollbar rather than cutting the board
  off.
- **The board must never be squeezed vertically.** It is width-driven and
  square, so a flex parent that shrinks its height stretches the ranks and hides
  the last one behind the board's own `overflow: hidden`. Cap its _width_ by the
  height the column leaves. This was the cause of two rounds of "the board looks
  wrong".
- **The board shows the fixture's own arrows and played move.** Engine and
  Elo-matched arrows come from the Review Session presentation; the played move
  is the highlighted last move. No synthesised positions.
- **Every fact appears exactly once.** The game identity is the header's; the
  move, its kind, and both evaluations are the picker's; the headline and recall
  are the note's; the arrows and the played-move highlight are the legend's. The
  candidate row lists only moves you have _not_ played, because it is the
  affordance for exploring an alternative, not a second verdict.
- **The note never scrolls.** The learning plan folds into an accordion so the
  note keeps its full height and the thread stays the only scroll container.
- **One moment header, on the note.** There is no separate focus card, and the
  moment picker carries navigation only — glyph, move, kind. Repeating the
  headline in a card above the note was the duplication this direction removed.
- **The Review Moment note is pinned, not a chat turn.** It scrolls inside the
  thread and sticks to its top, so it stays readable while the Player asks
  about something else. Once stuck it **compacts to a two-line truncated
  summary** — a sentinel above it drives an `IntersectionObserver`, because CSS
  alone cannot tell a sticky element that it has stuck — hiding the learning
  plan and the rest of the prose so a long thread keeps its context without
  losing the screen to it.
- **Follow-up chat is its own thread below the pinned note**, targeted at one
  move and labelled as such. Asking a question never pushes the note away, and
  the note is never re-rendered as conversation history.
- **Tailoring is never branded, and it is not explained per note.** The
  per-note "why this reads the way it does" disclosure is gone; the settings
  dialog on the coach avatar carries that once for the whole session. No model
  name, no provider, no evidence dump, ever.
- **Bounded wait, never a typewriter.** First-open authoring shows a skeleton, a
  countdown, and a way out. There is no Player-visible streaming.
- **Cancelling lands on the plain summary**, not on an empty state.
- **Asymmetric fallback, matching the contracts.** A comment degrades to the
  plain summary carrying a quiet reason line; a Coach Turn degrades to "can't
  answer right now" with a retry. Budget or provider exhaustion reads as
  "tailored notes are paused", never as an error or a cost.
- **The out-of-scope refusal is ordinary coach voice** and visibly distinct from
  an outage.
- **Votes live in the top-right of every card that carries coach prose** — the
  pinned note and each coach reply — as "Helpful?" with the two thumbs
  together, not a row under the message.
- **Feedback is the Coach App's thumbs widget** — the same `ThumbsUp` /
  `ThumbsDown` icon buttons and `chen-learning-feedback` markup the learning
  path cards use, so "was this useful" reads identically everywhere a Player is
  asked. Thumbs-down opens reason chips and an optional comment. It is attached
  to the pinned note and to each coach answer separately.
- **A profile change reads as "from here on".** Notes already written keep their
  wording, matching author-once-and-freeze.
- **Coaching preferences hang off the coach's avatar in the note.** The avatar
  _is_ the coach, so "how my coach talks to me" belongs there — a contextual
  control, not a settings row and not a card. Closed until asked for; open, it
  changes Explanation Style only and points at account settings for the rest.
- **The learning plan is part of the note.** The idea a moment teaches and the
  note explaining it are one thing, so `LearningPathCards` renders inside the
  pinned note rather than as a separate surface below the thread.
- **The page header is quiet.** The game title is a small line above the
  columns, not a display heading: the review is the content, not the fixture.
- **Both the note and the composer are compact.** The note runs a single
  avatar-plus-title header row with the recall as a one-line sub-head. The
  composer is one line — field and icon Send side by side, with no target
  label, because the note directly above already names the move.

## Deliberately not settled here

- The account settings surface itself — what your coach has noticed, clearing
  the signal half, and the Personalisation Preference off switch — belongs to
  [Prototype the Coaching Profile settings surface](#295). This prototype shows only the in-session entry point.
- The feedback payload, reasons taxonomy, and withdrawal path belong to
  [Define the web Review Moment feedback contract](#302). This prototype fixes the control's shape and placement only.
- Budget mechanics and any cost surfacing. The "tailoring paused" simulation
  shows what a Player sees, not how it is governed.

## Built from the design system, not beside it

Standing policy for prototypes in this repo, applied here on 2026-08-10: a study
is built from the existing components in `packages/ui` and the production
Review Session stylesheet, never from bespoke prototype widgets. A prototype
rendered in its own private styling looks fine in a vacuum and hides exactly the
density and component-fit problems it exists to expose.

What this study uses:

- `WatercolorChessboard` with `piecesFromFen` for the board, inverted from
  width-driven to height-driven so it fills the column; `ReviewMomentCarousel`
  for moment navigation.
- The Coach App's own `presentationComparisonArrows`, imported across apps the
  way the preview catalog already imports its move sequence fixture. `sequences.ts` builds the presentations with chessops from
  each moment's FEN, so every line in the fixture is legal chess — a throwaway
  test caught three illegal ones (a king walking into check, a blocked escape
  square, and a rook interposing where it could not reach).
- The `rs-shell` / `rs-header` shell classes, with the two-column grid following
  the landing page's `landing-review-showcase` shape. `BrandedReviewWorkspace`
  is deliberately not used: its three-slot layout separates the note from its
  thread, which is exactly what this direction joins.
- `WatercolorCard`, `WatercolorButton`, `WatercolorBadge`, `WatercolorNotice`,
  `WatercolorTextarea` for every surface and control.
- `@chenchess/ui/workspace/review-session.css` for the `rs-*` shell, thread,
  message, and composer presentation — the same stylesheet the real Review
  Session loads. The message markup mirrors `ConversationPanel`.

`tailored-review-session.css` is therefore down to the two-column grid, the
board column's viewport sizing, the thread-settings disclosure, and the four-row
conversation that gives the note its own row above the scrolling thread. If any
of them generalise, they belong in `packages/ui`.

One trap worth recording: **`.rs-shell` carries the dark Review Session theme**
(`--rs-bg: #090a0c`, light `--rs-text`) while every watercolor card is light
paper. Using the shell class without re-pointing those variables at the light
palette puts light cards on near-black and turns inherited text invisible. The
landing page already does this override; this prototype now does too. Surface
depth then runs paper page → workspace panel → ivory note → message cards, with
the conversation itself transparent so three surfaces do not stack.

## Reference files

- `TailoredReviewSessionPrototype.tsx` — the accepted interaction states.
- `tailored-review-session.css` — only what the design system lacks.
- `parts.tsx` — the authoring countdown and feedback state; no widgets.
- `fixture.ts` — four moments in game order covering tailored, authoring, and
  plain-summary notes plus assessment, refusal, and unavailable coach turns.

This stays reference code. Rewrite the accepted behavior against production data
and boundaries in `apps/central-host/src/review-session/` rather than promoting
these files.

`?settings=open` on the preview URL opens the settings dialog directly, which
is how it gets screenshotted during review.

The first moment carries a deliberately **long thread** so the sticky note can
be judged mid-scroll. Its extra turns are the other moments' captured questions
and answers, verbatim — real prose at real length, not filler.

## Run locally

```sh
bun install
cd apps/central-host && bunx vite --port 5199
```

- `http://localhost:5199/preview/web/tailored-review-session`

## Last verification

- `bun run --cwd apps/central-host check`
- `bun run --cwd apps/central-host lint`
- `bun run --cwd apps/central-host format`
- `bunx vitest run src/preview`
- Headless render smoke check of the composition: two-column shell, responsive
  board, pinned note as the only moment header, thread settings closed by
  default, and the feedback flow.
- **Reviewed visually** in headless Chrome at 1600x1000 and 1280x800. Both fit
  the viewport with no page scroll, and the thread scrolls inside its column.
  Four defects were found and fixed that way, listed below.

## Defects the screenshots caught

Worth recording, because none of them were visible in the markup:

1. **Light cards on the dark shell theme** — see the `.rs-shell` trap above.
2. **Message cards clipped to a single line.** They are flex children of
   `.rs-thread`, so they shrank under pressure and the watercolor card's own
   `overflow: hidden` cut the text. Fixed with `flex: 0 0 auto`.
3. **Prose rendered as a form control.** `.rs-badge-muted` carries a border and
   a raised background, so the recall line read as a disabled input.
4. **A starved thread.** The moment picker and a 6rem composer left the thread
   about one card tall, cutting coach answers in half. Fixed by moving the
   picker under the board and compacting the composer.
