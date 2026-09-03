# Opening study as small-world play

**Date**: 2026-08-30. **Status**: research note, no decision taken.
**Prototype**: `docs/prototypes/small-world-opening-study/`.

The question: how to make opening study on the **Coaching Board** teach
something, and whether "small world play" — the early-years practice of
building a miniature world and playing inside it — is a usable design frame or
just a pleasant metaphor.

The short answer is that it is usable, and the reason is not the metaphor. It
is that the cognitive-science account of chess expertise and the developmental
account of small-world play describe the same object: **a fixed frame with
open slots you fill**. Everything below is downstream of that.

Evidence is labelled: **measured**, **strong consensus**, **single source**,
**contested**, or **inferred**.

---

## 1. What the evidence says

### Amateurs leave book at move seven — measured

An archival study of **76,562 competitive games** measured the ply at which
each player first departed from theory
([PMC3217924](https://pmc.ncbi.nlm.nih.gov/articles/PMC3217924/)):

| Class            | Elo       | Games  | Mean depth               |
| ---------------- | --------- | ------ | ------------------------ |
| Class B          | 1600–1799 | 5,019  | **14.26 ply (≈ move 7)** |
| Class A          | 1800–1999 | 15,737 | 15.58 ply                |
| Candidate Master | 2000–2199 | 29,881 | 16.71 ply                |
| Master           | 2200–2399 | 25,925 | 18.01 ply                |

Fit: `depth = 0.0065 × Elo + 3.04`, ~99% of variance. 1600 is the _floor_ of
that sample; everything below is shallower.

This is the single most useful number in the note. A Player at ChenChess's
target level is off book at **move seven**. Study that stops at the end of a
catalog line has taught nothing about 100% of the moves they will actually
play. ADR 0057 already reached this conclusion from the product side, in the
sentence that motivates the whole opening root: _"the deviation is where the
Player's real question lives, usually around move four, off book."_

The same paper models masters as holding ~100,000 opening moves. That figure
is **single source and model-derived** — do not repeat it as measurement.

### Experts store templates with slots, not sequences — strong consensus

Gobet and Simon's chunking-and-template theory: expertise is stored as
~50,000 chunks that evolve into **templates** — a stable core plus **slots**
whose values are filled rapidly, and which are **linked to typical moves and
plans**. Masters recall real positions far better than novices but are _no
better on random positions_, so the advantage is organisational, not capacity.
Move selection runs by recognition triggering a candidate, not by replaying a
sequence.
([Gobet & Simon PDF](https://gwern.net/doc/psychology/chess/1996-gobet-2.pdf),
[pattern-recognition theory of search](https://www.tandfonline.com/doi/abs/10.1080/135467897394301),
[implications for education](https://onlinelibrary.wiley.com/doi/abs/10.1002/acp.1110))

The design consequence is direct: the thing to be retrieved is a **template
reachable from any move order**, not a position-within-a-known-sequence. A
sequence card trains a cue — "this position, in this line, after these moves" —
that is _absent_ in a real game where the opponent deviated.

### The card unit is wrong in every existing tool — strong

Andy Matuschak, on Chessable, as a memory-systems designer rather than a chess
player: _"there's no way to use this spaced repetition mechanism to reinforce
conceptual knowledge, since the chess board input interface only allows for
auto-graded move responses."_
([notes.andymatuschak.org](https://notes.andymatuschak.org/zDr94hP6bG3jJYrdYy8B5hx))

The input modality is the ceiling. A board can grade _which move_. It cannot
grade _what is the plan_, _which break does this structure want_, _which minor
piece do you want to trade_, or _what changed when he played …a6_.

Wozniak's rules for formulating knowledge sharpen the same point from the card
side ([20 rules](https://super-memory.com/articles/20rules.htm)): rule 4, the
**minimum information principle** — an item should be as simple as possible;
rules 9 and 10 — **sets and enumerations are near-unlearnable**, and sets of
more than five members are effectively impossible; rule 1 — **do not learn if
you do not understand**. An opening line violates 4, 9 and 10 at once. So does
"list the four plans in this structure." Rule 8, **graphic deletion** —
occlude part of an image and ask what is missing — is the one that turns out to
matter most here, because on a chessboard it is literally slot-filling.

### Bjork's desirable difficulties back "build it yourself" — strong consensus

Spacing, retrieval, interleaving, **generation**, and **variation** slow
acquisition and improve retention and transfer; storage strength and retrieval
strength dissociate, and only storage strength is learning
([Bjork & Bjork](https://www.unh.edu/teaching-learning-resource-hub/sites/default/files/media/2023-06/itow-introducing-desirable-difficulties-into-practice-and-instruction-bjork-and-bjork.pdf)).
Every existing opening trainer drills **blocked**: walk a line, replay that
line. Contextual-interference evidence favours interleaving, though a 2023
systematic review calls the effect a myth in sports practice — **contested**,
so claim it softly.

### Level-calibrated frequency — single analyst, replicable

FM Nate Solon pulled Lichess data along the path to the Nimzo-Indian: the move
distribution shifts drastically by rating band. The theoretically-principal
4.Qc2/4.e3 do not dominate until 2200+, while 4.Bd2 dominates below
([Openings vs. Ratings](https://www.zwischenzug.gg/p/openings-vs-ratings)).
Replicable against the Lichess explorer API; worth doing before building on it.

### Opening-study ROI — no trials exist

Be blunt about this. There are **no controlled trials** comparing opening study
against tactics or endgames for rating gain. Charness et al. 2005 found serious
solitary study the strongest single predictor of chess skill (~40% of rating
variance across activities), but says nothing about _which_ study
([Applied Cognitive Psychology](https://onlinelibrary.wiley.com/doi/10.1002/acp.1106)).
Meta-analysis puts accumulated practice at ~34% of variance
([Macnamara et al. 2016](https://artscimedia.case.edu/wp-content/uploads/sites/141/2016/09/14214856/Macnamara-Moreau-Hambrick-2016.pdf)).

The strongest defensible claim, and it is **inferred** from two measurements
rather than tested: deep line memorisation is misallocated below ~2000, while
shallow, level-calibrated, structure-anchored opening knowledge is not.

---

## 2. Small-world play, and why it is not just a metaphor

[Small world play](https://www.jaqueslondon.co.uk/blogs/posts/what-is-small-world-play)
is "the act of building a miniature version of the real world and then playing
inside it." Its stated principles, and what each becomes here:

| Small-world principle                                                             | Opening study                                                                                                      |
| --------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| A **contained** tray, rug, or frame                                               | One tabiya. Not an opening tree. The bound is the point.                                                           |
| Few **open-ended** pieces, quality over quantity                                  | A handful of reusable primitives: the break, the piece's home, the plan.                                           |
| **"Resist the urge to build the whole scene yourself; the building is the play"** | The learner reconstructs the position from its ideas. The coach does not present it finished.                      |
| Vygotsky's **zone of proximal development**                                       | Difficulty just past solo capability — what an adaptive agent is for.                                              |
| Scenes are **demolished and rebuilt** repeatedly                                  | Re-enter the same structure by a different move order or from the other side. Variation, not identical repetition. |
| The adult **follows the child's lead**, joins when invited                        | The agent is responsive, not lecturing.                                                                            |

Papert's **microworld** is the same idea for older learners: a domain-specific
environment with a **low floor and a high ceiling**, an "incubator" for a
specific species of powerful ideas
([EduTech Wiki](https://edutechwiki.unige.ch/en/Microworld), _Mindstorms_).
Small-world play and microworlds are one concept at two ages, and "low floor,
high ceiling" is a directly usable acceptance criterion.

**The load-bearing convergence.** Gobet's template is _a stable core with slots
you fill, linked to typical moves and plans_. A small world is _a contained
frame with open-ended pieces you assign meaning to_. These are the same
structure. The developmental frame and the expertise literature independently
prescribe the same unit of study — which is why this is a design frame and not
a decoration. A memorised line is the one representation that is neither: it
has no slots, so nothing can be filled in when the opponent deviates.

---

## 3. What this implies for cards

Five card types. The unit is always **one decision in one position**
(Wozniak rule 4), never a line.

| Card                                                              | Tests                                      | Graded by                                          | Novel?                                                                            |
| ----------------------------------------------------------------- | ------------------------------------------ | -------------------------------------------------- | --------------------------------------------------------------------------------- |
| **Place the piece** — remove a piece from the tabiya, put it back | Template slots                             | Structurally, no engine: a set of accepted squares | Wozniak rule 8 applied to a board; no chess tool does it                          |
| **Choose the break** — which pawn break does this structure want  | The plan, as one decision                  | Engine, via `evaluate_opening_continuation`        | Rare                                                                              |
| **Say the plan** — free text                                      | Conceptual understanding                   | **The host agent**, against a rubric               | Yes — this is the card Matuschak says is impossible on a board-only input channel |
| **Off book** — opponent leaves the catalog; now what              | Whether the plan survives without the line | Engine + agent                                     | Yes — nobody coaches the deviation, and it is 100% of real games                  |
| **Transposition** — same structure, different move order          | Template vs sequence                       | Position identity                                  | Yes — directly falsifies "I learned the path, not the position"                   |

Two of these are only possible because ChenChess has **an agent sitting on the
board**. That is the whitespace: every competitor's assessment channel is the
board itself, so the category ceiling is move-recall. Chessable, Lichess,
Chess.com, ChessTempo, Listudy, Chessbook, and Noctie all stop there.

---

## 4. What the repo already has

More than expected. This is a completion, not a greenfield build.

- **ADR 0056** gives the Coaching Board; **ADR 0057** adds the **Opening Line**
  as a second grounded root, stateless and identity-free, with off-book
  analysis explicitly allowed (12-ply cap, per-Player rate limit) and an
  **Opening Analysis Cache** keyed by normalised position so transpositions
  collapse onto one entry. **ADR 0058** mints exploration branches web-side
  through `evaluate_opening_continuation`.
- `apps/central-host/src/coaching-board/openingLineCatalog.ts` already carries,
  per row, `ideas: { plan, pawnBreaks, piecePlaces }` — **three slots, already
  authored**. Today `CoachingBoardOpeningStudy.tsx` renders them as three lines
  of prose you read. Nothing asks the Player to do anything with them. Turning
  those three fields from _read_ into _do_ is most of the feature.
- **ADR 0037** already deferred exactly this, and named it: _"A future
  application stage must instead generate a novel transfer position from the
  same idea category."_
- **ADR 0038** defers **cross-game learner state** and **concept-generated
  candidates**, and compiles a **Chess Knowledge Graph** with an acyclic
  `Prerequisite` relation — a curriculum DAG already exists in the binary.

Two catalogs coexist: the pinned 3,690-row ECO catalog (find/resolve/analyse)
and the ~7-row hand-authored study catalog with `ideas`. Reconciling them is
the first design question, not an implementation detail.

---

## 5. The payoff: this frame justifies staying stateless

The obvious reading of "quiz cards" is a deck with due dates. That would
collide with three separate existing constraints:

1. ADR 0057 makes the opening root **stateless with no Player-owned state**,
   and a durable shared study is called out as "a new decision, not a widening
   of this one."
2. `docs/research/chenchess-beta-release-readiness.md` says plainly: _"Do not
   add cross-game mastery, schedules, or spaced repetition yet."_
3. `OpeningLineRef` derives from the catalog path, so **a catalog pin bump
   invalidates every saved opening address**. ADR 0057 flags this as needing
   revisiting _before_ opening addresses are promoted anywhere durable. A deck
   keyed by `OpeningLineRef` would silently rot on the next pin bump.

Small-world play dissolves the collision rather than fighting it. **The session
is the container.** You build the tray, play in it, and demolish it. There is
no deck because the world is rebuilt from scratch next time — and rebuilding
_is_ the practice (generation, variation, and the explicit "scenes are
demolished and rebuilt" principle). A deck of a session's answers would only
test whether you remember that session.

This is a genuine design argument for statelessness, not a rationalisation of a
constraint. It is also falsifiable: if measurement later shows Players want
continuity across sessions, the thing to persist is the **concept** — a
Learning Track key, which is already stable and already survives a catalog pin
bump — never the line.

---

## 6. Prototype

`docs/prototypes/small-world-opening-study/` — a single self-contained HTML
file, no build, no server. Two worlds (Giuoco Piano as White, Najdorf as
Black), five stages: build the world, say the plan, choose the break, off book
(three deviations), demolish.

Board data is generated by `build-worlds.ts`, which replays every SAN move
through chessops and refuses to emit anything illegal. That validator caught
five errors in the authored content during this spike, including a pawn break
blocked by its own knight and a "reply" that was already on the board. Engine
verdicts in the prototype are **authored spike content**, not engine output;
in production they come from `evaluate_opening_continuation`.

---

## Unresolved questions

1. One catalog or two? Merge `ideas` into the pinned ECO catalog, or keep a
   separate authored study layer over it?
2. Who authors `ideas` for more than seven rows — hand-authored, generated and
   reviewed, or derived from the Chess Knowledge Graph?
3. Does the free-text "say the plan" card need a new web tool, or does the
   agent grade it in-conversation against the snapshot it already holds?
4. Is the transposition card reachable given the catalog is **not
   prefix-closed**?
5. Does stage 1 ("build the world") need a board-input tool the Coaching Board
   does not have — the Player placing a piece on an arbitrary square?
6. Confirm the Class B depth figure against our own imported games before
   citing move seven in product copy.
