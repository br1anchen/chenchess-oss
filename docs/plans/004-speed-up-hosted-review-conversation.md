# Plan 004: Speed up the hosted Review Session conversation

> **Status**: DRAFT for review. This document records measurements and proposes
> implementation slices; it does not authorize a production migration.
>
> **Measured on**: 2026-07-28, against local `b2638c55` and the staging web
> deployment of `c907db0c`.
>
> **Primary scenario**: the shared ChatGPT Review Session with a 65-ply pasted
> game, six automatic Critical Moments, follow-up discussion, and restoration
> of the Critical Moment picker.

## Outcome

Make the Review Session feel immediate when ChatGPT or Claude is the Language
Layer, without weakening grounded authoring, atomic publication, or
recoverability. Optimize and measure two distinct paths:

1. **Frame path**: render or restore the Critical Moment picker.
2. **Conversation path**: handle a short follow-up such as confirming or
   correcting Move Intent and returning a grounded coaching response.

The main conclusion from the observed session is that provider chess analysis
was not the bottleneck. The largest ChenChess-owned costs were Review Session
preparation, cross-region Firestore persistence, duplicated resume/render work,
and a low-level tool workflow that requires several host-model decisions.

### UX architecture

Treat these as independent pipelines with independent completion states:

1. **Narration**: the host model can stream a short acknowledgement and later a
   concise conclusion.
2. **MCP execution**: ChenChess validates, loads or computes authoritative
   state, and returns a useful compact result.
3. **App rendering**: the iframe paints a shell immediately, renders the
   compact result, and loads detail only after explicit interaction.

Neither narration nor the app should wait for the other to finish. Host
capabilities differ, so ChenChess cannot require that ChatGPT or Claude streams
text concurrently with a tool call; it can ensure its contracts do not
introduce an unnecessary dependency between them.

The first response should be a **Review Manifest**, not a fully hydrated
authoring session:

- compact plain-text summary;
- chronological Critical Moment summaries with stable IDs;
- lightweight board, arrow, and evaluation presentation for every selectable
  moment;
- iframe shell plus progressive status;
- deep evidence, engine lines, and authoring context fetched on demand.

The MCP result is the authoritative snapshot. The model receives a compact
summary, the app receives presentation data, and neither recomputes or refetches
the same snapshot unless it is recovering an old card.

### Rendering and functional-parity decision

Do not server-render a new iframe document for every tool result. The iframe
resource is loaded by the MCP host; it is not inserted into the Language
Layer's model context. Use a build-time-rendered immutable shell plus
server-derived presentation state in component-only metadata. This preserves
resource caching while keeping chess semantics out of the iframe.

Shrinking the iframe improves resource transfer, parse, and paint time.
Shrinking `content` and `structuredContent` improves Language Layer latency and
token use. Track these as separate budgets.

| Strategy                                                     | Decision | Reason                                                                                   |
| ------------------------------------------------------------ | -------- | ---------------------------------------------------------------------------------------- |
| Build-time-rendered immutable shell and skeleton             | Adopt    | cacheable and immediately paintable                                                      |
| Server-derived presentation state in component-only metadata | Adopt    | keeps chess semantics authoritative and the iframe small                                 |
| Per-request server-rendered iframe HTML                      | Reject   | repeats markup, weakens caching, and still needs client code for selection and animation |
| Server-rendered SVG export or no-JavaScript fallback         | Optional | useful outside the primary conversational path                                           |

### Railway-hosted Coach App preview

Expose a stable, read-only preview at `GET /preview/coach-app` on the deployed
Railway web origin. This is an engineering and design-review surface for the
actual chat iframe UI. It is not an MCP tool route, an authenticated coaching
session, or a second product implementation.

The hosted preview must render from the checked-in `Synthet1` benchmark Game
without invoking ChatGPT, Claude, another Language Layer, the MCP host bridge,
Coach Engine, Firestore, Stockfish, Maia, or any live provider. It must reuse
the production iframe renderer, presentation contract, local selection reducer,
styles, board, pieces, graph, and handoff states. Only the outer adapter differs:
a deterministic preview bridge supplies fixture state and captures host actions
in memory.

```text
Central Host Preview Catalog
  /preview/coach-app/review-session
            ↓
sandboxed iframe srcdoc + fixture-only host bridge
            ↓
exact ui://chenchess/review-session-v3.html artifact
```

The Preview Catalog loads the same manifest-backed resource returned by MCP;
there is no dedicated preview HTML build or public artifact route. Do not import
benchmark data into the authenticated `/app` entry. `COACH_APP_ARTIFACT_ROOT`
is the single runtime resource location, and each resource keeps its own digest
and cache identity in the artifact manifest.

The route is public because it contains only sanitized repository fixtures and
performs no authoritative operations. It must not read auth state, cookies,
local storage, environment secrets, or player data. It must not expose a
general fixture selector, arbitrary PGN input, session lookup, or backend proxy.

The visible preview must preserve the current iframe functionality:

- all seven chronological Critical Moments from the local benchmark;
- moment selection through both the top selector and evaluation-graph markers;
- static branded chessboard position, orientation, last move, overlays, arrow
  legend, played-versus-best values, and accessible announcement;
- complete evaluation curve and Critical Moment labels;
- identical busy, disabled, error, selected, and read-only visual states;
- the current “Discuss in chat” control and “Passed to chat” confirmation.

In the hosted preview, “Discuss in chat” must exercise the production handoff
state machine but stop at the preview bridge. Simulate a successful bounded
context update and message send in memory, then show the normal read-only
“Passed to chat” state. It must never open a chat, send a message, call an MCP
tool, or perform a network request. Reloading the route resets the fixture.

**Acceptance**:

- Railway serves `GET` and `HEAD /preview/coach-app` as `200 text/html` without
  Firebase sign-in, an SPA fallback, or an MCP host;
