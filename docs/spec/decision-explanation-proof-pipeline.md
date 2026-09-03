# Decision Explanation Proof Pipeline

## Status

Accepted design, implemented. The pipeline lives in
`services/coach-engine/src/decision_explanation/` and
`services/coach-engine/src/review_facts/decision_explanation.rs`; ADR 0041
amends the evaluation reconciliation recorded below, and ADR 0046 amends
Automatic Improvement eligibility and comparison fallback.

This specification records the decisions from the learning-proof architecture
grill completed on 2026-08-04. ADR 0038 records the architectural commitment;
this document supplies the implementation contract.

Amended 2026-08-04 after design review: added Concept migration disposition,
the construction-graph recompute invariant, and Deferred-work notes. No grill
decision is changed.

## Purpose

Replace concept-first Learning Track selection and its ephemeral structural DAG
with one graph-led explanation pipeline:

```text
Position
→ Decision Candidates
→ legal replay and candidate enrichment
→ candidate-owned Atomic Chess Facts
→ Chess Knowledge activation
→ optional Position Goal
→ candidate-local concept validation
→ Semantic Outcomes
→ engine and semantic comparison
→ minimal selected Explanation Paths
→ Learning Track projection
```

Stockfish remains authoritative for the best move, MultiPV ordering,
evaluations, retained variations, and engine provenance. Deterministic local
chess rules remain authoritative for facts, concepts, goals, outcomes, and
proof validity.

The proof explains what the retained evidence establishes. It does not
reconstruct Stockfish's internal reasoning and does not infer the Player's
intent or failed mental step.

## Scope

### V1 includes

- Every chess-concept detector that can currently execute, migrated to the new
  proof model.
- Replacement or removal of detector implementations that cannot satisfy the
  new proof obligations.
- One compiled, versioned Chess Knowledge Graph.
- One provider-free Decision Explanation module.
- Automatic Improvement Opportunity and Positive Highlight explanations.
- On-demand Player-Selected explanations using frozen SinglePV evidence.
- Complete selected minimal proof durability.
- Existing zero-to-two Learning Track projection and exact Learning Resources.
- Existing two-stage Missing Idea and Idea Reinforced presentation.
- Opening learning through its existing independent exact mapping path.

### V1 defers

- Proof graph or semantic-comparison rendering.
- Interactive backward traversal and failed-step localization.
- Claims about Player intent or a specific reasoning failure.
- Concept-generated or forcing-move-generated candidates.
- Cross-game learner state, mastery, or adaptive proof minimization.
- Independent knowledge publication, editing UI, or Firestore-backed knowledge.
- New concepts beyond the currently executable surface.
- Arbitrary performance thresholds or new performance release gates.

## Concept migration disposition

Coverage loss under the replace-or-remove clause must be a decision, not a
discovery. Every curriculum concept is assessed here against the V1 fact
vocabulary before implementation.

### Directly provable with V1 facts

- Attack relationships: fork, pin, skewer, x-ray, discovered attack and check,
  double check, hanging piece, trapped piece, overloaded piece,
  capture-the-defender/undermining, attacking f2/f7, exposed king, kingside
  and queenside attack (via `KingZonePressure`).
- Defender manipulation and line ideas: deflection, attraction, interference,
  clearance, collinear move (via `AttackSet`, `SoleRayBlocker`, transition
  facts).
- Tempo ideas: intermezzo and desperado (grounded by `LegalRecaptures`),
  counter check (grounded by `CheckersChanged`), Greek gift and sacrifice
  (`MaterialBalanceChanged` with later payoff).
- Mating: all named patterns, piece checkmates, checkmate technique
  (`Checkers`, `LegalDestinations`, occupancy geometry, `TerminalPosition`).
- Pawn and endgame: advanced pawn, promotion, underpromotion
  (`PawnFrontSpanOccupancy`, `PiecePromoted`), key squares, opposition,
  Lucena, Philidor, rook technique, material endgame classes
  (occupancy geometry, `MaterialInventory`).
