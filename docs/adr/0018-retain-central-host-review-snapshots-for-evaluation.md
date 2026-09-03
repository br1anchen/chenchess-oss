# ADR 0018: Retain Central Host Review Artifacts for Evaluation

## Status

Accepted in part. The Snapshot/Record/Manifest contract model,
manifest-based reproduction clauses, and speculative artifact types without an
implemented workflow are superseded by ADR 0020. Review Snapshot granularity is
refined by ADR 0021. ADR 0026 supersedes intent traces, Intent Response Records,
Player-intent retention, and intent-only replay inputs. ADR 0034 supersedes the
remaining storage shape, owner index, and default retention mechanism with
production account state, a transactional product-database outbox, and
identity-free captures in a separately credentialed quality database. The
consent, disclosure, 12-month expiry, withdrawal, identity exclusion, and
reproducibility decisions remain in force.

## Amendment by ADR 0034

The production Player account record owns the "Help improve coaching"
preference and acknowledged disclosure version. No quality capture occurs
before acknowledgement or while the preference is off.

The production Coach Engine writes a Player-owned outbox record in the same
transaction as the qualifying business result. A separately credentialed
exporter writes only identity-free captures to `coach-quality`. The product
outbox, not a quality-database owner index, retains the association needed for
expiry and withdrawal.

Quality captures contain no Player ID, Player-authored free text, full
transcript, or identity-bearing Game metadata. Evaluation prompts, provider
request and response traces, and evaluator reasoning traces are transient.

The Context, Decision, and Consequences below record the original rationale.
The amendment above governs wherever the original storage shape conflicts with
ADR 0034.

## Original context

Intent Selection Traces are needed to reproduce why the coach offered one Coach Intent Hypothesis or abstained. Player responses are needed to evaluate whether the hypothesis started a useful interaction. Keeping either only in an in-memory Review Session would prevent later debugging, evaluation, and improvement. The Central Host has authenticated backend storage suitable for retaining these artifacts; local and self-hosted deployments should not silently upload local review data.

Persisted traces are not accuracy labels. They contain ranking provenance but no Player-authored Move Intent captured before hypothesis exposure, so they cannot form an Intent Calibration Set by themselves.

## Original decision

This section is a historical record. Its Review Snapshot, Intent Response
Record, default-retention, owner-index, and durable-trace clauses are
superseded by the ADR 0034 amendment above.

After an Automatic Critical Moment's canonical comment is admitted, the Central Host may retain one immutable Review Snapshot for that moment, deduplicated by Review Session and Game ply. It contains the normalized Game, that one generated comment, its Intent Selection Trace, direct replay inputs, and internal authoring provenance. Session preparation alone creates no snapshot, and Player-Selected Moments remain transient for MVP. A later Player response cannot mutate the snapshot, so confirm, correct, provide-another-intent, and skip actions create separate immutable Intent Response Records linked to the exact Review Snapshot, exposed hypothesis or abstention, and Intent Selection Trace.

Each Review Snapshot stores the canonical input and exact deterministic and language-layer replay inputs directly. This includes Stockfish build and analysis configuration, Maia runtime and model, resolved Elo, intent-selection policy, Explainer Candidate, schemas, generation settings, and code revision. These inputs are not wrapped in a nested reproduction manifest, and mutable provider aliases are not sufficient.

The Central Host persists Review Snapshots and Intent Response Records by default for future evaluation and improvement. The Player can opt out of Central Host artifact retention. Opting out stops new retention immediately and deletes that Player's retained artifacts that have not undergone Dataset Admission.

The Central Host presents an enabled-by-default "Help improve coaching" retention preference before the Player's first retained review and exposes the same control in account settings. The disclosure states the improvement purpose, the 12-month retention window, and the deletion or dataset-withdrawal effect of opting out. The backend resolves and enforces the preference before persisting either artifact; a client-side toggle alone is not authoritative.

Before persistence, the backend removes direct account identifiers, usernames, profile URLs, external Game URLs, and equivalent identity metadata from Review Snapshots and Intent Response Records. Each artifact receives an opaque identifier. A separate, access-controlled Artifact Owner Index maps the authenticated Player ID to those identifiers only so the Central Host can enforce retention, expiry, deletion, and Dataset Withdrawal. The owner index is never evaluation input or included in Dataset Admission.

Retained artifacts are not available through general-purpose browsing or ad hoc database access. Authorized case-level inspection uses role-restricted evaluation tooling and requires a declared purpose. Every access creates an immutable Artifact Access Record containing the actor, purpose, opaque artifact identifier, and timestamp. That tooling cannot query or reveal the Artifact Owner Index.