- the built web container contains a dedicated preview artifact and the thin
  Node origin serves its directory index at the exact route;
- all seven benchmark moments render and selecting either navigation surface
  keeps the board, arrows, graph, evaluations, and handoff target in lockstep;
- confirming a moment produces the local “Passed to chat” state with zero
  outbound requests;
- the route works at desktop, narrow mobile, normal motion, and reduced motion;
- an automated browser smoke test fails on console errors, missing pieces,
  layout overflow that hides required controls, or any non-static request;
- the production MCP Coach App bundle and authenticated web entry contain no
  benchmark fixture, preview bridge, or preview-only marker;
- fixture drift against `Synthet1` fails the preview contract test rather than
  silently showing mismatched moves or positions.

Place the seam between authoritative chess meaning and presentation:

```text
Coach Engine and Node presentation adapter
  own chess semantics, state, and animation frames
                        ↓
              ReviewPresentation
                        ↓
Tiny iframe renderer module
  owns pixels, timing, accessibility, and local selection
```

`ReviewPresentation` should be a deep module interface: callers learn one
versioned presentation contract while the implementation hides FEN decoding,
special-move semantics, evidence, provider results, and persistence shape. The
iframe must not calculate legality, infer captures, or reconstruct the domain
aggregate.

“Presentational board” means that squares and pieces are not move inputs. It
does not mean a static screenshot or a reduced Review Session interface. The
replacement must preserve the current workflow:

| Current capability                     | Required replacement behavior                                                                                                                                            |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Chronological Critical Moment selector | retains move label, title, summary, tone/glyph, active and disabled states, horizontal navigation, and keyboard activation                                               |
| Static chessboard position             | updates immediately to the selected moment and preserves player orientation, last-move highlight, check state, evaluation label, and accessible position announcement    |
| Overlay arrows                         | preserves Engine, Maia, and best-reply arrows, colors, labels, endpoints, and visible legend                                                                             |
| Evaluation graph                       | preserves the complete real-game curve, numeric labels, selected-ply marker, Critical Moment markers, tone/glyph, selection, and matching disabled states                |
| Selected-moment evaluation comparison  | preserves played and Engine-best values for the active moment                                                                                                            |
| Confirm moment to chat                 | preserves the visible “Discuss in chat” handoff, active-work cancellation, host-context update, message fallback, busy/error state, and read-only “Passed to chat” state |

The top selector and evaluation-graph markers are two adapters for the same
local selection interface. Selecting through either must atomically update:

- active moment ID and selected ply;
- board position and last-move highlight;
- overlay arrows and their legend;
- active graph marker and evaluation label;
- played-versus-best comparison;
- the exact target that will be handed to chat.

No network request or model turn is allowed in that selection path. The bounded
host context may update asynchronously because it changes the subject of later
conversation, but a failed context update must not roll back or block local
selection. Selection remains disabled during busy work, active Coach work, and
after the card has been passed to chat, matching the current behavior.

## Observed baseline

### User-visible ChatGPT turns

The shared session reported these host-level durations:

| Interaction                                   | Reported duration |
| --------------------------------------------- | ----------------: |
| Initial game review and six-moment frame      |            1m 16s |
| Recovery after an invalid publication command |               51s |
| Discuss `12.Ne2`                              |               53s |
| Discuss `15.f3`                               |            1m 40s |
| Show all Critical Moments again               |               22s |
| Simple material-balance follow-up             |            1m 15s |

These durations include ChatGPT planning, tool selection, ChenChess calls,
resource delivery, app startup, rendering, and final prose generation. ChatGPT
does not expose a complete internal span breakdown, so the plan must retain
separate first-party and host-level measurements.

### Exact initial backend trace

Railway and Firestore timestamps reconstruct the first Review Session:

| Stage                                 |          Timestamp or duration | Finding                                                   |
| ------------------------------------- | -----------------------------: | --------------------------------------------------------- |
| Game import admitted                  |                  14:28:33.986Z | 65 reviewed positions                                     |
| Review facts complete                 |                          611ms | all 65 engine evaluations were cache hits                 |
| Maia work                             |                 2,330ms summed | 33ms median, 60ms max, executed concurrently              |
| Game import committed                 |            +4.315s after facts | cross-region persistence tail                             |
| Review Session start admitted         |                  14:28:41.521Z | six automatic moments                                     |
| Six intent contexts prepared          |                        12.699s | two uncached projection batches and engine-lease queueing |
| Initial checkpoint committed          | +14.182s after domain creation | 360 Firestore writes                                      |
| Import admission to committed session |                    about 34.4s | before confirmed app paint or ChatGPT prose               |

The backend therefore explains about 34 seconds of the 76-second initial turn.
The remaining time is above that boundary: host-model planning, MCP result and
resource delivery, app startup, duplicated resume work, rendering, and final
response generation.

### Follow-up persistence trace

The latest observed mutation had:

- domain `lastActivityAt`: `16:22:53.532Z`;
- Firestore `updateTime`: `16:23:01.181Z`;
- persistence tail: **7.649 seconds**.

By contrast, “show all Critical Moments again” did not mutate the Review
Session. A warm in-memory resume only clones the current aggregate, so its
22-second user-visible delay did not come from Stockfish, Maia, or a Firestore
write.

### Payload and document shape

Measured serialized sizes:

| Artifact                         |                         Current size |
| -------------------------------- | -----------------------------------: |
| Coach App single-file HTML       |                  1,185,893 bytes raw |
| Coach App single-file HTML       |                   468,736 bytes gzip |
| Inline WebP assets in the bundle |                                   24 |
| Game Import Firestore document   |                514,297 bytes encoded |
| Review Session root document     |                515,577 bytes encoded |
| Six moment documents             |                151,636 bytes encoded |
| 353 evidence documents           |        about 1,144,518 bytes encoded |
| Initial atomic checkpoint        | 360 writes, about 1.8MB of REST JSON |

