# Address frozen Game Reviews by durable Player-owned import

## Status

Accepted.

This decision supersedes ADR 0034 only where it treated a Player-owned Game
Import as TTL-bounded. It leaves the optional identity-free Game Analysis cache,
the transient Review Session lifetime, environment isolation, and account
deletion workflow unchanged.

Superseded in part by #258 on 2026-08-09, in the one clause that preserved the
Review Session as a resumable product interface. The object that clause named no
longer exists: #282 deleted the Review Session as a durable, protocol-level
thing — no identifier, no checkpoint, nothing to resume — and #283 deleted the
`/app/review-sessions/{sessionId}` route and `resume_review_session` with it.

Superseded further by ADR 0042 on 2026-08-10 in every clause that treated a
Review Session as having its own lifetime: the term now names transient,
process-local coaching state, and Review Session Checkpoint is retired
altogether. Everything this ADR decided about the durable Player-owned Game
Import stands unchanged.

Superseded further by #324 on 2026-08-10 where it prescribed an identity
resolver for historical input-only cards. MCP Apps provides no signal that can
distinguish such a replay from a live call whose result is still in flight, so
automatic identity recovery can race the import it is trying to resolve. The
durable `gameImportId` decision stands; every tool that mounts an app carries
that review address in its own input. A reference whose exact Review Moment is
known only after resolution relies on the completed tool result and fails
visibly when replayed input-only.

## Context

The `critical-moments-selector` renders a frozen Game Review. Historical host
conversations can outlive the Review Session that happened to create their
first widget result. Reconstructing that selector by `sessionId` therefore makes
durable, stateless review output disappear when unrelated interactive coaching
state expires.

The existing Game Import is already the self-contained ownership boundary. It
stores the normalized Game, resolved review profile, frozen Game Review,
Automatic Critical Moments, evaluation timeline, and provenance under the
hashed Player subtree. Review Sessions reference it but additionally store
mutable and retry-sensitive coaching state.

## Decision

Promote the Player-owned Game Import to durable product data. It has no
time-based expiry and no Firestore TTL fields. It remains scoped to the
authenticated Player and is deleted by recursive Player-subtree deletion during
account deletion or by another explicit product-data deletion workflow.

Use `gameImportId` as the canonical cross-surface address of the frozen review.
The authenticated web route is `/app/game-reviews/{gameImportId}` and the
read-only MCP operation that displays the whole review is
`list_critical_moments({gameImportId})`. `review_game` derives the same stable ID
from the exact original Game source, Review Side, and Elo, but mounts no app
because its input necessarily predates the address it creates. After it
succeeds, the Language Layer immediately calls `list_critical_moments` with the
returned ID. A result-less historical `review_game` card cannot be recovered
portably and must fail visibly rather than guess or race a live call.

~~Keep `/app/review-sessions/{sessionId}` and `resume_review_session` as current
product interfaces, not as aliases or migration shims. They resume a different
object: a temporary interaction checkpoint containing prepared Review Moment
state, canonical comments, publication and idempotency fences, alternatives,
Coach Turns, Player-selected moments, revisions, and coaching state. Review
Sessions retain their 72-hour idle and 14-day absolute lifetime.~~ Superseded by
#258; see Status. Every web address now hangs off the canonical Game Review
route: `/app/game-reviews/{gameImportId}/moments/{reviewMomentId}` and
`.../sequences/{kind}`.

The base Critical Moment selector renders directly from the durable Game Review.
~~Starting or continuing interactive coaching explicitly creates or resumes a
Review Session. Session expiry never makes the frozen selector unavailable.~~
Superseded by ADR 0042: there is no session to resume and no expiry to survive.

Represent normalized import data directly as `ImportedGame` inside the durable
Game Import Record. It contains canonical Game, Review Side, resolved Elo
Profile, and import provenance; it has no one-field snapshot/content wrapper.
Keep `frozenReview` and engine provenance as sibling record fields. Operations
that only retrieve the frozen review do not automatically expose
`ImportedGame`.

Reset the durable Game Import contract to schema v1 and use an operator-run,
fail-closed rewrite for legacy schema v1/v2/v3 records. Current v1 is
distinguished from legacy v1 by its direct payload shape and absence of
retention fields. Valid legacy TTL fields on v1/v2 are checked and then
removed. Legacy `snapshot.content` and `review` payload fields become direct
`importedGame` and `frozenReview` fields. The Player ownership path and frozen
values do not change. The same rewrite normalizes pre-cutover Player-selected
learning support and removes obsolete non-opening projections that the current
Review Session pipeline deterministically derives from retained facts.

## Consequences

Historical web and Coach App review widgets can reopen without relying on a
live Review Session. The Language Layer can retrieve the same exact frozen
review through an authenticated, read-only operation.

Player product storage now grows until explicit deletion rather than natural
Game Import TTL cleanup. A future product retention policy must delete
Player-owned imports deliberately; it must not silently reintroduce expiry as a
cache policy. The optional Game Analysis cache remains separately bounded.

~~Session URLs can still expire, because they promise continuation of mutable
coaching work. A client that only needs the frozen review should store or use
the Game Review URL instead.~~ Superseded by ADR 0042: no session URL exists,
and the Game Review URL is the only review address a client stores.
