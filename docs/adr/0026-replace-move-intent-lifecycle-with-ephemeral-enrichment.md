# Replace the Move Intent lifecycle with ephemeral enrichment

## Status

Accepted. This decision supersedes the intent-selection, intent-interaction,
intent-validation, intent-calibration, intent-trace, intent-specific retention,
and eager intent-preparation portions of ADRs 0017–0021. Their unrelated
fact-boundary, kind-aware authoring, retention-governance, real-seam contract,
chronological-session, and atomic-publication decisions remain accepted.

## Context

Move Intent is private to the Player. Stockfish, Maia, and a Language Layer can
suggest a plausible plan, but none can verify the Player's purpose. The existing
Review Session nevertheless treats a selected hypothesis as durable product
state: it ranks or abstains under a versioned policy, retains provider evidence
and traces, asks the Player to resolve the hypothesis, assesses the response,
propagates intent into later coaching, and reconstructs that state on restore.

That lifecycle adds latency and contract, persistence, restoration, UI, and
evaluation complexity without making the subjective claim objectively
knowable. The contract is unreleased, so carrying both old and new versions
would preserve accidental complexity rather than protect a deployed client.

## Decision

Move Intent remains Player-provided conversational context. It is not canonical
Review Session state or a retained record. A Coach Intent Hypothesis is one
explicitly uncertain sentence inside a Review-Side Critical Moment Comment. It
cannot establish classification, praise, correction, grade, or objective chess
facts.

For an unpublished Positive Highlight or Improvement Opportunity whose mover
belongs to the Review Side, the Coach Engine may lazily build Intent Enrichment
during comment authoring:

1. Hold the played move fixed.
2. At the resolved Elo Profile, use Maia for both sides to retain the three
   highest-joint-probability continuations over four half-moves.
3. Ask Stockfish to evaluate every candidate leaf from the Player's
   perspective.
4. Select the most favorable leaf, breaking equivalent evaluations by joint
   Maia probability.
5. Independently obtain Objective Counterplay from Stockfish, beginning with
   its strongest reply.

Stockfish scores candidate leaves but never contributes a move to the Projected
Plan. The Language Layer receives only the selected Projected Plan SAN line,
the separate Objective Counterplay SAN line, grounded Review Moment facts, and
classification-aware instructions. Candidate lines, probabilities, leaf
scores, selection details, provider identifiers, and traces remain inside that
single authoring attempt.

There is no confidence threshold, captured-mass threshold, probability floor,
abstention state, unavailability state, Intent Selection Policy version, or
intent-accuracy claim. If enrichment is unavailable, authoring continues from
the played move and grounded facts and may still offer one reasonable,
explicitly uncertain hypothesis. Neutral and outside-Review-Side moments offer
none.

Improvement Opportunity instructions may contrast the anticipated Projected
Plan with Objective Counterplay that disrupts it. Positive Highlight
instructions preserve the grounded achievement and describe Objective
Counterplay only as strongest defense. The Grounding Gate continues to validate
objective claims and every concrete chess literal, including literals inside
the hypothesis. It does not compare the subjective interpretation with Maia or
the Player's private purpose.

The complete published comment is the only canonical output that may contain a
Coach Intent Hypothesis. Publication checkpoints that comment atomically.
Resume and reopen return it exactly without reconstructing Intent Enrichment or
calling Maia or Stockfish. A failed pre-publication attempt commits no partial
intent state and may retry with fresh ephemeral evidence.

The active product removes hypothesis confirmation, correction,
provide-another, discussion-as-resolution, skip, clarification, and Intent
Assessment transitions and controls. It also removes inherited intent and
intent-fit fields from Coach Turns and Alternative Move Exploration. Agents may
optionally request one stateless Player Plan Evaluation when conversational
wording would benefit from engine grounding. That operation resolves
authoritative Position facts and Objective Counterplay server-side, uses one
request-scoped Language Layer exchange, validates concrete chess claims, and
retains neither its input nor output.

General Review Snapshots remain governed by the existing consent,
pseudonymization, ownership, access, expiry, Dataset Admission, and withdrawal
rules. They may retain final Player-visible prose as ordinary commentary, but
contain no Intent Enrichment, intent-only provider replay input, selection
trace, separately queryable hypothesis, Player wording, or Intent Response
Record.

The unreleased contract is replaced in place. There is no policy `v2`,
compatibility alias, dual decoder, or migration reader. Incompatible staging
Review Session checkpoints are disposable and may be deleted after a
fail-closed target audit. Production data is not part of that cleanup.

## Consequences

Session start admits chronological moments and prepares objective facts without
intent-provider latency. Intent work happens only for the first authoring
attempt of an applicable unpublished comment. Published comments restore
without provider replay, and transient provider failure cannot fail the Game
Review.

The active domain and wire contracts become smaller: typed intent lifecycle
state, traces, responses, calibration concepts, commands, intent-fit fields,
and intent-specific retained artifacts disappear. Checked-in recordings,
generated contracts, prototypes, delivery-surface guidance, and certification
must describe the simplified behavior directly.

Review Snapshots continue to support general explanation-quality evaluation.
Retained hypothesis prose is not an intent-accuracy label and cannot support a
calibrated confidence claim.