Decoded payload inspection also found:

- the session root contains about 221KB of duplicated `importedGame` data;
- six moment cores reference 359 evidence entries;
- the evidence payload is about 478KB as ordinary JSON;
- a full resumed result is inferred to be roughly 700KB before transport
  encoding because it repeats snapshots and carries all moment evidence.

The exact MCP wire size is not currently logged. Adding that measurement is
part of Phase 0 rather than treating the estimate as a release assertion.

The current iframe also carries chessboard capabilities that the product no
longer exposes: `react-chessboard`, `chessops`, generic motion infrastructure,
drag/click and keyboard move input, legal-destination presentation, promotion
handling, and client-side chess move support. The branded piece sources already
exist as twelve SVG assets. Remove the input machinery and package the pieces
once as a minified SVG symbol sprite without removing `ReviewMomentPicker`,
`EvaluationGraph`, overlay arrows, evaluation comparison, or the
selected-moment chat handoff.

## Current request flow

```text
Player
  -> ChatGPT model plans a tool call
    -> public Node MCP adapter on Railway US
      -> private Rust Coach Engine on Railway US
        -> Stockfish / Maia
        -> Firestore eur3
      <- full tool result
    <- ChatGPT delivers the app resource and redraw
      -> Coach App starts
        -> calls resume_review_session again
          -> Node -> Rust -> another full redraw
      <- first useful frame
  <- ChatGPT writes the conversational response
```

Important properties of the current implementation:

- `review_game` internally imports and starts a Review Session, but it is a
  data-only tool. Instructions then require ChatGPT to call
  `resume_review_session` to render the frame.
- The deployed Coach App does not consume an incoming
  `reviewSessionResumed` redraw directly. It extracts the session ID and calls
  `resume_review_session` again.
- The model-facing `structuredContent` for resume is already compact, but the
  app-facing redraw contains the full Review Session result.
- App startup also reads the artifact-retention preference.
- Eleven tools are visible to the model. Several are lower-level publication
  primitives whose ordering and payload construction are left to ChatGPT.
- Each persisted mutation reconstructs the current checkpoint by reading the
  root, every moment, and every evidence document before it writes the delta.
- Railway services run in `us-west2`; the Firestore database is in `eur3`.

### Target flow

```text
Player
  ├-> host model may stream a brief acknowledgement
  └-> start_review
        ├-> cached/admitted game facts
        ├-> small Review Manifest
        ├-> compact useful text ---------------------------> model
        └-> authoritative presentation --------------------> iframe shell
                                                              ├-> first frame
                                                              ├-> local picker/graph selection
                                                              └-> detail call on discussion
                                                                    └-> one prepared moment
  <- host model gives a concise conclusion independently
```

This flow computes the authoritative manifest once. Static positions for all
six selectors arrive in one small app-only snapshot. The selector, board,
arrows, evaluation graph, labels, and chat target update locally as one
projection, and only the selected stable ID is handed to the host. Detail
loading begins only when discussion requires authoring facts, and a failed or
slow iframe does not erase the useful text result.

## Ranked hypotheses

### H1 — Firestore read amplification and region distance dominate mutations

**Confidence: high.**

The 7.649-second measured mutation tail is consistent with the checkpoint
adapter rereading more than 1.8MB across hundreds of documents before a
conditional write. The path crosses from Railway US to Firestore Europe.

**Falsifier**: command spans show that Firestore read plus commit accounts for
less than 25% of mutation service time after response serialization is
measured.

### H2 — Duplicated resume and oversized redraw dominate frame readiness

**Confidence: high.**

The deployed controller discards the incoming full redraw, keeps only the
session ID, and performs the same resume again. A passive “show all” turn still
took 22 seconds despite no chess-provider work and no persistence.

**Falsifier**: app marks show that the second resume plus payload processing
accounts for less than 500ms and that most time elapses before the first host
tool result reaches the iframe.

### H3 — Tool-loop shape dominates simple coaching exchanges

**Confidence: medium-high.**

Simple follow-ups took 53–75 seconds. A confirmed ChenChess mutation contributed
about eight seconds, leaving most of the duration in host planning, repeated
tool calls, sampling, and final response generation. Eleven model-visible tools
include state transition, assessment, clarification, and publication
primitives.

**Falsifier**: correlated spans show only one host tool call and one model
generation, with ChenChess service time accounting for most of the turn.

### H4 — Provider analysis is the main source of the observed delay

**Confidence: low; contradicted for this session.**

The 65-position review completed in 611ms because engine work was cached. Intent
preparation did pay two cold projection batches, but this was an admission and
scheduling problem rather than slow Maia calls.

Provider optimization should not be the first implementation slice. Cold-cache
benchmarks remain necessary before generalizing beyond this trace.

## Provisional performance budgets

Record p50, p95, p99, maximum, payload bytes, duplicate-call rate, timeout rate,
and app-render failure with successful text fallback. Separate warm versus cold
engine cache and warm versus restored Review Session.

### Perceived-performance budget

| User-visible event                         |                                                        Target |
| ------------------------------------------ | ------------------------------------------------------------: |
| Local button/input feedback                |                                                   under 200ms |
| Skeleton or explicit loading phase visible |                                                   under 500ms |
| Meaningful first content                   |                                                    under 1.5s |
| Primary Review Manifest usable             |                                                      under 3s |
| Longer work                                | named phase, partial result, cancellation, or honest progress |

### First-party service budget

| Boundary                                          | Provisional p95 budget |
| ------------------------------------------------- | ---------------------: |
| Warm Review Manifest read                         |                  300ms |
| App result received to first meaningful paint     |                  500ms |
| App result received to interactive picker         |                   1.5s |
| Initial Review Manifest payload                   |         100KB raw JSON |
| One Critical Moment detail payload                |         100KB raw JSON |
| Incremental state update                          |          25KB raw JSON |
| Coach App entry bundle                            |             150KB gzip |
| Warm Railway preview navigation to first frame    |                   1.5s |
| Synchronous Review Session mutation persistence   |                   1.5s |
| Warm manifest creation after game facts exist     |                   1.5s |
| ChenChess-owned simple intent exchange            |                     5s |
| User-visible “show all” in a supported host       |                     5s |
| User-visible simple follow-up in a supported host |                    15s |