- Special moves: castling, en passant (typed move facts).

### Provable only with an explicit scoped recognition rule

| Concept                         | V1 scope                                                                                                                                                                                                                                                                           | Outside scope |
| ------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------- |
| Zugzwang                        | Mobility-collapse subclass: the constrained side's complete `LegalDestinations` where every destination is attacked-and-undefended per `AttackSet` or steps into a provable adverse transition on the retained variation. No engine counterfactual over unretained opponent moves. | Abstain       |
| Defensive move                  | Comparative evidence only: the focal candidate avoids an adverse outcome that a `Refutes`-class comparison establishes for every retained alternative. SinglePV builds abstain.                                                                                                    | Abstain       |
| Quiet move                      | Absence via complete-set facts: no capture or check move facts, and no adverse `CheckersChanged` or `MaterialBalanceChanged` on the immediate reply.                                                                                                                               | Abstain       |
| Equality / Advantage / Crushing | Typed state change only: conventional-value `MaterialBalanceChanged` under the versioned value policy, or `TerminalStateReached` draw. Engine magnitude stays in the Engine Assessment.                                                                                            | Abstain       |

### Removed

None. Full-generality zugzwang and defensive-move recognition fall to honest
abstention rather than removal. Metadata filters (phase, length, origin,
healthy mix, mate length) were never concepts.

Migration slices implement the scoped rules above or abstain; the cutover
verifies that no concept silently disappeared and reports abstention counts
per disposition class.

## Behavioral contract

### Automatic Critical Moments

Both Improvement Opportunities and Positive Highlights are eligible.

1. Run `preflight_decision` with SinglePV evidence.
2. A structurally valid Improvement Opportunity always returns
   `NeedsCandidateComparison`, including when no SinglePV concept is proven.
   Positive Highlights retain the proof-valid SinglePV cost gate.
3. When the result is `NeedsCandidateComparison`, request at most one
   `MultiPV = 3` search if the Engine Analysis adapter supports it.
4. Run `explain_decision` with `CandidateEvidence::MultiPv` when enrichment is
   valid. If comparison is unsupported, fails, or is rejected, fall back to
   `CandidateEvidence::SinglePv`; a valid comparison that proves no concept
   remains an honest abstention.
5. Project zero-to-two chess-concept Learning Tracks only from selected,
   validated Explanation Paths.
6. Merge independently selected Opening tracks.
7. Persist one explicit decision-learning outcome for every Automatic moment,
   then aggregate only Automatic moment projections into the frozen Learning
   Plan.

The SinglePV preflight is a non-authoritative cost filter. It may establish
eligibility but cannot author the final Automatic explanation when MultiPV
evidence is available.

### Player-Selected Moments

Player-Selected is moment provenance, not an evidence type or capability.

1. Keep the import-time Player-Selected moment data lightweight.
2. On first opening, call `explain_decision` with frozen
   `CandidateEvidence::SinglePv`.
3. Project local Learning Tracks and merge independent Opening tracks.
4. Persist the result in that Review Session's moment checkpoint.
5. Reuse the checkpointed result for later opens in the same session.
6. Never join or mutate the Game-level Learning Plan.

Neutral Player-Selected Moments remain without learning material.

No new engine request occurs during Player-Selected proof generation.

### Coaching-focal path ownership

Only these candidates may own selected Explanation Paths:

| Moment kind             | Attribution         | Path-owning candidate |
| ----------------------- | ------------------- | --------------------- |
| Improvement Opportunity | Missed Best         | engine rank-one root  |
| Improvement Opportunity | Conceded Refutation | Player-move variation |
| Positive Highlight      | Reinforcement       | Player's move         |

Other MultiPV candidates are replayed and enriched only for comparison.

## Module interface

The external seam is intentionally small:

```rust
fn preflight_decision(
    input: DecisionPreflightInput,
) -> DecisionPreflightResult;

fn explain_decision(
    input: DecisionExplanationInput,
) -> Result<DecisionExplanationBuild, DecisionExplanationContractError>;
```

