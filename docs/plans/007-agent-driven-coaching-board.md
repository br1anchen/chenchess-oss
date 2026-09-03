# Plan 007: Let the coach point at the board, and play a line out

> Read against `main` at `37880f34` on 2026-08-30. Prompted by a live ChatGPT
> session on `staging.example` in which the agent could describe
> `10.Bd2` vs `10.Ke2` in prose, correctly, and could not make the board show
> either of them.
>
> Companion to [ADR 0056](../adr/0056-coach-the-web-board-through-site-registered-tools.md)
> and [the Coaching Board spec](../spec/coaching-board.md), which establish the
> surface this plan extends. Sequenced against
> [Plan 006](./006-speed-up-move-exploration-and-review-loading.md), which owns
> the latency of the exploration path both features ride on.
>
> **Deadline context.** #521
> puts the WebMCP Challenge at Sep 3, 2026 1:00 PM PDT, with a live URL and a
> demo video as the deliverable. Annotation therefore comes before playback:
> similar build cost, and an arrow appearing on the live board mid-sentence is
> the thing that has not been seen before.

## Outcome

A Player who asks "what does that knight actually hit?" sees the knight's
targets drawn on the position they are looking at — verified against the board,
or refused because the relation is not there. A Player who asks "why is
stepping the king better?" gets the answer twice: as prose in ChatGPT, and as
the board walking the line move by move.

No new authority over chess facts moves out of Coach Engine. Everything this
plan draws is either a fact ChenChess already returned or a geometric property
of the current position that the page can check for itself.

## Evidence labels

**Read from this working tree (high confidence):** every `file:line` pin below,
every type shape, the `chessops` API surface, the tool surface map, and which
component renders which control. These were read, not remembered.

**Measured previously and reused:** the deixis reliability numbers from
#489 — 7/8 on the scripted
suite, with the non-deictic trap over-calling on roughly one run in five. The
exploration round-trip counts from Plan 006.

**Observed once, not measured:** the ChatGPT session in the prompt. One session
is not a rate; it is a reproduction of a defect whose cause is in the code
below.

**Inferred, not measured:** that annotation will reduce the confident-wrong
failure mode. Nothing here has been in front of a Player.

## The finding

Three separate defects sit behind one symptom. They have different causes and
different fixes, and only the first is a wiring mistake.

### A. Inside a branch, the Coaching Board renders exactly one control

`BoardWorkspace` draws its own navigation block at
`apps/central-host/src/review-session/BoardWorkspace.tsx:267`, and passes
`BoardNavControls` neither `onStrongestReply` nor `strongestReplyLabel`. The
button that previews the engine's answer is gated on all three being present
(`BoardWorkspace.tsx:555`). Only `ReviewMoveControls` supplies them
(`BoardWorkspace.tsx:416`), and only the Review Session renders that
(`ReviewSessionWorkspace.tsx:1556`, `:1584`).

The explored-branch list has the same shape: `ReviewBranchControls` exists at
`BoardWorkspace.tsx:457` and is rendered from exactly one place,
`ReviewSessionWorkspace.tsx:1504`. `CoachingBoardChosenGame` imports neither.

So on the Coaching Board, entering a branch produces **Exit branch** and
nothing else. The strongest reply is not missing data: it rode in on the branch
(`CoachingBoardExplorationBranch.strongestReply`, `coachingBoardSnapshot.ts`),
it is already in the snapshot the agent reads, and it is already drawn as the
board's one arrow (`coachingBoardDrive.ts:284`). It has no button.

The move strip has a third instance of the same gap: inside a branch,
`GameMoveButton` blanks the current-step marker outright —
`const viewed = !branch && viewedPly === move.ply`
(`BoardWorkspace.tsx:724`) — and the branch's own moves never appear in the
strip at all. A Player in a branch reads a move list that does not describe
where they are.

**None of this needs a new tool, an engine change, or a contract change.**