The first-party budgets can become release gates. Host-level budgets are
service-level objectives and should alert rather than block a release until
ChatGPT and Claude expose stable timing boundaries. Validate the size budgets
against representative mobile hosts before fixing them permanently.

First target 100–150KB gzip while retaining React. Consider a custom-element or
DOM renderer only if bundle attribution shows React dominates after unused
board-input, chess-rule, motion, and asset code is removed. Do not accept a
high-maintenance rewrite for an unmeasured theoretical gain; 50–75KB gzip is an
experiment, not the initial requirement.

## Phase 0 — Make the path measurable

Implement this before changing persistence or payload shape so every later
slice has a before/after comparison.

### Node MCP spans

Emit one structured completion event per tool call with:

- trace ID propagated from the public request;
- opaque operation, request, and session IDs;
- tool name and caller kind: `model`, `app`, or `server-compound`;
- `message_received`, `tool_selected`, and `tool_request_received` when the host
  exposes them;
- start, first Coach Engine byte, terminal Engine event, and return time;
- response `content`, `structuredContent`, and app `_meta` byte counts;
- tool-list count, serialized schema bytes, and description bytes;
- app resource raw and compressed byte counts;
- status, retry count, and normalized failure kind.

Do not log PGN, FEN, player text, evidence payloads, OAuth material, or
Firestore documents.

### Rust command spans

Add a completion event for every command, not only game import:

- queue wait and engine lease occupancy;
- cache hits, misses, and deduplicated positions;
- per-moment intent-preparation wall time;
- checkpoint read document count and bytes;
- validation, serialization, Firestore read, and Firestore commit time;
- response serialization bytes and total command wall time.

Propagate the Node trace ID. Preserve current operation IDs as domain handles,
not substitutes for a full request trace.

### App performance marks

Measure:

- resource parse and app boot;
- MCP connection ready;
- host tool result received;
- redraw projection complete;
- first skeleton paint;
- first meaningful frame;
- picker interactive;
- first board, arrows, and evaluation graph painted;
- selected-moment handoff ready;
- line animation ready, started, and completed;
- assistant/model first and final token when the host exposes them;
- every app-initiated tool call;
- `updateModelContext` start and completion.

Send only aggregate timings and sizes through a privacy-reviewed telemetry
path. App marks must not contain game or player content.

### Bundle attribution

Produce a compressed bundle report before choosing a renderer rewrite.
Attribute at least:

- MCP Apps runtime;
- React and React DOM;
- `react-chessboard`;
- `chessops`;
- motion and shared UI primitives;
- selector, graph, arrow, and handoff code;
- CSS;
- branded chess pieces, logos, illustrations, and other assets.

Record raw, gzip, parse, and first-execution cost. Removing unused input
machinery is preferred over replacing React or review functionality without
evidence.

### Hosted preview harness

Build the Railway-hosted `/preview/coach-app` route before replacing the board
renderer. Treat it as the executable visual contract for every Phase 1 iframe
change. The harness must run against the same production components and
active presentation decoder. It starts on the current redraw projector and
migrates to `ReviewPresentation` in the same slice that introduces that
contract. Copying JSX, CSS, or state logic into a web prototype does not satisfy
this requirement.

Produce and report the host-connected iframe bundle and preview bundle
separately. The preview bundle may contain the sanitized benchmark fixture, but
it must not relax the production iframe bundle budget or hide production
dependencies in a second measurement.

### Reproducible scenarios

Create a sanitized replay harness for:

1. 65-ply initial review, warm engine cache;
2. the same review, cold engine cache;
3. show all moments from a warm process;
4. restore after process restart;
5. select every moment from the top selector without a network request;
6. select every moment from its evaluation-graph marker;
7. verify board, arrows, evaluation labels, and chat target remain in lockstep;
8. confirm a selected moment to chat, including context-update fallback and
   read-only transition;
9. verify selector and graph disabled states during busy work, active Coach
   work, and after chat handoff, including active-work cancellation;
10. open one automatic moment for discussion;
11. correct or confirm one Move Intent;
12. inspect one alternative and request one Coach Turn;
13. animate capture, castling, promotion, and en passant fixtures.
14. load `/preview/coach-app` from the built Railway web image without auth,
    MCP, Firestore, providers, or a Language Layer;
15. exercise all seven hosted-preview moments and the simulated chat handoff
    while asserting zero outbound requests and no production-bundle growth.

Use a repository fixture rather than copying the shared conversation or player
content into telemetry.

Run every scenario against:

- a warm process and a cold Node/Rust process;
- ChatGPT and Claude MCP Apps hosts where their timing APIs permit;
- desktop and a constrained mobile viewport;
- normal and reduced-motion preferences;
- successful app rendering and an intentionally failed app-resource load to
  verify the plain-text fallback.

## Phase 1 — Remove duplicate work from the frame path

### 1. Consume the host redraw directly

When the incoming host result contains a compatible complete
`chenchess/redraw`, render it immediately. Call `resume_review_session` from the
app only as a recovery path when:

- the incoming result has only a session handle;
- the redraw version is unsupported; or
- the host restores an old card whose result is unavailable.

Keep the recovery idempotent and record which path was used.

**Acceptance**:

- one normal model-initiated resume produces zero app-initiated resumes;
- old-card restoration still performs exactly one app-initiated resume;
- first meaningful paint occurs before any retention-preference request
  completes.

### 2. Return useful text and a renderable manifest immediately

