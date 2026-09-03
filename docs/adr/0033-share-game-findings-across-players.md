# Share Game Analysis across Players

## Status

Accepted as amended by ADR 0034. The cache is environment-local, uses the
`gameAnalysis` collection, contains an identity-free Game shape, seeds a
self-contained Game Import, stores an opaque canonical-JSON payload, and
bounds sliding retention with an immutable hard expiry.

## Context

The import pipeline's cost is almost entirely provider work: the Engine analysis
and human-move model together dominate the measured latency of `review_game`,
far ahead of fetching the Game and committing records. That work is a pure
function of three things the Player names — the Game, the Review Side, and the
Elo Rating — and `ReviewSessionGameIdentity` already fingerprints exactly those
three for handle-less resume.

Popular Games are reviewed by more than one Player, and the same Player
re-reviewing a Game pays the full cost again. Every one of those runs produces
the same findings, and every one of them occupies Engine admission that a
first-time review is waiting on.

The obvious objection is authorization. `GameImportRecord` is owner-scoped and
`find(owner, id)` enforces it, so "reuse another Player's import" would move a
boundary. But the Game Review is not the Player's data: it is derived from a
public Game plus deterministic Engine analysis. Nothing in it is authored by,
about, or traceable to whoever paid for it.

## Decision

Add a **Game Analysis** store keyed by the schema version, analysis generation,
canonical Game digest, Review Side, and resolved Elo. It holds the
identity-free Game shape, Game Review, selected Automatic Critical Moments,
evaluation timeline, and Engine provenance.

An analysis entry has **no owner** and no Player identifier, no Review Session
reference, no Player name, event, site, source URL, or anything the Player
wrote. That absence is the property that makes sharing sound, so it is a
constraint on the record shape rather than an incidental fact about today's
fields.

The lookup sits after the Game is imported and before the review runs, so the
fingerprint is derived from resolved values and agrees with the one a later
resume derives from what the Player typed. A hit skips both Engine admission
and every provider call; the import diagnostic then honestly reports zero
provider calls rather than replaying the original run's timings.

Per-Player state does not move. Each Player receives one self-contained Game
Import Record for the current durability generation, Game, Review Side, and
resolved Elo. Repeating that import returns the record's stable ID and exact
frozen Game Review. A cache hit may seed the first record and creates no later
dependency on the analysis entry. A second Player who names the same Game gets
their own import and is still rejected on the first Player's handle.

The cache is never load-bearing. A read failure, a write failure, a malformed
entry, or an entry from a superseded shape degrades to recomputing the
analysis, because a Player waiting longer is strictly better than a Player
getting an error.

Invalidation is wholesale, not per-key. Engine identity is deliberately not
part of the Player-named Game digest. A single analysis generation participates
in the document ID, so bumping it makes every existing entry unreachable and
lets it age out on its TTL. Serving several Engine versions side by side for
comparison is a separate feature, deferred.

Retention is a storage-cost decision rather than a Player-data one. Analysis
outlives any single review because the next Player to name the Game may be days
away, and no Player's quality capture preference reaches it. A hit may slide
`purgeAt` forward but never beyond the `hardExpiresAt` fixed at creation.

## Consequences

- A repeat review of an already-analyzed Game in the same environment skips
  the dominant cost of the import pipeline and consumes no Engine admission.
- A matching repeat import by the same Player returns the existing Game Import
  instead of creating another copy; a new session-start operation creates a
  separate Review Session incarnation over it.
- The owner-scoped authorization on Game Imports, Review Sessions, and
  checkpoints is untouched; no existing read gains a new caller.
- `gameAnalysis` requires a Firestore TTL policy on `purgeAt`. Phase 1 replaces
  the predecessor `gameFindings` declaration in `firestore.indexes.json`.
- TTL policies are declared as `fieldOverrides` with `ttl: true` and deployed
  by `bun run deploy:firebase:firestore`, so a new TTL-bearing collection is a
  repository change rather than a console click. `purgeAt` is never queried,
  so its override deliberately uses an empty index list.
- Changing what the Engine reports requires bumping the generation constant.
  Forgetting to do so serves stale analysis until `hardExpiresAt`.
- Two Players racing on the same unseen Game both compute; last writer wins,
  and both results are correct for the same inputs.