### B. The agent has no way to point at anything

`BoardArrow` is `{from, to, label, tone}` with
`tone: "engine" | "peer" | "candidate"` (`packages/ui/src/contracts.ts:66`).
`BoardArrowLayer` draws lines and arrowheads only — there is no square-mark
primitive anywhere in the board stack, and `InteractiveChessboardGrid` takes
`destinations`, `selectedSquare`, `lastMove` and `arrows`, with no general mark
channel (`InteractiveChessboardGrid.tsx:35-64`).

On the Coaching Board, arrows are derived and capped at one:

```
arrows={boardArrowsFrom([engineMoveArrow(engineArrowUci(state))])}
```

— `CoachingBoardChosenGame.tsx:180`. And `engineArrowUci` returns `undefined`
whenever a line is shown (`coachingBoardDrive.ts:284`), so showing a line
actively removes the only arrow the board had.

"This knight hits the rook and the queen", "your rook owns the d-file", "that
pawn is what blocks the bishop" — the three most useful things a coach says
about a position — have no rendering path at all.

### C. `show_line` paints one arrow; it cannot play a line

`HostTurnShowLine` is a closed three-variant union
(`packages/coach-engine-sdk/src/HostTurnShowLine.ts:10`), and that closure is
deliberate — the spec's "Driving limits" section is explicit that the agent
must not be able to express an invented line. The closure is not the problem.

The problem is what the board does with an accepted line. `shownLineMoveUci`
(`coachingBoardDrive.ts:324`) reduces it to a single UCI, which becomes one
arrow and a caption chip. Meanwhile the Review Moment carries
`GameReviewObjectiveLines = { best: GameReviewLineMove[], refutation:
GameReviewLineMove[] }` — an ordered array of `{uci, san}` — and only `[0]`
ever reaches the board (`coachingBoardDrive.ts:284`, `:457`).

The whole line is already in the page. It is being thrown away.

There is a second half to this. `lineRender` and `viewedLines`
(`coachingBoardDrive.ts:433`, `:457`) key off `momentByPly.get(viewedPly)` —
the Review Moment on the *game's* ply. Inside a branch, `engineBest` therefore
still means "the moment's best line", not "the engine's best from the position
on screen". Asking for the engine line from an explored position is not
answerable in the current vocabulary.

And the asymmetry that makes this worth fixing rather than explaining away:
`MoveSequenceSnapshot` already exists —

> One canonical continuation played out ply by ply, addressed by its kind.
> — `packages/coach-engine-sdk/src/MoveSequenceSnapshot.ts`

with `kind: "engineBest" | "playedMoveRefutation"`, a board per move, and a
title. `render_move_sequence` is on `["model", "app"]`
(`tool-surface.ts:78`). **The Coach App can step a line out. The Coaching
Board, which has a real board on screen, cannot.**

## The design

The gate in the spec is *evaluate, then show*: an unevaluated line has no
render option, so it cannot reach the board. This plan keeps that gate exactly
and adds one sibling for the new capability.

**Annotation is verify-then-draw.** The page owns the *geometry* of the
position on screen; Coach Engine keeps sole authority over *evaluation*. A mark
the agent asks for names a relation — attacks, defends, controls — and the page
checks it against the current FEN before drawing a pixel. A relation that is
not on the board comes back as a typed refusal, not as an arrow.

That split is the whole argument, and it is defensible because the two claims
are different kinds. "Is the knight on f3 attacking d4?" is answerable from the
board with certainty and zero round trips. "Is that good for you?" is not, and
stays where it already lives.

`chessops` 0.15.0 is already a `central-host` dependency and already used in the
page (`openingMoves.ts:1`). It exports `attacks(piece, square, occupied)`,
`ray(a, b)` and `between(a, b)`. Every relation below is one call against a
`SquareSet`: microseconds, in-page, no allowance spent, no engine touched.