Attach the app resource and a render-ready Review Manifest to the successful
terminal result of `review_game` and the recovery `start_review_session` path.
Remove the instruction that ChatGPT must make an immediate second
`resume_review_session` call after starting a session.

Layer the result deliberately:

- `content`: a complete compact textual answer, for example the number of
  moments, the earliest move, and its one-sentence reason;
- `structuredContent`: summary, stable moment IDs, labels, importance, reason,
  revision, and the next valid actions;
- component-only metadata: the compact manifest view and rendering
  configuration that should not enter model context.

The text path must remain useful when the app is slow, unavailable, collapsed,
or unsupported by a host. Retain an explicit show/restore tool for later turns.

**Expected effect**: remove one host planning/tool round trip from initial
review, one full redraw transfer from every normal render, and the model's need
to narrate a large application payload.

### 3. Introduce a compact presentation contract

Add a versioned `ReviewPresentation` interface designed for the iframe:

- session and presentation revision;
- opening, review side, Elo, orientation, and stable selected moment ID;
- one shared evaluation timeline with display label and normalized value per
  ply;
- compact Critical Moment marker data: ID, selection target, ply, move label,
  title, summary, tone, glyph, and active state;
- one compact piece placement, last move, check square, and accessible
  announcement per selectable moment;
- played and Engine-best evaluation labels per moment;
- overlay arrow sets per moment with label, color, source, and destination;
- handoff state: ready, busy, passed-to-chat, error, and exact selected target;
- optional server-authored line-animation script;
- no evidence packet or full Coach Turn Context;
- no repeated domain aggregate per moment.

The server-side presentation adapter decodes FEN, derives arrows, and formats
the evaluation timeline. All six static positions and their presentation
metadata arrive once through component-only metadata. Load bounded authoring
context only when a moment is actually discussed.

Keep model-facing `structuredContent` compact. Treat the returned presentation
as authoritative; the iframe must not refetch it during normal startup.

**Acceptance**:

- the Review Manifest is at most 100KB raw JSON for the measured six-moment
  game;
- the top selector and graph markers switch all six boards locally;
- board position, arrows, legend, graph marker, evaluation values, and handoff
  target always identify the same selected moment;
- the current opening/header, retention state, status, selector, board,
  evaluation graph, and “Discuss in chat” behavior remain available;
- opening a moment for discussion still returns the full bounded authoring
  context required for grounded coaching.

### 4. Replace only the interactive chessboard implementation

Create a small board renderer module behind `ReviewPresentation`:

- CSS or SVG 8×8 board with fixed dimensions during loading;
- one minified SVG symbol sprite for the twelve branded pieces;
- CSS variables for board colors, highlights, shadows, and brand variants;
- SVG/CSS layers for last move, check, Engine/Maia/best-reply arrows, and arrow
  labels;
- no drag-and-drop, square selection, legal destinations, promotion input, or
  keyboard move entry;
- no client-side legality, chess analysis, or special-move inference.

This is an implementation replacement, not a review-workspace redesign.
Preserve the current selector and evaluation graph as accessible controls and
preserve their common `momentSelected` behavior. Preserve the arrow legend and
the played-versus-best evaluation comparison.

Remove `react-chessboard`, `chessops`, and generic move-input machinery from the
iframe path only when bundle attribution proves that each has no remaining
caller. Keep branded piece assets in a focused module so importing the piece
sprite does not pull unrelated logos, icons, illustrations, or motion masks.

Retain React initially. Keep React types out of the presentation interface so a
later DOM renderer adapter can be measured without changing server contracts or
tests.

### 5. Preserve the selected-moment chat handoff

The “Discuss in chat” control remains a first-class iframe action:

1. use the currently selected moment and exact canonical selection target;
2. attempt the bounded `updateModelContext` handoff;
3. send the discussion message even when the context update is unsupported or
   fails;
4. show busy and actionable error states;
5. after success, display “Passed to chat” and make the card read-only so later
   turns cannot retarget it.

Selection alone must not send a chat message. A failed asynchronous context
update during selection must preserve the local board and show the current
status fallback.

### 6. Animate server-authored line frames

The server owns all chess semantics. A discussed line returns a bounded
presentation script such as:

```ts
type LineAnimation = {
  initialPosition: PiecePlacement[]
  frames: Array<{
    durationMs: number
    moveLabel: string
    motions: Array<{ pieceId: string; from: Square; to: Square }>
    removedPieceIds: string[]
    positionAfter: PiecePlacement[]
  }>
}
```

The iframe interpolates motions, then reconciles to the authoritative
placement. Castling, promotion, en passant, and captures are already resolved
by the server. The iframe never guesses which pieces move or disappear.

Animation requirements:

- bounded frame count and total duration;
- cancel and replace stale animation on a newer revision;
- pause/replay control without board manipulation;
- visible notation and a useful static final frame;
- `prefers-reduced-motion` support that advances frames without travel
  animation;
- accessible announcements for the current move and final position;
- no automatic infinite loop.

### 7. Reduce and budget the app resource

- replace the 24 inline image variants with the focused piece sprite and only
  the first-frame brand mark actually used;
- keep the immutable shell, CSS, graph, and piece sprite build-time rendered
  and cacheable;
- avoid per-result server-rendered HTML or dynamic resource URIs;
- add raw and gzip bundle-size reports to the existing app verification;
- fail CI only after a reviewed budget and baseline are committed.

Render fixed-dimension board, selector, and graph skeletons before MCP
connection and resource hydration finish. Replace a spinner-only state with
named phases such as “Loading moments” and “Preparing selected analysis”, plus
a compact retry state. Retention preference should hydrate progressively and
must not hold the whole app in a busy state.

Keep visual-only state inside the iframe:

- selected Critical Moment;
- selected tab;
- expanded/collapsed panels;
- local timeline focus;
- animation playback and reduced-motion state;
- sorting and filtering of already loaded moments.

