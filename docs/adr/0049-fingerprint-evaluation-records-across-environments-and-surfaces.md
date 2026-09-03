# Fingerprint evaluation records across environments and surfaces

## Status

Accepted.

This decision replaces ADR 0034's rule that only a production exporter may write
identity-free Quality Captures to `coach-quality`, and the matching rule that
staging has no capture path or quality credential. It amends ADR 0018's
evaluation retention shape by admitting call-shape facts and rejected
generations into a capture, and extends its explanation-evaluation machinery
with a second admission source. It does not change the Grounding Gate,
the Language Layer fact boundary of ADR 0009, deterministic chess authority, or
the deliberate, merge-gated nature of Dataset Admission.

## Context

The tailored OpenRouter web Language Layer ships to Beta Access Players on the
staging Central Host, so the Players whose coaching quality most needs measuring
are on the environment that currently has no capture path at all. The same
coaching prose will also come from three delivery surfaces — the web app through
a pinned OpenRouter model, and ChatGPT and Claude through a host model nobody
can pin — and from a succession of model, prompt, and schema revisions that
#236 will choose between.

Comparing any of that requires knowing which configuration produced a given
piece of prose. A model name is not sufficient: output changes when the prompt
template, response schema, evidence schema, Coaching Profile Projection schema,
generation settings, or code revision change, and the existing Explainer
Candidate already bundles exactly those. What is missing is one identity that a
Quality Capture, a Review Feedback Report, and a cost row can all be joined on,
across two environments and three surfaces.

Two constraints pull against that. A Quality Capture is identity-free and
excludes request IDs, timings, and provider traces, specifically so it cannot be
joined back to one Player's request in the server logs — yet choosing budgets
and defending a rollout needs cost, tokens, and latency. And a Player who has
turned the Quality Capture Preference off can still press "not helpful", which
produces feedback about an output nothing retained.

## Decision

Introduce the **Evaluation Fingerprint**: a SHA-256 digest over a canonical,
ordered axis set, resolvable to one immutable axis record in `coach-quality`.
The digest rides on every Quality Capture, Review Feedback Report, and Language
Layer Operational Record; the axes are written once and referenced.

Axes are **declared configuration only** — computable at process start, so a
deployment has a fixed, assertable fingerprint. They are the Evaluation Contract
Version, environment, Capture Origin, Delivery Surface, Language Layer
Attestation, code and pipeline revision, and, when attested, the exact pin, the
provider allowlist, generation settings, Structured Output Mode, and the prompt,
response-schema, evidence-schema, and Coaching Profile Projection-schema
digests. An unattested fingerprint substitutes the Coach App Host, its version,
and its instruction-bundle digest, and carries no pin.

Everything observed per call sits beside the digest, never inside it: the served
provider, the Pin Verification verdict, the Capture Trigger, and the Capture
Outcome. The Evaluation Contract Version is itself an axis; changing the axis set
bumps it and yields new digests, while historical fingerprint records keep
theirs and are never recomputed. One canonicalization function in the repo owns
digest computation and is pinned by a golden test.

Record in three tiers:

- **`coach-quality`** holds identity-free content plus call-shape facts — token
  counts, cost, finish reason, attempts, deadline-hit, outcome, and a
  day-precision date. Staging and production share one collection, separated
  structurally by the environment and Capture Origin axes, through distinct
  write-only, database-scoped service accounts.
- **The product database** holds the Player-associated Language Layer
  Operational Record: request ID, latency, cost, tokens, budget decision, and
  error class, under ordinary operational retention. It is authoritative for
  money and is never exported to `coach-quality`.
- **Nothing durable** holds raw provider payloads. A request is regenerated from
  its prompt digest and captured inputs; a response lives in memory for the call,
  leaving only a bounded, free-text-stripped excerpt on output-shaped failures.

Failed and rejected generations are captured under the same consent as published
ones. A Review Feedback Report is a thin annotation carrying a capture reference,
the fingerprint, and its reason codes; when the preference is off, submitting
feedback induces a capture under its own submit-time disclosure, tagged
`feedback-induced`. When the preference is on, the generation has already left
the product database by the time the Player votes, so the outbox row keeps its
Evaluation Fingerprint digest past export and the annotation is anchored by
that. A withdrawn row drops the digest and stops being an anchor. Both export
through a Player-owned outbox, so withdrawal reaches them. An Explanation Evaluation Dataset case may now be admitted from a
sampled capture as well as a feedback-anchored one, with every existing guard
unchanged.

## Considered alternatives

**Latency inside the capture.** Rejected. A millisecond timestamp plus a token
count is close to a unique key into the request logs, so admitting it would
undo the identity-free guarantee for the sake of an analysis the operational
tier can already do.

**Cohort-only joining, with no operational numbers in the capture.** Rejected.
It keeps the exclusion list untouched but makes a truncated cheap generation
indistinguishable from a complete one, so no single bad comment can be
explained.

**A separate staging quality database.** Rejected. It gives the cleanest blast
radius, but cross-environment comparison is the point, and two schemas would
drift.

**Best-effort host model pins for ChatGPT and Claude.** Rejected. The value is
unverifiable and silently wrong when a host swaps models mid-conversation, which
reintroduces the substitution risk the web path bans outright.

**Gating feedback on the Quality Capture Preference.** Rejected. It silences the
opted-out Players whose dissatisfaction matters most, and makes a visible
product control quietly conditional on a privacy setting.

## Consequences

Cost is recorded in two tiers on purpose. The two copies can disagree after a
partial write, and the operational record wins.

Quality rates must always be read per Capture Outcome and per Capture Trigger. A
cohort now mixes published with rejected generations, and preference-triggered
captures with feedback-induced ones that are selection-biased toward complaints.
An unqualified average over a cohort is wrong.

Consumer-chat surfaces can never win an experiment. Their captures are a
baseline cohort for the instrumented comparative hypothesis, nothing more.

Adding an axis is a deliberate act with a corpus-wide effect: it bumps the
Evaluation Contract Version, so cohorts before and after it only pool when an
Explanation Experiment Run says so explicitly.