**Playback is a view, not an authorship.** A line the board plays out must
already be a line ChenChess returned: a Review Moment's `objective.lines`, or a
path through the retained exploration tree. Nothing new is authored, so nothing
new needs grounding. `show_line` keeps its closed union and its
generated-from-Rust shape is not touched.

## Phases

### Phase 0 — wire the controls that already exist

No new tools, no contract change, no deixis risk.

1. Pass `onStrongestReply` and `strongestReplyLabel` from the Coaching Board's
   branch state into `BoardNavControls`. The data is already on
   `branch.strongestReply`; the SAN comes from `playerVisibleSanFromLegalUci`
   against the branch's resulting position, which `CoachingBoardChosenGame`
   already imports for the heading.
2. Render `ReviewBranchControls` on the Coaching Board, fed from
   `snapshot.exploration.branches`, so every explored alternative is one click
   away and the active one is marked. `set_board_position` already accepts an
   `alternativeMoveId`, so the agent and the Player reach the same list by two
   routes.
3. Make the strip describe a branch. Splicing branch moves into the Game's own
   strip was rejected on contact: the strip is the Game, and a branch is a
   different line, not a continuation of it. Instead the branch path gets its
   own row beneath, and the Game's strip stops blanking its current marker
   (`BoardWorkspace.tsx:724`) so it names the ply the branch departed from.
   Two rows, each honest about what it lists.

Verification: extend `CoachingBoardBoard.test.tsx` and
`CoachingBoardExploration.test.tsx`. Before/after screenshots to the user —
there are no styling gates, deliberately.

**Ship Phase 0 on its own PR.** It changes no contract and no tool surface; the
later phases change the snapshot every board tool result carries. Mixed
together, a reviewer cannot tell wiring from contract.

### Phase 1 — board annotation

One new web-only, board-kind tool: `annotate_board`.

**Input.** `{ revision: number, marks: Mark[] }`, at most six marks. `revision`
is the snapshot revision the agent believes it is annotating; a mismatch is
refused, so a mark can never describe a board that has moved on.

| Mark | Shape | What the page verifies with `chessops` |
| --- | --- | --- |
| `attacks` | `{from, to, label}` | `attacks(pieceAt(from), from, occupied)` contains `to`, and `to` holds an enemy piece |
| `defends` | `{from, to, label}` | as above, `to` holds a friendly piece |
| `multiAttack` | `{from, targets[], label}` | ≥2 targets, all enemy, all in `from`'s attack set |
| `controls` | `{from, to, label}` | `pieceAt(from)` is a slider, `ray(from, to)` covers it, `between(from, to)` is empty |
| `square` | `{square, label}` | the square exists; asserts no chess relation |
| `move` | `{uci, label}` | the UCI names a move already grounded: a `linePlayback` step, a branch move, or the active branch's strongest reply |

`multiAttack` is deliberately not called `fork`. Geometry can verify that a
knight attacks two enemy pieces; it cannot verify the fork is worth having,
because forking two pawns passes every check above. Naming the kind for what is
checked keeps the verified claim and the drawn claim identical. The word "fork"
lives in the `label`, which is prose, governed by the constraints block the
result already carries — exactly as the rest of this surface works. Say that in
the tool description rather than implying the check is stronger than it is.

Typed refusals: `relationNotOnBoard`, `moveNotGrounded`, `tooManyMarks`,
`staleRevision`. Refusals return the snapshot, like every other board refusal
(`driveRefusal`, `coachingBoardDrive.ts`).

**Lifetime.** Marks belong to exactly one position. Any revision bump — the
Player moves, the agent sets a position, a line steps — clears them. A stale
arrow describing a different board is the one failure this feature could
introduce, and clearing on revision removes it by construction rather than by
care.

