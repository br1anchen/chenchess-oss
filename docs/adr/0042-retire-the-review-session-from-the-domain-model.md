# Retire the Review Session from the domain model

## Status

Accepted.

This decision retires **Review Session Checkpoint** as a domain term and
redefines **Review Session** as transient, process-local coaching state with no
durable referent. It completes the retirement ADR 0039 named but deferred, and
supersedes ADR 0039's remaining clauses that treated a Review Session as a
resumable product interface with its own lifetime.

It does not disturb ADRs 0016–0038. Those record what was decided when they
were written and are read as history; where they say "Review Session" they mean
the object this ADR retires.

## Context

The Review Session was, until #258, four things at once: a conversation, a
durable aggregate, an addressable handle, and a lifetime. #279 deleted its
tools, #282 deleted its lifecycle, checkpoint store, and identifier, and #283
deleted its web route. What survives in the running system is only the first of
the four — an in-memory actor keyed by Player and Game Import, holding engine
leases, prefetched analysis, and one in-flight Coach Turn.

The glossary still described all four. Roughly forty relationship clauses were
written against an object with a revision, a checkpoint, an idle deadline, and a
resume path, none of which exist. Three of those clauses were not merely stale
but wrong in a way a reader would act on:

- Interaction state was described as private to each chat. Canonical Review
  Moment Comments are now durable and shared across every conversation over one
  Game Import (#280).
- Player-Selected Moments were described as in-session feedback retained only in
  a short-lived checkpoint. They are durable twice over and outlive every
  conversation: classified at import onto the Player-owned Game Import Record,
  which has no expiry, and materialized as prepared analysis into the
  identity-free Review Analysis Cache when one is opened.
- Review Session Checkpoint was described as the store behind a review. There
  are two stores now, and neither belongs to a session: the shared, identity-free
  Review Analysis Cache (#281) and the Player-owned, append-only Review
  Annotation Store (#280).

A domain model that names a deleted object is worse than an incomplete one. It
tells a reader — human or model — to look for a handle that is not issued, a
revision that is not compared, and a resume that does not exist.

## Decision

Redefine rather than delete. "Review Session" still names something real: the
transient coaching interaction one Player holds over one Game Import. Keeping
the term costs one clarifying definition; deleting it would leave the actor,
its module names, and its command vocabulary unnamed in the domain language.

The redefined term carries four constraints, and everything else follows:

1. It is keyed by Player and Game Import and by nothing else. It has no
   identifier of its own.
2. Nothing about it is durable. Nothing a Player can see depends on it
   surviving.
3. Losing one costs a rebuild from state the Player already owns — the durable
   Game Import Record, the Review Analysis Cache, the Review Annotation Store.
4. It is never a handle a caller carries, an address a surface links to, or a
   lifetime a Player-visible guarantee depends on.

Delete **Review Session Checkpoint**. Its replacements are named directly, and
neither is scoped to a conversation.

Introduce **Review Session Residency** for the 72-hour idle and 336-hour
absolute bounds. They are a memory bound on an actor's engine leases, not a
retention window on anything a Player owns. Naming them separately is what stops
the deleted lifetime from creeping back in as "session expiry".

Rename **Review Session Evidence Packet** to **Review Evidence Packet**. It is
evidence accumulated while one Player studies one Game Import; the session is
where it happens to live, not what it is about.

Reverse the shared-mutable-state clause and correct the Player-Selected Moment
retention clause to match the code.

Delete the two places where the running system still names the retired object.
Both are naming, not behaviour, and both would otherwise contradict the
definition above the moment anyone read them:

- The dead `expiredSession` command rejection reason, in the Rust contract and
  the generated SDK. No code path constructs it, because no session expiry is
  Player-visible any more. `unknownSession` stays: an actor can age out
  mid-command and the Player is asked to start again, which is a rebuild rather
  than a loss.
- The web application's `chenchess.review-session:` local-storage key and its
  `usePlayerReviewSessionId` / `useResumeReviewSession` hooks, which held a Game
  Import ID under the vocabulary of a session handle and a resume. They become
  `chenchess.last-studied-review:` and `useLastStudiedReview` /
  `useOpenLastStudiedReview`. Existing keys are orphaned rather than migrated;
  the cost is one Player landing on the import form instead of their last
  review, and the product is pre-release.

Module and directory names that merely contain "review session" —
`review_session_processor`, `ReviewSessionWorkspace`, `review-session-telemetry`
— keep them. They name the transient actor and the coaching surface, which is
exactly what the redefined term covers.

## Consequences

The glossary and the running system agree. A reader looking for durable review
state finds exactly three things: the Game Import Record, the Review Analysis
Cache, and the Review Annotation Store.

The word "session" survives, which is a deliberate risk. It reads as durable to
most people, and the module names (`review_session_processor`,
`ReviewSessionWorkspace`) still carry it. The definition, the Residency term, and
the `_Avoid_` list are the mitigation; a future reviewer who finds a session
identifier, a session-scoped durable record, or a resume path has found a
regression, not a design.

Comment durability is a genuinely new guarantee, not a restatement. Before #280 a
published comment lived only in a checkpoint bounded by a 14-day absolute
lifetime. The glossary now promises that a comment outlives every conversation
and is erased only with the Player subtree, which binds it to the retention and
account-deletion paths rather than to a cache eviction policy.

Anything reading the retired vocabulary off the wire — a host, a fixture, a
recorded conversation — no longer finds `expiredSession`. This is a contract
break, taken deliberately before release rather than carried as a dead variant.
