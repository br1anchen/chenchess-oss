# Show a provisional move on the board, and say in the snapshot that it is provisional

## Status

Accepted (2026-08-31). Implements
[Plan 006](../plans/006-speed-up-move-exploration-and-review-loading.md),
Phase 2 item 1, which the plan deliberately deferred to a decision of its own.

This decision extends ADR 0056, which governs *who may call the Coaching
Board's tools and what every board-tool result must carry*. It answers a
different question — *what the snapshot says while a Player's move is in
flight* — and does not disturb 0056's registration gate, its per-tool
descriptions, or the rule that every board-tool result carries a snapshot.

It also sits under ADR 0058's fact boundary: nothing here authors a chess fact.
A derived position is a position, not an evaluation.

## Context

A Player drags a piece on the Coaching Board and the piece does not move until
Coach Engine answers. Measured on hosted staging on 2026-08-31, across twelve
moves of one line: **448 ms median from drop to the board repainting, 827 ms at
the worst**, against a stated budget of *under 100 ms for the piece to land*.
That is the one line of Plan 006's perceived-performance budget that nothing
shipped so far has moved, and after Phases 1 and 3 it is the largest remaining
Player-visible cost. Every cheaper explanation is now measured and gone: one
command per move, zero queue wait, a 48 ms durable write.

The board could move the piece immediately. Plan 006's decision D3 already
settles that deriving a position is not authoring a fact — the web derives
positions from moves with `chessops` for Opening Lines, and legal-move
generation for drag validation is the same category. Nothing prevents drawing
the resulting position the instant the drag is legal.

What prevents it is the **Coaching Board Snapshot**. ADR 0056 requires every
board-tool result to carry the snapshot precisely so an agent that skips the
read cannot coach from a stale picture, and it names the failure it is
protecting against: fluent, confident, wrong coaching to a Player least able to
detect it. A provisional move opens a window — a few hundred milliseconds, but
a window — in which the Player sees one position and the snapshot reports
another. An agent that reads during that window is in exactly the state 0056
exists to prevent, and it does not know it.

The snapshot's `currentPosition` and `exploration.pathFromRoot`
(`coachingBoardSnapshot.ts:78-92`) are the two fields in tension. `pathFromRoot`
is a list of `AlternativeMoveId`s — identifiers Coach Engine mints when it
commits a branch. A move in flight has no such identifier yet, because minting
one is the round trip being waited on.

## Decision

**Draw the provisional move, and name it in the snapshot as a distinct field.**

Three parts.

**1. The board places the piece from the derived position, immediately.** On a
legal drag the board renders the resulting position computed in the page, marks
it provisional in a way the Player can see, and reconciles when Coach Engine
answers. If the engine disagrees with the derived position, the engine wins and
the board corrects without asking — ADR 0058's boundary is unchanged, and a
derived position was never a fact.

**2. The snapshot gains a `pendingMove` field, and `currentPosition` keeps
meaning what it means.** `currentPosition` continues to name the last position
Coach Engine confirmed, and `pathFromRoot` continues to explain how the board
reached it. A move in flight appears as:

```
pendingMove: { uci, derivedPosition } | null
```

Null whenever nothing is in flight, which is almost always. An agent reading
mid-flight is told both things it needs: the confirmed position it may reason
about, and the fact that the Player has already played something the engine has
not confirmed.

**3. An agent must not coach on a pending move.** The constraint block says so,
in the same channel every other Coaching Board rule travels in. `pendingMove`
is a statement about what the Player is looking at, not a fact to build on: no
evaluation is attached, none may be inferred, and a claim about the resulting
position is a claim about a position the engine has not evaluated.

## Consequences

The signed snapshot shape changes, which is a contract break. ChenChess is
pre-release, so this costs a version bump rather than a migration, and the
generated SDK carries the field to every consumer at once.

**Every board-tool result grows by one nullable field.** Almost always null.
That is a smaller cost than the two alternatives below, both of which spend
something that cannot be recovered by paying more bytes.

**The agent gains a state it must handle.** An agent that ignores `pendingMove`
behaves exactly as it does today — it reads a confirmed position and coaches on
it, which is correct, merely a few hundred milliseconds behind the Player's
screen. The field is additive in the sense that ignoring it is safe; only
*misreading it as a fact* is not, which is what the constraint forbids.

**The Player's board stops waiting on the network.** Against the measured
448 ms median, the piece lands in a frame. The evaluation still takes as long as
it takes and still arrives as authored engine output; what changes is that the
Player is no longer watching a frozen board while it does.

**Client-side queueing becomes worth doing, and is not done here.** Plan 006
Phase 2 item 2 keeps `interactionDisabled` while a move is in flight because
without optimistic placement a second drag has nothing to attach to. With this
decision it does, so a queued second drag becomes possible — and is left to its
own change, because it needs a decision about what cancelling the first one
means.

## Spec delta

`docs/spec/coaching-board.md` gains `pendingMove` in the Coaching Board Snapshot
section, with the constraint that no evaluation may be attached to it or
inferred from it. The snapshot's existing sentence — that `currentPosition` is
the position on screen — is corrected: it is the position Coach Engine last
confirmed, which is the position on screen except while a move is in flight.

## Alternatives within the shape

**Report the derived position as `currentPosition`.** Rejected, and it is the
dangerous option rather than merely the lossy one. `pathFromRoot` could no
longer explain how the board reached the position it reports, so an agent would
receive a position with no provenance and **no way to tell it was unconfirmed**.
That is precisely the confident-wrong coaching ADR 0056 was written to prevent,
reintroduced through a field the agent trusts.

**Hold board-tool reads until the move settles.** Rejected. It keeps the
snapshot honest by making a tool call stall for up to the measured 827 ms, which
moves the latency from the Player onto the agent and makes a read's duration
depend on whether the Player happens to be dragging. A grounding rule that
sometimes hangs is a worse rule than one that reports an extra field.

**Do nothing, and accept the frozen board.** Rejected on the measurement. The
budget line is 100 ms and the board takes 448 ms median; every other cost on
that path has been removed, so there is nothing left to fix instead.