**Rendering.** A `BoardMarkLayer` beside `BoardArrowLayer`: square tints for
`square`, lines for everything else. `multiAttack` draws an arrow per target
rather than tinting them — tints would show *what* is hit and lose the piece
doing the hitting, which is the whole claim. The tone union gains `coach`,
with its ink beside the others in `theme/chenTokens.css`, and is renamed
`BoardInkTone` now that squares carry a tone too. A `describeBoardMarks`
joins `describeBoardArrows` in the accessible announcement.

An agent's assertion must not be visually indistinguishable from the Player's
own exploration, which is what reusing `candidate` would do — and ADR 0056's
stated top risk is a Player who cannot detect bad coaching. Provenance ink is
the only visual defence the board has. One new tone, not two; a good/bad split
waits until something asks for it.

**A legend is not optional.** The Coach App already renders one
(`apps/coach-app/src/momentPresentation.ts:13`). Without labels under the
board, a Player sees coloured arrows in one window and prose in another and has
to fuse them. Render the legend from the same marks that drew.

**Why a tool and not a field.** Marks clear on every revision bump. If marks
rode only on `show_line` and `set_board_position`, annotating a position that
is *already* correct — the common case, since the Player just asked about what
they are looking at — would need a no-op `set_board_position` to the current
ply. That bumps the revision, clearing the marks it just set, and it lies to
the `revisionChangedBy` signal proposed below. A field cannot express "point at
this, do not move anything." If Phase 3 measures the agent dropping the
show-then-annotate pair, *add* an optional `marks` field to `show_line` as an
atomic convenience; adding it later is cheap, removing a tool is not.

**ADR.** This phase carries a new ADR — the page is the authority on the
geometry of the position on screen; Coach Engine remains the sole authority on
evaluation. That is a different decision from ADR 0056, which governs who may
call the board's tools and how grounding policy travels without an instructions
channel. It has its own consequences: a second source of chess truth in the
repository, `chessops` becoming load-bearing in the page rather than
incidental, and refusal semantics for a claim that is not on the board.
Retrofitting it into 0056's Context would rewrite an accepted decision; add
0059 and a one-line forward pointer from 0056 instead.

### Phase 2 — line playback

A shown line gains a cursor, and the cursor is drivable by both the Player and
the agent.

**Snapshot.** `CoachingBoardSnapshot` gains

```
linePlayback: {
  steps: readonly { san: string; uci: string }[]
  index: number          // 0 = the position the line starts from
  source: "engineBest" | "playedMoveRefutation"
} | null
```

Steps carry SAN and UCI only. The page derives each intermediate position with
`chessops` and reports the current FEN through the snapshot's existing
`currentPosition`. **Do not put a FEN on every step** — every board tool result
carries the whole snapshot (ADR 0056), and a twelve-ply line of FENs would tax
every unrelated call.

**Where steps come from — nothing new is fetched.**

| Source | Already in the page | Rooted at |
| --- | --- | --- |
| `engineBest` | `moment.objective.lines.best` | the viewed ply |
| `playedMoveRefutation` | `moment.objective.lines.refutation` | the ply **after** it |

**The exploration path is deliberately not a third source**, though it was
planned as one. A cursor over it cannot work: the path is *derived from the
selected node*, so the index is always its end — forward is a permanent no-op,
and backward re-selects a shallower node, which shortens the path the cursor
is walking and throws the rest of the line away. The capability already exists
in a form that does work: the branch strip for the Player, an Alternative Move
target for the agent. Two ways to walk one tree, one of them broken, is worse
than one that isn't.

The two Review Moment lines root one ply apart, and this is not a detail: the
best line *replaces* the move at this ply, so it starts before it; the
refutation *answers* the move that was played, so it starts after it. Walking
either from the other's root is a sequence of illegal moves — the fixture's
refutation opens `1...e5`, which is not legal from the position the best line
starts in. A line the board cannot root offers no transport at all; the arrow
still names its first move.

