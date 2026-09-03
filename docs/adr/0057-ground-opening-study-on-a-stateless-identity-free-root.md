# Ground opening study on a stateless identity-free root

## Status

Accepted.

This decision adds a second grounded root for the **Coaching Board** — the
**Opening Line** — without adding a second aggregate, and introduces the
**Opening Analysis Cache**. It leaves ADR 0042's constraint that a **Review
Session** is keyed by Player and Game Import and by nothing else intact.

It builds on ADR 0056, which introduced the Coaching Board.

## Context

The Coaching Board's lobby has two exits: bring in a played Game, or start from
an opening. The second one had nowhere to stand.

**Every engine command is keyed by a Game Import.** The whole
`ReviewSessionCommand` union takes a `gameImportId`, and `exploreAlternativeMove`
additionally takes a `reviewMomentId` and a `BranchParent`. Nothing accepts a
bare position. So an opening board is not a missing route; it is a missing root.

**The catalog names lines but does not assess them.** The pinned CC0 ECO
catalog is compiled into the binary with `include_bytes!` and read by
`opening_identification.rs`. The glossary is explicit that it "establishes
descriptive opening knowledge, never objective move quality." A board that can
name the Najdorf but cannot say whether a deviation is bad is recitation, not
coaching — and the deviation is where the Player's real question lives, usually
around move four, off book.

**A catalog line is not importable.** Rows are three to ten moves; `importGame`
requires a `CanonicalCompletedGame`. There is no way to smuggle an opening in
through the Game path.

**Only the move path identifies a line.** Across 3,690 rows there are 499
distinct ECO codes, 3,160 distinct names, and 3,313 distinct ECO+name pairs, but
3,690 distinct `pgn` paths. A00 alone names 143 lines, and ECO+name still
collides 377 times, on transpositions of the same named line. An address built
on ECO, or on a name, is ambiguous by construction.

**Nothing scopes the compute.** `evaluate_player_line`'s allowance is
`max_committed_alternative_moves` minus committed nodes, held by the in-memory
Review Session actor. A root with no actor inherits no cap, and unbounded
Stockfish on caller-supplied lines is a service anyone with Beta Access could
farm.

The obvious alternatives were a second transient actor keyed by Player and
Opening Line, mirroring Review Session's leases and residency; or widening the
Review Session key to a Coaching Subject that is either a Game Import or an
Opening Line. The first duplicates a lifecycle #282 deliberately shrank. The
second supersedes ADR 0042's first constraint and puts roughly forty relationship
clauses back in play.

## Decision

**Add no aggregate.** Opening analysis is a stateless engine route: given an
Opening Line and an ordered line from it, return the position and per-ply
evaluations — the shape `evaluate_player_line` already returns, rooted at the
initial position instead of a Review Moment. There is no actor, no key, no
residency policy, and no Player-owned state, so ADR 0042 is untouched. The
exploration tree for an opening lives in the page exactly as the game one does.

**Address a line by its move path; address analysis by position.** These are two
keys because they answer two questions. `OpeningLineRef` is
`<eco>-<name-slug>-<digest4>` over the catalog row's path — the digest is the
identity, the slug is legibility, and it is always present so every address has
one shape rather than a suffix that appears only on the colliding rows. The
**Opening Analysis Cache** is addressed by normalized position, with no owner and
no session segment, mirroring `review_analysis_cache`. Transpositions therefore
collapse onto one cache entry, which is correct — two move orders reaching one
position are one board — while remaining distinct addresses, which is also
correct.

**Allow off-book analysis, bounded twice.** The line is capped at the same twelve
plies a Player Line is capped at, and the route is rate-limited per Player using
the `beta_access` rate-limit document pattern. Restricting analysis to catalog
positions would have made the cache a finite warm set and removed the need for a
limiter entirely, but it would refuse the question the Player actually asks.

**Split offer, find, and open; keep analysis identity-free.** Empty-state
offer names only openings the Player has played; no imported Game means no
offer. That is “names no opening it cannot attribute.” Typed find is
Player-scoped: the lookup route is authenticated and returns catalog rows
that already match the query, played matches ranked first, unplayed matches
allowed. `open_opening_line` is navigation of a path-identified catalog
line; find ranks, open does not re-rank. Analyzing a line is not
Player-scoped: nothing about who searched, or what they have played,
reaches the cache. The Player's opening history is known only as an ECO
code and a name — the pair that collides — so it ranks rows and never
identifies one; a played opening resolves to the shortest-path row of its
pair, the canonical order of that named line.

**Return the aggregate rather than the ingredients.** Played openings are counted
over every imported Game with no window, and exposed both as a route and as a
tool. `search_reviewed_games` is already on this surface and is truncated, so an
agent asked what the Player plays most would aggregate over a partial page and
answer confidently from it. The same reasoning governs the Coaching Board
Snapshot carried on every board-tool result: where ChenChess holds an
aggregate, the agent is handed it rather than left to rebuild one. Lobby
import and find are not board tools and do not carry that snapshot.

## Consequences

The Opening Analysis Cache is reachable by any Player with Beta Access, and no
per-Player authorization boundary sits in front of it, because the data is public
chess knowledge that identifies nobody. The rate limit is therefore the only
guard on compute, and it is load-bearing rather than defensive.

Opening exploration does not survive a page reload. Nothing durable is lost —
re-exploring is a cached stateless call — and the Coaching Board Snapshot is
self-sufficient by construction, so a reloaded page tells the agent everything.

The catalog pin is now an address dependency, not only an identification input.
Moving `data/chess-openings/<version>/` changes paths and therefore
`OpeningLineRef`s, so any pin bump invalidates saved and shared opening
addresses. That is acceptable while pre-production and should be revisited before
opening addresses are promoted anywhere durable.

Two routes now read the catalog where one did. `opening_identification.rs` stays
the only reader; lookup and analysis go through it rather than parsing the TSV
again, so identification logic stays single-implementation.
