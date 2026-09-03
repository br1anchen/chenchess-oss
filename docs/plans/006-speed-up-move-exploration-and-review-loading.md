# Plan 006: Make move exploration feel instant, and stop refetching a frozen review

> Spike date: 2026-08-30. Read against working-tree revision `28b6745d`.
>
> This plan answers two questions the Player asked: can a client-side WebAssembly
> Stockfish make Alternative Move exploration feel fast, and can a persistent
> client cache make returning to a reviewed Game feel instant. The spike says
> **not first to the first, and yes to the second** — and it says why: the
> dominant cost on the exploration path today is not engine search. It is
> request serialization the browser performs against itself.
>
> A browser engine turns out to be more practical than expected — a
> single-threaded build needs no cross-origin isolation and is not meaningfully
> slower than native. It is still the *last* phase here, because it is sixth on
> the ranked list of costs and it is the only one gated on an undecided
> `LICENSE`.
>
> Companion to [Plan 004](./004-speed-up-hosted-review-conversation.md), which
> covered the hosted Coach App conversation. This plan covers the two web
> surfaces 004 did not: the **Coaching Board** (ADR 0056) and the web **Review
> Session** board.

## Outcome

A Player who drags a piece on the Coaching Board sees the piece land
immediately and sees an evaluation attach to it within a few hundred
milliseconds, without the board locking. A Player who reopens a Game they
reviewed last week sees the board and the moments before the network answers.
Neither change moves the authorship of a chess fact out of Coach Engine.

## What this spike measured, and what it only inferred

Keeping these apart matters, because one of the conclusions below is arithmetic
over two documented constants and has never been observed in a running system.

**Read from code in this working tree (high confidence):** every round-trip
count, every admission constant, every Stockfish option, every cache header,
every storage API in use. These are quoted with `file:line` throughout.

**Measured previously and reused here:** the Stockfish and Firestore timings,
which come from
[game-review-wall-time-attribution.md](../research/game-review-wall-time-attribution.md),
[review-session-operating-limits.md](../research/review-session-operating-limits.md),
and
the compute-placement benchmark (withheld from this snapshot).
They were measured on the certified local machine or on staging, **not** on the
interactive exploration path this plan is about.

**Measured during this spike:** the browser-engine sizes, speeds and header
requirements behind Phase 5, recorded separately in
[browser-stockfish-wasm-feasibility.md](../research/browser-stockfish-wasm-feasibility.md).
Those are one-machine desktop numbers with no mobile evidence behind them.

**Inferred, not measured:** the end-to-end wall time a Player experiences per
board move, and the rate-limit collision in Phase 0. Nothing in this repository
has ever timed a Coaching Board move. Phase 0 exists to fix that before any
optimisation is judged.

## The finding

A single piece drag on the Coaching Board costs **`k + 4` sequential
browser-to-Railway NDJSON round trips**, where `k` is the number of moves
already in the line — not one. The board is fully disabled for the whole
sequence, with no spinner, no progress, and no cancel. Stockfish search is a
minority of that time.

The cause is that the Coaching Board drives the Player's own drag through
`evaluate_player_line`, the orchestrator built for an *agent* proposing a whole
line from nothing. `apps/central-host/src/coaching-board/useGameLineExploration.ts:63`
appends the new move to the whole line and re-submits it; the orchestrator then
re-establishes the session root from scratch and walks every ply again:

| Step | Round trips | Where |
| --- | --- | --- |
| `openAddressedReviewMoment` | 1 | `apps/central-host/server/coach-app/player-line-evaluate.ts:193` |
| `startReviewSession` | 1 | `player-line-evaluate.ts:207` |
| `openReviewMoment` | 1 | `player-line-evaluate.ts:222` |
| `inspectPosition` | 1 | `player-line-evaluate.ts:241` |
| `exploreAlternativeMove`, once per ply in the line | `k` | `player-line-evaluate.ts:281` loop, command at `:451` |

`preparePlayerLineRoot` is called unconditionally on every evaluation
(`player-line-evaluate.ts:129`), so the four preparation trips are paid again
for every drag. The ply loop `await`s each command before issuing the next
(`player-line-evaluate.ts:281-327`), so they cannot overlap.

The already-walked plies are deduplicated **server-side** by a content
idempotency key (`coachingBoardCoachTools.ts:286-313`), so they cost no second
Stockfish search and no second allowance — but they still cost a full
authenticated HTTP request, an NDJSON stream, and a Coach Engine command
admission each.

This code runs **in the browser**: `apps/central-host/src/coaching-board/coachingBoardCoachTools.ts:25`
imports the `server/` orchestrator and drives it with `coachingBoardCommandExecute`,
which issues `POST /api/v1/review-session/commands` per command through
`CoachEngineClient.stream` (`packages/coach-engine-sdk/src/client.ts:110`, `:387`).
A fresh `CoachEngineClient` is constructed per command
(`apps/central-host/src/review-session/client.ts:70`).

So the twelfth move of a line is 16 sequential browser round trips to Railway EU
West. The first move is 5.

**The web Review Session board already does this correctly.**
`ReviewSessionWorkspace.exploreMove` sends exactly one incremental
`exploreAlternativeMove` whose `parent` is the currently active branch
(`ReviewSessionWorkspace.tsx:713-736`) — no root re-preparation, no re-walk. The
Coaching Board should adopt that shape. It then makes one *further* call the
Review Session board does not need (below).

### Three more costs on the same path

**A fresh Stockfish process per interactive evaluation.** `StockfishSession::run`
is start → analyze → `quit` for a single position
(`services/coach-engine/crates/pipeline/src/engine_analysis/stockfish_session.rs:29-44`,
`:114-152`, `:254-273`). Process reuse exists only on the batch review path
(`run_positions`, `:69-104`). The batch measurement found session reuse removed
**65–73% of summed engine time**, taking the per-call median from 439–606 ms to
137 ms
([wall-time attribution](../research/game-review-wall-time-attribution.md), line 70).
The interactive path pays the un-reused cost on every move.

**A single-slot engine lease shared with full Game imports.**
`EngineAdmission::v1()` is `new(1, 4, 30s)` — one simultaneous lease, four
waiters, 30-second queue deadline
(`services/coach-engine/src/review_session_processor/admission/engine.rs:13`, `:33-42`).
The alternative-move path acquires it
(`review_session_processor/exploration.rs:142`) from the same pool as game
import (`lifecycle.rs:270`) and moment preparation (`readiness.rs:111`). One
Player importing a Game can stall every other Player's piece drag on that cell
for the length of an engine phase. The contract already names this state
(`AlternativeMoveProgressStage::WaitingForStockfish`) and the Coaching Board
throws the event away (below).

