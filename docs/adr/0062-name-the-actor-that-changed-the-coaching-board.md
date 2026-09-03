# Name the actor that changed the Coaching Board

## Status

Accepted (2026-09-01). Implements
[Plan 007](../plans/007-agent-driven-coaching-board.md), opinion 1, which the
plan recommended and did not build.

This decision extends ADR 0056, which requires every board-tool result to carry
the current Coaching Board Snapshot. It answers a question 0056 leaves open —
*what the snapshot says about who changed the board and when* — and disturbs
neither the registration gate nor the rule that every board-tool result carries
a snapshot. It is the same shape as ADR 0060, which added a field so the
snapshot could stay honest about a move in flight.

## Context

WebMCP has no server-to-agent push. `registerTool` gives the page a way to
answer calls; it gives the page no way to speak first. So an agent cannot be
told that the Player dragged a piece, browsed to another ply, or walked a line
while it was idle. It discovers the changed board mid-answer, or it does not
discover it and answers from the position it last saw.

ADR 0056 names that as its top risk in as many words: fluent, confident, wrong
coaching delivered to a Player least able to detect it. Carrying the snapshot
on every result is 0056's mitigation, and it is a good one — any call refreshes
the agent's picture. But it collapses the failure surface without closing it.
An agent that reads a fresh snapshot still learns only the *current* board. It
cannot tell that the board is not the one it was reasoning about, because
nothing in the snapshot distinguishes "you are looking at what you last saw"
from "the Player has been playing while you were away".

The snapshot already carries a monotonic page revision, so an agent that
remembered the last one it read can tell **that** something changed. It cannot
tell **what**, or **who**, without re-deriving the whole board and diffing it
against a picture it would have to have kept.

## Decision

Do not simulate push. Make the next result self-describing.

The Coaching Board Snapshot names the actor:

- `revisionChangedBy: "player" | "agent" | null` — who advanced the revision to
  this one, null while it is still the revision the board loaded on.
- `playerChangedAtRevision: number | null` — the last revision the Player
  advanced the board to.
- `addedAtRevision` and `addedBy` on every exploration branch — so the moves
  added since a revision the agent already read are the branches stamped later
  than it, each saying who put it there.

One transition helper owns the rule, the same way ADR 0059 made one helper own
the revision-and-marks rule, so a transition added later cannot forget to say
who changed the board.

**Two fields, not one, and this is the load-bearing part.**
`revisionChangedBy` is last-writer-wins. An agent that answers a question by
calling `show_line` before reading has overwritten the Player's stamp with its
own, and the Player's activity is gone from the only field that recorded it.
That is not an edge case: it is the ordinary shape of a turn. And the three
things the Player does that add no branch — browsing a ply, selecting a
branch, walking a shown line — leave no branch arrival to fall back on, so for
those there would be nothing left at all. `playerChangedAtRevision` survives
the agent's own calls, which is the whole reason it exists.

**The agent asks for none of this.** `read_coaching_board` still takes no
arguments. A delta the agent must request is a delta it only gets once it
already suspects staleness, which is exactly the state this decision exists to
rescue it from; and an argument on the one tool whose description says it takes
none raises the call-shaping burden on a surface where over-calling is already
[measured](#489). The facts ride
on every board result, asked for or not.

**Reading guidance travels on results, not on descriptions.** How to compare
these fields is not actionable before there is a snapshot to compare, and every
sentence in the shared board constraints is paid for once per registered board
tool. So the two sentences that explain them are carried by the result's
constraints block alone.

## Consequences

An agent that kept the revision it last read can now open with what the Player
did rather than discovering it. "You played two moves while I was away, let me
look again" is a sentence the snapshot supports, where before it was a
discrepancy the agent had to notice.

The page keeps provenance it did not keep before: which actor caused each
revision, and which revision each branch arrived at. The arrival is stamped
where the tree merge already distinguishes a new branch from a re-analyzed one,
so a re-analyzed line keeps its original arrival and does not read as something
the Player just played.

Who moved the board is now part of the drive's contract rather than an
implicit property of which code path ran. Threading it surfaced one existing
defect immediately: the Player's branch strip drove the board through the
agent's tool host, so a Player's click reported the agent as having moved it.
The actor is bound once per surface — the Player's affordances and the agent's
tool host each hold a facade of the same transitions — so no call site names an
actor and none can name the wrong one.

**These counts belong to one board, not to the session.** The revision restarts
when the drive remounts, which happens whenever the origin changes, so on an
origin the agent has not read on the fields carry no history and the constraints
say so rather than inviting an inference. That leaves a Player-driven switch of
Game or Opening Line undetectable, and it means the page revision is not the
"life of the page" revision the spec's decision 7 describes. Closing it needs
either a genuinely page-scoped revision or a per-mount identity in the
snapshot, and an agent tool (`open_opening_line`) causes the same remount, so
attribution alone would not do. Tracked as
#563.

**Closed on 2026-09-01 by #563, taking the page-scoped revision.** The count is
held above the keyed board, so it survives a remount and only a reload starts it
over, and a navigation advances it and names whoever navigated. Decision 7 was
not amended; the implementation came back to it. The current rule is in the
snapshot section of `docs/spec/coaching-board.md`.

## Alternatives rejected

**A polling tool so the agent can watch the board.** It burns a tool call per
turn to learn nothing most of the time, and it worsens exactly the over-calling
#489 measured. It also does not help the case that matters — the agent that
did not think to check.

**A `since` argument on the board read.** Rejected with the cursor in decision
6 for the same reason it is rejected here: a caller that has to ask can
under-read, and this one has to already suspect staleness to ask at all.

**Deriving the actor rather than storing it.** `revisionChangedBy` is
derivable from `playerChangedAtRevision` and the revision, since there are
exactly two actors. It would save a field and cost a three-branch derivation
that silently reports "agent" for any future transition that forgets to stamp.
Storing what happened is the more direct account of it.
