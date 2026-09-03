# Seam inventory: OpenRouter-backed web Language Layer

Research asset for [Inventory the web Language Layer and evaluation seams](#230),
a child of [Ship the tailored OpenRouter web Language Layer to beta](#229).

Scope: what the current repository already provides, what is missing, and what the deferred
[Ship the Game Review feedback loop](#14) and the separate
learning maps contribute. Code references are to the working tree at the time of writing.

## Headline

The Coach Engine is **already shaped for a hosted Language Layer and has none**. Every authoring
seam exists as a trait with a deliberate "unavailable" production implementation, the Grounding Gate
around one of them is written but has no caller, and the web UI already renders the safe-fallback
message. There is no HTTP client for any model provider anywhere in the workspace — `openrouter`
appears only in `CONTEXT.md`, `tickets.md`, `.scratch/`, and release scripts as prose
(as of writing; `tickets.md` and `.scratch/` have since been removed).

The work is therefore mostly **filling declared holes**, not carving new boundaries. The genuinely
new domain surfaces are the Coaching Profile, cost control, cross-environment Evaluation
Fingerprints, and Review Feedback Reports on the web.

## Reusable seams

### Authoring traits (the plug points)

| Seam | Where | Status |
| --- | --- | --- |
| `AlternativeMoveAssessmentAuthor` | `services/coach-engine/src/review_session_coaching.rs:103` | Wired into the processor (`review_session_processor.rs:83`); production binds `NoHostedLanguageLayer` (`review_session_runtime.rs:106`) which always returns `Err(ProviderUnavailableReason::LanguageLayer)`. **This is the Move Intent / Alternative Move discussion plug point.** |
| `CriticalMomentCommentAuthor` | `services/coach-engine/src/critical_moment_comment.rs:88` | Trait plus a full Grounding Gate; **no production implementation and no caller of `author_grounded_comment`**. The live comment path is host-submitted `publishReviewMomentComment` (Coach App). This is the Review Moment comment plug point. |

Both traits already take immutable, minimized typed input (`CoachTurnAuthorInput`,
`CriticalMomentCommentAuthorInput`) and return a draft plus grounding ledger — matching the map's
privacy constraint without redesign.

### Grounding Gate

`author_grounded_comment` (`critical_moment_comment.rs:196`) implements exactly the contract the
destination needs: fail closed on invalid tagged facts *before* any prose is requested, attempt
generation, ground the draft, **retry once**, then deterministic safe rendering by moment kind.
Outcomes are typed (`Authored { attempts }` / `SafeRendered { attempts, reason }`) and the
provenance is validated by `is_valid_for`. Coach-turn publication has the parallel gate at
`review_session_coaching.rs:859` (`validate_coach_turn_publication`).

### Generation contract / candidate identity

`CriticalMomentCommentGenerationContract` already carries `code_revision`, a
`CriticalMomentExplainerCandidate` (provider, model, version, prompt digest, schema digest), and
`CriticalMomentGenerationSettings { randomness, stable_seed, seed_supported, max_output_tokens }`,
with an `is_reproducible()` predicate. The placeholder
`CriticalMomentCommentAuthoringProvenance::hosted_generation_contract()`
(`critical_moment_comment.rs:117`) stuffs fixed `a…`/`b…` digests and `coach-app-host` because the
Coach App host model is unidentifiable. **A pinned OpenRouter candidate can populate these fields
for real** — this is the natural core of an Evaluation Fingerprint, not a new type.

### Provenance and unavailability vocabulary

- `ProviderKind` (`packages/coach-engine-sdk/src/ProviderKind.ts`) already includes `languageLayer`.
- `DeliverySurface` already distinguishes `web` / `coachSkill` / `coachApp`.
- `ProviderUnavailableReason::LanguageLayer` flows through the processor's terminal handling
  (`review_session_processor/terminal.rs:70`) to a written Player-facing message:
  *"The Language Layer is unavailable. Your earlier review remains intact, and Stockfish exploration
  still works."* (`apps/central-host/src/review-session/useReviewSessionCommands.ts:300-302`).
  **The safe-degradation UX path is already built and reachable today** — the web coach conversation
  currently always lands there.
- `provider_provenance.rs` builds pinned Stockfish/Maia `EvidenceProvenance` from constants; it has
  no language-layer arm.

### Quality Capture

`services/coach-engine/src/quality_capture.rs` (+ `model.rs`, `firestore.rs`) already provides:
disclosure versioning, per-Player `RetentionPreference` (available / enabled / disclosure required),
identity-free `QualityCaptureDraft` with `case_key` + `content_digest`, a `purge_at`, the
transactional Quality Outbox, and idempotent export to `coach-quality`.
`QualityCaptureContent::CoachingResponse` already carries `grounding_facts`, `generated_response`,
and `CoachingResponseReproducibility { authoring: CriticalMomentCommentAuthoringProvenance, … }`.

**The blocking constraint is environmental, not structural**: `quality_capture.rs:143`,
`quality_capture.rs:231-237` ("must be absent outside production") and `firestore.rs:478` gate
capture on `is_production_application()`. Staging cannot capture at all today. The map's stated
staging+production shared-`coach-quality` model contradicts this rule and the corresponding
`CONTEXT.md` relationship ("Staging has no capture path or quality credential") and ADR text.

### Pinning and reproducibility precedent

`evaluation_recording.rs:11-52` pins Stockfish version/depth/threads/hash and three binary digests,
Maia package/model/image/digests, the `Cargo.lock` digest, and canonical fixture digests, under
`REVIEW_SESSION_CAPTURE_VERSION` and `PROVIDER_RECORDING_SCHEMA_VERSION`. **This is the exact
template for pinning one OpenRouter model contract**, and `ReviewSessionProviderRecording` is the
existing record/replay format for provider interactions.

### Evaluation and certification tooling

- `pipeline_evaluation.rs` — fixture-driven `EvaluationReport` / `CaseDifference` /
  `EvaluationDrift::require_clean()` with a `last-accepted.diff` acceptance file;
  `pipeline_evaluation/learning.rs` adds frozen learning-review evaluation and migration
  dispositions.
- `tooling/scripts/review-session-baseline*.ts`, `review-session-certification*.ts`,
  `review-session-mcp-conformance.ts` — the existing baseline and certification harnesses. The
  topology-benchmark harness cited by the original research was removed after that one-time
  measurement.
- `services/coach-engine/src/bin/chenchess/certification/{review_session,web_journey}.rs` — the
  certification binaries already stub authors (`GroundedAuthor`, `UnavailableAuthor`,
  `BlockFirstAuthor`), i.e. **the certification harness can already exercise a Language Layer that
  succeeds, is unavailable, or blocks**.
- `tooling/scripts/release-gate.ts` + `release-targets.ts` — affected-unit gating over
  `railway-central-host` and `railway-coach-engine`; a Language Layer touching both the engine and
  the SDK/web hits both units.

### Budget and concurrency precedent

`operating_limits.rs` holds every deadline and concurrency ceiling as a documented constant
(`COACH_TURN_DEADLINE_SECONDS: 30`, `PROVIDER_POSITION_TIMEOUT_SECONDS: 30`,
`REVIEW_FACTS_ENGINE_CONCURRENCY`, …), and `ReviewSessionLimits` already ships per-session ceilings
to clients (`maxStartedCoachTurns`, `maxActiveCoachTurns`, `maxPlayerMessageBytes`, …).
`provider_concurrency.rs` and `request_single_flight.rs` provide admission control.
**Per-session/per-operation ceilings exist; monetary budgets and a kill switch do not.**

### Player feedback precedent

`learning_path_feedback.rs` + `LearningPathVote` + `useLearningPathFeedback.ts` /
`useCoachLearningPathFeedback.ts` give a working Player-vote loop (exposure recording, vote, undo)
on learning paths, with SDK commands `recordLearningPathExposure` / `updateLearningPathVote`.
**Reuse its shape for Review Moment helpful / not-helpful feedback.**

## Gaps

1. **No model provider client at all.** No OpenRouter (or OpenAI-compatible) HTTP adapter, no key
   handling, no request/response schema, no retry/timeout policy, no streaming decision. `reqwest`
   clients exist only for Lichess, Chess.com, Firestore, Firebase, Maia, and the local runtime.
2. **No Coaching Profile.** Zero occurrences of `CoachingProfile` / `ExplanationStyle` in code —
   `ExplanationStyle` survives only as `CONTEXT.md` vocabulary. Today's personalization is
   `EloProfile` plus deterministic learning signals (`decision_learning.rs`, learning-path
   feedback). Profile storage, editing UI, SDK type, and the "a Language Layer cannot persist
   profile changes" enforcement are all new.
3. **No web Review Moment comment authoring path.** `author_grounded_comment` is uncalled; the web
   renders `reviewedMoment.comment` and throws if a Player-selected moment lacks a canonical comment
   (`reviewMoments.ts:151-161`). Where that comment currently comes from on web needs to be settled
   in the task-contract ticket.
4. **No Review Feedback Report on web.** Only learning-path votes exist. The map's helpful /
   not-helpful + structured reasons + comment surface is new; issue 14 specified a *different*
   thing (manual GitHub-issue JSON), see below.
5. **No Evaluation Fingerprint type.** The term appears nowhere in code or `CONTEXT.md`. Its parts
   exist scattered: `DeliverySurface`, `CriticalMomentCommentGenerationContract`,
   `ArtifactDigest`, `REVIEW_SESSION_CAPTURE_VERSION`, `QUALITY_CAPTURE_SCHEMA_VERSION`. Missing:
   environment, Coach App Host, pipeline identity, provider route.
6. **Staging is structurally barred from Quality Capture** (see above). Needs an ADR amendment plus
   a rules/indexes/credential change — `firestore.quality.indexes.json` exists but the staging
   service account has no `coach-quality` access.
7. **No monetary accounting.** No cost, token, or spend concept; no operation/session/Player/global
   budget; no kill switch. `operating_limits.rs` is the place, but every value is new.
8. **No hosted secret path for a model key.** `.env.compose.example` and Railway config
   (`tooling/scripts/safe-railway-config.ts`) are the seams; no provider credential exists.
9. **Comparative benchmarking against ChatGPT/Claude** has no harness. The Coach App path is the
   only place a host model is used, and it is deliberately unidentifiable
   (`hosted_generation_contract()` placeholder digests), so cross-surface comparison cannot be
   grounded without new identity fields.

## Disposition of [Ship the Game Review feedback loop](#14)

That map is **fully charted and entirely unimplemented** — all nine children resolved into
contracts, "Not yet specified: None", status Deferred, and its children may not block work outside
it. Nothing in the working tree implements it beyond
`docs/research/github-review-feedback-issue-contract.md` and
`docs/research/rust-candidate-extraction-selector-seam.md`. Proposed disposition:

**Reuse (design, not code):**
- [Design explanation experiments with a versioned LLM Judge](#22)
  — Explainer Candidate identity, blinded calibrated judging, Human Audit before promotion,
  case-level metrics, human-only promotion. This is the closest existing precedent for the
  destination's evaluation requirements and should be adopted rather than re-derived.
- [Find the Rust candidate-extraction and selector seam](#16)
  — confirms `RuleExtraction` stays unchanged; keeps the fact boundary intact for this map.

**Supersede:**
- [Define the Review Feedback Report and GitHub issue contract](#15)
  and [Prototype one-ply feedback in the Coach UI](#17)
  — manual copy-paste into a GitHub issue by reporters with repo access cannot serve "every Beta
  Access Player leaves helpful / not-helpful feedback on every web Review Moment". Keep the report
  *schema* discipline (one ply, fixed reason codes, redacted headers, schema version) and replace
  the transport with an authenticated in-product path. Explicitly reuse the reason-code vocabulary
  rather than inventing a second one.

**Keep independent:**
- [Define the interpretable selector score and diversity policy](#19)
  and [Design selector evaluation and weight promotion](#21)
  — Critical Moment selection is deterministic and out of this map's scope.
- [Shape the feedback reproduction CLI](#18),
  [Define report triage and dataset admission](#20),
  [Lock the implementation route and release proof](#23)
  — the destination explicitly excludes automatic Dataset Admission, so triage/admission machinery
  stays in map 14.

**Constraint carried forward:** map 14 decided *CI never calls an LLM*. Any evaluation the release
gate runs for this map must obey that or amend it deliberately.

## Disposition of the separate learning maps

- [Design grounded learning recommendations for Game Review](#127)
  owns Learning Plan, Learning Tracks, Explanation Paths, Learning Resource Catalog, and selection
  policy (ADRs 0035–0038; `learning_plan.rs`, `critical_moment_comment/learning_grounding.rs`).
  **Keep independent.** This map consumes `ReviewMomentLearningMaterial` and the active moment's
  selected tracks read-only; a Language Layer may explain but never select, rank, or author
  resources or URLs. The only shared surface is Practice Recommendation *selection wording*, which
  must stay inside the existing bounded-selection contract (ADR 0015 as extended by 0035).
- [Design passive Daily Coaching from playing profiles](#217)
  — **keep independent**, but it is the most likely future owner of a durable Player profile. The
  Coaching Profile defined here should be scoped to Review Sessions and shaped so that map can
  extend it rather than fork it. Flag as a coordination point, not a dependency.

## Constraints any design must respect

- ADR 0009 — the Rule Extractor packet is the only source of chess claims; no free-form lesson or
  training-plan fields; structured output keyed by exact ply; providers rejecting `response_format`
  are retried without it but still validated locally.
- ADR 0014 — deterministic facts are evaluated separately from prose.
- ADR 0034 / `CONTEXT.md` storage topology — `coach-app-staging` / `coach-app-production` never read
  each other; `(default)` prohibited; every dynamic Firestore path segment is a SHA-256 digest.
- `CONTEXT.md` — the Coach MCP Server has no hosted model provider. Adding one to the **Coach
  Engine** must not leak into the MCP path, or that statement needs amending.
- Every Language Layer invocation receives a complete `CoachTurnContext` **by value**, excluding the
  full chat transcript and evidence payload.

## Open questions this inventory surfaces

1. Where do web Review Moment comments come from today, given `author_grounded_comment` has no
   caller? (For [Define the full web Language Layer task contracts](#233).)
2. Does the placeholder `hosted_generation_contract()` get replaced for the web, or does the web get
   its own contract constructor and the Coach App keep the placeholder?
3. Does the Evaluation Fingerprint extend `CriticalMomentCommentGenerationContract` or wrap it as a
   new envelope shared by both authoring seams?
4. Does the staging Quality Capture change amend ADR 0034 or add a new ADR?
