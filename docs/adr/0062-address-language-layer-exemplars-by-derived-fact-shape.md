# Address Language Layer exemplars by derived Fact Shape

## Status

Accepted (2026-09-01). Settled by grilling on
#534 and implemented in
steps 0–2 of
[the prose-regression plan](../plans/plan-language-layer-prose-regression.md).

It narrows nothing in ADR 0053 or ADR 0050: the pinned route, the budgets, and
the privacy claim stand. What it replaces is how the measurement *addresses the
work it measures*.

## Context

The Language Layer prose benchmark named its exemplars by `(case, ply)`.
`evaluation/bake-off/frozen-set.json` held nineteen entries, each a case id and
a ply number, and the task set was pinned in prose as §5.1 of
#345.

Every payoff-rule change invalidated some of those plies. The last round —
the depth gate that stopped crediting a played move with material its line only
reaches later — re-pointed three entries and found **no replacement anywhere in
the corpus** for two of them: `Improvement / mate / pop` and
`Positive / great / capturedPiece / pop`. The harness's own test asserted
`entries.len() == 19` against §5.1, so retiring a dead entry was a spec change
rather than a data change. The harness was retired in `23bcd58c` rather than
patched again.

That is not bad luck. A benchmark whose exemplars are hand-picked ply numbers
cannot survive the rules it measures moving underneath it, and the rules move
whenever Rule Extraction is corrected — which is the work the benchmark exists
to keep honest.

Four measurements taken against the refreshed ladder decided the replacement:

- The corpus is four recorded GothamChess Games plus constructed cases. Its
  shape coverage is whatever those Games happen to contain, so it cannot be its
  own coverage authority.
- The reviewed GothamChess ladder is 142 Games and 721 selected Critical
  Moments, exhibiting **37** distinct authoring problems against the corpus's
  **23**.
- Both combinations that ended the frozen set are present in the ladder — 17
  moments and 22 moments. They were never unfillable; they were unreachable by
  the addressing scheme.
- Every ladder moment is Positive or Improvement and `analyzed`. Not one is
  Neutral, and not one is Terminal, because both arise only from a moment the
  Player selected. A census taken over the ladder alone would silently drop a
  whole branch of the comment gate.

Two candidate keys were on the table, and neither subsumes the other. The
marker structure the comment gate offers distinguishes whether `{takeaway}` and
`{playedPopularity}` are present; the enum discriminants distinguish grade,
achievement kind, and mate-versus-centipawns, which the marker *names* collapse.
`Improvement / mate` is only visible through the second: the correction
rendering does not branch on mate — both arms render the same evaluation — so
only `residualOutcome` separates a missed forced mate from a centipawn
correction. Measured on the ladder, `MissedForcedMate` and a mate
better-evaluation agree **21 of 21, exact**.

## Decision

**A measurement is addressed by the authoring problem it exemplifies, not by a
ply.**

**Fact Shape.** The authoring problem one Review Moment presents to the Language
Layer: the marker slots its facts offer, with the rendering branch each slot
took. It is *derived* — from `CommentFactsPolicy::for_facts`, the one place a
moment's markers are chosen, times the enum variants that selected each
rendering. The key is the product of the two candidate keys, not either alone.

Every derivation in `services/coach-engine/src/critical_moment_comment/fact_shape.rs`
is an exhaustive `match`. Adding a variant to the contract is a **compile error
in that file** rather than a shape that silently collapses into its neighbour.
That property is the whole point, and it is the one the string-keyed frozen set
could not have.

**Exemplar.** The one recorded Review Moment a Fact Shape resolves to. Resolved
against the pinned corpus and recorded in
`evaluation/corpus/fact-shape-resolution.json` with a digest of the facts it
resolved to. Resolution is incumbent-stable: an Exemplar that still exhibits its
shape is kept, so a rule change rewrites only the entries it moved, and an
unchanged corpus rewrites the file byte for byte. A moved digest refuses a
replay rather than silently changing the subject of a comparison.

**Fact Shape Census.** The coverage authority is a census over the GothamChess
ladder, **union the Player-selected family the ladder structurally cannot
exhibit** — contributed by the corpus's own constructed `selectedMoment` cases,
and reported as a separate count so the union is visible rather than implied.
**No Fact Shape count is pinned anywhere.** The count follows coverage; when the
rules move, the census moves with them.

The check is split by cost. A fast `#[test]` reads only the corpus and asks
whether every recorded Exemplar still resolves and still digests to what was
recorded — that runs in every gate, in fractions of a second. The census reads
the 65 MB ladder and is a deliberate CLI command
(`language_layer_prose_regression census`), which exits non-zero while any
censused shape has neither an Exemplar nor a recorded gap, and names the corpus
moments one axis away so the shape can be searched for.

**Filling a shape is a search before it is a construction.** The ladder supplies
the moment; a case is minted from a real Game with real recorded provider
evidence. Construction is for shapes the ladder cannot supply. Provider evidence
is never synthesized.