These actions must not invoke ChatGPT, Claude, or the MCP server. The selected
moment may update a bounded host context because it changes the subject of later
conversation; that handoff must not include board state, arrows, or evaluation
timeline data.

## Phase 2 — Make persistence proportional to the mutation

### 1. Add an optimistic delta-write fast path

The in-memory aggregate already validates the successor. Persist:

- the root revision and changed summary fields;
- only the changed moment;
- only newly created immutable evidence or assessments.

Use a Firestore precondition on the expected revision or update time. On
conflict, perform the existing full restore and validation path, then return a
typed retry result. Do not silently overwrite concurrent state.

Do not reread every moment and all immutable evidence for an uncontended
mutation.

**Acceptance**:

- one intent mutation reads the root/version and affected documents only;
- stale writers are rejected in a concurrency test;
- crash restoration still validates the complete aggregate;
- synchronous persistence p95 is at most 1.5 seconds in staging.

### 2. Return revision deltas after mutation

Do not resend the complete Review Session after a successful interaction.
Return:

- prior and resulting revision;
- changed stable IDs;
- changed display fields;
- any newly available detail handle;
- a recovery flag when the app must request a full snapshot.

The app applies the delta optimistically only when its current revision matches.
On a gap, it requests one compact Review Manifest rather than attempting to
merge unknown state.

**Acceptance**:

- a normal mutation response is at most 25KB raw JSON;
- local UI state survives authoritative domain updates;
- duplicate and out-of-order deltas are harmless;
- revision-gap recovery is covered by integration tests.

### 3. Pack immutable prepared data by moment

Evaluate replacing hundreds of evidence documents with one versioned prepared
core document per moment. The measured evidence for each moment is comfortably
below Firestore's document limit, but enforce a serialized-size guard and
retain chunking as an overflow format.

Target an initial checkpoint of:

- one small session root;
- six prepared moment documents;
- only separate documents that have independent update lifecycles.

This reduces both the current 360-write initial transaction and restore read
amplification.

### 4. Stop duplicating the imported game in the session root

Store the immutable Game Import once and reference it from the Review Session.
Keep only the summary fields needed for session listing and optimistic
validation in the root.

Before changing this seam, prove that Game Import and Review Session
retention windows cannot leave a live session with an expired import. Add a
restore test for the expiry edge.

### 5. Align compute and persistence regions

Benchmark staging with the public Node adapter, private Coach Engine, and Maia
in a European Railway region close to Firestore `eur3`. Compare:

- private service latency;
- Firestore read and commit latency;
- Maia image availability and cold start;
- ChatGPT-to-public-service latency;
- cost and operational constraints.

Given the player and Firestore locality, moving stateless compute to Europe is
the preferred experiment. Do not migrate Firestore or production traffic
without a rollback plan and measured staging result.

## Phase 3 — Split the fast manifest from Critical Moment detail

Revise ADR 0021 deliberately: automatic moments remain equal chronological
peers in the Review Manifest, but complete intent evidence is no longer a
precondition for the first frame.

### Fast path: create the Review Manifest

After game facts exist:

- select and order the six Critical Moments;
- produce compact labels, importance, and bounded one-sentence reasons from
  already admitted review facts;
- include lightweight board, arrow, evaluation, and marker presentation for
  all six moments;
- persist the small session root and manifest;
- return useful `content`, `structuredContent`, and the app view immediately.

No all-moment evidence expansion or language authoring belongs on this path.

### Detail path: prepare one discussed moment

Expose a narrow `get_critical_moment` contract keyed by stable session and
moment IDs. Selecting the moment in the iframe does not call it. On the first
request to discuss, explain, or explore that moment:

- prepare or retrieve its intent projection and evidence;
- report visible phases when work exceeds three seconds;
- return the bounded authoring context to the model and a compact detail view
  to the app;
- cache the immutable result by its semantic position, perspective, Elo
  profile, and engine configuration;
- persist it as one versioned moment core.

The Language Layer calls this tool after the iframe hands the confirmed moment
to chat. A model turn is required only when the player asks for explanation,
comparison, or coaching.

### Optional bounded prefetch

After the manifest is visible, prefetch at most the initially selected moment
when capacity is idle. Use bounded concurrency and cancel or deprioritize
speculative work when the player selects something else.

If product research still requires all six details to become ready eagerly,
submit one session-scoped batch that deduplicates positions and shares the
eight-worker pool. Never launch six operations that each contend for an
exclusive full-pool lease.

**Acceptance**:

- Review Manifest creation after game facts meets the 1.5-second p95 budget;
- the first frame does not wait for all-moment intent preparation;
- switching selectors, graph markers, boards, arrows, and evaluation labels is
  immediate and offline;
- discussing an uncached moment has a visible shell and named progress phase;
- cached detail meets the 300ms read budget;
- automatic moments retain equal navigation and recovery semantics;
- ADR 0021 is updated in the same implementation change;
- partial detail failure affects that moment, not the whole Review Session.

## Phase 4 — Give each caller a small, layered tool surface

Reduce the model-visible surface from eleven low-level tools to a small set of
user-intent contracts while preserving the fast list/detail split:

- `start_review`: return the Review Manifest and useful text;
- `show_review`: return the current compact manifest;
- `get_critical_moment`: return one admitted detail for discussion;
- `respond_to_move_intent`: resolve and, when needed, author one response;
- `explore_alternative`: evaluate one selected line;
- `request_coach_turn`: author one grounded alternative assessment.

Names are illustrative; preserve domain terminology and compatibility aliases
where necessary. Keep cancellation and retention app-only. Make publication,
clarification, and assessment primitives server-internal where a compound
write tool can own their ordering.

Tool definitions should be short enough to inspect as one coherent choice set:

- concise one-sentence descriptions;
- stable IDs instead of repeated records;
- no copied policy prose in every tool;
- shared rules in server instructions and invariants enforced in code;
- measured schema and description byte budgets.