Two consequences of deriving positions rather than being handed them. An
exploration path needs no derivation — its nodes already carry the positions
the engine returned — so stepping one selects that node, which is the same
transition the Player's branch chips use. And a derived FEN names an
en-passant square only where a capture could actually be made, so it can read
`-` where an engine FEN for the same position says `e3`. Same position, two
spellings; anything comparing board FENs by string across the two sources will
be wrong.

**UI.** A step control (`‹ ›`, first/last) under the board whenever
`linePlayback` is non-null, with the step's SAN and the line's moves visible.
The Player drives it without asking the agent for anything.

Board interaction is off while a line is being walked. The board is then
showing a position off the Game's own line, and a piece moved there would be
evaluated as an Alternative Move at the ply the walk *started* from — a move
in a position it was never played in. Stepping back to the line's start hands
the board back.

**Tool.** One new web-only, board-kind tool: `step_line`, taking
`{ to: number | "next" | "previous" | "start" | "end" }`. It refuses when no
line is shown. Playback can only exist for a line `show_line` already accepted,
so the gate is unchanged — `step_line` cannot conjure a line, only walk one.

**The gap this deliberately does not close.** Inside a branch, the engine offers
one move (`strongestReply`), not a line, so `engineBest` from an explored
position stays unanswerable. The honest answer is the existing vocabulary:
`evaluate_player_line` with `opponentReplies: "engineBest"` already returns a
whole line of branches, which becomes an `explorationPath` and plays back like
any other. Evaluate, then play back. Do not add a second path that authors a
continuation the engine was never asked for — that is what a client-side PV
would be, and Plan 006 Phase 5's browser engine must not be allowed to become
one.

### Phase 3 — re-measure deixis

The board surface is seven tools today. This plan makes it nine, and
#489 measured the
non-deictic trap over-calling on roughly one run in five *at seven*. Carrying
that number forward unchanged would be pretending.

Extend the scripted suite with the new referent classes — "show me what the
knight hits", "which file does my rook own", "play that out" — plus one new
trap: an annotation request the position does not support, which must come back
refused and be reported as refused, never drawn. Reported as measured, not
wired as a gate, per the spec's stated reasoning.

## Other UX opinions

Ranked, each with a verdict, because a list of possibilities is not advice.

1. **Recommend — make the snapshot say who moved the board.** WebMCP has no
   server-to-agent push, so an agent cannot learn that the Player dragged a
   piece while it was idle; it discovers a changed board mid-answer, or worse,
   does not. The honest fix is not to simulate push but to make the next call
   self-describing: add `revisionChangedBy: "player" | "agent" | null` and the
   moves added since a revision the agent names. Cheap, and it converts silent
   staleness into a sentence the coach can say out loud.
2. **Recommend — orientation as a drive target.** The board is pinned to
   `importedGame.reviewSide` (`CoachingBoardChosenGame.tsx:178` → `ChessBoard`).
   "Show it from Black's side" is an ordinary coaching request with no grounding
   hazard whatsoever. Fold it into `set_board_position` rather than minting a
   tenth tool.
3. **Recommend — a Player-initiated handoff.** Nothing on the board lets the
   Player say "ask about *this*". The hardest half of the deixis problem is the
   Player not knowing how to refer to what they are looking at. An "Ask about
   this position" affordance that puts a short, exact referent on the clipboard
   costs almost nothing and attacks the failure at its source instead of at the
   model.
4. **Recommend — sequence behind Plan 006.** A single drag already costs `k + 4`
   sequential round trips with the board fully disabled. Phases 1 and 2 are
   in-page and add none of that. But an agent-driven *evaluate-then-play-back*
   flow multiplies exactly the cost 006 Phase 0 exists to measure. Do not ship a
   UX that invites multi-move agent evaluation until 006 Phase 0/1 has landed.
5. **Consider — a short transcript of what the coach drew.** Marks and lines are
   ephemeral by design, and the chat scrolls. A stack of the last three
   annotations under the board keeps the coaching visible after the sentence
   that produced it has gone. Low cost; defer until Phase 1 is in front of a
   Player, because it may turn out that clearing is the whole point.
