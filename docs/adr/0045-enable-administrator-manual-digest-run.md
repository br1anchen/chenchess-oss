---
status: accepted
---

# Enable administrator Manual Digest Run

ADR 0044 remains authoritative for Digest Email Replay. This decision adds a
separate action for the case where no Coaching Digest has been published yet.

ADR 0048 supersedes this decision only where it forbids rerunning a terminal
window and changing an existing Coaching Digest, and only for the Administrator
action it defines. Every other constraint below stays in force.

## Context

Digest Email Replay lets an Administrator verify an already-published email,
but it cannot help when a Player's previous-calendar-day Daily Window is due
and no digest exists. Waiting for the periodic Tick makes staging email checks
needlessly slow and gives the Beta Back Office no recovery action for a delayed
due window.

## Decision

The Beta Back Office may start one Manual Digest Run for a Player with active
Beta Access. The row-bound endpoint resolves the redeemed Beta Access Request
to a Player ID on the server, rejects account deletion or revoked access, and
returns before the Run finishes.

The action is available only when no Coaching Digest exists and the Player can
receive digest email. It promotes the exact Daily Window that the ordinary
arrival path already considers due. That window covers the previous local
calendar day and uses the normal Daily Game Selection, Game Review, Coaching
Digest publication, and digest-email delivery pipeline.

The action accepts no date or Player ID from the browser. It cannot start a
future window, rerun a terminal window, bypass the Run-claims kill switch,
bypass email preference or suppression state, or change an existing Coaching
Digest. Existing nudge admission and Run creation keep concurrent or repeated
requests from creating duplicate work.

Digest Email Replay stays unchanged. Once a Coaching Digest exists, the Beta
Back Office offers replay instead of Manual Digest Run.

## Consequences

An Administrator can start yesterday's missing digest and exercise the same
email notification path used by the scheduler. A successful HTTP response
means the Run was admitted, not that publication or provider delivery already
finished.

The action advances the ordinary Daily Coaching window only through the same
Run claim and state transition used by a scheduled Tick. It adds no separate
schedule, arbitrary backfill control, or digest-regeneration path.
