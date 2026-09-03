# Resolve profile Games as independent Game Imports

## Status

Accepted.

## Context

Daily coaching digests need a bounded set of a Player's newest public Lichess
or Chess.com Games. Issue #127 remains intentionally scoped to one Game's
grounded Learning Plan and excludes cross-Game history, scheduling, cadence,
and reassessment.

The existing Game Import boundary also owns exactly one Game. It validates one
source, resolves one Review Side and Elo Profile, persists one Game Import
Record atomically, and returns one Game Import ID and Game Review. Making a
profile URL another `GameInputSource` would change that cardinality and force
long-running provider discovery, several full Engine reviews, partial-failure
policy, and batch persistence into a boundary whose recovery semantics are
single-Game.

## Decision

Add a separate **Profile Game Feed** to the Coach Engine. Given one exact
Chess.com member URL or Lichess profile URL (including its `/all` game-history
form) and an explicit count from one through ten, it returns a
newest-first list of ordinary Game Import requests. Each request contains a
canonical supported Game URL, the Review Side inferred by matching the profile
handle to one player, and `FromImportedMetadata` as its Elo Profile request.
The feed retains neither the profile URL nor the returned list.

The Lichess adapter uses the official user-games endpoint with finished
standard-chess performance types and reverse-chronological ordering. It scans
a small bounded surplus so aborted games do not prevent filling the requested
count. The shared client identifies ChenChess with its public support URL,
serializes feed requests, and surfaces rate limits rather than retrying them
concurrently.

The Chess.com adapter uses only official PubAPI endpoints. Exact-window reads
address the one or two UTC archive months intersected by the window directly;
initial discovery traverses at most twelve directly addressed months from
newest to oldest. It does not read or follow the archive index. The adapter
selects completed standard live and Daily Games. Daily Games enter the shared
selection order as correspondence Games without a clock-based sub-order.

Each accepted archive Game carries its final PGN into an internal Daily
Coaching import request. The one-Game import boundary therefore does not make a
second Chess.com request. It records the existing Chess.com provenance fields
with the PubAPI archive contract version, the monthly response capture time
and digest shared by Games from that response, and the digest of each Game's
exact PGN bytes. This archive-sourced input is not part of the public
`GameInputSource` contract or generated SDK.

The durable Daily Coaching lifecycle owns scheduling, deduplication,
last-seen state, retry, and partial-failure policy. It submits every selected
feed item independently through the ordinary one-Game Game Import boundary,
then aggregates the resulting grounded Learning Plans. The interactive Review
Session command remains synchronous and does not become a background batch
protocol; Player-initiated imports accept the same supported live, computer,
and Daily Game URLs.

The existing release decision for Chess.com remains in force: do not
distribute an automated Chess.com educational integration until written
authorization has been obtained.

## Consequences

- Provider discovery and one-Game import remain deep, independently testable
  modules.
- All current eligibility, provenance, rating, persistence, and Game Review
  behavior is reused without a second batch implementation.
- One failed Game does not invalidate or ambiguously roll back other Game
  Imports; the Daily Coaching lifecycle records each unit's outcome.
- Daily scheduling, durable profile linkage, digest aggregation, and Learning
  Plan history remain outside the single-Game import boundary.
