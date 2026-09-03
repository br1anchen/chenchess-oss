# Verify board annotations in the page, against the position on screen

## Status

Accepted (2026-08-30). Implemented under
[Plan 007](../plans/007-agent-driven-coaching-board.md), Phase 1; the spec
delta below is applied with it.

This decision extends ADR 0056, which governs *who may call the Coaching
Board's tools and how grounding policy travels without an instructions
channel*. It answers a different question — *where a chess claim may be
checked* — and does not disturb 0056's registration gate, its per-tool
descriptions, or its constraint blocks.

## Context

A coach talking about a position says three kinds of thing. Two of them the
Coaching Board can already carry: a move (`show_line`, `set_board_position`)
and an evaluation (every tool result). The third it cannot express at all.

"This knight hits the rook and the queen." "Your rook owns the d-file." "That
pawn is what blocks the bishop." These are the sentences a Player most needs
drawn, because they are spatial — the prose in ChatGPT and the squares on the
board are describing the same thing, and the Player is left to fuse them.

Nothing on the board can draw them. `BoardArrow` is `{from, to, label, tone}`
with `tone: "engine" | "peer" | "candidate"`
(`packages/ui/src/contracts.ts:66`); `BoardArrowLayer` draws lines and
arrowheads and nothing else; there is no square-mark primitive anywhere in the
board stack, and `InteractiveChessboardGrid` accepts `destinations`,
`selectedSquare`, `lastMove` and `arrows` with no general mark channel. The
Coaching Board hardcodes at most one derived arrow
(`CoachingBoardChosenGame.tsx:180`), which `engineArrowUci` then suppresses
whenever a line is shown (`coachingBoardDrive.ts:284`).

The existing gate does not extend to cover this. ADR 0056's driving limit is
*evaluate, then show*: a line reaches the board only by being an
`AlternativeMoveResult` the engine produced, and `HostTurnShowLine` is a closed
union precisely so an invented line cannot be expressed. That gate works
because a line **is** an evaluation — sending it to the engine is both the
grounding and the point.

An annotation is not. "Does the knight on f3 attack d4?" has no evaluation in
it. Asking Coach Engine would spend Alternative Move allowance, add a round
trip to a path Plan 006 is already trying to shorten, and return a fact the
page could have computed from the FEN it is already rendering. But leaving it
unchecked is worse: an agent free to draw an arbitrary arrow can assert a
tactic that is not on the board, to a Player who — per 0056's stated top risk —
is the least able to detect it.

So the question is not whether to check. It is who checks, and against what.

One relevant fact about this codebase: `chessops` 0.15.0 is already a
`central-host` dependency and already load-bearing in the page. `openingMoves.ts`
builds positions with it, `useBoardExploration` derives legal destinations
through `legalDestinations`, and ADR 0058 already established the web deriving
`occupied` and `sideToMove` from a FEN the engine returned. `chessops` exports
`attacks(piece, square, occupied)`, which is occupancy-aware; every relation
above is one call against the `SquareSet` it returns.

## Decision

**Split the claim by kind. The page is the authority on the geometry of the
position on screen; Coach Engine remains the sole authority on evaluation.**

Geometry is decidable from the FEN the board is rendering, with certainty, in
microseconds, with no round trip and no allowance spent. Evaluation is not
decidable in the page at any price, and this decision does not move one inch of
it.

**Add one web-only board tool, `annotate_board`.** It joins the authored
`coachToolSurface` map with target `["web"]` and web kind `"board"`, so the
derived model-visible list does not change and the standing MCP model-list lock
is untouched. Input is

```json
{ "revision": 12, "marks": [ /* 1..6 */ ] }
```

`revision` is the snapshot revision the agent believes it is annotating. A
mismatch is refused, so a mark can never be applied to a board that moved
between the read and the draw.

**Six mark kinds, each verified before a pixel is drawn.**

