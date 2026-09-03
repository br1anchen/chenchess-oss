# Compose the OpenRouter web Language Layer runtime

> **Retired 2026-08-31.** The bake-off harness, its frozen task set, and its
> route, preflight, and probe files were removed in #534. Paths below that name
> `evaluation/bake-off/frozen-set.json`, `routes.json`, `preflight-*.json`,
> `probes-*.json`, `replicate-control-*.jsonl`, `marker-seam-smoke-*.jsonl`, or
> `takeaway-marker-slice-*.jsonl` no longer resolve. The recorded generations
> three tests still replay -- `pilot-2026-08-14.jsonl`,
> `full-run-2026-08-16.jsonl`, `challenger-2026-08-17.jsonl` -- survive, and
> `evaluation/bake-off/README.md` says what each is for. This document stays as
> the record of what was measured and why.

## Status

Accepted.

## Amendment

**Pin Verification is telemetry, not a publication gate (2026-08-28).**
`GET /generation` remains the receipt for the served model and route because
live completions do not carry `openrouter_metadata`. The match still runs
after the completion, still records both identities on the Quality Capture
and Language Layer Operational Record, and still alerts on mismatch. It no
longer discards paid output or degrades a HostTurn or Review Moment Comment.
Publication waits on the completion and the Grounding Gate. A late or
missing `/generation` document is `verifyError` / `unverified` on the
record, not Player-visible unavailability. OpenRouter's own dashboard
remains the operator view of routing. The Decision bullets below that say
publication waits for Pin Verification, and that a mismatch discards paid
output, are superseded.

The operator availability switch named in this decision
(`WEB_LANGUAGE_LAYER_AVAILABILITY`) was never shipped, and the later
shipping name `HOSTED_LANGUAGE_LAYER` is withdrawn. Env supplies only
`OPENROUTER_API_KEY`. The operator kill that unbinds is removal of that
key from Railway plus a Coach Engine redeploy. Revoking the key at
OpenRouter without a redeploy leaves the process Bound; hosted calls
then fail, and web **HostTurn** plus remaining Coach App **Coach Turn**
paths go Player-visible unavailable. The automatic global monthly
ceiling is unchanged.

#331 no longer replaces these conservative defaults by saturating the
pinned route. OpenRouter publishes no paid-model RPM, TPM, or concurrency
figure and no 408/524 duration; Vertex publishes 429 semantics and backoff
guidance, but its quota numbers belong to OpenRouter's GCP project. A 429
is a typed `CompletionOutcome`; admission honours `Retry-After` or
`google.rpc.RetryInfo.retryDelay` when the header is missing, applies the
1 s `rate_shaped_retry_delay` floor with consecutive-429 doubling capped at
15 minutes, holds an engine-wide cooldown, and degrades through the existing
`HostedFallback` asymmetry. Observed p50/p95/p99 and any 429 arrive from
Language Layer Operational Records once staging carries hosted traffic.

The **counterparty terms check is withdrawn** (operator decision,
2026-08-22, boot-fetch withdrawal). Boot makes one live assertion — the
account posture of #294 — not two. Decision point 2 below, its accepted
cost, and the rejected scheduled-re-verification alternative are all
superseded.

The check fetched the Google abuse-logging page each boot and refused when
its digest moved. It detected *vendor edits, not term changes*. On
2026-08-21 it took staging dark twice in ten hours; a byte-diff of the two
snapshots showed the whole difference to be a sidebar entry for an unrelated
model and a footer date moving from 2026-08-19 to 2026-08-21. No operative
term moved either time.

It also could not catch what it existed for. The terms that bind Google live
in **GCP ToS §4.3**; that page only describes them, so a real change to what
the counterparty may do with Player data need not move the digest at all. The
ADR anticipated "a five-minute re-read and a one-line digest update" as the
remedy for boilerplate churn; at observed cadence that is a recurring
re-pin-and-redeploy against a signal that was wrong in kind.

What replaces it is what #330 and #363 already established: the assurance is
transitive and unverifiable at serve time, so it is carried by wording rather
than by a fetch. The published claim is worded so it does not quote a vendor
page.