6. **Reject — free-form drawing.** An agent choosing arbitrary shapes, colours
   and endpoints has an unbounded vocabulary and no grounding, and it invites
   drawing a claim the page cannot support. The typed mark list is the feature.
7. **Reject — annotating an arbitrary FEN.** Already out of scope in the spec,
   for the same reason it should stay out of scope here: a position outside the
   Game Import and the catalog has no engine root, and legality is not
   grounding.
8. **Reject — a polling tool so the agent can watch the board.** It would burn a
   tool call per turn to learn nothing most of the time, and it worsens exactly
   the over-calling that #489 already measured. Opinion 1 gets the same
   information for free.

## Risks

- **Over-drawing.** A coach that annotates every turn produces a noisy board and
  trains the Player to ignore it. Mitigated by the six-mark cap, by clearing on
  every revision, and by a tool description that says annotate when the claim is
  spatial or when asked — not by default.
- **Verified geometry, wrong coaching.** Named above; not solved. The check
  proves the relation, never its significance. The `multiAttack` naming keeps
  the tool honest about which of the two it did.
- **Snapshot weight.** Every board tool result carries the whole snapshot. Two
  new fields (`marks`, `linePlayback`) ride on every unrelated call. Keeping
  FENs out of the step list is the mitigation, and `measureModelToolSurface`
  should be read after Phase 2, not assumed.
- **Tool count and deixis.** Two more tools on a surface whose over-call rate
  was measured at seven. Phase 3 exists because that number does not carry
  forward.
- **Generated contracts.** `HostTurnShowLine` and `MoveSequenceSnapshot` come
  from Rust through `ts-rs`. This plan touches neither: playback is a page-side
  view over data already returned, and the new tools are web-only entries in
  `coachToolSurface`.

## Rejected alternatives

- **Extend `HostTurnShowLine` with a "play out" variant.** Rejected: it is a
  generated engine contract, and the closure of that union is a deliberate
  safety property. A page-side cursor over an already-accepted line buys the
  same capability without touching it.
- **Put `render_move_sequence` on the web surface.** Rejected: it mints a
  `MoveSequenceRef` and returns an artifact for a host that renders cards. The
  Coaching Board has a live board on screen; it needs the moves stepped into
  that board, not a second board rendered beside it.
- **Let the browser engine supply a PV to play back from a branch.** Rejected:
  it would move authorship of a chess fact into the page, which is the one line
  ADR 0056 and Plan 006 both refuse to cross. `evaluate_player_line` with
  `opponentReplies: "engineBest"` already answers the same question with the
  engine's authority.
- **Verify annotations on the engine.** Rejected: a geometric relation is not an
  evaluation. A round trip per arrow would spend allowance, add latency to a
  path Plan 006 is already trying to shorten, and buy no additional truth.
- **A material threshold on `multiAttack`.** Rejected: a threshold drags
  evaluation into the page, which is the one line this design refuses to cross.
  Renaming the kind answers the same worry without moving the boundary.
- **Ship annotation before Phase 0.** Rejected: drawing on a board whose branch
  controls are still a dead end optimises the wrong half. The Player must be
  able to reach the position by hand before the coach starts pointing at it.

## Issue structure

Children of #480, which is
open with all eleven of its existing children closed. No second parent: the
epic already owns this surface, and nothing in the spec's Out of scope excludes
playback or annotation — they were simply not considered. Append the new
children to the spec's `## Children` list so the contract and the tracker agree.

1. Phase 0 — wire the branch controls the Coaching Board already has
2. `BoardMarkLayer`, the `coach` tone, and the legend, in `packages/ui`
3. `annotate_board`, the geometry verifier, and ADR 0059
4. `linePlayback` and `step_line`
5. Re-measure deixis with the grown suite