| Kind | Shape | Verified |
| --- | --- | --- |
| `attacks` | `{from, to, label}` | `attacks(pieceAt(from), from, occupied)` contains `to`; `to` holds an enemy piece |
| `defends` | `{from, to, label}` | as above; `to` holds a friendly piece |
| `multiAttack` | `{from, targets[], label}` | at least two targets, all enemy, all in `from`'s attack set |
| `controls` | `{from, to, label}` | `pieceAt(from)` is a slider and `attacks(...)` — which is occupancy-aware, stopping at the first blocker — contains `to`, which may be empty |
| `square` | `{square, label}` | the square exists; asserts no chess relation |
| `move` | `{uci, label}` | the UCI names a move ChenChess already put on this board — a branch move, the active branch's strongest reply, or a move of the Review Moment's own lines — and is still legal in the position on screen (Phase 2's `linePlayback` steps join that set) |

Refusals are typed and carry the snapshot, like every other board refusal
(`driveRefusal`). A refused annotation is a fact the agent must report, not a
retry. `relationNotOnBoard`, `moveNotGrounded`, `tooManyMarks` and
`staleRevision` each answer a well-formed call; `outsideMarkVocabulary`
answers a call that was not in the vocabulary at all, which is a different
thing and earns its own name rather than being dressed as a false claim.

**`multiAttack` is deliberately not called `fork`.** Geometry proves that a
knight attacks two enemy pieces. It cannot prove the fork is worth having:
forking two pawns passes every check in the table. Naming the kind for what is
actually checked keeps the verified claim and the drawn claim identical. The
word "fork" belongs in the `label`, which is prose, governed by the constraints
block the result already carries — the same channel that governs every other
sentence on this surface. The tool description says so rather than implying the
check is stronger than it is.

**Marks are scoped to one position, and every transition that moves the board
clears them.** The Player moving, the agent setting a position, a line
stepping — each drops what was drawn on the position it left. A stale arrow
describing a different board is the one failure this feature could introduce,
and the lifetime rule removes it by construction rather than by care: one
`movedBoard` helper owns the rule, so the next transition added cannot forget.

Annotating is not one of those transitions — it replaces the marks on the
position it names and moves nothing. It still advances the page revision,
because Decision 7 of the Coaching Board spec makes equal revisions mean
nothing changed, and what the board shows did change. The consequence is that
`annotate_board` is not idempotent and a second annotation needs a fresh read;
that is the price of keeping Decision 7 true, and it is the cheaper of the two.

**A tool, not a field on the drive tools.** Marks clearing on revision is what
forces this. If marks rode only on `show_line` and `set_board_position`, then
annotating a position that is *already* correct — the common case, since the
Player has just asked about what they are looking at — would need a no-op
`set_board_position` to the current ply. That bumps the revision, clearing the
marks it just set, and it misreports which actor moved the board. A field
cannot express "point at this, do not move anything." An optional `marks` field
on `show_line` stays additive later if measurement shows the agent dropping the
show-then-annotate pair.

**Agent marks get their own ink.** The tone union gains `coach`, with its value
beside the others in `theme/chenTokens.css`; it is named `BoardInkTone`, since
squares carry it now as well as arrows. `candidate` already means
"a move the Player is exploring"; reusing it would make an agent's assertion
visually indistinguishable from the Player's own work, which is exactly the
signal 0056's top risk says the Player needs. One new tone, not two — a
good/bad split waits until something asks for it.

**Marks are announced and legended.** A `describeBoardMarks` joins
`describeBoardArrows` in the board's accessible description, and the board
renders a visible legend from the same marks that drew, as the Coach App
already does
(`apps/coach-app/src/momentPresentation.ts:13`). Coloured lines with no labels
under them push the fusing work back onto the Player, which is the problem this
decision exists to remove.

## Consequences

**There are now two places chess facts are computed, and the boundary between
them is a kind, not a module.** That is the cost of this decision and it should
be stated plainly: a reader of the codebase must know that geometry is answered
in `apps/central-host/src/coaching-board/` and evaluation only ever in Coach
Engine. The boundary holds as long as no mark kind takes a value judgment. Any
future kind that does — "hanging", "winning", "the best square" — is a new
decision, not a widening of this one.

**`chessops` becomes load-bearing rather than incidental in the page.** It was
already a dependency and already used for positions and legal destinations;
this makes a Player-visible claim depend on its attack generation. A version
bump is now a behavioural change, not a housekeeping one.

**Every board tool result grows by the marks field.** The snapshot rides on
every board-tool result (ADR 0056), so an unrelated `list_critical_moments` call
now carries whatever is drawn. Marks hold squares and short labels, not
positions, and the cap counts *drawn marks* rather than requests — one
`multiAttack` request draws one arrow per target, so counting requests would
have let six calls put twenty-four marks on every later snapshot.

Measured on a game-origin snapshot with no branches: **1,644 bytes empty,
2,087 bytes with six maximum-length marks — a 443-byte ceiling**, and that is
the worst case, since the cap is six. `annotate_board` does not join the
model-visible tool surface, so `measureModelToolSurface` is unchanged by this
decision; the weight is on the board snapshot, and this is it.

**The web board tool count goes from seven to eight, and to nine with Plan 007
Phase 2.** #489 measured
the non-deictic trap over-calling on roughly one run in five *at seven*. That
number does not carry forward, and Plan 007 Phase 3 re-measures rather than
assuming.

**The tool name lists grow by one**, so the oracles pinning them —
`useCoachingBoardTools.test.tsx`, `coachingBoardCoachTools.test.ts`, and the
drift assertion against the authored map — must be updated in the same change.
That is the drift gate working, not a migration.

**No generated contract moves.** `annotate_board` is web-only, its marks are
page state, and nothing here touches `HostTurnShowLine`, `MoveSequenceSnapshot`,
or any `ts-rs` output.

## Spec delta

Applied to `docs/spec/coaching-board.md`:

- **Decisions, new row**: the page verifies the geometry of the position on
  screen and refuses a relation that is not on the board; Coach Engine remains
  the sole authority on evaluation; annotation is verify-then-draw, the sibling
  of evaluate-then-show.
- **Driving limits**: annotation joins showing as a thing the agent may do to
  the board, with its own closed vocabulary of six mark kinds and its own typed
  refusals.
- **The snapshot**: marks are carried, scoped to one revision, and cleared by
  any revision bump.
- **Validation, deterministic tier**: each mark kind verified and refused
  against a fixture position, the stale-revision refusal, the six-mark cap, and
  marks clearing across a position change join the vitest suite.
- **Validation, behavioural tier**: the scripted suite gains the annotation
  referent classes and one trap — an annotation the position does not support,
  which must be reported refused and never drawn.

## Alternatives within the shape

**Verify annotations on the engine.** Rejected. A geometric relation is not an
evaluation, so the engine would be answering a question it has no special
authority over. It would spend Alternative Move allowance per arrow, add a
round trip to the path Plan 006 exists to shorten, and buy no additional truth
— the engine would compute it from the same FEN the page is already holding.

**Draw whatever the agent asks, and govern it with prose alone.** Rejected.
Every other capability on this surface is gated by construction rather than by
instruction, for the reason 0056 gives: descriptions are host-summarised
context seen once. An arrow is the most literal possible assertion about a
board, and it is exactly the wrong place to fall back on asking nicely.

**A material threshold on `multiAttack`.** Rejected. A threshold is an
evaluation, and admitting one into the page is the boundary breach this whole
decision is built to avoid. Renaming the kind answers the same worry without
moving the line.

**Free-form marks — arbitrary shapes, colours, endpoints.** Rejected. The
vocabulary would be unbounded and nothing in it would be checkable, which
collapses back to the previous alternative with extra surface area.

**Annotating an arbitrary FEN.** Rejected, consistent with the Coaching Board
spec's existing out-of-scope entry: a position outside the Game Import and
outside the catalog has no engine root, and legality is not grounding.
