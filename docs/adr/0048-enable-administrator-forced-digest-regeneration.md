---
status: accepted
---

# Enable administrator Forced Digest Regeneration

This decision supersedes ADR 0045 only where it forbids the Beta Back Office
from rerunning a terminal Daily Window or changing an existing Coaching Digest.
Every other prohibition in ADR 0045 stays in force. ADR 0044 remains
authoritative for Digest Email Replay.

## Context

The Beta Back Office has two digest actions and neither can answer the question
an Administrator actually has after fixing the pipeline: *does this Player's
last window now produce the right digest?*

Digest Email Replay re-sends the bytes of an already-published digest. It
verifies the mail path and nothing upstream of it.

Manual Digest Run is admitted only while the Player's Run archive is empty, and
it promotes the window the ordinary arrival path already considers due, which
advances the Player's schedule.

The gap is not hypothetical. A Daily Coaching digest published for two days
with one connected provider silently missing, because a serde struct required a
field that provider has never published. The provider feed failed, the Run
dropped that connection as transient, and the digest published looking
complete. After correcting the contract there was no way to rebuild the affected
window: the digest existed, so Manual Digest Run refused, and replay would only
have re-sent the wrong digest.

Verifying a provider-contract fix therefore required waiting a full day for the
next scheduled window, on a Player whose provider history might not even
intersect it.

## Decision

The Beta Back Office may start one **Forced Digest Regeneration** for a Player
with active Beta Access. The row-bound endpoint resolves the redeemed Beta
Access Request to a Player ID on the server and rejects account deletion or
revoked access, exactly as the two existing digest actions do.

The action rebuilds the Player's **latest terminal Daily Window** through the
ordinary Daily Game Selection, Game Review, Coaching Digest publication, and
digest-email delivery pipeline. It re-selects and re-reviews Games rather than
republishing frozen reviews, because exercising provider fetch, decode, and
selection is the entire purpose.

It accepts no date and no Player ID from the browser. It reads the window
bounds from the stored Run rather than recomputing them from the current
instant, so a regeneration run cannot drift onto a different window.

It replaces the Coaching Digest for that coverage date in place. The digest
identity stays derivable from the window. Superseded digest contents are not
retained; a `regenerationCount` on the Run document carries the audit trail.

Because the digest identity is stable, the **delivery** identity must not be:
the digest-email idempotency key is derived from the delivery identifier, so a
regenerated send carries a regeneration ordinal. Without it the provider
collapses the second send as a duplicate and the failure is invisible — the
same shape of silent success this decision exists to remove.

The action cannot start a future window, bypass the Run-claims kill switch,
bypass email preference or suppression state, advance the Player's schedule, or
change `next_daily_window`, nudge admission, or initial-backfill state. A
successful HTTP response means the regeneration was admitted, not that
publication or provider delivery already finished.

## Consequences

An Administrator can rebuild the last window on demand and exercise the
complete pipeline, which is what verifies a provider fix.

The action is materially more expensive than either existing one: it runs up to
ten full Game Reviews and re-reads both providers. It is Administrator-only,
one in flight per Player, and behind a confirmation in the Back Office. It is
not available to Players and must not enter any automated loop.

Replacing a digest in place means the Player's previously delivered email and
the current digest can disagree. That is accepted: the regenerated digest is
the corrected one, and the Player sees no marker distinguishing it.

Daily Coaching keeps three distinct Administrator actions, separated by what
they rebuild rather than by who may call them.
