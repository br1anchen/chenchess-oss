# Unify a Review Moment's evaluation onto one search

## Status

Accepted.

## Context

A Review Moment reported the same move's evaluation twice, with two different
numbers at the same stated perspective.

The two numbers come from two different Stockfish searches of the same
before-position:

- The **screening pass** analyses every ply single-PV. Its result is the Moment's
  `objective.bestEvaluation`, the centipawn loss derived from it, and the
  classification that decides whether the ply becomes a Critical Moment at all.
- The **comparison pass** runs `analyze_multi_pv` at MultiPV 3, only on Moments
  that already passed the decision preflight. Its rank-one result was shipped as
  `candidateEvidence.rankedCandidates[0]`.

Both run the same binary at the same depth. They still disagree, because MultiPV
mode disables pruning that single-PV mode uses. Measured over the 643 MultiPV
Moments in `evaluation/gotham/reviews`: **589 (92%) disagree** — 320 by 1–10cp,
187 by 11–30cp, 59 by 31–100cp, 12 by more than 100cp — and some disagree
qualitatively, one search reporting mate where the other reports centipawns.

Nothing reconciled them. `normalize_evidence` checked that MultiPV rank one named
the same _root move_ as the authoritative single-PV record, then attached the
MultiPV _evaluation_ to the Decision Candidate carrying the
`authoritativeSinglePv` origin.

No test corpus contained MultiPV evidence, and the one MultiPV fake in the suite
returned a rank-one score equal to its single-PV score. That is why this survived,
and why closing the corpus gap is part of this decision rather than follow-up work.

### What the MultiPV search is actually for

Tracing every consumer settles the question, and it is not what the field names
suggest:

- `decision_explanation/validation.rs` `preference_for` gates on
  `preferred.assessment.rank != Some(1)` and builds each `EngineComparison` from
  candidate and assessment **references**. It never compares two evaluations
  numerically. `derive_capability` counts comparisons.
- Nothing in `decision_learning` reads a candidate evaluation.
- No TypeScript surface reads `assessment.evaluation`.

MultiPV supplies exactly three things: the **set of alternative roots**, their
**lines**, and their **rank**. Its numeric scores were hashed into
`assessment_ref` and shipped, and drove nothing.

That matches how the searches actually behave. At fixed depth, single-PV
concentrates extensions and a full window on the best line, so its score is the
engine's most-considered answer for that move. MultiPV must give each of N roots
a full window and disables pruning that assumes only the best matters, so each of
its lines is searched with less effective depth — its per-line scores are
noisier. But its **relative ordering among its own N lines is valid**, because
all N were searched under one regime.

## Decision

**SinglePV owns the absolute. MultiPV owns the ordering. Neither is asked for the
other.**

The defect, restated correctly, is that `rankedCandidates[0]` carried a _second
absolute evaluation_ for a move `objective` had already scored. So the duplicate
is deleted rather than reconciled:

- `objective.*`, `centipawnLoss`, the classification, and Critical Moment
  selection are **untouched**. The screening pass remains authoritative for every
  absolute a Moment reports.
- `CandidateEvidence::MultiPv` carries `ranked_alternatives`: **ranks two and up
  only**. Rank one _is_ `authoritative_single_pv` and is no longer restated, so
  there is no second reading of the best move to contradict the first. The
  contradiction becomes unrepresentable rather than validated.
- A `RankedAlternativeEvidence` carries no absolute evaluation. It states a
  `CandidateGap` — how far it fell behind the best move **inside that one MultiPV
  search**. The variants cover every shape measured in the corpus: centipawn
  shortfall (1,199 occurrences), a slower forced mate (31), missing a forced mate
  the best move finds (53), and conceding one the best move avoids (1).
- `EngineAssessment` therefore carries an `EngineAssessmentScore` that is either
  `Absolute` — the authoritative record, or the position after the Player's move,
  both SinglePV — or `BehindBest`. A candidate is never scored both ways, and
  never absolutely by a search that does not own the absolute.

`normalize_evidence` additionally requires that every published absolute state
one perspective, including the Player move injected unranked when it is absent
from the ranked roots.

### The gate that let this ship

The blind spot is closed at the same time as the defect. Each Game case in the
Pipeline Evaluation corpus, and the canonical `Synthet1` provider recording, now
record their Critical Moments' MultiPV searches in `multiPvEvidence`. Fast
evaluation replays them through the real comparison path and pins the resulting
Decision Explanations in the baseline, so a Ranked Alternative that regained an
absolute — or lost its gap, or restated rank one — moves a checked-in file.

The recording is offline and Stockfish-only: a comparison never consults the
Human Move Model, and `enrich` and `explain_decision` are pure over a recorded
MultiPV output. The searches confirm the design on measured data rather than a
fake: in the canonical Game, five moments compare on centipawn gaps, one on
`missesForcedMate`, and one is refused outright because MultiPV rank one names a
different root move than the screening pass.

## Consequences

- A surface may show `objective.bestEvaluation` beside the candidate comparison
  without contradiction. This unblocks the explanation projection's candidate
  comparison, which is why the split had to be resolved.
- **No evaluation value changes anywhere.** No golden fixture needs recomputing,
  no classification moves, no Moment loses its comparison. Only the shape of
  `rankedCandidates` changes, and it is a pre-production contract.
- A gap measured in MultiPV space is not an absolute in SinglePV space. We
  therefore assert the **comparison** — "this alternative fell 70cp short" — and
  never the alternative's own worth. Presentation must not add a gap to
  `objective.bestEvaluation` and render the result as an alternative's score.
- MultiPV rank one is now discarded after being used to compute the gaps. If a
  future consumer needs a mutually-comparable absolute set, it must take all of
  them from one MultiPV search and label them as such — not mix one in beside the
  SinglePV absolute.
- The comparison is still refused when MultiPV rank one names a different root
  move than the screening pass, which would mean the two searches disagree about
  the best move rather than about its score.

## Alternatives considered

**Reconcile onto MultiPV: make the comparison search authoritative and restate
`objective.bestEvaluation` from its rank one.** Implemented first, then rejected.
It moves the absolute onto the noisier search, and onto the one whose numbers no
consumer reads. It also forces a recomputed centipawn loss on 373 Moments and
lands 8 of them on the wrong side of a classification threshold, requiring a
threshold-mirroring guard and dropping those Moments' comparisons — a large
apparatus in service of the wrong direction.

**Make rank one adopt the screening score.** Rejected on measurement: 48 Moments
would get a ranked list whose rank one is numerically worse than its rank two.

**Namespace both absolutes and ship them side by side.** Rejected: a Moment card
would still show two numbers for the same move, and requirement 32 of the parent
redesign asks for one evaluation source per Review Moment, not two well-labelled
ones.

**Drop alternative scores entirely and ship ordering only.** Closest to what the
code uses today, and the smallest contract. Rejected because the explanation
projection's candidate comparison needs a magnitude to be worth showing; the gap
supplies one without inventing an absolute.

**Run MultiPV on every ply so one search serves selection and comparison alike.**
Correct in principle, prohibitive in cost — the comparison pass exists precisely
because MultiPV is too expensive to run everywhere.
