# Admit chronological Review Sessions as equal peers before rich authoring

## Status

Accepted in part. ADR 0026 supersedes eager or batch intent preparation and
durable intent readiness. Chronological admission, objective-fact preparation,
equal-peer navigation, and atomic publication remain in force.

## Context

The original Review Session contract started one session around one selected Critical Moment, generated an `entryComment`, and initialized other moments later. That made the first or highest-ranked Critical Moment an architectural role even though Critical Moment selection and presentation are chronological. It also forced Coach Skill to start several independent sessions or orchestrate per-moment preparation before it could validate one complete Game Review.

The automatic set is bounded at ten moments, but rich authoring preparation is
not required to display it. Treating presentation and authoring as one readiness
state made the first Coach App result wait for Engine and Human Move Model work
that the Player might never request.

## Decision

Game Import returns an opaque Game Import ID. The server owns the imported Game, Review Side, automatic Critical Moments, and supporting facts; clients do not carry or sign that state through follow-up requests.

One Game Review may ground several independent Review Sessions. For one Player,
Game Import ID, and semantic session-start operation ID, concurrent or repeated
requests join the same in-progress preparation or return the same ready
session. A new operation ID creates another session incarnation over the same
frozen import, so separate chats do not share mutable interaction state.
Cancellation or hard failure commits no acknowledged session and permits retry
with the same operation ID.

Session start first admits every Automatic Critical Moment as a complete ordered
presentation set. Admission validates Automatic provenance, classification,
Game identity, chronological order, and the legal Position Snapshot, then
atomically checkpoints the whole set before reporting display readiness. Each
admitted moment exposes a typed authoring readiness state: `pending` or
`prepared`. A pending presentation contains no rich Review Moment Authoring
Context, exploration state, or publication material.

Coach App session start returns after durable display admission and does not
wait for rich authoring. Delivery surfaces that require the complete prepared
automatic set use one server-owned batch path; clients never orchestrate
per-moment preparation. Coach Skill and the current Web surface use that path.
The server prepares pending moments under the existing bounded projection
deadline and durably advances their typed readiness before returning the
complete batch.

Preparation failure is scoped to the addressed Review Moment and never removes
or reorders the admitted presentation set. Successfully prepared peers remain
durable and a later batch request resumes from the mixed readiness state.
Terminal Coach Intent Unavailability remains a valid prepared outcome.
Contradictory facts, invalid classification, an illegal position, authorization
failure, or failure of the atomic presentation admission still fails closed.

The start result contains the complete admitted automatic set in ascending Game
ply together with the Game Review summary, timeline, and other product state.
Every moment carries typed authoring readiness. A prepared result may carry its
server-owned core for an authoring surface; the lightweight Coach App
presentation carries only the readiness summary and display facts. The result
contains no Entry Moment, active-moment pointer, highest-ranked presentation
role, eager canonical comment, generic manifest, or public authoring
provenance. `start_review_session` remains model-and-app visible in the Coach MCP
interface.

The currently displayed moment is delivery-surface navigation state. An interactive surface may display the earliest returned automatic moment as its initial default without another server operation, or it may display another moment first. A review with no Automatic Critical Moments is valid and displays none automatically.

Within a Review Session, one Game ply resolves to at most one Review Moment.
Opening a pending Automatic moment asks the server to prepare that exact
admitted moment; reopening a prepared one resumes its existing intent, fence,
comment if admitted, and exploration state. `open_review_moment` also remains
the operation that creates and prepares a previously unseen Player-Selected
Moment. Selecting an automatic ply never changes its Automatic provenance, and
a Player-selected moment joins chronological navigation without changing the
frozen automatic set or selector trace.

Comment prose remains surface-authored because web, Coach Skill, and Coach App have different Language Layers. Web applies the Grounding Gate internally, Coach Skill drafts and validates the complete ordered set, and a Coach App host model publishes each canonical comment through the server boundary. Review Moments have independent publication fences and exploration state, while the Review Session retains shared evidence and one session-wide active Coach Turn.

The Central Host may retain one Review Snapshot for each Automatic Critical Moment whose canonical comment is admitted, deduplicated by Review Session and Game ply. The artifact contains that moment's comment, trace, direct replay inputs, and internal authoring provenance. Session preparation alone creates no snapshot, and Player-Selected Moments remain transient for MVP.

## Consequences

The `StartReviewSession` command does not accept a selected moment or return an
`entryComment`. Ordering and presentation admission remain behind one atomic
session-start seam, while rich authoring preparation has an independent typed
lifecycle. Coach App gets a meaningful first frame without paying for every
intent projection. Coach Skill retains one complete-preparation request.
`open_review_moment` serves exact-moment preparation, Player-selected creation,
and idempotent resumption.

This preserves one cross-surface contract without conflating readiness states.
The complete automatic presentation is durable before success, mixed authoring
readiness is explicit, and the server—not the Language Layer—owns batch
preparation and partial-failure recovery.

This ADR supersedes ADR 0017's Entry Critical Moment, lazy later-moment initialization, and Player-triggered intent-retry decisions; ADR 0017's intent selection, uncertainty, interaction, evidence, and calibration decisions remain in force. It refines ADR 0018's artifact granularity and ADR 0019's Review Session orchestration while preserving their retention, kind-aware authoring, grounding, and publication decisions.
