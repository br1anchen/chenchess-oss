# Classify Learning Motifs from Best and Refutation Lines

## Status

Accepted.

## Context

ADR 0035 made the Game Review Engine authoritative for Learning Plans, but the
first selection policy classified an Improvement Opportunity only from the
engine-best line. That answers “what should I have played?” but misses
instructional ideas that exist only in the opponent's punishment of the
played move.

The canonical `13...Bxb5 14.cxb5` moment demonstrates the gap. The best line
does not satisfy an accepted motif predicate, while the post-move engine line
shows the bishop being captured without an immediate recapture. The Review
Session already retains both lines.

Lichess does not expose an API that classifies an arbitrary Position or
principal variation. Its puzzle themes are the scalable learning-resource
surface, while Practice is a fixed curriculum with exact companions for only
some themes. The supporting primary-source analysis is recorded in
[Mapping missed chess ideas to Lichess learning material](../research/lichess-missed-motif-learning-mapping.md).

## Decision

Learning Plan selection policy v3 generalizes the engine-backed instructional
episodes for an Improvement Opportunity:

1. the missed-best episode, starting from the pre-move Position and following
   the engine-best line; and
2. the conceded-refutation episode, applying the played move as the setup and
   following the post-move engine line from the resulting Position.

Both episodes must be legal and are evaluated by the same typed,
ChenChess-native motif predicates. A motif key may contribute at most one
support for a Critical Moment. Existing missed-best evidence wins a same-key
tie, preserving stable evidence where the new episode adds no information.
Invalid or unavailable post-move lines fail closed without suppressing valid
best-line, endgame, or opening candidates.

The initial v2 expansion applies the existing `fork` and `hangingPiece`
predicates to bounded prefixes of both episodes. The canonical
`13...Bxb5 14.cxb5` refutation therefore maps to the exact Lichess
`hangingPiece` puzzle stream. It has no Learn resource because Lichess has no
exact hanging-piece Practice module.

The original motif release used `learning-resources/2026-07-25`; the complete
curriculum activation advances the catalog to `learning-resources/2026-08-03`.
Previously mapped resource identities and URLs remain stable while the pinned
catalog adds exact decisions for the complete curriculum. The
Language Layer receives only materialized tracks and must never infer a motif,
author a URL, substitute an adjacent Practice lesson, or disguise an empty
track set as mapped material.

## Consequences

- Bad moves can now produce learning material from the tactical punishment
  they concede, not only from a tactical mechanism in the best line.
- The frozen Gotham audit covers both episode types and pins the additional
  refutation-positive cases.
- The canonical cross-surface conformance journey requires the grounded
  hanging-piece track at ply 26.
- Adding more Lichess themes requires independent typed predicates and corpus
  validation; evaluation loss, phase, or a suggestive move name is not enough.
- The proposed `advancedPawn` mapping for the later `...b3` line is rejected:
  it does not meet Lichess's exact advanced-pawn rank predicate.
