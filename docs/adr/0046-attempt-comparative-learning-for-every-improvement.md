# Attempt Comparative Learning for Every Improvement

## Status

Accepted.

## Context

Automatic Decision Explanation used a proof-valid SinglePV concept as the cost
gate for MultiPV enrichment. That made comparative-only concepts impossible to
discover: for example, `DefensiveMove` needs ranked alternatives and therefore
cannot prove its own eligibility from SinglePV evidence. A rejected or failed
optional MultiPV search also replaced proof-valid SinglePV learning with an
empty result.

The frozen review did not record why chess-concept learning produced no track.
Missing candidate evidence, rejected comparison evidence, no proof-valid
concept, and a proven concept without a resource mapping all collapsed to the
same empty list.

## Decision

Every structurally valid Automatic Improvement Opportunity receives at most one
bounded `MultiPV = 3` comparison attempt. Positive Highlights keep the existing
SinglePV proof-positive cost gate. Comparison remains optional enrichment: an
unsupported adapter, provider failure, or rejected comparison falls back to the
authoritative SinglePV evidence. A valid comparison that proves no concept
honestly abstains.

Every Automatic Review Moment persists exactly one typed decision-learning
outcome: a selected track, a proof with no exact resource mapping, or abstention
with one stable reason. `NotAttempted` is reserved for lightweight
Player-Selected moments before their on-demand pass. Explanation references,
curriculum tracks, and the outcome are validated as one invariant; independently
selected Opening tracks may coexist with any outcome.

The internal review durability schema and analysis generation advance together
so existing frozen reviews cannot masquerade as results of the new policy. The
pre-release public Decision Explanation, Knowledge Graph, and Learning Plan
contract labels remain V1; this changes eligibility and persisted orchestration
state, not proof semantics, graph content, or resource identity.

## Consequences

Improvement Opportunities can now discover proof families that require ranked
alternatives, while optional Engine Analysis failures no longer erase an
ordinary proof-valid lesson. A moment may still have no Learning Track: the
Engine must never manufacture a concept or resource when exact proof or mapping
is absent. Surfaces and host narration can state that bounded result instead of
presenting a silently empty learning section.
