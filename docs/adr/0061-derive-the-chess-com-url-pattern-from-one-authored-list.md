# Derive the Chess.com Game URL pattern from one authored list

## Status

Accepted (2026-08-31). Resolves the open question posed by
#549.

## Context

Which Chess.com Games ChenChess imports was stated in four places, none derived
from another: a `strip_prefix` chain in `services/coach-engine/src/chess_com.rs`,
a regex literal in `packages/ui/src/import/chess-com.ts`, a second regex literal
published in the Coach App tool schema, and prose in `CONTEXT.md` and the SDK
README.

That is not a hypothetical cost. #318 added Daily Games on 2026-08-12 and
updated two of the four. The tool schema kept `computer|live` for nineteen days,
so a Daily Game was reviewable in the web app and unreachable from a model host
— the host refused the argument before the Engine was ever asked, so the Player
got a schema mismatch rather than the typed rejection and its recovery
(#548). The prose was worse
than silent: `CONTEXT.md` listed "Daily Game URL import" under _Avoid_, teaching
the narrower rule the tool schema had been written to.

#549 asked whether the fix should publish a URL regex through the generated SDK
at all, on the grounds that provider trivia does not belong in a contract that
has so far held the Player-facing model.

## Decision

**The contract crate owns provider URL grammar, and the generated SDK may carry
values that are not wire types.**

Two facts settle the question the issue raised.

`services/coach-engine/crates/contract/src/lichess.rs` already holds a
provider's URL grammar, documented as "Pure parsing only — export transport
stays in the app crate". Provider URL grammar in the contract crate is
established precedent, not a new category, and Chess.com is the asymmetry.
`crates/contract/src/chess_com.rs` now mirrors it exactly: the kind list, the
URL prefix, the Game id grammar, `ChessComGameUrl` and its parser, and
`chess_com_game_url_pattern()` built from the same list. The callback transport
and the per-kind contract versions stay in the app crate as free functions over
that type, which is the shape `lichess`'s `export_request` already had; the app
module re-exports the grammar so existing paths keep working.

#549's first step said to move the kind list "leaving the parser where it is".
That would have split one grammar across two crates — the pattern built in the
contract crate, the parsing done in the app crate — and forced the URL prefix
and the Game id predicate public purely so the app crate could re-implement
what the contract crate already knew. Moving the parser with the list keeps
them honest together and makes both private again.

The published *wire* contract is `review-session.schema.json` plus the ts-rs
types, and both are walked from the `contract_roots!` list. A TypeScript
constant is in neither, so publishing one leaves the schema byte-identical. The
SDK package is the published *client*, not the wire schema — it already ships
`construct.ts`, `decoder.ts` and `client.ts`, none of which are wire types
either. `generate_review_session_contract` therefore emits
`chessComGameUrl.ts`, exporting `CHESS_COM_GAME_URL_PATTERN` from the package
root and from a `./chess-com-url` subpath, and `tools.ts` and
`packages/ui/src/import/chess-com.ts` read it instead of restating it.

**The published pattern is deliberately looser than the Engine.**
`[1-9][0-9]*` does not encode the `u64` bound the parser enforces. The pattern
is a client-side pre-filter that keeps a surface from refusing what the Engine
accepts; the Engine stays the authority on what it rejects, and rejects with a
typed reason a Player can act on. Tightening the pattern to match exactly would
trade a recoverable rejection for a schema mismatch — the failure #548 was. The
divergence is asserted rather than described: the accept/reject table carries an
id past `u64`, which the pattern admits and the parser refuses.

## Consequences

**The provider grammar has one home, and a smaller public surface than the
split would have needed.** The URL prefix and the Game id pattern are private to
the contract module, because nothing outside it parses a Chess.com URL any more.
Two callers in `profile_game_feed` that rebuilt a URL string only to re-parse it
and compare the parts back against the values that built it now ask the grammar
directly.

**Adding a Game kind is one edit.** `ChessComGameKind::ALL` and `as_path` are
the authored list; the parser walks it, the pattern is built from it, and
`--check` drift on the generated SDK fails the moment a surface disagrees with
the Engine. The accept/reject table in `tests/review_session_game_import.rs`
runs the published pattern beside the parser, so a pattern that stops matching
the parser fails a test rather than a Player's import.

**`packages/ui` gains its first workspace dependency.** It depended on no
`@chenchess/*` package before this. The `./chess-com-url` subpath exists so the
web import field pulls one constant rather than the SDK's client, and the edge
is `packages/* -> packages/*`, which the package-boundary rules already allow.

**Three prose statements remain, and stay prose.** `CONTEXT.md` defines the
Game Import term for readers, the SDK README describes the import for
consumers, and the Coach Skill's `SKILL.md` tells a local agent what input to
accept; none is machine-read, and reducing them to a generated line would cost
more in legibility than it buys. The README template in the contract crate now
names the published constant, so a reader has somewhere to go.

`SKILL.md` was the fourth surface #548 escaped to and the one nobody counted:
it still listed `computer` and `live` nineteen days after Daily Games shipped,
so the Coach Skill would refuse a Daily URL the Engine imports. It is corrected
here. #549 counted four restatements; there were five.

The parent commit `59d6319f` hand-edited the *generated* README rather than its
template in the contract crate, which left the drift gate red. That is fixed
here, and it is the same class of mistake this decision exists to make
impossible.

**Lichess is unchanged and still restates its pattern** in `tools.ts`. The same
argument applies to it and the grammar is already in the contract crate, so the
move is mechanical — but #549 scopes to Chess.com and this decision follows it.

**`ImportedChessComGameKind` in `imported_games.rs` is a third Rust statement of
the kinds** and stays one. It is a stored serde shape; collapsing it onto the
contract enum retires stored records, which is a separate decision with a
separate cost.

## Alternatives within the shape

**A shared TypeScript constant with a Rust test asserting the two agree.** The
alternative #549 named, to keep provider parsing out of the published contract.
Rejected because the premise does not hold — the contract crate already holds
Lichess's grammar — and because it keeps two authored statements and adds a test
to watch them, where deriving leaves one statement and nothing to watch.

**Read the kind list from `packages/shared-assets` as JSON, the way shared
limits are read.** Rejected. It would make Rust the reader rather than the
authority, and a runtime-parsed list cannot be matched exhaustively, so the
compiler would stop catching a kind the Engine's per-kind transport forgot.

**Put the pattern in the JSON Schema as a contract type.** Rejected. A
provider's URL grammar is not a Game Review concept, and admitting it to the
wire contract would make every host's decoder carry it.
