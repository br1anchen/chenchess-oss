# Select Learning Plans in the Game Review Engine

## Status

Accepted. Supersedes ADR 0015.

## Decision

The Game Review Engine replaces the language-selected Practice Recommendation with a required, Rust-selected `LearningPlan` on every `GameReview`. The plan records one immutable selection-policy version, one Learning Resource Catalog version, and the canonically ordered union of `LearningTrack` values selected at Automatic Critical Moments. Each moment selects zero, one, or two tracks—the earliest causal idea and at most one independent payoff or resulting pattern—but the Game-level union has no separate track cap. Tracks with the same semantic `LearningTrackKey` aggregate nonempty Game-ordered improvement or reinforcement support with complete kind-matched motif, endgame, curriculum, or opening evidence and fully materialized verified `LearningResource` values. Track kind and aggregate purpose are derived rather than duplicated.

Candidate construction fails closed per candidate: invalid evidence or resource mappings omit only that candidate with internal diagnostics. Selection accepts validated candidates only; duplicate keys, mixed versions, malformed candidates, or an invalid selected result fail Game Review construction and never masquerade as an empty plan. Rust owns target eligibility, aggregation, ranking, resource identity, instructional role, concrete resource kind, canonical URL, and release verification.

Every Review Moment carries fully materialized `ReviewMomentLearningMaterial` under the same track contract. An Automatic moment receives its exact moment-selected subset of the frozen Learning Plan, while a Player-Selected Critical Moment may receive an independent zero-to-two-track local selection without joining or mutating the plan; a neutral Player-Selected Moment receives none. Raw detector matches, aliases, parents, fallback candidates, and redundant matches remain internal evaluation data and cannot reappear through Game-level aggregation. The active material may enter Review Moment Comment Facts, but the Language Layer sees neither rejected candidates nor catalog or ranking internals, never authors URLs, and must ground any learning claim through the existing Grounding Gate.

`GameReview.practiceSelection`, the language-layer Practice proposal and validator, and the free-form Draft Game Review `lesson` and `trainingPlan` fields are removed rather than wrapped or retained in parallel. Because the contract has not been released, the change is made in place without legacy wire decoders, aliases, dependency-version bumps, or compatibility versions. Incompatible Firebase data is reset if necessary; a persisted-store version changes only when that reset requires one.

The highest contract seam is one canonical mixed-review conformance journey across Web, Coach Skill, and Coach App. Focused Rust selector and validator tests, Web empty/local/multi-track surface tests, generated-schema drift checks, the frozen 141-case precision and coverage corpus, and release-time Learning Resource verification supplement that journey.

The complete contract decisions and accepted evidence vocabularies are indexed by [Define the LearningPlan contract and domain rules](#136).