### Compound Move Intent exchange

Add one purpose-built tool that:

1. applies the typed player resolution;
2. freezes the bounded authoritative context;
3. obtains at most one host sampling response when authoring is required;
4. validates evidence and publication authority;
5. atomically admits the result;
6. returns compact canonical state plus the display-safe response.

This follows the existing `request_coach_turn` precedent: Node owns the
workflow while the host model remains the Language Layer. It removes repeated
planner decisions without moving language authorship into Coach Engine.

For pure discussion that does not change state, instructions should explicitly
allow ChatGPT to answer from the already-open bounded context with no tool
call. For a state transition, require the compound tool exactly once.

Use fine-grained tool-input streaming only if a measured host supports it and a
legitimate compact argument remains large. It must not compensate for oversized
schemas or repeated domain objects, and partial JSON must never cross the
validated command seam.

### Tool contract constraints

- no tool accepts a full session or evidence packet back from ChatGPT;
- use opaque handles and server-frozen context;
- one user action has one idempotency handle;
- retryable persistence failure is recovered inside the compound seam;
- repeated equivalent reads collapse through single-flight;
- model-visible results contain only the facts needed for the next response;
- app redraw metadata is separate from model context.

**Acceptance**:

- a confirmed/corrected Move Intent uses one model-initiated tool call;
- the model does not construct grounding ledgers or publication fences;
- simple discussion uses zero calls when no state changes;
- grounded-authoring and invalid-evidence tests remain unchanged or stronger;
- ChenChess-owned simple-exchange p95 is at most five seconds.

## Phase 5 — Bound, cache, and degrade dependency work

### Cache by semantic identity

Use separate lifetimes and invalidation rules:

- process: Firestore/HTTP clients, auth discovery, validators, tool schemas;
- domain result: engine and Maia work keyed by position hash, perspective, Elo
  profile, and engine configuration;
- Review Session: compact manifest, prepared moment cores, selected moment, and
  stable revision;
- iframe: immutable bundle/assets and previously viewed detail records.

Collapse concurrent equivalent manifest/detail reads with single-flight.
Idempotency keys on writes must distinguish an already-completed operation from
a failure and prevent repeated language sampling or chess computation.

### Bound concurrency and optional dependencies

- parallelize auth, independent compact reads, and static resource work only
  after their necessity is known;
- cap detail prefetch and engine projection concurrency;
- give primary persistence an explicit failure budget;
- omit or defer optional enrichment, images, analytics, and adjacent-moment
  prefetch when their budget expires;
- return the manifest or cached detail with an honest partial-status field
  rather than fail the whole frame for optional work.

Never fire ten speculative downstream operations to make one frame appear
faster; record queue depth and cancellation effectiveness at p95.

### Optimize cold paths

- reuse database and HTTP connections;
- keep capability and tool-list responses memory-resident;
- precompile server code and validators in the deployment artifact;
- initialize heavy provider clients lazily;
- keep stateless compute close to Firestore and the main player population;
- serve immutable app resources with stable cache validators where MCP hosts
  support them;
- keep the board shell, selector/graph skeleton, CSS, and SVG piece sprite in
  the immutable entry resource;
- avoid third-party scripts and large secondary panels in the app entry path.

**Acceptance**:

- cold and warm latency are reported separately;
- one semantic detail request computes at most once per cache key;
- optional dependency timeout still returns useful text and a renderable
  manifest;
- retry storms do not duplicate language sampling or persistence;
- bounded concurrency lowers or preserves p95 under a multi-session load test.

## Interaction ownership

Keep this table as an implementation and review gate:

| Action                                                               | Owner                                    |
| -------------------------------------------------------------------- | ---------------------------------------- |
| Select a loaded moment from picker or graph, switch tab, replay line | iframe only                              |
| Hand the confirmed selected moment to later conversation             | bounded host-context update plus message |
| Load authoritative detail, restore, or save typed state              | direct MCP call                          |
| Explain, compare, reason, or compose grounded coaching               | model-mediated flow                      |

Crossing a more expensive seam requires a concrete authority or reasoning need.
The iframe must not call the server merely to change a loaded board, arrows, or
graph marker. It must not send piece placements, arrows, or the evaluation
timeline into model context to keep visual state synchronized.

## Rollout and verification

Ship each slice independently behind narrowly scoped compatibility switches.
For every slice:

1. run the sanitized replay suite before and after;
2. compare p50/p95/p99/max stage timings and bytes;
3. inspect correctness and recovery fixtures;
4. exercise both ChatGPT and Claude MCP Apps hosts;
5. keep old-card and old-redraw compatibility for a documented window;
6. roll back automatically on error-rate or latency regression.

Recommended implementation order:

| Order | Slice                                                                 | Expected leverage                               | Risk        |
| ----- | --------------------------------------------------------------------- | ----------------------------------------------- | ----------- |
| 1     | Correlated spans, bytes, app marks, and bundle attribution            | makes every later choice falsifiable            | Low         |
| 2     | Railway-hosted deterministic Coach App preview route                  | makes iframe changes directly reviewable        | Low         |
| 3     | Consume incoming redraw; defer retention read                         | removes one resume from every frame             | Low         |
| 4     | Return text plus a compact manifest from `review_game`                | removes one initial host round trip             | Medium      |
| 5     | `ReviewPresentation` parity contract and regression suite             | prevents UX loss during optimization            | Medium      |
| 6     | Build-time shell, branded board, arrows, graph, and bundle budget     | removes unused interaction weight               | Medium      |
| 7     | Preserve chat handoff and add server-authored line animation          | retains workflow and adds lightweight motion    | Medium      |
| 8     | Split manifest from discussed-moment detail; revise ADR 0021          | removes all-moment preparation from first paint | High        |
| 9     | Optimistic Firestore delta write and delta response                   | makes mutation cost proportional                | Medium-high |
| 10    | Moment-packed checkpoint                                              | cuts initial writes and restore fan-out         | High        |
| 11    | Semantic caches, single-flight, and dependency budgets                | prevents repeated and hanging work              | Medium      |
| 12    | European staging topology experiment                                  | removes cross-region RTT                        | Medium      |
| 13    | Layered and compound Language Layer tools                             | removes planner round trips                     | High        |
| 14    | Alternative non-React renderer experiment, only if budget still fails | tests the remaining bundle floor                | Medium      |