**A synchronous durable write before the Player sees the evaluation.**
`commit_staged_exploration` calls `persist_session_mutation`
(`review_session_processor/mutation.rs:190-212`), which calls
`analysis_cache.replace_moment` (`mutation.rs:140`) and only *then* applies the
result and returns it. That is a Review Moment document rewrite whose measured
upper bound is 533,679 bytes
([durable-storage-inline-measurement.md](../research/durable-storage-inline-measurement.md), line 14).
Firestore checkpoint p50 is 303 ms since the EU West alignment, down from
2,023 ms ([central-hosting.md](../central-hosting.md), "Compute placement").
Commit `28b6745d` ("Stop the exploration commit appending evidence it already
holds") is already trimming what this write carries; this plan does not
duplicate that work.

### What the Player sees while all of this happens

Nothing. `CoachingBoardChosenGame.tsx:191-193` sets
`interactionDisabled={!execute || exploration.busy}` and
`navigationDisabled={exploration.busy}`, so the board cannot be moved *or
navigated*. There is no optimistic move: branches are folded in only after the
await resolves (`coachingBoardCoachTools.ts:206`). There is no spinner and no
progress text. There is no cancel: `BoardWorkspace` renders Cancel only when an
`onCancel` is supplied (`review-session/BoardWorkspace.tsx:230-239`) and the
Coaching Board supplies none. A `WatercolorNotice` appears on failure only
(`CoachingBoardChosenGame.tsx:173`).

The progress stream exists and is discarded. The transport is already
progressive — `Content-Type: application/x-ndjson`, one envelope per line
(`services/coach-engine/src/routes/review_session.rs:39-42`) — and Coach Engine
emits `WaitingForStockfish`, `EvaluatingMove`, `CommittingMove`
(`exploration.rs:133-160`). `coachingBoardCommandExecute` forwards events but
retains only the terminal one (`coachingBoardCoachTools.ts:229-243`); the sole
consumer uses them for rate-limit accounting. The web Review Session board *does*
render them, with labels at `useReviewSessionCommands.ts:351-356`.

### The inferred rate-limit collision

Coach Engine allows **120 structurally valid Review Session commands per Player
per rolling minute** ([central-hosting.md](../central-hosting.md), "Restart and
health boundaries"). At `k + 4` commands per move, walking out a full 12-move
line costs `Σ(k+4) for k=1..12` = **126 commands**. A Player who explores
briskly can exhaust a limit designed to stop a runaway model loop.

This is arithmetic over two documented constants. It has not been observed.
Phase 0 must confirm or refute it before it is treated as real.

## Where the time goes, ranked

Ranked by expected contribution to a Player-visible board move, highest first.
Every rank is a hypothesis for Phase 0 to confirm; none is measured end to end.

1. **Sequential round-trip amplification.** `k + 4` serialized browser→Railway
   requests. Removable entirely in the client, with no contract change.
2. **Board lock and absent feedback.** Not latency, but it is most of the
   *perceived* latency: 200 ms of frozen board reads worse than 800 ms of a
   board that moved instantly and is filling in an evaluation.
3. **Fresh Stockfish process per evaluation.** A measured 65–73% of summed
   engine time on the comparable batch path.
4. **Single-slot engine lease shared with imports.** A tail problem, not a
   median one; worst case is a 30-second queue deadline. **Measured 2026-08-31
   and re-stated:** for a Player contending with their own background import the
   worst case is an immediate `admissionLimit` rejection, not a wait; queuing is
   cross-Player only, where a batch hold is 6.5 s median and 9.2 s maximum. See
   "Item 2 measured".
5. **Synchronous durable write on the critical path.** ~303 ms p50, more for a
   large moment document.
6. **Stockfish depth-16 search itself.** 291–391 ms median in isolation. This is
   the thing a client WASM engine would replace, and it is sixth.

That ordering is the whole argument for the phase order below.

## Perceived-performance budget

Adopted from Plan 004's budget, narrowed to exploration. These are objectives
for Phase 0 to measure against, not release gates.

| Player-visible event | Target |
| --- | --- |
| Piece lands on the board after a legal drag | under 100 ms, local |
| Board accepts the *next* drag | immediately; never blocked on a network call |
| Provisional evaluation visible | under 500 ms |
| Authored evaluation replaces the provisional one | under 1.5 s p95 |
| Reopening a previously reviewed Game to first board paint | under 500 ms warm |
| Any wait beyond 500 ms | named phase, live progress, and a cancel control |

Measured on 2026-08-31. The first line is **met**: 4 ms median and 9 ms
worst, against 455 ms before optimistic placement — see "Item 1 implemented and
re-measured". The second is not, and is not scheduled: `interactionDisabled`
still holds while a move is in flight, which Phase 2 item 2 states as its
intent, and lifting it needs a decision about what cancelling an in-flight move
means.

## Decisions this plan takes

**D1 — The Player's drag and the agent's line are different operations.**
`evaluate_player_line` keeps its contract exactly as signed: it is the WebMCP
tool an agent calls to evaluate a proposed line, and re-walking a whole line
from a moment is correct for that caller. The Player's own drag stops using it
and uses incremental `exploreAlternativeMove` instead, as the Review Session
board already does. This is a client change behind an existing engine command.

**D2 — Coach Engine remains the only author of chess facts.** ADR 0058's
consequence stands unchanged: every evaluation a Player is coached on, every
evaluation an agent reads, and every evaluation that is persisted or cited is
engine output. Anything computed in the browser is **provisional presentation**
— never persisted, never returned from a WebMCP tool, never placed in a
Coaching Board Snapshot, never quoted in prose. If a phase cannot hold that
line, the phase does not ship.

**D3 — Deriving a position is not authoring a fact.** The web already derives
board state from moves with `chessops` for Opening Lines (ADR 0058, "the web
already derives positions from moves with chessops"). Extending that to
legal-move generation for *drag validation* and to optimistic piece placement is
the same category. Plan 004's rejection of "client-side chess rules for
animation" was scoped to the Coach App iframe, where `chessops` would have been
new; on the web it is already bundled and already used.

**D4 — A client cache is a transport cache, not remembered state.**
`CONTEXT.md:905` says the web application remembers nothing between visits. This
plan does not change what a Player's *address* means or restore navigation
state: opening a Game Import still renders that Game Import from its own
address. It changes only where the bytes come from. The invariant sentence still
needs amending, and Phase 4 owns that.

**D5 — The Coach App gets none of the client-side work.** Reasons in the Coach
App section below. It gets the server-side phases only.

## Phase 0 — Measure a board move

Nothing here is optional. Every ranking above is a hypothesis.

1. Instrument the Coaching Board drag path with the existing app-performance
   marks: drag accepted, each command dispatched, each command's first byte,
   each terminal event, branch folded, board repainted. Correlate with the
   existing `trace:review-session:<uuid>` handle so the Rust spans
   (`queue wait`, `engine lease occupancy`, `cache hit/miss`) line up — the
   trace contract in
   [hosted-review-session-latency-baseline.md](../research/hosted-review-session-latency-baseline.md)
   already defines every boundary needed. Add no new telemetry vocabulary.
2. Record, for moves 1 through 12 of one line on staging: command count, summed
   network time, `waitingForStockfish` duration, `evaluatingMove` duration,
   `committingMove` duration, and Player-visible total.
3. Confirm or refute the 126-command arithmetic by counting admitted commands
   for a full 12-move walk inside one minute.
4. Record the same for the web Review Session board, which uses the incremental
   path, as the internal control.

**STOP condition:** if the measured breakdown contradicts the ranking above,
re-rank before implementing Phases 1–3. Do not implement a phase whose
hypothesis the measurement refuted.

### Phase 0 measured 2026-08-31

Measured against hosted staging in a real signed-in browser, driving the
Coaching Board through its own squares. The instrument was an injected probe
that wrapped `fetch`, tee'd the NDJSON stream to timestamp every progress
stage, and watched the board grid for the settling repaint — a measurement
harness, not shipped telemetry, so no vocabulary was added. Twelve moves of one
line on a 66-ply Game.

**Steps 2 and 4 — the walk, and its control.**

| Per move                       | Min | Median | Max |
| ------------------------------ | --: | -----: | --: |
| Commands                       |   1 |      1 |   1 |
| First byte                     | 180 |    208 | 251 |
| Command total                  | 250 |    448 | 827 |
| Queue wait                     |   0 |      0 |   0 |
| Engine search                  |   0 |    196 | 568 |
| Durable write                  |   0 |     48 |  93 |
| Player-visible (to repaint)    | 341 |    455 | 834 |

All values milliseconds. The first move of a session costs one extra command —
the memoised moment root Phase 1 hoisted — and every move after it is exactly
one. The Review Session board, as the internal control, still costs **two**
commands per move (`exploreAlternativeMove` then `inspectPosition`) at 559 to
1,000 ms, which is the documented price of Phase 1 step 3 not shipping.

**Step 3 — the rate-limit collision is refuted.** The whole twelve-move walk
cost **17 typed commands**: one snapshot read, one addressed moment open, one
session start, one moment open, one inspection, and twelve explorations. The
inferred figure was 126 against a 120-per-minute limit. It is not close, and it
disappeared by construction when move `k` stopped costing `k + 4`.

#### The ranking, re-ranked

The STOP condition asked for this comparison. Three of the six ranks moved.

1. **Sequential round-trip amplification — gone, and confirmed gone.** One
   command per move, measured in production rather than argued.
2. **Board lock and absent feedback — now the largest Player-visible gap.** The
   progress stages render, but the piece still does not move until the command
   returns: 448 ms median against a 100 ms budget. Phase 2 item 1 is the fix
   and is deferred on a snapshot grounding decision, so the budget line most
   visible to a Player is the one nothing is scheduled to meet.
3. **Fresh Stockfish process per evaluation — resolved.** Engine search is
   196 ms median inside a 448 ms command, consistent with Item 1's session
   reuse having landed.
4. **Single-slot engine lease — not a factor, as measured.** Queue wait was
   0 ms in twelve of twelve. It remains a real cross-Player risk (see "Item 2
   measured"), but it costs a lone Player nothing.
5. **Synchronous durable write — over-estimated about six-fold.** The plan
   carried ~303 ms p50; it measures **48 ms**. Unresolved question 2, whether
   the write must block the evaluation's return, is worth far less than the
   plan assumed and should probably be closed rather than answered.
6. **Stockfish depth-16 search itself — now the largest single component of a
   move.** It was ranked last on the assumption that the five costs above it
   dominated. Four of them are gone or negligible, so the ordering has
   inverted: what remains of a board move is mostly the search.

**What this says about Phase 5.** Its own precondition was that Phases 1–3 ship
and be measured, because "after they land, the remaining engine time may not be
worth a multi-megabyte download". The remaining engine time is 196 ms median
and 568 ms maximum, inside a 448 ms median command — so a client engine now has
a real target rather than a speculative one. That does not clear any of its
three gates, and #535 still holds them.

**Limits of this measurement.** One Player, one machine, one network, one Game,
one line, on staging with no contention. It measures a board move honestly and
says nothing about a contended engine, a slow network, or a mobile device.

## Phase 1 — Collapse the round trips

Largest expected win. Client-only. No engine change, no contract change, no new
dependency.

1. **Make the Player's drag incremental.** Replace the
   `evaluatePlayerLineOnBoard` call in `useGameLineExploration.explore` with a
   single `exploreAlternativeMove` whose `parent` is the drive's active branch
   (`{ kind: "move", branchRef }`) or the moment root for the first move —
   exactly `ReviewSessionWorkspace.tsx:717-728`. Move `k` becomes 1 command.
2. **Hoist and memoise the root preparation.** The four preparation commands
   become once per `(gameImportId, moment)` for the life of the board, held in
   the board session and invalidated on `UnknownSession` so the next drag
   rebuilds it. Coach Engine already evicts a session it cannot persist
   (`mutation.rs:152`), and that rejection is the invalidation signal.
3. **Stop fetching the evidence packet on the drag path.** The Review Session
   board follows every explore with `loadInspection`
   (`ReviewSessionWorkspace.tsx:738`). `PositionInspection` carries
   `CoachTurnContext` and a `ReviewSessionEvidencePacket`
   (`packages/coach-engine-sdk/src/PositionInspection.ts`) — needed to *ask the
   coach a question*, not to move a piece. `AlternativeMoveResult.resultingPosition`
   is already a complete `PositionSnapshot` with FEN, occupancy, castling
   rights, en passant and status. Render from that, and fetch the inspection
   lazily when a Host Turn or Coach Turn actually needs it.

   **Gate checked 2026-08-30: the answer is no, and this step does not ship.**
   The question was whether `evaluation.selectedMove` could stand in for
   `activeBranch.inspection.evaluation` at `ReviewSessionWorkspace.tsx:183-187`.
   It cannot. `normalize_child_evaluation`
   (`services/coach-engine/src/review_session_exploration/position.rs:152-174`)
   deliberately re-expresses the child analysis **from the mover's
   perspective** — negating the centipawn value and adding one ply to a mate
   distance — while `PositionInspection.evaluation` is the resulting position's
   own evaluation, from the side to move. Substituting one for the other would
   sign-flip every branch evaluation on the Review Session board, which is far
   worse than one round trip. The Review Session board keeps its inspection
   call; steps 1 and 2 stand on their own.

   One thing this turned up and did not fix: **the two boards already display
   different numbers for the same branch.** The Coaching Board reads
   `branch.evaluation.selectedMove` (mover's perspective,
   `CoachingBoardChosenGame.tsx:184`); the Review Session board reads
   `inspection.evaluation` (side to move). That may be deliberate — the mover's
   own loss is the coaching-relevant framing — but it is undocumented and
   deserves a decision of its own rather than being quietly normalised by a
   performance change.

Expected result: move `k` on the Coaching Board costs **one** round trip
instead of `k + 4`, and the rate-limit collision disappears by construction.

**Verify:** a test asserts exactly one command per drag after the first; the
existing `coachingBoardDrive` and `CoachingBoardExploration` suites stay green;
`evaluate_player_line`'s own oracles are untouched, proving the agent contract
did not move.

### Implemented 2026-08-30

Steps 1 and 2 are in `apps/central-host/src/coaching-board/`:
`gameBoardExploration.ts` holds the memoised moment root and the single-move
command; `useGameLineExploration.ts` drives it. `evaluate_player_line` is
untouched and still walks a whole line for its agent caller, per D1.

Phase 2 landed partially in the same change: navigation is no longer held by an
evaluation in flight, the engine's own progress stages are rendered, and Cancel
is wired to `CancelOperation`. **Optimistic placement did not ship** — see the
note in Phase 2.

**ADR 0058 amendment.** Parenting a move onto the branch the board stands on
needs that branch's resulting `PositionRef`, so
`CoachingBoardExplorationBranch.resultingPosition` gained a required
`positionRef`. Nothing about the decision changed — game branches already
carried the engine's reference as `AlternativeMoveResult`s, and opening
branches mint one with the existing `positionRefForFen`, exactly as they
already mint their root parent's. It was made required rather than optional so
the game path has no unreachable "branch without a reference" case to guard.

Recorded where each document's convention puts it: ADR 0058 takes an amendment
note in its Status, with its Decision body left as the historical record and
annotated at the two points that quote the old field list;
`docs/spec/coaching-board.md` is the live implementation contract, so its
wording is corrected outright.

## Phase 2 — Unblock the board

Perceived latency. Client-only.

1. **Optimistic placement. Decided and shipped — see below.** The decision it
   needed is [ADR 0060](../adr/0060-show-a-provisional-move-and-say-it-is-provisional.md),
   which took the pending-move field; the deferral and its reasoning are kept
   as written because the alternatives it weighed are why the field exists.
   The intent stands: on a legal drag, place the piece from the `chessops`
   projection (D3), mark it provisional, reconcile when the engine answers.
   What stopped it is not the rendering but the **Coaching Board Snapshot**.
   Every board-tool result carries the snapshot precisely so an agent cannot
   coach from a stale picture (ADR 0056), and a pending move creates a window
   where the Player sees one position and the snapshot reports another. The
   three ways out each cost something: report the derived position as
   `currentPosition` and `pathFromRoot` no longer explains how the board got
   there; add a pending-move field and the signed snapshot shape changes; hold
   board-tool reads until the move settles and a tool call can stall. That is a
   grounding decision, not a performance one, and it should be made
   deliberately rather than as a side effect of this plan.
2. **Stop locking the board. Navigation only.** `navigationDisabled` is gone:
   waiting on Stockfish is no reason to stop a Player reading the rest of their
   own Game. `interactionDisabled` deliberately stays while a move is in
   flight — the engine admits one Alternative Move evaluation at a time
   (`max_active_alternative_move_evaluations: 1`,
   `crates/contract/src/operations.rs:861-872`), and without optimistic
   placement the board has not moved yet, so a second drag would have nothing
   to attach to. Client-side queueing becomes worth doing once item 1 does.

   Browsing away mid-flight is handled rather than blocked: the branch is
   folded into the tree either way, and only followed if the Player is still
   standing where they played it.
3. **Render the progress that already exists.** Consume the discarded
   `WaitingForStockfish` / `EvaluatingMove` / `CommittingMove` stream on the
   Coaching Board with the labels the Review Session board already uses
   (`useReviewSessionCommands.ts:351-356`). `WaitingForStockfish` is honest and
   specific: it means queued behind the engine lease, and a Player who sees it
   is being told the truth.
4. **Give it a cancel.** `BoardWorkspace` already renders one when handed an
   `onCancel`; wire it to the existing `CancelOperation`, which the compute
   policy names as the only cancellation authority.

### Item 1 implemented and re-measured 2026-08-31

Shipped under [ADR 0060](../adr/0060-show-a-provisional-move-and-say-it-is-provisional.md).
The board draws the derived position the instant a drag is legal; the snapshot
gains a nullable `pendingMove` and `currentPosition` keeps meaning the last
Position Coach Engine confirmed, which `pathFromRoot` can still explain.

Re-measured against deployed staging with the same probe, Game and twelve-move
line as the Phase 0 walk, timing the moment the **source square empties** —
which is when the piece lands, rather than when the board settles. Before the
change those were the same event, because nothing rendered until the engine
answered.

| Per move                    | Before | After |
| --------------------------- | -----: | ----: |
| Piece lands, median         | 455 ms |  4 ms |
| Piece lands, maximum        | 834 ms |  9 ms |
| Board settled, median       | 455 ms | 382 ms |
| Board settled, maximum      | 834 ms | 698 ms |
| Commands per move           |      1 |     1 |
| Typed commands, whole walk  |     17 |    17 |

**The budget line is met with two orders of magnitude of margin**, and no round
trip was added: the command profile is identical. The settled figures moved from
455 to 382 ms median, which is inside run-to-run variance on one machine — the
change did not make the engine path faster, it made the board stop waiting for
it.

The deployed bundle carries `pendingMove` and the constraint sentence, so the
contract half shipped and not only the rendering. The measurement also exhibits
the window ADR 0060 exists to describe: the piece is on its destination at 4 ms
while the command runs until 382 ms, which is exactly the interval in which the
Player sees one position and `currentPosition` reports another.

Same limits as the Phase 0 walk, and one more that matters here: the derived
position is computed in the page, so a slower device is precisely where that
4 ms would grow. Nothing here measures a phone.

## Phase 3 — Take the interactive path off the batch engine's terms

Server-side, in Coach Engine. Two independent changes; either can ship alone.

1. **A warm Stockfish session for interactive evaluations.** Keep one or two
   long-lived single-threaded depth-16 processes reserved for exploration, as
   the batch path already does per review slot. The batch measurement puts the
   gain at 65–73% of summed engine time and the per-call median at 137 ms
   against 439–606 ms. Depth, threads, hash, and the pinned provenance
   constants (`PINNED_STOCKFISH_DEPTH = 16`, `evaluation_recording.rs:14-16`) do
   **not** change — this is process lifetime only. Note the measured caveat: a
   warm transposition table can change exact numeric evaluations at fixed depth
   (wall-time attribution, line 76), so the corpus must be re-proved under the
   existing 15-centipawn tolerance with exact best moves still required.
   *(Superseded by the measurement below: process reuse and table reuse are
   separable, and taking only the first keeps evaluations identical.)*
2. **Give interactive work its own admission class.** An Alternative Move is one
   position; a Game import is hundreds. Sharing a one-slot lease makes the cheap
   operation wait on the expensive one. Add an interactive lease sized from
   Phase 0's queue-wait measurement, leaving the batch lease as it is. This
   changes `admission.rs` only; the compute policy explicitly contemplates a
   bulkhead as "a local `admission.rs` change if production traffic ever
   justifies one"
   ([coach-mcp-compute-policy.md](../research/coach-mcp-compute-policy.md), line 26).
   *(Measured below. The number arrived, and it moved the reason for doing
   this: the same-Player collision is a rejection, not a wait.)*

Also consider, pending Phase 0: whether the durable moment write must block the
evaluation's return, or whether the evaluation can be returned first and the
write acknowledged after — noting that Coach Engine deliberately evicts a
session whose write failed, so this reorder needs its own decision and is not a
free win.

### Item 1 measured and implemented 2026-08-31

Measured first-party on the certified machine against the pinned Stockfish of
unit `0.2.0-local-coach.4`, depth 16, `Threads=1`, `Hash=16`, over twelve
distinct positions (opening, middlegames, endgames, a mate):

| Condition                          | Median | Min | Max |
| ---------------------------------- | -----: | --: | --: |
| Fresh process per call             | 341 ms | 218 ms | 1,261 ms |
| Reused session, table cleared      | 104 ms | 5 ms | 465 ms |
| Reused session, table retained     | 135 ms | 10 ms | 528 ms |

**The saving is process startup, not table warmth — about 237 ms per
interactive evaluation.** Retaining the transposition table across *distinct*
positions bought nothing; the batch path's 65–73% figure came from repeatedly
analysing a contiguous lane, which an exploring Player does not do.

That splits the two things the plan had bundled, and removes the risk it
flagged. Clearing the table with `ucinewgame` before each reused search starts
it from the same empty state a fresh process would, so reuse becomes a timing
change and nothing else: **best move and score were identical to a fresh
process in 12 of 12 positions.** No corpus re-proof is required, and the
15-centipawn tolerance is not being leaned on — the numbers are equal, not
close.

Implemented as a one-session cache on `StockfishAdapter`, shared across its
clones, covering every single-position caller (exploration, opening analysis,
plan projection). One session is retained rather than a pool: interactive
evaluations already serialize on the engine lease, so a deeper pool would not
shorten a queue and would multiply the resident NNUE network. It is released
after 60 seconds idle, so a quiet cell gives the memory back, and a caller that
finds the slot taken starts its own process exactly as before. A session whose
search failed or timed out is killed rather than handed on.

Reproduce the measurement with the pinned binary and these UCI options; the
harness is twelve `position fen` / `go depth 16` pairs against one process
versus twelve fresh ones.

### Item 2 measured 2026-08-31 — the engine lease queue

Measured first-party against hosted staging (`coach-engine`, Railway,
`staging.example`), reading the admission spans the service already emits:
`coach_engine_admission_completion` carries `queue_wait_milliseconds` and
`queue_depth`, `coach_engine_lease_completion` carries
`lease_occupancy_milliseconds`, and the enclosing `review_session_operation`
span names the workload. No new telemetry vocabulary was added, per Phase 0
step 1. The deployed revision is `main`, so these are pre-Item-1 numbers.

**Observed queue wait is zero, and that is not the answer.** Across every
admission in Railway's full retained window plus the runs below — 40 events —
`queue_wait_milliseconds` was `0` and `queue_depth` was `0`. Staging never
contends. The retained window is about thirteen hours, so it cannot be widened;
the queue-wait number had to come from the quantity a waiter actually waits on,
which is how long the batch class holds the single slot.

**Batch lease occupancy, `GameImport`, ten ply-labelled Games:**

| Plies |     Hold | Plies |     Hold |
| ----: | -------: | ----: | -------: |
|    30 | 5,013 ms |   101 | 6,567 ms |
|    35 | 2,970 ms |   150 | 7,517 ms |
|    66 | 6,518 ms |   155 | 7,539 ms |
|    67 | 5,694 ms |   180 | 9,245 ms |
|    93 | 5,784 ms |   218 | 8,862 ms |

Median 6,543 ms, minimum 2,970 ms, maximum 9,245 ms. Three further unlabelled
`GameImport` holds already in the retained window (2,401, 2,836 and 4,892 ms)
sit inside that range. The hold grows with ply count but sub-linearly — 218
plies costs under twice what 30 plies costs — because the batch path already
spreads positions across eight workers.

**Interactive lease occupancy, `AlternativeMoveEvaluation`, nine holds:** 306,
307, 395, 425, 428, 448, 459, 464 and 466 ms; median 428 ms. Idempotent replays
that returned an existing commit held the lease for 0 ms and are excluded.
`ReviewSessionStart` holds 0–4 ms and does no engine work.

**The asymmetry the bulkhead exists to fix is fifteen-fold.** A Game import
holds the one slot for 6,543 ms median; an Alternative Move holds it for 428 ms.
The plan asserted that ratio from operation shape — one position against
hundreds. It is now measured on the hosted runtime, and it is the whole argument
for splitting the class.

#### What the measurement changed

**Rank 4 is mis-stated, and this is the STOP condition firing.** The ranking
calls the shared lease "a tail problem, not a median one; worst case is a
30-second queue deadline." For the case the plan actually describes — a Player
exploring while their own Game import runs — the worst case is not a wait of any
length. It is an immediate rejection. `EngineAdmission` holds a `PlayerClaim`
keyed by Player ID, and a second engine workload for a Player who already holds
one returns `AdmissionLimit` without ever entering the queue
(`admission/engine.rs:111-124`, proved by the existing
`rejects_a_second_operation_for_the_same_player`).

That collision is reachable from the product, not theoretical. Daily Coaching
submits its background imports as `ProcessorPrincipal::Player(player_id)`
(`daily_coaching/reviewer.rs:132`) — the same principal the Player's own board
work uses — and `dispatch_admitted` spawns each command on its own task with no
per-Player serialization ahead of admission
(`review_session_processor.rs:560-565`). A Player who drags a piece while Daily
Coaching is importing one of their Games gets a hard `admissionLimit`, not a
slow move.

**Queuing is therefore strictly cross-Player, and there the 30-second deadline
is close to the bone.** With one slot and FIFO, a waiter at position *p* waits
the holder's remainder plus *p*−1 full holds. At the measured median that is
about 26 s for the fourth waiter — inside the deadline. At the measured maximum
it is about 37 s, past it. Five distinct Players importing long Games
concurrently is enough to make the fifth one time out, so the pool is marginal
rather than comfortable at its documented limits.

#### The size

- **Slots: one.** A single interactive slot removes the batch wait entirely for
  the drag path, and the interactive holds measured here (428 ms median) never
  queue against each other at any plausible staging population.
- **Queue deadline: 5 seconds, not 30.** Four interactive waiters at the
  measured 466 ms maximum model to about 1.9 s. Five seconds is roughly 2.7×
  that worst modelled queue; thirty seconds is the wrong order of magnitude for
  a class whose holds are sub-second, and it would make a doomed interactive
  wait outlast the Player's patience rather than failing honestly.
- **Waiters: four**, unchanged, which the interactive hold makes generous.
- **The `PlayerClaim` must not be shared between the two classes.** This is the
  constraint the measurement surfaced and the one that decides whether the
  bulkhead works at all. If the interactive lease keeps the existing per-Player
  claim, a Player whose Daily Coaching import holds that claim is still rejected
  on their own board, and the new slot buys nothing in exactly the case that
  motivates it. Each class needs its own claim set.

Reproduce by importing Games of known ply count against staging and reading the
two admission events for the operation; the queue-wait arithmetic follows from
the hold distribution and needs no separate contention experiment, because a
one-slot FIFO queue wait *is* the residual of the hold ahead of it.

#### Item 2 implemented 2026-08-31

`EngineAdmission` now holds two classes with nothing shared between them —
neither slot nor per-Player claim (`admission/engine.rs`). `EngineWorkload`
names which one a caller wants: `Interactive` for an Alternative Move
evaluation, `Batch` for a Game import and for Review Moment preparation. Each
keeps one slot and four waiters; the interactive class carries a **5-second**
queue deadline against the batch class's thirty, because four waiters at the
measured 466 ms maximum model to about 1.9 seconds and a doomed interactive
wait should fail while the Player is still watching.

Separating the claim sets is the part that fixes the motivating case, and it has
its own test: a Player already holding a batch lease can now take an interactive
one, where before the shared claim returned `AdmissionLimit`. Optional prefetch
still rides only idle **batch** capacity — it exists to use a quiet engine, not
to compete with a drag.

Both admission events now carry `workload_class`, so the next measurement can
tell the classes apart; that is one added field on an existing event, not new
vocabulary, and the baseline summary reads fields by name and ignores the rest.

`ReviewMomentOpen` deliberately stays on the batch class. It is Player-facing
and a candidate to move, but its lease hold was never measured — every
observation in the retained window hit the prepared cache and took no lease —
and putting an unmeasured operation into the interactive lane would undermine
the thing the lane exists for. Measure it before moving it.

#### Found on the way, and fixed: long Games could not be stored

Two of the ten test imports failed after the engine work finished. Firestore
rejected the commit with `INVALID_ARGUMENT`: `The value of property "payload" is
longer than 1048487 bytes`, on both `game_analysis_put` and `game_import_reuse`,
surfacing to the Player as an `unavailable`/`persistence` outcome that invites a
retry which can never succeed. The 155-ply Game succeeded and the 180- and
218-ply Games failed, so the ceiling sat between them.

Payload sizes read back from staging, against Firestore's 1,048,487-byte field
cap:

| Plies |         Canonical JSON | Share of cap |
| ----: | ---------------------: | -----------: |
|    35 |    276,627 bytes       |        26.4% |
|    67 |    521,122 bytes       |        49.7% |
|   101 |    763,721 bytes       |        72.8% |
|   155 |    976,876 bytes       |        93.2% |

About 6.3 KB per ply, so the cap binds a little past 160 plies. Spreading the
payload across sibling fields cannot help, because Firestore caps a whole
document at 1 MiB as well — the document limit binds before the field limit is
escaped.

`DurablePayload` now stores gzipped canonical JSON in Base64
(`firestore/codec.rs`), which is the one place every durable store already
funnels through. Measured on the same four payloads, the stored field falls to
6.3%, 12.1%, 17.7% and 21.4% of the cap, so the 155-ply Game that nearly failed
now uses a fifth of its budget and the ceiling moves past any legal Game.
Compressing the largest of them costs 11.3 ms against a 6,543 ms import.
Deserialization accepts both encodings, so documents written before the change
still read and nothing needs migrating.

**Verified on staging 2026-08-31**, against the deployed build. Both Games that
had failed now import: 218 plies stores 196,465 bytes (18.7% of the cap) and 180
plies stores 234,109 bytes (22.3%), both carrying the `gzip:` prefix, with no
`INVALID_ARGUMENT` and no persistence failure in the logs. A compressed review
opens a Review Moment with its full proof, so the round trip holds end to end.
The pre-fix 155-ply document is still stored uncompressed at 93.2% of the cap
and still reads through the deployed build, which exercises the compatibility
branch on live data rather than only in a test.

## Phase 4 — Persist the frozen review on the client

This is the second question the Player asked, and the answer is yes — with one
piece of server work first.

**Why the browser is the right place.** `CONTEXT.md:911` records that completed
Alternative Moves are deliberately in neither durable store, because the Review
Analysis Cache is identity-free and shared across Players, so persisting one
Player's exploration would leak it into another's analysis. The browser is the
one location that is Player-scoped by construction. Caching exploration there
does not have that problem — and it fixes the related complaint that
`CONTEXT.md:895` names, that Alternative Move Exploration dies with the process
holding it.

**What is safe to cache, and why.**

| Artifact | Cacheable | Reason |
| --- | --- | --- |
| Frozen `GameReview`, `ImportedGame`, moment display facts | Yes | The Game Import Record is immutable and self-contained after creation (`CONTEXT.md:257`) |
| `AlternativeMoveEvaluation` keyed by position + engine identity | Yes | An identity-free immutable fact; the engine already caches it exactly this way (`ExactEngineCache`) |
| The Player's own exploration tree | Yes | Player-scoped by construction in the browser |
| LLM Review Moment Comments | **Only behind a validator** | Staleness is decided server-side today and the client has no representation of it |
| Anything a Coaching Board Snapshot returns to an agent | Per-page only | The snapshot is the agent's picture of live state; a stale one is exactly the failure ADR 0056 guards against |

**The one server change needed.** Commentary staleness is resolved entirely
inside Coach Engine by comparing a stored `promptDigest` / `responseSchemaDigest`
against the compiled ones (`critical_moment_comment.rs:199-206`), and the
fingerprint never crosses into TypeScript — the client receives only
`{ text }`. So a client cache cannot currently tell whether its stored prose is
current. Expose a cheap validator: a content digest on the review read that
folds in the comment prompt and schema digests, so the client can send what it
holds and be told `unchanged` or be given the delta. `ReviewSessionRevisionDelta`
(`{ priorRevision, resultingRevision, changedMomentIds, fullRefreshRequired }`)
is already a generated contract type and is the natural shape for the answer.

**Storage.** IndexedDB, keyed by the same identity the engine uses: the review
key already hashes `REVIEW_DURABILITY_SCHEMA_VERSION` and
`REVIEW_ANALYSIS_GENERATION` (`review_durability.rs:47-56`), so a bump is a miss
by construction rather than a stale hit. Mirror that in the client key. HTTP
caching cannot be used and should not be attempted: review data arrives over
`POST` NDJSON, and every `/api/**` response is forced `cache-control: no-store`
at the proxy (`apps/central-host/server.ts:264`).

**Privacy obligations, non-negotiable.** Reviewed Games are Player data on a
possibly shared device. The cache must be namespaced by Firebase `uid`, purged
on sign-out and on account deletion, and must never hold anything the telemetry
allowlist forbids. `CONTEXT.md:905`'s "remembers nothing between visits"
sentence must be amended in the same change, not silently contradicted — and
the amendment should say plainly that bytes are cached while navigation state
is not. Note the repo already has two accounts on staging with different uids;
namespacing is not theoretical.

## Phase 5 — Client-side WebAssembly Stockfish, conditional

**Recommendation: do not start this until Phases 0–3 have shipped and been
measured.** It is sixth on the ranked list, it is the only phase that needs a
new dependency, and it is the only one carrying a licensing decision the project
has not yet made. Phases 1–3 remove serialization, process churn and queue
waits — after they land, the remaining engine time may not be worth a
multi-megabyte download.

Three gates must be cleared *before* any implementation, in this order.

**Gate 1 — Licensing.** Stockfish is GPL-3.0. Today it is a separately
downloaded server-side binary invoked over UCI; the project redistributes
nothing (`runtime/THIRD_PARTY_NOTICES.md:5-7`). Shipping Stockfish WASM into the
browser bundle means **redistributing GPL-3.0 code inside the client artifact**,
which makes the distributed combined work GPL. Issue #521 has the root `LICENSE`
still undecided between AGPL-3.0 and Apache-2.0/MIT, with a Sep 3 deadline. If
the project picks Apache-2.0 or MIT, this phase is closed until someone reopens
the licensing question. **This gate must be resolved by the #521 decision, not
by this plan.**

**Gate 2 — The fact boundary (D2).** A browser-computed evaluation is
provisional presentation and nothing else. Concretely: it is rendered in a
visually distinct provisional state; it is never written to IndexedDB as if it
were engine output; it never appears in a Coaching Board Snapshot, a WebMCP tool
result, a Grounding Ledger, or any prose; and it is replaced the moment Coach
Engine answers. If it disagrees with the engine, the engine wins silently — the
Player is never shown two numbers. This needs an ADR before code, because it
adds a second producer of evaluation-shaped values to a codebase whose whole
grounding architecture assumes one.

**Gate 3 — Cross-origin isolation. Solved by build choice; verify it stays
solved.** Details and measurements in
[browser-stockfish-wasm-feasibility.md](../research/browser-stockfish-wasm-feasibility.md).
Multi-threaded WASM needs `SharedArrayBuffer`, hence
`Cross-Origin-Opener-Policy: same-origin` plus `Cross-Origin-Embedder-Policy`.
That is not available to us: `COOP: same-origin` breaks Firebase
`signInWithPopup` structurally, Firebase has declined to fix it
([firebase-js-sdk#6467](https://github.com/firebase/firebase-js-sdk/issues/6467),
open since 2022), `same-origin-allow-popups` does not grant isolation,
`credentialless` does not exist in Safari, and `Document-Isolation-Policy` is
Chrome-desktop-only. The `/app/` surfaces sign in through Firebase and proxy
`/__/auth/` (`production.ts:33-46`).

**So: single-threaded only, and never isolate the authenticated shell.** That
costs far less than assumed — the single-threaded lite build measured
**789k–1,018k nps and depth 27+ in 8 s**, faster than a non-PGO native binary on
the same machine, and the "WASM Stockfish is 2–3× slower" folklore did not
reproduce. If threading is ever wanted, isolate one analysis route the way
chess.com does, never `/app/`.

If all three gates clear, the shape is: `stockfish-18-lite-single` from npm
`stockfish` — one 7.3 MB file (5.6 MB gzip) with the network embedded, no
separate net download, no headers — in a Web Worker, loaded lazily on first
drag, producing a provisional evaluation rendered as provisional and superseded
by Coach Engine's authored depth-16 result.

Two things to size honestly. **The dominant cost is transfer and
instantiation, not search**: depth 12 lands in ~72 ms and depth 15 in ~161 ms on
a laptop, so the 5.6 MB download is the whole budget. And **there is no mobile
evidence at all** — no published time-to-depth benchmarks for WASM Stockfish
appear to exist, on any device. Measure on a real mid-range phone before
committing, because that is the device this would help most and the one nothing
is known about.

Note the prior art runs the other way than assumed. Chess.com does split
lite-local against full-NNUE-server, on exactly this asset-size boundary.
Lichess does not: it arbitrates client and server evaluations by node count, and
its browsers *populate* the server's eval cache.

## The Coach App gets the server phases only

Everything client-side above is unavailable to `apps/coach-app`, for reasons
that are structural rather than incidental.

- **No persistence exists or is portable.** There is zero browser-storage usage
  in the widget, and that is deliberate: `setWidgetState` is ChatGPT-only and
  the MCP Apps MVP defers state persistence
  ([mcp-apps-cross-host-contract.md](../research/mcp-apps-cross-host-contract.md), line 130).
  The widget's whole rehydration is `render(snapshot, selection)` from one
  addressed immutable resource (`CONTEXT.md:904`). Phase 4 cannot cross this
  line.
- **No WASM can be bundled.** Artifacts are single-file HTML with every asset
  inlined (`vite.config.ts:52`), and `verifyBundle.ts:20-27` forbids external
  `<script src=`, external stylesheets, and nested iframes — so a separate
  `.wasm` fetch is impossible by gate. The current artifact is already
  353,912 bytes gzip against a 153,600-byte objective, over by 200,312. Phase 5
  cannot cross this line either.
- **Its round trips are already fused.** The widget's `explore_alternative_move`
  is one tool call in which the server performs inspect → explore → inspect
  (`apps/central-host/server/coach-app/exploration-tools.ts:106-158`). The
  widget was never paying the web's amplification. Phase 1 is, in effect, the
  web catching up to the widget.

What the Coach App *does* get: Phase 3 in full, because warm engine sessions and
interactive admission are server-side and shared. That is the correct split, and
it should be stated in the phase acceptance rather than discovered later.

## Considered and rejected

**Caching review responses with HTTP headers (`ETag` / `Cache-Control`).**
Rejected. Review data arrives over `POST` NDJSON, which HTTP caches will not
store, and the proxy forces `no-store` on every `/api/**` response. An
application-level cache with an explicit validator is the only shape that works,
and it is also the only shape that can express the commentary-staleness rule.

**Caching the Coaching Board Snapshot across page loads.** Rejected. The
snapshot is the agent's picture of live board state, and ADR 0056 carries it on
every board-tool result specifically so an agent cannot coach from a stale one.
A persisted snapshot reintroduces exactly the failure the design paid for.

**Making `evaluate_player_line` incremental.** Rejected. Re-walking from the
moment is correct for its actual caller — an agent proposing a line it has not
walked — and its schema, description and refusal vocabulary are signed. Fix the
Player's drag, which is a different operation (D1).

**Speeding up Stockfish first.** Rejected as the opening move. It is sixth on
the ranked list. Plan 004 said the same thing in its "do not optimize first"
list, and it is still true.

**Persisting Alternative Move Exploration server-side to survive a reload.**
Rejected. `CONTEXT.md:911` gives the reason: the shared analysis cache is
identity-free, so one Player's exploration would leak into another's analysis.
Phase 4 puts it in the browser instead, which is where Player-scoped state
belongs.

## Risks

- **Optimistic placement diverging from engine truth.** Mitigated by D2/D3: the
  client projects a *position*, which is deterministic from FEN plus a legal
  move, and never an *evaluation*. A divergence in position projection is a
  `chessops` bug and should fail loudly in the reconciliation step, not be
  papered over.
- **Phase 3's warm sessions changing evaluations.** Measured and real: a warm
  transposition table changed exact numeric evaluations at fixed depth on the
  batch path. The corpus must be re-proved; the tolerance and the exact-best-move
  requirement do not move.
- **Phase 4 caching Player data on a shared device.** Addressed by uid
  namespacing and sign-out purge, which must be in the same change, not a
  follow-up.
- **This plan touching files another session is editing.** `review_session_exploration.rs`
  and `mutation.rs` changed today under `28b6745d`, `4cac527f` and `d391abbc`.
  Sessions share this working copy. Rebase before starting Phase 3 and do not
  assume this plan's line numbers survive.

## Unresolved questions

1. Root `LICENSE`: AGPL-3.0 or permissive? Phase 5 is closed under a permissive
   licence. (#521, Sep 3.)
2. ~~Does the durable moment write have to block the evaluation's return?~~ —
   worth closing rather than answering. Measured 2026-08-31 at **48 ms
   median**, against the ~303 ms the plan carried. Reordering it buys a
   twentieth of a board move and costs a decision about a session whose write
   failed.
3. ~~Interactive lease size~~ — settled and implemented 2026-08-31: one slot,
   four waiters, 5-second queue deadline, with a claim set of its own. See
   "Item 2 measured" and "Item 2 implemented". Two things it leaves open:
   whether `ReviewMomentOpen` belongs in the interactive class, which needs its
   lease hold measured first; and whether the split claim needs recording as a
   decision of its own, since it changes when a Player is told no.
4. ~~Does Phase 4's client cache also hold the Player's exploration tree across
   reloads?~~ — **no**, decided 2026-08-31. Only the frozen review is cached;
   Alternative Move Exploration still dies with the process. The board's
   *position* does survive a reload, carried in the address as `?ply=` rather
   than remembered, so one address still means one thing.
5. ~~Who owns amending `CONTEXT.md:905` and ADR 0058's fact-boundary
   consequence?~~ — split, 2026-08-31. This plan amended `CONTEXT.md:905` in
   the same change that cached bytes, as the plan required. The grounding half
   went to its own decision,
   [ADR 0060](../adr/0060-show-a-provisional-move-and-say-it-is-provisional.md),
   because a provisional move is a snapshot question rather than a caching one.
   ADR 0058's fact boundary was not disturbed: a derived position is not an
   authored fact.
6. ~~Is the 126-command rate-limit collision real?~~ — **no.** Measured
   2026-08-31: a twelve-move walk costs 17 typed commands, not 126, against a
   120-per-minute limit.
