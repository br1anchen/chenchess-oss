# Plan: regenerate stale web commentary when the comment prompt changes

2026-08-30. Executor plan. Evidence: code reading on `services/coach-engine`
(paths below are current as of this date).

## Problem

Hosted web Review Moment commentary is authored once at first web open and
stored durably (`reviewAnnotations`, newest-wins). Every later open serves the
stored comment unconditionally — after a prompt edit in
`language_layer_prompt.rs`, old reviews keep pre-edit prose forever. The only
invalidation lever today is `REVIEW_ANALYSIS_GENERATION`, which throws away all
engine analysis, not just commentary.

No stored integer version is needed: engine-authored provenance already records
`generation_contract.candidate.prompt_digest` / `response_schema_digest`
(`critical_moment_comment/hosted_author.rs` `generation_contract_from_pin`).
The digest **is** the version — bumps automatically on any prompt edit.

## Blockers (all read-path)

1. `review_session_processor/readiness.rs:674` — web branch serves any
   published opening comment before reaching hosted authoring.
2. `review_session_processor/eager_authoring.rs:113` — batch authoring skips a
   moment with any published comment.
3. `review_session_processor/session.rs` `opening_authoring_context` — returns
   `None` when an active comment exists, so re-author has no context.
4. `session/comment_publication.rs:256` `stage_first_open_comment` — any
   active comment short-circuits to `Existing`, so a fresh authoring result
   would be discarded anyway.

Non-blockers (verified, no change): idempotency key + first-open flight key
already include the hosted fingerprint digest (which hashes promptDigest), so a
prompt change yields a new logical write; the annotation store is append-only
with newest-wins `active`, so a re-authored comment supersedes naturally;
durable moment payload never embeds the comment.

## Design rules

- Stale := active comment is engine-authored (NOT `is_host_submitted`) AND its
  candidate `prompt_digest` or `response_schema_digest` differs from the
  current compiled digests.
- Host-submitted (Coach App host model) comments are never regenerated.
- Pin/model change alone does NOT invalidate ("pin mismatch does not discard
  hosted prose" stance is preserved — digests only).
- Staleness only triggers regeneration where regeneration is possible: web
  surface with a bound hosted Language Layer. Other readers keep serving the
  stored comment (stale prose beats a template downgrade).
- On re-author failure, serve the stored stale comment (published=true), not
  the template fallback.

## Phases

### 1. Staleness predicate

`critical_moment_comment.rs`: on `CriticalMomentCommentAuthoringProvenance`,
`is_stale_web_artifact(prompt_digest, response_schema_digest) -> bool` per the
rules above. Helper producing the current digests as `ArtifactDigest`s (reuse
`comment_prompt_digest()` / `comment_schema_digest()`), reachable from the
processor via the hosted runtime. Unit tests: host-submitted immune; prompt
mismatch stale; schema mismatch stale; current match not stale.

### 2. Read-path gate (web + eager)

- `session.rs`: `web_opening_comment()` returns `WebOpeningComment::{Absent,
  Current, Stale}` — the three cases each call site has to tell apart, rather
  than an `Option` plus a separate staleness question.
  `opening_authoring_context` now discloses context when the active comment is
  stale, so re-authoring has the same authority a first open has.
- `readiness.rs` web branch: `Current` serves; `Stale` falls through to
  `author_first_open_hosted_comment` carrying the superseded comment, which
  every failure path serves as `published = true` rather than downgrading the
  Player to a template rendering.
- `eager_authoring.rs`: counts only `Current` as settled.

Refinement found while implementing: staleness must only divert opens that can
actually re-author. A `Stale` comment on a surface with no hosted Language
Layer is served as-is, so CoachSkill never pays Intent Enrichment for a
rewrite it cannot perform.

### 3. Supersede in staging

`stage_first_open_comment`: the active comment short-circuits to `Existing`
only when it is not stale; a stale active falls through to `Mutation` staging
(append supersedes, old record retained).

### 4. Tests + validation

- Unit (`critical_moment_comment.rs`): host-submitted immune; prompt mismatch
  stale; schema mismatch stale; compiled pair current. 4 tests.
- Integration (`tests/review_session_processor/first_open_hosted_comment.rs`):
  an `AgingAnnotationStore` reproduces the durable state a prompt edit leaves,
  then asserts the next web open re-authors and the open after that does not.
  Convergence is the load-bearing half — a rewrite that never settles would
  re-author on every open forever.
- Swept once: 1039 tests across all five Cargo targets, `cargo fmt`,
  `cargo clippy --all-targets`. All clean.

Fixture note: ageing the digests alone is not faithful. The first-open
idempotency key hashes the evaluation fingerprint, which hashes the prompt
digest, so a real edit also changes the key. A fixture that moves only the
digests collides on the key, `append` dedupes, and the rewrite is silently
discarded. The fixture moves both.

Measurement note: one authoring event can cost more than one provider call,
because the Grounding Gate has a bounded retry. Assertions compare authoring
events, not raw hit counts.

### 5. What review changed

Two defects the first cut shipped, both found on the spec axis and both now
covered by `an_outage_during_a_rewrite_keeps_the_prose_it_could_not_replace`:

- **A fallback rendering could permanently replace authored prose.**
  `author_grounded_comment` returns its safe rendering as an `Ok` — the code
  says so: *"The fallback is an `Ok`, so nothing downstream can tell it from
  authored prose."* The `Err` arm only fires on malformed facts, so on the
  ordinary failure (provider down, or grounding rejected twice) the rendering
  reached staging, and with the stale short-circuit gone it superseded good
  prose. Permanently, since the rendering carries the compiled digests and no
  later open would retry.
- **Three paths dropped the superseded prose**, turning what used to be a
  guaranteed render into `ReviewMomentUnavailable` on an annotation-store
  hiccup.

The guard is deliberately narrow: a fallback may found a Review Moment that has
no comment, and may replace an earlier fallback — that is how a moment which
once failed converges after a prompt edit. It may never replace prose the
Language Layer authored. Blocking every fallback instead was tried and breaks
convergence: with a stub that cannot pass the Grounding Gate, the rewrite never
lands and every open re-authors forever. `WebOpeningComment::Stale` therefore
carries whether the superseded prose was authored.

Fixture note: the stub's canned comment does not pass the Grounding Gate, so
everything it publishes is itself a safe rendering, and comparing comment text
cannot separate a preserved comment from a replaced one. The outage test
asserts durable identity (annotation count and the superseded prompt's
idempotency key) and marks its stored prose as authored. Both new tests were
confirmed to fail with their guard disabled.

## Unresolved questions

1. Backfill: regeneration is lazy — it reaches a review only when someone
   opens it on the web, or on the next import/web-route for eager authoring.
   Already-imported games nobody revisits keep pre-edit prose. Leave it, or
   add a sweep?
2. Should `coaching_profile_projection` drift also count as stale? (Shipped:
   no — only the prompt and response-schema digests count.)
3. Old superseded annotations are retained forever (append-only audit) — OK?
