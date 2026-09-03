# Enable administrator Digest Email Replay

## Status

Superseded by ADR 0048.

Forced Digest Regeneration rebuilds a window and sends its digest email, which
is a strict superset of replaying an unchanged one. Offering both asked an
Administrator to choose between an action and part of itself, so replay is
withdrawn: both endpoints and the replay delivery path are removed rather than
left unreachable. Re-sending an unchanged digest without re-reviewing its Games
is deliberately no longer possible.

This decision's supersession of ADR 0029 stands: the Beta Back Office may read
coaching data. ADR 0029 remains authoritative for the Beta Access and
invitation lifecycle.

## Context

Daily Coaching publishes and sends digest emails on its schedule. Testing the
same email path in staging currently requires waiting for the next digest
window. An Administrator also needs a way to verify a recent live digest email
without rebuilding the digest or changing scheduler state.

The Beta Back Office already lists Players with Beta Access. It can resolve a
redeemed request to a Firebase UID on the server, so the browser does not need
the UID. Staging and production share Firebase Authentication but keep all
Daily Coaching data separate. A replay must therefore read and send only the
digest produced in the environment that received the request.

## Decision

The Beta Back Office may request a Digest Email Replay for a Player with active
Beta Access. The list exposes only the latest published digest's coverage date,
publication time, game count, learning-path count, and email readiness. It does
not expose the digest identifier, digest contents, game identities, profile
URLs, or learning content.

Both listing and replay require an Administrator. The row-bound endpoint
resolves the redeemed request to the Player ID on the server. Before reading
Daily Coaching state or accepting a replay, Coach Engine rejects a Player whose
account deletion has started or whose Beta Access is not active.

The replay reads the latest already-published digest in the current deployment
environment and creates a unique delivery record for an asynchronous provider
handoff. It does not change the digest, digest run, archive, or schedule. It
uses the Player's current verified email, opt-out preference, and provider
suppression state. A provider failure does not enter the scheduler's retry
loop; another replay requires another deliberate Administrator request.

Production bypasses the temporary Beta Access authorization gate for product
features. A future production backoffice roster may call the environment-local
replay endpoint, but it must not read staging invitations or staging Daily
Coaching data.

## Consequences

An Administrator can test yesterday's staging digest through the same email
composition and provider path as scheduled delivery without waiting for the
next window. Each accepted action has independent delivery bookkeeping, while
the published digest and scheduler history remain unchanged.

The Beta Back Office now has one narrow coaching-data exception. New metadata
or replay targets require another explicit privacy decision. Production still
needs its own roster before this action is operable from a production
backoffice; shared Firebase Authentication does not bridge environment data.