**Superseded later on 2026-08-22 (#416), after the boot-fetch withdrawal.**
`pin-record.json` no longer keeps a counterparty attestation. The leftover
`governingClause`, `resolvedCeiling`, `pageUrl`, and `readDate` fields had
no runtime readers; they recorded a vendor-doc read and are deleted. No
code path reads, fetches, or digests a vendor documentation page. No test
or release gate asserts a vendor-page digest, URL, or read date. #340's
admissibility rule is not encoded in the v1 pin. ToS / #340 revisit is
post-v1 (#438). `counterparty.readDate` no longer exists to age. ADR 0050's
re-verification obligation keeps its shape and its target, but neither a
pin field nor a deploy failure prompts it: a re-read happens because a
human decides to. This amendment writes no cadence, reminder, or new
attestation schema.

This decision closes #237 by fixing the runtime architecture that carries the
settled Language Layer decisions into code: the two-task Language Layer Task
Contract of #233, the endpoint posture of #294, the counterparty admissibility
rule of #340, the Evaluation Fingerprint of ADR 0049, and the pin and budget
ladder of ADR 0050. It decides the four things those left open — the provider
port, the spend ledger and its enforcement point, the kill switch and
re-verification placement, and the authoring idempotency key — and names the
admission, concurrency, and backoff values as declared conservative
configuration. #331 honours provider rate-limit signals against those
defaults; it does not replace the numbers by saturation measurement. It
changes no task semantics, no Grounding Gate rule, no fingerprint axis, no
budget number, and no privacy claim.

## Context

Everything semantically hard about the web Language Layer was decided upstream
and is transcribed here only as constraint: two tasks in one governance
envelope, structural topical containment with a typed `OutOfScope` refusal,
asymmetric fallback (comment → deterministic safe rendering, Coach Turn →
unavailable), one task deadline with a single byte-identical retry, cancellation
given up so an abandoned call wastes at most one bounded completion,
`nativeSchema` fixed with no mid-flight downgrade, Pin Verification through a
second `/generation` call because `openrouter_metadata` does not exist on live
completions (#346 §5.5), lazy authoring on first open with publish-once-freeze,
and the pin of ADR 0050 with its four-tier budget ladder.

What remained was architecture. The only OpenRouter client in the workspace is
inside the bake-off harness (`services/coach-engine/src/bin/language_layer_bake_off.rs`,
2041 lines, `reqwest` direct), whose own header says #237 owns the shipping
client. The web runtime composition binds `NoHostedLanguageLayer`
(`review_session_runtime.rs`), so production web comments come from safe
rendering today. The Service Operator's swappability requirement (recorded on
#237, 2026-08-17) already rejected `aisdk.rs` for this seam: its OpenRouter
provider cannot express the `provider` routing block that *is* the pin, its
response type drops `usage.cost` — the only per-Player cost source OpenRouter
offers (#231) — and its typed model constants predate the pinned permaslug, so
they would be a second, unverified source of truth about per-`(model, tag)`
facts this map has been burned by. That rejection is transcribed so it is not
re-litigated.

Four operator decisions taken on 2026-08-17 while refining map #229 feed
directly in: budgets deny at admission; the counterparty re-verification is a
boot check beside #294's posture assertion, with no scheduled routine in v1;
#238's rollout questions fold into the implementation epic; and the port shape
is a named port under the Task Contract with the harness and runtime sharing
one client.

## Decision

### One provider port under the Task Contract

A named **Language Layer provider port** — `LanguageLayerProvider`, in a new
`services/coach-engine/src/language_layer_provider` module — is the only way a
hosted completion is issued. It has three operations:

1. **`complete`** — one buffered structured completion under a pinned
   generation contract. The adapter streams internally purely for abort
   semantics and buffers fully before returning; no partial output crosses the
   port. It returns the parsed candidate output, the provider generation id,
   `usage.cost`, token counts, and finish reason.
2. **`verify_generation`** — the second authenticated `GET /generation` call,
   structurally separate because it lands after the completion.
3. **`assert_posture`** — the boot assertion described below.

The OpenRouter adapter is **lifted out of the bake-off binary** into this
module, and the harness is rewired onto it, so the thing measured is the thing
served. Request invariants from #294 live in the adapter, not at call sites:
`provider.only` carrying the **full endpoint tag**, `allow_fallbacks: false`,
`require_parameters: true`, `zdr: true`, `data_collection: "deny"`, no `models`
array, no `route`, no variant suffix. A `response_format` rejection on the
pinned `nativeSchema` route **fails hard and alerts** — never a downgrade —
enforcing ADR 0050's narrowing of ADR 0009 at runtime.

The two authoring seams (`CriticalMomentCommentAuthor`,
`AlternativeMoveAssessmentAuthor`) gain OpenRouter-backed implementations that
wrap task input in the Language Layer Task Contract envelope and call the port.
They bind **only** in the web runtime composition, through a
`configured_language_layer_runtime()` following the existing `configured_*`
idiom; MCP, Coach Skill, and Coach App keep `NoHostedLanguageLayer`. The
`OPENROUTER_API_KEY` reaches only the Coach Engine process environment of the
web deployment — no other composition has a binding or a key.

### The pinned generation contract is an in-repo asset

The pin record — model permaslug, full endpoint tag, determinism controls,
Structured Output Mode, and `max_tokens` — is a declared, in-repo
configuration asset compiled into the Coach Engine like the prompt templates,
not env vars and not call-site constants.

**Superseded later on 2026-08-22 (#416), after the boot-fetch withdrawal.**
The counterparty attestation of #340 (governing clause, resolved ceiling,
page URL, read date) is no longer part of the pin record. See Amendment.

Changing the pin is a reviewed edit that mints a new Explainer Candidate, and
a model swap stays what ADR 0050 made it: a configuration change plus a
re-run of the frozen set, no code change. Env supplies only the secret.

At process start the runtime computes its Evaluation Fingerprint axes from this
record and the compiled digests. The **one canonicalization function ADR 0049
requires, pinned by a golden test, lands with this port as its first consumer**
— today the repo has three private digest helpers and none is it.

### Boot refuses before serving

Two assertions run at startup, and either failing refuses to serve the hosted
Language Layer (the composition falls back to `NoHostedLanguageLayer`; the web
app keeps working on safe rendering).

**Superseded 2026-08-22: one assertion runs at startup, not two.** Point 2
below is withdrawn; only the account posture of point 1 is asserted. See the
Amendment.


1. **Account posture (#294 rule 6).** Read the live OpenRouter account
   settings and `/api/v1/endpoints/zdr`; any contradiction with the declared
   posture, or the pinned endpoint missing from the ZDR list, fails the
   assertion.
2. **Counterparty terms (operator decision, 2026-08-17; withdrawn
   2026-08-22 — see Amendment).** Fetch the Google
   abuse-logging page that GCP ToS §4.3 resolves to, normalize its text, and
   compare its digest against the one in the pin record. Divergence means the
   document the admissibility verdict was read from has changed, and #340 made
   that a gate: refuse until a human re-reads the page and re-records the
   attestation. There is no scheduled re-verification routine in v1 — every
   boot is the cadence.

Accepted costs, extending #294's: a startup dependency on OpenRouter's API
**and one Google documentation page**, either of which can block a deploy. A
boilerplate churn on that page produces a false refusal whose remedy is a
five-minute re-read and a one-line digest update; the alternative — serving
through a silently changed retention commitment — is the thing four tickets
were spent preventing.

**Superseded 2026-08-22.** The documentation-page dependency is removed, so
only OpenRouter's API can block a deploy. The five-minute-re-read remedy was
priced too cheaply: on 2026-08-21 the churn arrived twice in ten hours, and
the check never had the reach the last sentence claims — a silently changed
retention commitment lives in GCP ToS §4.3, which that page's digest does not
observe.

### Admission denies before spending

Every hosted call passes one admission check, in order, before any provider
request:

1. **Budget tiers** (ADR 0050's ladder). The operation tier is enforced by
   construction — `max_tokens: 512` and the single retry bound the worst case
   at \$0.0045 against the \$0.005 ceiling. The Review Session tier is a
   counter in Review Session working state; the Player 30-day and global
   monthly tiers are read from the ledger. Any exceeded tier → denied.
2. **Concurrency slot.** A bounded engine-wide in-flight cap; a request waits
   for a slot only within its remaining task deadline. The existing
   one-in-flight-Coach-Turn-per-session invariant stands.

A denial spends nothing and degrades exactly as provider failure does: comment
to safe rendering, Coach Turn to unavailable. It records a Language Layer
Operational Record with a `denied` budget decision and, where a capture is
induced, the `budget-refused` Capture Outcome of ADR 0049. Post-hoc
enforcement was rejected by the Service Operator: the money is spent before a
post-hoc check runs.

### The ledger lives in the product database

Per ADR 0049, the product database is authoritative for money. It gains:

- **Language Layer Operational Records**, one per settled attempt — including
  cancelled, timed-out, and pin-mismatched attempts, which bill and therefore
  record (#233).
- **Spend counters**: day-precision per-Player buckets summed over the
  trailing 30 days, and one global calendar-month counter. The counter update
  commits in the same transaction as the operational record write, on attempt
  settlement.

Admission reads counters without locking them, so concurrent admissions can
overshoot a ceiling by at most (concurrency cap − 1) × the operation ceiling —
under two cents at the default cap — which is accepted rather than serialized
away. The Review Session counter is process-local working state: a session that
loses residency re-meters from zero, accepted because a rebuilt session *is* a
new Review Session and the Player 30-day tier still bounds real exposure.

### The kill switch is the ceiling plus the key

Three mechanisms; the third is provider-triggered and self-clearing:

- **Automatic**: the global monthly ceiling. When the global counter reaches
  \$25, admission denies everything until the month turns or the ceiling is
  raised by a reviewed change. The trip alerts.
- **Operator**: remove `OPENROUTER_API_KEY` from Railway and redeploy Coach
  Engine. An absent, empty, or whitespace-only key binds no hosted Language
  Layer; the web surface stays on safe rendering. Revoking the key at
  OpenRouter without a redeploy leaves the process Bound; hosted calls then
  fail, and Coach Turns go Player-visible unavailable until a restart
  without a key. There is no env availability switch.
- **Provider cooldown**: a replica-local wait opened by an upstream 429. It
  honours `Retry-After` or `RetryInfo`, otherwise the 1 s floor doubled per
  consecutive 429, and expires at the honoured duration (capped at 15
  minutes). It is acceptable where an operator-controlled env flag was
  rejected because it honours an upstream signal and clears itself; it is
  not a second operator kill switch.

### Idempotency: the authoring key is moment identity plus candidate

A Review Moment Comment is authored lazily on first open and frozen on
publication. The **authoring idempotency key is (Game Import ID, Review Moment
Reference, Evaluation Fingerprint digest)** — moment identity plus the exact
Explainer Candidate. The existing publication machinery
(`stage_comment_publication`) already replays a published key and allows the
single retry; this key extends the same guarantee upstream of the provider
call: an open that finds a published comment for the key replays it and spends
nothing, concurrent opens of one moment collapse onto one in-flight authoring
in session state, and a crash between provider call and publication leaves at
most one billed, recorded, unpublished attempt — re-authoring after such a
crash is a second operation under the same key, admissible within the budget
ladder, not double-publication.

The write pattern is the Quality Outbox precedent: comment publication, its
annotation-store entry, and the Quality Outbox writes share **one product-
database commit**, while the operational record writes at attempt settlement
independently of publication — the two can disagree after a partial write and
the operational record wins (ADR 0049). Coach Turns take no durable
idempotency: nothing durable is written, and the one-in-flight-turn invariant
is the dedupe.

### Attempts, deadlines, verification

Transcribed from #233/#294 and made concrete:

- One task deadline per task. `COACH_TURN_DEADLINE_SECONDS` stays 30; comment
  authoring gets its own constant sized to the Player-visible first-open wait.
  Per-attempt provider timeouts are **derived from remaining deadline**, capped
  by config, never additive; a retry that cannot finish in remaining time is
  not attempted. Deadline exhaustion records its own typed reason.
- **Pin Verification runs concurrently with the Grounding Gate**, and
  publication waits for both. #233 ordered verification before grounding via
  `openrouter_metadata`; that field does not exist on live completions, so the
  `/generation` call lands after the completion and serializing it before
  grounding buys nothing. Verification that cannot complete inside remaining
  deadline is a failure like any other; a mismatch discards the paid output,
  records both pinned and observed identities, and **alerts as an operational
  fault** rather than counting as a bad generation.
- The served endpoint, region, and `routed_service_tier` are recorded beside
  every capture as per-record facts (`null` tier means the declared default was
  served — the declaration, not the response, is the record of what was asked
  for).
- Alert classes, through the deployment's existing log-based monitoring: boot
  refusal, schema-downgrade rejection, pin mismatch, global-ceiling trip.

### Named configuration; #331 does not replace the numbers

Conservative defaults ship and stay. Changing any of these is configuration,
not candidate identity — none is a fingerprint axis. #331 does not measure
them by saturating `google-vertex/global`. Language Layer Operational Records
are the measurement source once staging carries hosted traffic.

| Key | Default | What a later change would use |
| --- | --: | --- |
| `maxConcurrentProviderCalls` | 4 | observed 429 rate under hosted staging traffic |
| `providerAttemptTimeoutCeilingSeconds` | 20 | tail latency at `global` tier from Operational Records |
| `commentAuthoringDeadlineSeconds` | 10 | first-open p99 against #235's bounded wait |
| `rateShapedRetryDelayMs` | 1000 | cooldown floor when a 429 carries no usable `Retry-After` or `RetryInfo`; consecutive 429s double this floor up to 15 minutes |

## Considered alternatives

**`aisdk.rs` as the client.** Rejected on #237, transcribed above: cannot
express the pin, drops `usage.cost`, stale typed constants. The port is small
enough that a generic SDK buys nothing the routing block does not immediately
take back.

**Post-hoc budget enforcement.** Rejected by the Service Operator: it turns
every ceiling into a report about money already spent, and the admission read
is one indexed query.

**A durable Review Session spend record.** Rejected. The Review Session is
deliberately identity-less and transient (`CONTEXT.md`); giving its budget a
durable key would re-mint the session identifier ADR 0042 retired, to harden a
tier the Player 30-day ceiling already bounds.

**A dynamic flag store for the kill switch.** Rejected for v1. The operator
kill is key removal plus redeploy; revocation without a restart leaves the
process Bound. The automatic ceiling needs no human at all. A provider-
triggered replica-local cooldown is not this alternative: it expires, is
not operator-toggled, and exists only to honour an upstream 429.

**A scheduled counterparty re-verification routine.** Moot since 2026-08-22:
the boot check it was rejected in favour of is itself withdrawn. Recorded as
taken at the time. Rejected by the Service
Operator in favor of the boot check: a routine is new operational machinery
with its own failure modes, while boots are frequent at beta cadence and a
stale-but-running process is bounded by the next deploy.

**Verification strictly before grounding.** Superseded by the facts: the
metadata field #233 assumed does not exist on live completions (#346 §5.5);
concurrent-with-grounding keeps the same publication gate without the serial
latency.

## Consequences

The harness and the runtime now share one client, which is the point — but it
makes the bake-off binary production-adjacent, and edits to the shared adapter
deserve production review even when motivated by a measurement run.

Boot depends on OpenRouter's API and one Google documentation page. A deploy
can be blocked by either. That is the admissibility gate working as specified,
and the failure mode is a refusal to serve hosted prose while the web app
degrades to safe rendering — never an outage.

**Superseded 2026-08-22.** Boot depends on OpenRouter's API alone. The
documentation-page dependency produced two refusals on 2026-08-21, neither
from a term change, and is withdrawn.

Budget ceilings can overshoot by bounded cents under concurrency, and the
session tier resets with session residency. Both are accepted and recorded
here so a later audit reads them as decisions, not bugs.

The Evaluation Fingerprint canonicalization function and its golden test are
now on the implementation critical path — the port cannot record a capture
without them.

Implementation slicing, acceptance gates (including the folded #238 rollout,
certification, and rollback questions), and the provisioning of the funded
OpenRouter account belong to the implementation epic that `/to-issues` cuts
from this ADR plus map #229's "Not yet specified" list.
