# Mint opening exploration branches in the web from stateless analysis

## Status

Accepted (2026-08-28). Implemented under
#520; the spec delta
below is applied.

This decision completes ADR 0057's opening root with the one piece
#493 deliberately
deferred: the web tool that turns an evaluated continuation into exploration
branches on the opening Coaching Board. It builds on ADR 0056 (the Coaching
Board) and ADR 0057 (the stateless identity-free root).

**Amended 2026-08-31 in `c67d1194`.** `CoachingBoardExplorationBranch` now
carries a required `positionRef` on `resultingPosition`, so the field list in
the Decision below, and the type it quotes, are narrower than the shipped type.
The decision itself is unchanged: branches are still built in the page, and the
engine still returns no `PositionSnapshot`.

The field is needed because parenting a Player's next move onto the branch the
board stands on requires that branch's resulting position reference. Without
it, the game board had to re-establish the moment root and re-walk the whole
line on every drag — `k + 4` round trips at the k-th move. A game branch
already carried the reference as an `AlternativeMoveResult`; an opening branch
mints one with the existing `positionRefForFen`, which this decision already
relies on for a branch's root parent, so no new kind of web-minted identifier
appears. It is required rather than optional so the game path has no
unreachable "branch without a reference" case to guard.

## Context

An agent on the opening Coaching Board can navigate and read, but nothing
produces exploration branches there. The surrounding machinery is finished
and waiting:

- **Engine**: `POST /api/v1/opening-lines/analysis` — authed, stateless,
  twelve-ply cap, typed `illegalMove`/`plyLimitReached` verdicts that retain
  the evaluated prefix, per-Player rate limit. The continuation is rooted at
  the **line's end position**: the route parses the catalog path, builds the
  final position, and walks the supplied moves from there.
- **SDK**: `CoachEngineClient.analyzeOpeningLine` returns
  `OpeningAnalysisOutcome` — per-ply `OpeningAnalyzedPly` carrying `moveUci`,
  `mover`, a full `AlternativeMoveEvaluation` (`selectedMove`, `bestMove`,
  `bestMoveUci`, `comparison`), and `resultingFen`. Kept deliberately without
  a web caller for this tool.
- **Web**: exploration is retained per Opening Line (bounded five, oldest
  evicted); the drive state, snapshot projection, and the
  `show_line`/`set_board_position` targets all consume branches.

Three gaps stood between the route and the board:

**The shapes disagree.** Drive-state branches are contract
`AlternativeMoveResult`s: `branchRef`, `parent`, and a `resultingPosition`
that is a full `PositionSnapshot` — `positionRef`, castling rights, en
passant, repetition state, status, `historyDigest`. The analysis route
returns `OpeningAnalyzedPly`, whose position is one FEN. Either the engine
grows a snapshot-shaped payload, or the web closes the gap.

**Nothing mints branch identities.** Game-side branches get their
`AlternativeMoveId` and `BranchRef` from the Review Session actor. The
opening root has no actor by design, so ids must come from somewhere else.

**`evaluate_player_line` cannot serve**: its schema is keyed by
`gameImportId` and a Review Moment, which an opening board does not have.

What the drive actually consumes is much narrower than the contract shape.
Every read of a branch in `coachingBoardDrive.ts` and
`coachingBoardSnapshot.ts` touches only: `alternativeMoveId`, `branchRef`,
`parent`, `moveUci`, `evaluation.selectedMove`, and
`resultingPosition.{fen, occupied, sideToMove}` (since amended to add
`positionRef`). And the web already derives
positions from moves with chessops (`openingLineMoves`,
`presentationPiecesFromFen`) and already mints identifiers where the engine
offers none — `PositionRef`s from FEN hashes in `openingMoves.ts`,
`OperationId`s and `IdempotencyKey`s from deterministic fingerprints in
`coachingBoardCoachTools.ts`.

## Decision

**Add one web-only board tool: `evaluate_opening_continuation`.** It joins
the one authored map (`coachToolSurface`) with target `["web"]` and web kind
`"board"`. The derived model-visible list does not change, so the standing
MCP model-list lock is untouched. Like the other board coach tools it
registers on every board surface and answers a game-origin board with a
typed unavailable result carrying the snapshot.

**Input is the board's own line plus a continuation from its end.**

```json
{
  "openingLineRef": "<eco>-<name-slug>-<digest4>",
  "continuation": [{ "kind": "san", "san": "..." } | { "kind": "uci", "uci": "..." }]
}
```