**A shape nothing can supply is a recorded gap** — dated, with the reason —
in `evaluation/corpus/fact-shape-gaps.json`. A gap keeps the census green; an
unfilled shape does not. The same file carries the one deliberate coarsening of
the derivation rule: the teaching theme behind `{takeaway}` is not a
discriminant, because it renders one of four fixed one-liners carrying no
moment-specific content, so the authoring variable is whether the model uses the
slot. Splitting on it would add nine shapes measuring one problem. That is a
hand decision, so it is written down beside the gaps rather than left implicit
in the code.

**The verdict is a per-shape delta against the prior recorded run, with no
pinned rate floor.** At roughly thirty generations a single failure is over
three points, and ADR 0050's 92 % was measured at n ≈ 40; a floor at this sample
size fires on noise. The question the instrument answers is *did this prompt
change make it worse*.

## Consequences

The count of generations a run issues is now a measurement rather than a
constant. As of 2026-09-01 the census observes **43** shapes — 37 from the
ladder, 6 from the Player-selected family — and every one of them is filled. It
started at 22 filled and 21 unfilled; the ladder supplied all 21, 14 Games
covered them between them, and nothing had to be constructed or recorded as a
gap. Among them were both combinations that ended the frozen set. The corpus
grew from 16 cases to 31 to get there, which is the real price of the decision
and is paid in one place rather than spread through a spec.

A contract change to `GameReviewMomentClassification`, its qualification
reasons, its payoffs, or the residual classification now fails to compile until
someone decides whether the new variant is a shape axis. That decision was
previously made by nobody, and its answer was "no".

**The census authority is a snapshot, and a change to what a moment carries
invalidates it.** The ladder is 142 reviewed Games on disk, produced by the code
that was current when `gotham review` last ran. Change what a Review Moment
carries — a new derived fact, a new marker — and the ladder's moments still
exhibit the old shapes while the corpus, which is replayed live, exhibits the
new ones. The census then reports the corpus as unfilled against shapes that no
longer exist. This is not a defect in the census; it is the census correctly
reporting that its authority is stale. The remedy is the one step 0 already
established: re-review the ladder, about half an hour of provider time, as part
of any change to the fact surface. Plan for it in the change, not after it.

The corpus grows deliberately. Each unfilled shape either gains a case minted
from a ladder Game or gains a dated gap. Neither is free, and both are visible
in review — which is the point, because the frozen set's failure mode was a
coverage hole that looked like a passing test.

Cost is not the constraint on breadth. The first baseline — 43 filled shapes,
one generation each, on the pinned route — cost **$0.0411** and took about two
minutes. The binding limit is how much prose a human will read, so a run record is
ordered rarest shape first.

The no-floor decision survives its first contact with data, for a better reason
than the one it was made on. Two independent same-prompt pairs each moved **zero
shapes**: the pin fixes temperature off and seed on, so one generation per shape
is reproducible, and a shape that moves between two runs is a change in the
prompt, the facts, or the harness rather than sampling noise. A rate floor would
still fire on noise across *sets* of shapes as coverage grows; a per-shape delta
does not need to.

That reproducibility earned its keep immediately. The instrument's first finding
was a defect in the runner — it authored every shape with no Player-intent
context, a state production reaches only on Neutral moments, and the gate refused
the hedged plan sentence the prompt asks for. Fixing the harness moved the run
from 13/23 to 18/23. The one intent rejection that survived is real, and three
prompt edits aimed at it each cost five shapes to fix one, with the damage
landing on marker discipline rather than on the rule edited. All three were
reverted on that evidence. Addressing by shape is what made "this edit cost five
shapes, here they are" sayable at all; a publish rate alone would have shown 18 →
13 with nothing to point at.

The runner spends real money, so it stays out of the product CLI: census,
resolve, and later run and compare live in a separate `[[bin]]`,
`language_layer_prose_regression`.

§5.1's nineteen generations and §2's hand-written thirteen-shape taxonomy are
superseded. The derived Fact Shape is finer than §2 on `{takeaway}` presence and
coarser on nothing.

## Alternatives within the shape

**Keep `(case, ply)` addressing and re-point after each rule change.** This is
what died. It cost a spec edit per rule change and still ran out of corpus.

**Key on marker names alone, or on the §2 fact shape alone.** Neither subsumes
the other: markers see `{takeaway}` presence and collapse mate; discriminants
see mate and collapse marker presence. The key is their product.

**Enumerate the shapes in a specification.** That is the frozen set again with a
different file name. A count in prose cannot be re-derived when the rules move.

**Census over the corpus instead of the ladder.** The corpus would be measuring
itself: every shape it exhibits is by construction filled, and every shape it
lacks is invisible. The ladder is the only sample of what production actually
produces.

**Census over the ladder alone, without the Player-selected union.** Cheaper and
wrong: Neutral and Terminal never appear in a full-Game review, so a whole
branch of the comment gate — its own marker set, its own forbidden literals, its
own one-line target — would go unmeasured while the census reported full
coverage.
