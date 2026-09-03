# Daily Coaching dashboard prototype

The prototype code (`apps/central-host/src/preview/daily-coaching-dashboard/`,
1944 TSX + 1901 CSS) was removed on 2026-08-25 once the authenticated Daily
Coaching dashboard shipped this direction on the watercolor primitives. This
file is the surviving record of the study; the live surface is
`apps/central-host/src/daily-coaching/`.

Status: accepted on 2026-08-07. Variant A, "Today first", is the reference direction for the
authenticated Daily Coaching dashboard.

GitHub context:

- Wayfinder map: [Design passive Daily Coaching from playing profiles](#217)
- Prototype ticket: [Prototype the Daily Coaching dashboard](#224)

## Question answered

How should the authenticated Daily Coaching dashboard let a Player enable URL-only coaching,
understand first-time backfill, read the latest Coaching Digest and archive, inspect every included
Game's findings, enable email, and start a fresh Review Session without becoming a Game library or
operations console?

## Variants studied

All three remain in `DailyCoachingDashboardPrototype.tsx` as the record of the study.

- **A — Today first (accepted).** The latest digest is the page. A priority band sits directly under
  the digest header, the included Games follow as expandable disclosures, and connection, email, and
  archive live in a secondary sidebar.
- **B — Coach brief (rejected).** Editorial single column with a horizontal date rail and Games as
  tabs. It reads well but shows one Game at a time, which hides the shape of a ten-Game day, and its
  date rail promotes the archive above the day's coaching.
- **C — Archive workspace (rejected).** Three-pane shell with a nav rail, archive sidebar, Game
  table, and sliding inspector. It crosses the boundary this ticket draws: the table and inspector
  read as Game management and an operations console rather than daily coaching.

## Accepted information hierarchy

Top to bottom in the main column, with everything else demoted to the sidebar:

1. **The digest lead.** State first, then date. Latest digest, an archived digest, first-time
   backfill, "no eligible Games yesterday", and "no connection" are all the same slot — the Player
   never hunts for what today means.
2. **Priorities.** At most two, numbered, each carrying its purpose (Improve or Reinforce), its
   supporting-Game count, and its exact learning resources. Zero priorities is a legitimate digest
   and renders the Games without a priority band.
3. **Included Games.** Every selected Game, in canonical order, as a disclosure showing result,
   side, opening, time control, and its Learning Path count. The first Game is open by default.
4. **Findings inside a Game.** The complete frozen Learning Path list, each marked Missing idea or
   Idea reinforced, with its moment reference and its exact resources. Nothing is summarized away.
5. **Sidebar: connection, email, archive.** Configuration and history are present on every visit but
   never compete with the day's coaching.

The setup state replaces the whole lead with a single profile-URL form, and states plainly that it
takes only a public URL, needs no Lichess authorization, and does not verify profile ownership.

## Interaction boundaries

- **Connection is one line, not a console.** Provider, username, "Daily Coaching on", and the
  detected timezone as a read-only fact. Managing the connection is one affordance behind a settings
  control. No schedule control, no timezone picker.
- **Backfill is a state, not a progress report.** "Preparing your first digest", an indeterminate
  wash, and permission to leave the page. No counts, no per-Game progress, no queue position.
- **Silence is a valid day.** A day with no eligible Games produces no digest and says so, without
  fabricating an empty digest entry in the archive.
- **No failure surface.** V1 shows no job-failure details and no skipped-Game counts anywhere in
  this hierarchy.
- **The archive is coaching-first.** It lists published digests by date with their Game and path
  counts. There is no arbitrary Game browsing, no search, no folders, no tagging, and no Saved Game
  management. A Game is reachable only through the digest that included it.
- **Email is one toggle.** On or off, against the verified ChenChess account email. No
  address entry, no per-digest delivery options, no frequency choice.
- **Reviewing is an explicit handoff.** "Review this Game" opens a dialog offering the web
  workspace, ChatGPT, or Claude, and states that the new session is temporary and leaves the digest
  unchanged. The dashboard itself never becomes the Review Session surface, and it never mutates the
  frozen digest.

## Edge states exercised

Switchable with `?state=` on the preview route:

| `state`    | Covers                                                   |
| ---------- | -------------------------------------------------------- |
| `setup`    | No connection; profile URL entry                         |
| `backfill` | One Lichess connection, first-time backfill running      |
| `empty`    | No eligible Games yesterday, archive still populated     |
| `one`      | One-Game digest                                          |
| `ten`      | Ten-Game digest                                          |
| `history`  | Historical digest navigation with email delivery enabled |

Email disabled is the default in every other scenario, and the fresh-session handoff dialog is
reachable from any digest state.

## Reference files

- `DailyCoachingDashboardPrototype.tsx` — the three variants, the shared scenario fixtures, and the
  accepted interaction states.
- `daily-coaching-dashboard-prototype.css` — the watercolor presentation for all three.

The prototype remains reference code. Rewrite the accepted behavior against production data and
boundaries rather than promoting this file directly.

## Run locally

From the repository root:

```sh
./tooling/nix-develop --command bun run dev:central-host
```

- Dashboard study: `http://127.0.0.1:5173/preview/web/daily-coaching-dashboard`
- Variants: `?variant=A|B|C`, or the left and right arrow keys.
- Edge states: `&state=setup|backfill|empty|one|ten|history`.

## Last verification

In `apps/central-host`, under `./tooling/nix-develop`: `bun run typecheck`, `bun run lint`,
`bun run format`, and `bun run test` (335 of 335). The suite needs the Coach App artifacts built
first — `bun run --cwd apps/coach-app scripts/buildArtifacts.ts` — otherwise
`server/oauth-lifecycle.test.ts` fails with a 500 on MCP initialize.

The three variants were reviewed in the browser against the six scenarios before Variant A was
accepted.