### Input types

```text
DecisionPreflightInput
  occurrence
  canonical pre-move Position Snapshot
  Critical Moment classification and provenance
  Player move
  authoritative SinglePV evidence

DecisionPreflightResult
  Ineligible
  NeedsCandidateComparison

DecisionExplanationInput
  occurrence
  canonical pre-move Position Snapshot
  Critical Moment classification and provenance
  Player move
  CandidateEvidence

CandidateEvidence
  SinglePv
  MultiPv
```

`CandidateEvidence` owns exact engine analysis and provenance. Moment
provenance remains a separate field.

### Output types

```text
DecisionExplanationBuild
  Durable
    DecisionExplanation
    projected chess-concept Learning Tracks
    transient diagnostics
  Abstained
    transient diagnostics
```

`DecisionExplanationContractError` is reserved for contradictory state that
cannot safely produce a valid aggregate. Candidate-local failure and
unavailable optional evidence produce diagnostics and abstention.

### Ownership

The Decision Explanation module owns:

- candidate normalization and legal replay;
- Atomic Chess Fact extraction;
- Chess Knowledge Graph lookup and recognition;
- goal instantiation;
- concept validation;
- Semantic Outcome derivation;
- engine and semantic comparison;
- deterministic minimization;
- proof validation and capability derivation; and
- chess-concept Learning Track projection.

Callers own:

- Engine Analysis calls and concurrency;
- import and Review Session orchestration;
- Game-level Learning Plan aggregation;
- separate Opening track selection and merging; and
- durable store writes.

No caller may construct a chess-concept Learning Track directly.

## Candidate evidence

### Candidate universe

V1 candidates are limited to:

- returned MultiPV roots;
- the authoritative SinglePV root; and
- the Player's move when it is not already present.

No detector, knowledge rule, or proof builder may add a candidate.

A candidate has non-exclusive origins. A move may be both Player-played and
engine-ranked. Candidate identity is its legal root UCI move within the moment.

### MultiPV invariants

- Requested count is exactly three.
- Returned count matches the legal-root count up to three.
- Ranks are contiguous and start at one.
- Every variation begins with its candidate root.
- Every retained move legally replays from the exact Position.
- Candidate assessments share exact engine provenance.
- Every published absolute states one perspective, including the injected Player
  move.
- **Ranked alternatives are ranks two and up only.** Rank one is the
  authoritative SinglePV record and is not restated.
- Candidate roots are unique.
- The Player move is injected once when absent.
- Maximum retained candidates are one authoritative root, two ranked
  alternatives, and one Player move.

Invalid MultiPV evidence is rejected and the moment falls back to its
authoritative SinglePV evidence. It never changes the authoritative best move
or erases a proof-valid SinglePV Learning Track.

### Which search owns which number

SinglePV owns the **absolute**; MultiPV owns the **ordering**. The two searches
score the same before-position differently — MultiPV disables pruning SinglePV
uses — so neither is asked for the other's answer.

