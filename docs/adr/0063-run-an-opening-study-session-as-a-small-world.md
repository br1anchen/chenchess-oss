# Run an opening study session as a small world the Player builds

## Status

Accepted (2026-08-31). Decided under
#526; the first slice is
implemented under #527.

This decision structures study on the opening Coaching Board that ADR 0056
built, ADR 0057 grounded, and ADR 0058 gave exploration branches. It picks up
what ADR 0037 deferred — application beyond recall — without touching what
ADR 0038 keeps deferred: cross-game learner state and concept-generated
candidates. The evidence and the design frame are in
`docs/research/2026-08-30-opening-study-as-small-world-play.md`; the runnable
prototype is `docs/prototypes/small-world-opening-study/`.

## Context

The opening study panel rendered each line's `ideas` — plan, pawn breaks,
piece places — as three lines of prose, and asked the Player for nothing.
The measured constraint says where that fails: across 76,562 competitive
games, a Class B player first leaves theory at 14.26 ply — move seven — so a
surface that stops at the end of a catalog line has said nothing about any
move the Player will actually choose. ADR 0057 reached the same point from
the product side: the deviation is where the Player's real question lives.

The expertise literature and the developmental frame prescribe the same unit
of study. Gobet's template — a stable core with slots linked to typical moves
and plans — and a small-world tray — a contained frame with open pieces the
child assigns meaning to — are one structure. A memorised line is the one
representation that is neither: it has no slots, so nothing can be filled in
when the opponent deviates.

Issue #526 named five decisions that had to be settled before the surface
grew, and an ADR as the place to settle them.

## Decision

An opening study session is a **small world**: one tabiya, bounded, that the
Player builds before playing inside it, and that comes apart when they leave.

**The session is a sequence of cards, and the order is the pedagogy.** Build
the world (slot cards — where does this piece belong in this structure), say
the plan (free text), choose the break (which pawn break the structure
wants), then meet deviations (the opponent leaves the catalog; answer from
the plan, not from a line). Every card asks the Player to produce something
before the surface explains it. The unit is always one decision in one
position, never a line. The model is
`apps/central-host/src/coaching-board/openingStudySession.ts`; the authored
worlds are `openingStudyWorld.ts`.

**The board is part of the card, and the session is its least-privileged
driver.** A slot card rewinds the board to the ply before the asked-about
piece arrives, so the question is not already answered on the board; every
other card studies the finished tabiya. The viewed ply already has three
owners — the address, the Player's own navigation, and the agent's
`set_board_position` — so the session arranges the board only on arrival at
the line's end when none of them asked for a position, and thereafter only as
an answer moves the world on. Driving it from an effect on every render broke
deep-linking to a ply; an existing test guards this.

**Nothing durable is written.** The session lives in page state. There is no
deck, no interval, no due date, and nothing keyed by `OpeningLineRef`. This
is the design, not a shortcut: the session is the container, rebuilding the
world next time is the practice (generation and variation are the desirable
difficulties), and a deck would collide with three standing constraints —
ADR 0057's stateless root, the beta-readiness ban on spaced repetition, and
the catalog-pin hazard that would silently rot anything keyed by
`OpeningLineRef`. If measurement later shows Players want continuity, the
thing to persist is the concept — a Learning Track key, already stable across
pin bumps — never the line, and that is a new decision.

### The five decisions #526 posed

**One catalog or two? Two layers, one address.** The pinned ECO catalog
remains the only authority for finding and addressing a line. The study
layer is an authored overlay: worlds are keyed by catalog row at module load,
addressed by the same `OpeningLineRef` the board opens on, so the two cannot
drift apart. Merging authored study content into the pinned catalog is
refused — the catalog is generated and pinned by digest, and hand-authored
pedagogy inside it would either block pin bumps or be silently regenerated
away.

**Who authors worlds beyond the current rows? Hand-authored,
machine-verified.** A world is authored by hand and admitted only through the
chessops replay suite (`openingStudyWorld.test.ts`, and
`build-worlds.ts` in the prototype): every break, deviation, answer, and
distractor is replayed independently, and each slot must name the ply that
actually places its piece. The verifier caught five content errors during
the spike, including a pawn break blocked by its own knight. This pair —
author freely, verify mechanically — is the named path for growing coverage.
Deriving worlds from the Chess Knowledge Graph's concept relations stays
deferred with ADR 0038. Every row the web catalog carries has `ideas`; a
line whose row has no authored world falls back to the prose Ideas card, so
the surface never pretends to a session it does not have.

**Does the plan card need a tool? No.** A free-text plan is exactly the
answer a board cannot mark — the input channel carries moves — so the card's
verdict is typed `ungraded` and carries its rubric, deferring to the host
agent, which grades the plan in conversation against the snapshot it already
holds. The type system states the gap instead of faking a verdict. This is
the card no board-only competitor can have, and it needs no new surface
area because the agent is already on the board.

**Does building the world need a placement input? No.** A slot card is
Wozniak's graphic deletion applied to a board: the board is rewound so the
piece is absent, and the Player chooses among candidate squares. Choosing a
square tests the template slot; dragging a piece to it would test the same
thing through a board input the Coaching Board does not have. A placement
input is deferred until a card exists that a choice cannot express.

**Is the transposition card reachable? Not yet.** The catalog is not
prefix-closed, so "same structure, different move order" has no second
address to arrive from. When a transposition card becomes worth building,
position identity is already the mechanism the Opening Analysis Cache keys
by, so it waits on catalog shape, not on new machinery. Deferred.

### Off-book evaluation

A deviation card's opponent move, answer, and distractors are authored world
content, verified by replay — the card grades a choice the author already
made. Anything beyond the authored card — the Player or the agent walking a
continuation the world does not carry — goes through the existing stateless
route and ADR 0058's web-minted branches, under the same twelve-ply cap and
per-Player rate limit. This decision adds no engine surface, no aggregate,
and no Player-owned state.

## Consequences

- The session teaches by demanding production: a placement, a plan, a break,
  a reply — each graded where the surface can grade, and handed to the agent
  where only an agent can.
- Coverage grows content-first: a new world is a data change admitted by the
  replay suite, not a code change. Until a line has a world, its panel keeps
  the prose ideas it had.
- The demolition line at the session's end says plainly that nothing was
  saved. Statelessness is presented to the Player as the point, not hidden
  as a limitation.
- The measured transcript showing an agent running a session rather than
  narrating a line is captured in
  its acceptance record (withheld from this snapshot) — it passes, and it
  confirms the first slice needs no new tool: the existing surface staged
  every position, including everything beyond the catalog line's end.
- The Class B depth figure was checked against our own imported games:
  `docs/research/2026-09-01-imported-games-leave-the-catalog-by-move-three.md`.
  Measured against the pinned catalog, games are off book by move three —
  the paper's move-seven figure stays a citation (its theory base could not
  be replicated), and product copy has a stronger owned claim available.