## Correctness gates

Performance work must preserve:

- one canonical Coach Engine aggregate and revision order;
- exact grounding and evidence validation;
- publication-fence and idempotency behavior;
- typed recovery from persistence failure;
- session recovery after Node or Rust process restart;
- six equal automatic moments in the manifest, with ADR 0021 explicitly
  revised to permit progressive detail readiness;
- functional parity for selector, static board, arrows and legend, evaluation
  graph and Critical Moment labels, played-versus-best values, and the selected
  moment chat handoff;
- one selected moment drives every visual projection and the exact chat target;
- one authoritative server interpretation of castling, promotion, en passant,
  captures, checks, arrows, and resulting positions;
- stable animation cancellation and revision behavior;
- keyboard and screen-reader access to selector, graph markers, evaluation
  labels, arrow legend, and the chat handoff;
- board orientation, notation, contrast, and accessible position announcements;
- useful static content under reduced motion or animation failure;
- artifact-retention and deletion behavior;
- no game, player, or secret content in timing telemetry;
- the public preview contains only the sanitized benchmark, performs no
  authoritative or outbound operation, and remains isolated from both
  production entry bundles.

## Considered and rejected

### Per-request server-rendered iframe documents

Rejected for the main conversational path. It does not reduce model context,
because the iframe resource is not model-visible. It would weaken immutable
resource caching, resend board and graph markup on every result, create
state/resource identity races, and still require client JavaScript for
selection, chat handoff, and line animation.

Use server rendering only for the build-time shell, optional static export, or
a no-JavaScript fallback.

### Client-side chess rules for animation

Rejected. A generic chess library in the iframe duplicates Coach Engine
semantics and makes special-move correctness part of the renderer module's
interface. Server-authored frames keep the presentation seam small and
testable.

### Simplifying the review UI to meet the bundle budget

Rejected. Remove unused chessboard input machinery and duplicated assets, not
the Critical Moment selector, board positions, overlay arrows, arrow legend,
evaluation graph, labels, evaluation comparison, or chat handoff.

### Immediate React removal

Rejected until bundle attribution is available. Removing unused board-input,
chess-rule, generic motion, and duplicated asset code is lower risk. Replace
the renderer adapter only if React's measured fixed cost prevents the reviewed
bundle budget.

## Do not optimize first

- Stockfish or Maia latency based on this trace;
- prompt wording without measuring tool-count and generation spans;
- dropping atomicity, validation, or acknowledged durability;
- preloading every authoring payload into the frame;
- speculative all-moment prefetch without a bounded-concurrency measurement;
- per-request iframe SSR;
- client-side move legality or engine logic;
- deleting existing review-navigation functionality to meet a size target;
- a framework rewrite before bundle attribution;
- moving databases or production regions before the staging benchmark.

## Exit criteria

The work is complete when:

- every turn can be decomposed into host, Node, Rust, provider, Firestore, and
  app-render stages without sensitive content;
- a normal frame performs no duplicate resume;
- the compact redraw and app bundle meet their budgets;
- both selector surfaces update the board, arrows, graph, evaluation values,
  and exact chat target locally and atomically;
- all current Critical Moment labels, legends, and accessible states remain;
- confirming a moment still reaches chat when `updateModelContext` fails and
  makes the originating card read-only after success;
- the iframe contains no square move input or chess-rule implementation;
- branded board pieces are packaged once in the immutable resource;
- a server-authored line animates captures and special moves correctly, with a
  reduced-motion fallback;
- uncontended mutations do not reread the complete session graph;
- initial display does not wait for all-moment intent preparation;
- cached and uncached moment detail have separate budgets and visible states;
- one simple Move Intent exchange requires at most one model-visible tool call;
- plain text remains useful when app rendering fails;
- the Railway web origin exposes `/preview/coach-app` as the production-renderer
  benchmark preview without auth, MCP, Language Layer, Firestore, or provider
  calls, while the production iframe bundle remains fixture-free;
- staging meets the first-party p95 budgets for seven consecutive days;
- a repeated ChatGPT session materially improves the 76s initial, 22s show-all,
  and 75s simple-follow-up baselines without a correctness regression.

## Relevant implementation references

- `apps/central-host/server/coach-app-tools.ts`
- `apps/central-host/server/coach-app-mcp.ts`
- `apps/central-host/server/coach-app-coach-turn.ts`
- `apps/central-host/server.ts`
- `apps/central-host/Dockerfile`
- `apps/coach-app/src/CoachAppController.tsx`
- `apps/coach-app/src/bridge.ts`
- `apps/coach-app/src/CoachReviewContext.tsx`
- `apps/coach-app/src/selectedMomentHandoff.ts`
- `apps/coach-app/src/workspaceBoard.ts`
- `apps/coach-app/src/boardMoves.ts`
- `packages/ui/src/review/ReviewContextNavigation.tsx`
- `packages/ui/src/board/ChessboardSurface.tsx`
- `packages/ui/src/assets.ts`
- `packages/ui/src/assets/brand/chess-pieces/`
- `services/coach-engine/src/review_session_checkpoint/firestore.rs`
- `docs/adr/0021-eager-review-moment-intent-preparation.md`
- `docs/adr/0023-centralized-hosted-review-session-topology.md`
- `docs/research/game-review-wall-time-attribution.md`
- `docs/research/centralized-review-pipeline-throughput-mechanics.md`