`objective.*`, the centipawn loss, the classification, and moment selection are
all SinglePV and untouched by the comparison pass. A `RankedAlternativeEvidence`
carries no absolute at all: it states a `CandidateGap`, its shortfall against
rank one measured **inside that one MultiPV search**. `EngineAssessment`
therefore holds an `EngineAssessmentScore` that is either `Absolute` (the
authoritative record, or the position after the Player's move — both SinglePV) or
`BehindBest`.

A gap is not an absolute. Presentation asserts the comparison — "this alternative
fell 70cp short" — and never the alternative's own worth. See ADR 0041.

## Identity and versioning

`DecisionExplanationRef` is content-derived from:

```text
GameRef
CriticalMomentId
DecisionExplanationGeneration
canonical selected proof content
knowledge and detector versions
exact engine provenance
```

Canonical hashing excludes the reference field being derived and any transient
diagnostics.

Other semantic references are also content-derived:

- `DecisionCandidateRef`
- `PositionSnapshotRef`
- `LineStepRef`
- `AtomicFactRef`
- `ExplanationPathRef`
- `EngineAssessmentRef`
- `SemanticOutcomeRef`
- `KnowledgeNodeRef`
- `KnowledgeRuleRef`

`DecisionExplanationGeneration` versions the fact vocabulary, goal and outcome
semantics, comparison rules, validator, and minimization policy as one
reproducibility boundary. Each activated concept also retains its exact
recognition-rule version.

Learning Plan selection and Learning Resource Catalog versions do not
participate in Decision Explanation identity. They affect only downstream
Learning Track and Learning Path identity.

Any identity-affecting change creates a new frozen import result; existing
results are never rewritten.

## Atomic Chess Facts

An Atomic Chess Fact is independently recomputable and concept-neutral. It may
record a complete deterministic set so that absence is provable; a missing
fact never implies a negative fact.

### Snapshot facts

Every snapshot fact references one exact Position Snapshot.

```text
PieceOccupancy
  side, role, square

AttackSet
  attacking piece and complete attacked-square set

SoleRayBlocker
  sliding attacker, sole blocker, target

Checkers
  king and complete checking-piece set

KingZonePressure
  king, king-zone squares, complete attacking-piece set

LegalRecaptures
  side, target square, complete legal recapture set

LegalDestinations
  piece and complete legal destination set

PawnFrontSpanOccupancy
  pawn, relevant forward same/adjacent-file squares,
  opposing pawns occupying them

MaterialInventory
  side and counts by role

TerminalPosition
  ongoing, checkmate, stalemate, or draw

PhaseClassification
  phase and phase-policy version
```

### Move facts

Every move fact references one exact Line Step.

```text
PieceMoved
  side, role, from, to, UCI

PieceCaptured
  capturing move and captured side, role, square

PiecePromoted
  move, pawn origin, promotion square and role

Castled
  side, wing, king transition, rook transition

EnPassantCaptured
  move, capturing-pawn transition, captured-pawn square
```

Several facts may describe one move. Promotion by capture emits
`PieceMoved`, `PieceCaptured`, and `PiecePromoted`.

### Transition facts

Every transition fact references one Line Step and its before/after facts.

```text
AttackSetChanged
  piece, before/after AttackSet refs, added/removed squares

CheckersChanged
  before/after Checkers refs, added/removed checkers

MaterialChanged
  before/after MaterialInventory refs, exact delta

TerminalStateChanged
  before/after TerminalPosition refs
```

Opened rays, discovered attacks, pins, forks, and other teaching concepts are
derived from these facts. They are not additional fact variants.

## Chess Knowledge Graph

The graph is repo-authored, build-validated, and compiled into Coach Engine.
Runtime uses indexed in-memory lookups and requires no database, file I/O, or
artifact parsing.

### Node types

```text
Concept
RecognitionRule
GoalTemplate
Procedure
ResourceMapping
```

### Edge types

```text
Refines          Concept → Concept
Prerequisite     Concept → Concept
Related          Concept ↔ Concept
Counters         Concept/Procedure → Concept
RecognizedBy     Concept → RecognitionRule
SuggestsGoal     Concept → GoalTemplate
UsesProcedure    Concept → Procedure
MapsToResource   Concept → ResourceMapping
```

### Graph invariants

- Every migrated concept has at least one `RecognizedBy` edge.
- Goal templates and procedures exist only when truthful Candidate Generation
  Proof is supported.
- Pedagogical relationships may be sparse.
- `Refines` is acyclic.
- `Prerequisite` is acyclic.
- `Related` and `Counters` may create cycles.
- Difficulty is a typed property of Concept or Procedure.
- Learner progression initially uses `Prerequisite`; no separate progression
  relation exists before cross-game learner state.
- Every selected chess-concept path resolves one exact Concept,
  RecognitionRule, and ResourceMapping.

The Knowledge Graph is reusable knowledge. It contains no Position-specific
candidate, outcome, proof capability, or Player state.

## Position Goals

Position Goals and Semantic Outcomes are duals:

```text
Position Goal = desired candidate consequence
Semantic Outcome = observed variation consequence
```

`goal.is_satisfied_by(outcome)` is deterministic and typed.

The V1 Position Goal vocabulary is:

```text
GainMaterial
CreateAttackAccess
RemoveAttackAccess
IncreaseLegalMobility
RestrictLegalMobility
ApplyCheck
ResolveCheck
IncreaseKingZonePressure
ReduceKingZonePressure
AdvancePawn
PromotePawn
ReachMaterialConfiguration
ReachTerminalState
```

`GainMaterial` is the first compiled goal template. The remaining names reserve
the domain vocabulary until their matching Atomic Facts, Semantic Outcomes,
and truthful pre-move rules are implemented; they are not serialized as
unsupported contract variants.

Every goal identifies concrete pieces and squares or a typed terminal or
material target. Goals contain no concept label, free-form priority, or engine
score.

Urgency and ordering are derived from proof structure, payoff step, and engine
comparison rather than duplicated in a goal.

## Semantic Outcomes

Semantic Outcomes are reusable, concept-neutral state changes:

```text
MaterialBalanceChanged
  inventory refs, exact pieces gained/lost/promoted,
  conventional-value delta and value-policy version

AttackAccessChanged
  AttackSet refs, added/removed squares and occupied targets

LegalMobilityChanged
  LegalDestinations refs, added/removed destinations

CheckStateChanged
  king and Checkers refs

KingZonePressureChanged
  KingZonePressure refs and attacker-set delta

PawnProgressed
  pawn identity, square transition, front-span change,
  optional promotion result

MaterialConfigurationChanged
  MaterialInventory refs and typed material-class transition

TerminalStateReached
  ongoing before-state and non-ongoing terminal result
```

Every Semantic Outcome contains a meaningful nonempty change and references
its supporting facts. A concept name and an Engine Evaluation are not Semantic
Outcomes.

## Proof aggregate

The persisted contract is a typed aggregate whose semantic references form a
DAG. It is not a generic public node-and-edge container.

```text
DecisionExplanation
  ref
  generation
  knowledge_graph_version
  moment Position ref
  shared snapshots and facts
  candidates
  selected_paths[1..2]
  optional preference proof
  derived capability

DecisionCandidate
  ref
  non-exclusive origins
  root move
  retained variation
  snapshots and line steps
  Semantic Outcome refs
  Engine Assessment ref

ExplanationPath
  ref
  attribution
  candidate_ref
  knowledge_activation
  optional CandidateGenerationProof
  required ConceptValidationProof
  nonempty outcome_refs

KnowledgeActivation
  concept_node_ref
  recognition_rule_ref
  supporting_fact_refs

CandidateGenerationProof
  pre-move fact refs
  concept_node_ref
  instantiated Position Goal
  suggested candidate ref

ConceptValidationProof
  candidate_ref
  causal step ref
  payoff step ref
  recognition rule ref
  supporting fact refs
  nonempty outcome refs
```

The model-visible grounded path resolves an existing Candidate Generation
Proof into its exact `positionGoal`; absence stays absence and never becomes an
inferred Engine intention. It also derives a `materialTransaction` from every
capture and promotion in the selected candidate's complete legal `line_steps`.
That ordered, root-side transaction may extend beyond the six plies retained
for spoken variation, so an apparent sacrifice is not narrated before a later
recovery. This is a read-time projection only: it does not retain pruned
Semantic Outcomes, parse SAN for chess facts, or change proof identity.

The typed structure encodes these semantic relationships:

```text
facts activate knowledge
knowledge suggests a goal
goal suggests a candidate
variation facts support a recognition rule
the rule validates a concept realization
the variation produces outcomes
```

A Conceded Refutation path is owned by the Player-move candidate but explicitly
records the opponent's causal and payoff steps. The opponent's concept must
never be attributed to the Player.

### Generation versus validation

`CandidateGenerationProof` is optional. Some concepts can be recognized only
after a candidate line is replayed. The system must not fabricate a pre-move
discovery story for them.

`ConceptValidationProof` is mandatory. Every selected path proves:

1. exact candidate and variation legality;
2. candidate-owned before/after facts;
3. one versioned recognition rule validating one candidate-local concept;
4. at least one typed Semantic Outcome; and
5. a separate engine assessment.

A concept label alone never serves as evidence.

## Preference

```text
PreferenceProof
  preferred_candidate_ref
  engine_comparisons for every alternative
  zero or one semantic comparison per alternative

EngineComparison
  preferred and alternative candidate refs
  assessment refs
  ranks
  evaluation ordering
  engine provenance ref

SemanticComparison
  preferred and alternative outcome refs
  relation: Dominates | Refutes | Tradeoff
```

Relations mean:

- `Dominates`: preferred outcomes are no worse on all compared typed
  dimensions and strictly better on at least one.
- `Refutes`: the alternative permits a concrete adverse best-response outcome
  referenced by a Conceded Refutation path.
- `Tradeoff`: typed outcomes differ but do not establish dominance.

`Tradeoff` is explanatory only and cannot qualify `SemanticPreference`.

### Proof Capability

Capability is derived by final validation and never supplied by a caller.

```text
ValidationOnly
  candidate-local concept validation and Semantic Outcome;
  no comparative preference claim

EnginePreference
  ValidationOnly plus authoritative engine ordering
  against every retained alternative

SemanticPreference
  EnginePreference plus Dominates or Refutes coverage
  for every retained alternative
```

Sparse semantic comparisons may exist on an `EnginePreference` explanation
without upgrading its capability.

## Minimal sufficient proof

V1 minimization is fixed policy and does not vary by Elo:

1. Choose the valid realization with the earliest Semantic Outcome payoff.
2. Break ties by shortest cited variation prefix.
3. Break remaining ties by fewest Atomic Chess Facts.
4. Break final ties by canonical semantic ID.
5. Retain only facts needed for recognition, goal satisfaction, outcomes, and
   comparison.
6. Retain one validation for each selected concept path.
7. Store complete candidate records and provenance once at aggregate level.
8. Let each path cite only its required variation prefix.
9. For complete Automatic explanations, keep engine preference against every
   retained alternative.
10. Allow semantic comparisons to remain sparse.

The persisted object contains complete selected minimal proofs. Exhaustive
working matches, rejected concepts, and the construction graph are transient
evaluation diagnostics.

Recompute invariant: persisted `CandidateEvidence`, engine provenance, and the
pinned `DecisionExplanationGeneration` must remain sufficient to
deterministically recompute the full construction graph — including rejected
concepts — offline. No future change may trade selected-proof persistence for
loss of that sufficiency.

## Learning Track projection

For chess concepts:

```text
selected Explanation Path
→ Knowledge Node and Resource Mapping
→ Learning Track Key
→ materialized Learning Resources
→ Learning Track Support referencing ExplanationPathRef
```

Required invariant:

```text
selected chess-concept Learning Track
⇒ exactly one persisted selected Explanation Path
```

The reverse is not required. A valid selected explanation may have no projected
Learning Track when downstream resource materialization abstains.

`LearningTrackEvidence` is removed for chess concepts. Learning Track support
must not duplicate Atomic Chess Facts, detector proof, or candidate evidence.

Opening remains:

```text
Opening Identification
→ exact Opening Resource Mapping
→ Opening Learning Track
```

Opening does not enter Decision Explanation V1.

## Failure semantics

- Detector or path construction failure omits that path and emits a structured
  diagnostic.
- Invalid, inconsistent, unavailable, or failed MultiPV evidence falls back to
  SinglePV for that moment; ordinary SinglePV Game Review remains valid.
- Zero valid paths is a valid absence with empty chess-concept material.
- SinglePV evidence may validly produce `ValidationOnly`.
- No old detector evidence or legacy pipeline is a fallback.
- Missing concept fixtures do not disable a structurally proof-valid concept.
- A dangling Learning Track projection, unresolved knowledge reference,
  malformed proof, or capability mismatch is contradictory persisted state and
  fails construction.

In short: abstain on unavailable learning evidence; fail only before
contradictory state would be persisted.

## Durability and migration

### Automatic

`GameReviewCriticalMoment` carries:

```text
decision_explanation: Option<DecisionExplanation>
learning_material: ReviewMomentLearningMaterial
```

`None` represents no chess-concept proof. An empty `DecisionExplanation` is
invalid.

### Player-Selected

Import-time Player-Selected moment data does not carry a proof for every ply.
The first opened moment receives a SinglePV Decision Explanation and local
Learning Track projection in its Review Session checkpoint.

### Generation cut

The implementation advances together:

- Game Analysis generation;
- Game Import schema or durability generation;
- Review Session checkpoint schema;
- Decision Explanation generation;
- Learning Plan selection policy;
- Knowledge Graph version; and
- generated Review Session contracts.

Older Game Analysis, Game Import, and Review Session records are immediately
unusable by generation. They are regenerated on the next import or require a
fresh Review Session. They receive no compatibility decoder or in-place
translation and expire through existing retention. No one-time physical purge
is required.

## Canonical end-to-end fixture

### Input

```text
Position
  r2qk3/2p5/8/1N6/8/8/8/4K3 w - - 0 1

Classification
  Automatic Improvement Opportunity

Player move
  Ke2 (e1e2), evaluation 0

MultiPV
  1. Nxc7+ Kd7 Nxa8   +500
  2. Nd6+ Kd7          +300
  3. Na7 Kd7           +200

Engine provenance
  Stockfish 18 fixture, depth 16
```

### Expected result

- Four candidates: three ranked roots plus injected `e1e2`.
- One selected Missed Best path owned by `b5c7`.
- Advantage Knowledge activation validates the selected variation under the
  explicit proof-minimality policy.
- A separate Fork Candidate Generation Proof cites only the root position's
  two target occupancies and the knight's Legal Destinations.
- The Fork goal template instantiates `GainMaterial` for the king on e8 and
  rook on a8 because `c7` is reachable from the pre-move position and attacks
  both pieces.
- Payoff facts establish `Nxa8` and material gain.
- `MaterialBalanceChanged` Semantic Outcome.
- The later rook-gain outcome satisfies the Position Goal and is retained even
  though it is distinct from the Advantage validation outcome.
- Engine comparisons cover every alternative.
- Capability is `EnginePreference` because semantic preference does not cover
  every alternative.
- Derived Improvement Learning Track references the selected path and
  materializes the existing advantage resources.
- No legacy motif evidence is persisted.

## Validation

### Required behavioral checks

- Canonical fixture passes through persistence and Learning Track projection.
- Every selected path references only facts, steps, and outcomes from its
  candidate.
- Cross-candidate reference mutation fails validation.
- Knowledge references resolve against the pinned compiled graph.
- Content-derived IDs and serialization round trips are stable.
- No projected chess-concept track lacks exactly one valid path reference.
- Regenerated 141-case Gotham processing produces no malformed proof or import
  failure.
- Historical Learning Track counts are reported but are not compatibility
  assertions.
- Curated negatives that exist must remain negative.
- Missing positive or negative fixtures do not disable a proof-valid concept
  and do not block release.
- Abstention is valid and counted.

Tests should exercise observable behavior through the Decision Explanation
module interface. Obsolete helper-level detector and generic graph tests should
be replaced rather than layered underneath the new seam.

### Performance and storage

Preserve structural bounds:

- at most one MultiPV search per eligible Automatic moment;
- at most three engine-ranked roots plus the Player move;
- no Player-Selected engine request;
- existing provider deadlines;
- existing checkpoint document limits; and
- existing durable-store document limits.

Measure deterministic explanation CPU time and serialized size during
implementation. Use the measurements for engineering judgment. Do not add an
arbitrary latency, byte-size, p95, test, or release gate without a measured
baseline and a separate decision.

## Deferred-work notes

Non-normative pointers for post-V1 work; nothing here adds a V1 obligation.

- Criticality is derived, never stored: the rank-one versus rank-two
  assessment gap is computable from persisted Engine Assessments. Future
  learner-profile work should expose a read-time accessor rather than widen
  the schema. `depth_to_find` remains out — it violates the one-search bound.
- Reinforcement paths exist only at Positive Highlights, so persisted strength
  evidence is a highlight-biased sample. A future learner profile must weight
  strength evidence by moment eligibility, not treat it as uniform coverage.
- The Lichess practice-and-theme curriculum taxonomy (prerequisite families,
  learn/drill resource roles) remains external seed content for the Chess
  Knowledge Graph; an entry may join the compiled graph only when its concept
  has a proof-valid recognition rule.

## File ownership

### Add the deep module

```text
services/coach-engine/src/decision_explanation.rs
  external interface and module wiring

services/coach-engine/src/decision_explanation/
  candidate.rs
  facts.rs
  preflight.rs
  knowledge.rs
  knowledge/catalog.rs
  detectors/attack.rs
  detectors/line.rs
  detectors/mate.rs
  detectors/pawn.rs
  detectors/position.rs
  goals.rs
  outcomes.rs
  preference.rs
  minimization.rs
  validation.rs
  projection.rs
  tests.rs
  corpus_tests.rs
```

Internal files are cohesive implementation responsibilities, not external
seams. The Decision Explanation module has one interface.

### Add contract ownership

```text
services/coach-engine/src/review_session_contract/decision_explanation.rs
services/coach-engine/src/review_session_contract/decision_explanation/facts.rs
services/coach-engine/src/review_session_contract/decision_explanation/proof.rs
services/coach-engine/src/review_session_contract/decision_explanation/identity.rs
```

### Modify orchestration and durability

```text
services/coach-engine/src/lib.rs
services/coach-engine/src/review_facts.rs
services/coach-engine/src/review_facts/decision_explanation.rs
services/coach-engine/src/review_facts/game_review.rs
services/coach-engine/src/review_session_processor/readiness.rs
services/coach-engine/src/review_session_checkpoint*
services/coach-engine/src/game_import_store*
services/coach-engine/src/game_analysis_store*
services/coach-engine/src/review_session_contract/mod.rs
services/coach-engine/src/review_session_contract/game_review.rs
services/coach-engine/src/review_session_contract/learning.rs
services/coach-engine/src/pipeline_evaluation/learning.rs
packages/coach-engine-sdk/src/* generated contract outputs
```

### Reduce or remove old ownership

`learning_plan.rs` and its directory retain only Learning Plan aggregation,
Opening mapping, and shared Learning Resource materialization.

Move reusable chess rules into Decision Explanation and remove obsolete:

```text
learning_plan/proof_graph.rs
learning_plan/episode.rs
learning_plan/features.rs
learning_plan/mechanism.rs
learning_plan/fork.rs
learning_plan/hanging_piece.rs
learning_plan/endgame.rs
learning_plan/motif.rs
learning_plan/curriculum.rs
learning_plan/curriculum/detector.rs
learning_plan/curriculum/registry.rs
legacy helper-level tests replaced at the new interface
```

Exact resource definitions may move rather than be recreated. Opening mapping
and its focused tests remain independent.

`packages/ui/src/review/LearningPathCards.tsx` should require no behavioral
change.

## Implementation stop conditions

Stop and return to design if implementation requires any of the following:

- a concept-labelled Atomic Chess Fact;
- a concept linked to candidates it does not validate;
- a Semantic Outcome copied from Engine Evaluation;
- a Candidate Generation Proof fabricated from post-candidate facts;
- a caller-authored chess-concept Learning Track;
- a second provider or persistence seam inside Decision Explanation;
- Player-Selected MultiPV or import-time proof materialization for every ply;
- compatibility translation of old Learning Track evidence;
- runtime Firestore or file parsing for Chess Knowledge; or
- a new Player-facing proof or diagnosis UI.