These artifacts are pseudonymized rather than anonymous: the protected owner index deliberately preserves a revocable link to the Player. Product and engineering documentation must not describe the retained artifacts as anonymous.

The restricted Review Snapshot preserves the exact resolved Elo supplied to the Human Move Model because changing it can change candidate probabilities and the selected Coach Intent Hypothesis. Evaluation reports, dashboards, and exports expose only the corresponding Elo Profile band; they do not emit exact Elo.

The Review Snapshot also preserves the complete canonical Game move sequence and an allowlist of chess metadata required to replay import, Critical Moment selection, and coaching generation. The backend persists normalized moves rather than raw PGN text. Player names, usernames, Site and profile URLs, external Game identifiers, and other identity-bearing headers are discarded before the artifact is written.

A retained artifact that has not undergone Dataset Admission expires 12 months after its own creation and is deleted automatically. Account activity and later reviews do not reset or extend that artifact-specific retention window. Admitted cases follow the versioned dataset withdrawal policy instead.

If a withdrawn artifact has already undergone Dataset Admission, the governed withdrawal removes its Player-derived payload and leaves a non-content Dataset Tombstone. Future dataset revisions and experiment runs exclude tombstoned cases. Withdrawal does not silently relabel the case or treat it as negative feedback.

When a completed experiment run used a subsequently tombstoned case, the withdrawal process deletes that case's inputs and outputs from the run. It preserves aggregate metrics and non-content audit metadata, marks the run as affected by withdrawal, and makes the run ineligible as future promotion evidence. This controlled redaction takes precedence over case-level reproducibility; the remaining audit shell is not represented as a complete immutable run.

Local and self-hosted deployments do not create or automatically upload these Central Host evaluation artifacts unless explicitly exported. Persisting an artifact does not automatically admit it to an evaluation dataset, tune a model, change a prompt, or promote a production candidate; those remain deliberate, versioned workflows.

This policy does not retain the full Review Session transcript. Intent Assessments, discussion messages, and Alternative Move Exploration remain in-memory. Player-provided Move Intent is retained only when it is the payload of an Intent Response Record.

When an Intent Response Record contains Player-provided Move Intent, the backend preserves the Player's original wording after a narrow privacy scrub replaces detected direct identifiers or secrets. The scrub runs before persistence and again at export boundaries. It does not summarize, paraphrase, normalize chess language, or substitute a coach interpretation. The retained value is Sanitized Intent Wording, not raw input.

## Original consequences

This section describes the consequences of the original decision. The ADR 0034
amendment above replaces its storage and capture consequences.

The Central Host accumulates reproducible evidence that can support debugging and future candidate evaluation while allowing Players to decline collection. Storage and APIs must distinguish default artifact retention from Dataset Admission, associate retained artifacts with the Player strongly enough to honor withdrawal, and enforce deletion or tombstoning without relying on manual cleanup.

The Central Host needs scheduled, observable expiry processing rather than indefinite accumulation or best-effort manual cleanup.

The product needs a first-review disclosure state and an account setting backed by authenticated retention-policy APIs. Turning the preference off invokes withdrawal rather than merely changing future UI behavior.

Artifact content storage and the Artifact Owner Index require separate access boundaries. Evaluation tooling consumes opaque artifact IDs and content only; it cannot query or export the owner mapping.

Operational access controls must distinguish aggregate evaluation, case-level artifact inspection, retention administration, and owner-index administration. Case inspection is auditable; owner resolution is limited to automated retention and withdrawal paths plus separately authorized administration.

Privacy scrubbing needs deterministic replacement markers and regression cases covering common identifiers and secret formats without treating chess notation, opening names, or piece-square language as personal data.

Reporting and export boundaries must derive the Elo Profile band inside the trusted backend rather than sending exact Elo to downstream evaluation presentation layers.

Replay tooling consumes the normalized canonical Game representation. It must not depend on retrieving the original public Game or reconstructing removed PGN headers.

Deterministic stages must replay from the artifact's direct typed inputs without live-provider drift. For stochastic language generation, those inputs reproduce the exact request and generation contract; byte-identical prose is guaranteed only when the pinned provider contract supports deterministic generation.

Review Snapshots therefore retain both the generation contract and the emitted output. Reproduction reports distinguish deterministic evidence replay, identical-request regeneration, and byte-identical output rather than collapsing them into one pass condition.

Artifact withdrawal can make an earlier experiment irreproducible and invalidate otherwise acceptable promotion evidence. A replacement experiment must run over a current dataset revision before promotion.

Because Intent Response Records are collected after hypothesis exposure, retained production artifacts cannot support a defensible Intent Hypothesis Precision claim or replace the future Intent Calibration Set. They can measure interaction outcomes and supply correction examples for deliberately governed improvement work.
