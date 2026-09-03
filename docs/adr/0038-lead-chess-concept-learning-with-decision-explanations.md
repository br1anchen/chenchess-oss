# Lead Chess-Concept Learning with Decision Explanations

## Status

Accepted.

## Context

The first Learning Plan architecture made the Game Review Engine authoritative
for typed learning material, but it selected concepts before MultiPV
enrichment and then built an ephemeral structural DAG around the result. Its
Fact nodes already contained concept-labelled evidence, goals repeated the
Learning Track Key, concepts connected to unrelated candidates, Outcome
repeated Engine Evaluation, and preference repeated MultiPV rank. The graph
therefore could not independently explain discovery, candidate-local
correctness, semantic consequence, or preference.

## Decision

Chess-concept learning is led by one deterministic `DecisionExplanation`
module. It first normalizes and replays the available Decision Candidates,
extracts candidate-owned Atomic Chess Facts, activates a compiled and versioned
Chess Knowledge Graph, validates concepts, derives Semantic Outcomes, compares
candidates, minimizes sufficient proof, validates the resulting aggregate, and
only then projects Learning Tracks. Opening Resource Mapping remains an
independent learning source.

One moment-local `DecisionExplanation` stores shared candidates and one or two
selected Explanation Paths. Every path requires candidate-local Concept
Validation Proof and at least one Semantic Outcome. Candidate Generation Proof
is optional because several valid concepts can be recognized only after a
candidate variation is replayed; its absence never becomes a claim about the
Player's reasoning. Proof Capability is derived as `ValidationOnly`,
`EnginePreference`, or `SemanticPreference`. Stockfish remains authoritative for the
best move, candidate ranking, evaluation, retained variations, and provenance.

Automatic Critical Moments use a non-authoritative SinglePV preflight followed
by at most one bounded MultiPV search. Player-Selected Moments use the same
module with SinglePV evidence on first use and freeze their `ValidationOnly`
proof in the Review Session checkpoint rather than multiplying proofs across
every selectable Game ply. The complete selected minimal proof is durable;
the exhaustive construction graph and rejected matches are not.

`DecisionExplanation` is authoritative for chess-concept Learning Tracks, whose
support references an Explanation Path instead of duplicating detector
evidence. The old motif, endgame, curriculum-detector, and proof-DAG
implementations are migrated or removed; an implementation that cannot satisfy
the new proof model must be replaced or abstain. Existing learning durability
generations are invalidated and regenerated without translation. Missing
per-concept fixtures do not disable a proof-valid concept or create a release
gate.

The reusable Chess Knowledge Graph is repo-authored, build-validated, and
compiled into Coach Engine. Its complete graph may contain cycles through
relationships such as Related and Counters, while Refines and Prerequisite
remain independently acyclic. Runtime operation requires neither Firestore nor
artifact parsing.

## Consequences

The current Missing Idea and Idea Reinforced Learning Path presentation remains
unchanged, and ADR 0037 continues to govern its two-stage Player-facing shape.
Rich proof rendering, interactive failed-step localization, Player-intent
claims, concept-generated candidates, cross-game learner state, and independent
knowledge publication are deferred.

This ADR supersedes ADR 0035 and ADR 0036 where they make embedded
`LearningTrackEvidence`, legacy episode detectors, or pre-MultiPV concept
selection authoritative. It preserves their Review Engine ownership,
missed-best, conceded-refutation, and reinforcement attribution, exact resource
materialization, and bounded engine-enrichment decisions.