`continuation` is one to twelve plies, both sides supplied — the route
analyzes every ply, so there is no `opponentReplies` knob. `openingLineRef`
must equal the board origin's ref; a mismatch is a typed refusal with the
snapshot, catching an agent whose picture of the board is stale. There is no
`from` selector: the route roots at the line's end, and a deeper branch or a
sibling is expressed by re-walking the shared prefix, which deterministic
ids dedupe on the page and the position-keyed Opening Analysis Cache dedupes
on the engine. A mid-line deviation (before the line's end) is out of scope
for v1; the Player reaches an earlier root by opening the shorter catalog
line, which find already surfaces. Because the catalog is not prefix-closed,
that workaround is not guaranteed, so the tool description and the refusal
must both state line-end rooting explicitly — the agent's repair is routing
the Player to a shorter line, never retrying. An optional `fromPly`
truncation stays additive later if usage shows mid-line asks are real; the
cache is position-keyed, so truncation would cost it nothing.

**The web builds the branches; the engine and the contract do not change.**
Narrow the branch type the drive consumes to a structural
`CoachingBoardExplorationBranch`:

```ts
{
  alternativeMoveId: AlternativeMoveId
  branchRef: BranchRef
  parent: BranchParent
  moveUci: string
  evaluation: AlternativeMoveEvaluation
  // Since amended to add `positionRef: PositionRef` — see Status.
  resultingPosition: { fen: string; occupied: readonly OccupiedSquare[]; sideToMove: Color }
}
```

`AlternativeMoveResult` is assignable to it, so the game path and the
retention container retype without behavior change. Each analyzed ply
becomes one branch node chained by `parent` — the first ply's parent is
`root(positionRef)` over the line-end FEN, each later ply's is
`move(branchRef)` of its predecessor — exactly the shape the exploration
tree, `pathFromRoot`, and the drive targets already handle. `occupied` and
`sideToMove` derive from `resultingFen` through the existing pure FEN
projection, as does the amendment's `positionRef`. A partial verdict (`illegalMove`, `plyLimitReached`) still mints
the evaluated prefix and reports the verdict in the result.

The rejected alternative — the engine returning full `PositionSnapshot`s —
was heavier everywhere and lighter nowhere: a generator-published contract
round-trip, engine work computing castling, repetition, and history digests
no consumer reads, and a fatter payload on a rate-limited route, all to
avoid a FEN projection the web already performs for the line itself.

**The web mints ids, deterministically.** `alternative-move:web-opening-<fp>`
and `branch:web-opening-<fp>`, where `<fp>` is the existing web-fingerprint
hash over `(openingLineRef, uci path from the line end)`. The branded
constructors admit any prefixed string, so these are well-formed contract
ids that no engine id can collide with. Determinism means re-evaluating a
continuation converges on the same nodes instead of duplicating them, and a
retention recall five lines later still addresses the same branches.

**Evaluation is agent-callable without Player confirmation.** Decision 3's
reading holds: the tool writes nothing durable — no import, no comment, no
vote — and its compute is bounded twice (twelve plies, per-Player rate
limit). It is the opening board's instance of the evaluate-then-show gate:
evaluation activates no branch and moves no board; showing is a separate
`show_line` or `set_board_position` call. A tripped rate limit returns the
typed retry directive. Annotations: `idempotentHint: true`,
`readOnlyHint: false` (it grows page exploration state).

**The drive targets need nothing new.** Once branches with real ids sit in
drive state, `show_line { kind: "alternativeMove" }` and
`set_board_position { kind: "alternativeMove" }` resolve them through the
existing lookup, satisfying Decision 8's "a node of the exploration tree."

**The tool facts echo the evidence of this call.** The result carries the
line identity, the root evaluation, the verdict, the new branch ids, and the
per-ply `AlternativeMoveEvaluation`s — `selectedMove`, `bestMove`,
`bestMoveUci`, `comparison` — for the plies of this call only, plus, per
Decision 13, the updated Coaching Board Snapshot in which the new branches
already appear. The snapshot's branch list stays lean (selected-move
evaluations only): the comparison is the aggregate the agent's coaching talk
needs, it was computed engine-side this call, and per Decision 12 evidence
travels in the result that produced it rather than fattening every snapshot
on every board-tool result. The root evaluation has no snapshot seat at all;
the facts block is its only home.

## Consequences

Opening exploration branches are page state built from engine evidence, not
engine artifacts. The engine remains the only author of chess facts — every
FEN and evaluation in a branch is route output — while the web owns tree
shape and identity, as it already owns them for retention. If a future
surface needs engine-authored opening branches (say, a durable shared
study), that is a new decision, not a widening of this one.

Branch count per line is unbounded in v1. The rate limit bounds accumulation
speed, page lifetime bounds duration, and retention bounds line count at
five; no separate web cap is added until measurement says otherwise.

The web tool name lists grow by one, so the deliberate oracles pinning them
(`useCoachingBoardTools.test.tsx`, `coachingBoardCoachTools.test.ts`, and
the drift assertion against the authored map) must be updated in the same
change — that is the drift gate working, not a migration.

The opening board surface must start carrying an access token: today
`CoachingBoardOpening` only performs the unauthenticated resolve read, and
`analyzeOpeningLine` is authed.

## Spec delta

Applied to `docs/spec/coaching-board.md`:

- **Decisions table, new row 15**: `evaluate_opening_continuation` is the
  opening board's evaluate-then-show gate — web-only, rooted at the line's
  end, both sides supplied, branches minted web-side with deterministic ids,
  agent-callable without confirmation because it writes nothing durable and
  is bounded twice.
- **The snapshot / Architecture**: note that opening exploration branches
  are web-built `CoachingBoardExplorationBranch`es and that game and opening
  branches share one consumed shape.
- **Validation, deterministic tier**: branch minting determinism, prefix
  dedupe, mismatch refusal, game-origin refusal, and partial-verdict prefix
  retention join the vitest suite; the boundary tier already covers the
  route.

## Alternatives within the shape

**`explore_opening_continuation`.** Rejected. "Evaluate" is the gate's
vocabulary — the spec says "Evaluate, then show" — and the parallel with
`evaluate_player_line` teaches the agent the same contract. "Explore" is
taken by `explore_alternative_move`, the app-only Review Session command
whose actor and allowance semantics deliberately do not exist here.

**A `fromPly` truncation in v1.** Rejected for now, not forever. It is the
only way to express a mid-line deviation, and the shorter-catalog-line
workaround is not guaranteed because the catalog is not prefix-closed. But
adding it costs an engine and generated-contract change to serve a case no
measurement has yet shown occurs, and it stays additive afterwards. Ship on
the deployed route, state line-end rooting in the description and the
refusal, and let usage evidence decide.

**Snapshot-only evaluations, with no per-ply echo.** Rejected. The snapshot's
branch list carries `selectedMove` alone, dropping the `comparison` the
coaching talk needs, and the root evaluation has no snapshot seat at all.
Fattening `CoachingBoardBranch` instead would charge every board-tool result
for evidence one call produced.
