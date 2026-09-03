# Chessiro account connection and game import compared with ChenChess

Research date: 2026-08-08

## Finding

Chessiro connects the two providers in different ways. Its current public web
bundle gives a fairly crisp answer.

- **Lichess:** Chessiro starts Lichess OAuth with `preference:read` and
  `email:read`, then keeps the returned Lichess username in its Chessiro
  session. The dashboard does not use that OAuth token to download games. It
  calls Lichess's public user-games endpoint without an `Authorization` header
  and falls back to Chessiro's saved-game API.
- **Chess.com:** the onboarding UI looks up a typed username and stats through
  Chessiro same-origin proxies, then saves `{ platform: "chesscom", username }`
  to the Chessiro user profile. The dashboard reads public monthly PubAPI
  archives through another proxy and falls back to saved Chessiro games. No
  Chess.com OAuth redirect, token, or ownership proof appears in the inspected
  client flow.

The Lichess OAuth observation comes from Chessiro's
[public auth chunk](https://chessiro.com/_next/static/chunks/67b5aa658a9a7d9a.js)
and [platform helper chunk](https://chessiro.com/_next/static/chunks/78d76f2f1669310e.js).
The Chess.com onboarding and both history loaders come from its
[public onboarding chunk](https://chessiro.com/_next/static/chunks/c3e0b8ab0fa4b965.js),
[Chess.com connection helper](https://chessiro.com/_next/static/chunks/c4e6af38c6774816.js),
and [dashboard chunk](https://chessiro.com/_next/static/chunks/3a1dd5a4e7c45ccf.js).
These URLs identify the inspected August 8 deployment and will change when
Chessiro deploys a new build.

ChenChess currently has neither kind of chess-account connection in its web
journey. Its login identity is Firebase email/password or Google. The shipped
review request accepts one completed Lichess URL, one completed Chess.com URL,
or pasted PGN. A separate Coach Engine `ProfileGameFeed` can resolve the newest
public games for an exact profile URL, but it has no production caller outside
its own module. It is a public-profile reader, not an account-linking system.

The useful product lesson is to name these capabilities honestly. "Follow my
public Chess.com profile" and "Authorize Lichess play" have different proof,
credentials, revocation, and privacy requirements.

## Confidence and limits

This note uses three evidence labels:

- **Verified** means Chessiro states it on a first-party page, the behavior was
  visible on its public site, an official provider specification defines it,
  or the ChenChess repository implements it.
- **Inference** means the provider contract makes the mechanism likely, but an
  authenticated Chessiro network trace was not available.
- **Unknown** means the public evidence does not settle the question.

I inspected Chessiro anonymously and did not create an account, authorize a
provider, or submit a game. The public home page, sign-in and connection UI,
first-party articles, policies, and JavaScript delivered to those pages were
available. Bundle inspection proves which requests the browser is prepared to
make, not what Chessiro's server does internally. Its published OpenAPI file
does not document the private connection or import routes
([Chessiro OpenAPI](https://chessiro.com/openapi.json)).

Chessiro's privacy policy says it collects game data when a Chess.com or
Lichess account is connected, fetches that data for analysis, does not sell
personal data, and lets a user disconnect either provider in profile settings.
It does not settle the server-side token storage or whether disconnect deletes
already imported games. [Chessiro privacy policy](https://chessiro.com/privacy)
The terms say that Chessiro integrates with both providers for analysis and
that game data remains the user's property.
[Chessiro terms](https://chessiro.com/terms)

## What Chessiro exposes publicly

### Product journey

Chessiro's own getting-started article describes this sequence:

1. Sign in to Chessiro with Google or email.
2. Connect a supported chess account or upload PGN.
3. Chessiro automatically imports recent games.
4. Select a game to analyze it.

That is verified product behavior as documented by Chessiro. The article does
not identify the provider endpoints, prove ownership of a supplied handle, or
describe refresh cadence. [Chessiro free game reviews](https://chessiro.com/blog/free-game-reviews)

The live anonymous onboarding first asks whether the visitor has played online,
then offers Lichess and Chess.com connections. Choosing Chess.com opens a form
with a single "Chess.com Username" field and no Chess.com login UI. Choosing
Lichess displays a redirect-to-Lichess state. Chessiro's sign-in page offers
Google, Lichess, and email; it does not offer Chess.com as a Chessiro identity
provider. [Chessiro home](https://chessiro.com/home),
[Chessiro sign-in](https://chessiro.com/auth/signin)

Chessiro also supports account-free PGN analysis. Its public PGN page says a
user only needs to sign in to save the imported game. This is separate from
provider linkage. [Chessiro PGN analyzer](https://chessiro.com/game/pgn)

Chessiro's FAQ says provider connections import games directly. Signed-in
accounts keep games, build a skill profile, and generate training from them.
It also identifies connected Lichess as the account used for online human
play. [Chessiro FAQ](https://chessiro.com/faq)

### Lichess

The current auth bundle calls Lichess sign-in with the scopes
`preference:read email:read`. The resulting Chessiro session exposes a
`lichessUsername`. This verifies an OAuth-backed identity association, with
the requested scopes visible in the client
([auth chunk](https://chessiro.com/_next/static/chunks/67b5aa658a9a7d9a.js),
[platform helper](https://chessiro.com/_next/static/chunks/78d76f2f1669310e.js)).

For game history, the dashboard constructs this public Lichess request:

```text
GET https://lichess.org/api/games/user/{username}
    ?max={pageSize}&pgnInJson=true[&until={cursor}]
Accept: application/x-ndjson
```

The client code does not attach an `Authorization` header. If that request
fails, it asks Chessiro for saved games through
`/api/user-games?platform=lichess&limit=100`. Thus OAuth binds the Lichess
identity to Chessiro, while the game feed itself uses public export data
([dashboard chunk](https://chessiro.com/_next/static/chunks/3a1dd5a4e7c45ccf.js)).

Chessiro separately states that a signed-in user can connect Lichess, request
rated or casual matchmaking, and play the Lichess game on Chessiro's board. It
currently limits this surface to rapid and slower time controls.
[Chessiro Play announcement](https://chessiro.com/blog/introducing-play)

Lichess documents two relevant mechanisms:

- The user-games endpoint downloads any user's games in reverse chronological
  order as PGN or NDJSON. Anonymous requests are supported, with lower
  throughput than authenticated requests. This is sufficient for public game
  discovery and import. [Lichess user-game export operation](https://github.com/lichess-org/api/blob/master/doc/specs/tags/games/api-games-user-username.yaml)
- Third-party play uses the Board API. Lichess documents authorization-code
  OAuth with PKCE, a `board:play` scope, and Board API support for normal
  accounts. It prohibits engine assistance while playing. [Lichess API specification](https://github.com/lichess-org/api/blob/master/doc/specs/lichess-api.yaml)

Therefore:

- **Verified:** Chessiro uses Lichess OAuth for identity association, requests
  `preference:read email:read`, imports history anonymously from the public
  user-games endpoint, and can present Lichess-backed play on its own board.
- **Verified:** the OAuth scopes visible in the account-link flow do not include
  `board:play`. Those scopes cannot authorize Board API moves.
- **Unknown:** whether Chessiro has a second Lichess authorization step for
  play, what token the server retains, and how token revocation or disconnect
  affects saved imports. The public bundle and published OpenAPI examined do
  not settle the actual Play transport.

### Chess.com

Chess.com's official PubAPI is read-only and republishes data visible without
login. It cannot make moves or issue account commands. The same documentation
defines these public endpoints:

```text
GET https://api.chess.com/pub/player/{username}
GET https://api.chess.com/pub/player/{username}/games/archives
GET https://api.chess.com/pub/player/{username}/games/{YYYY}/{MM}
```

The monthly response includes the game URL, final PGN, end time, rules, time
class, and both players' usernames and ratings. Requests should be serialized;
parallel access may receive HTTP 429. [Chess.com Published-Data API](https://www.chess.com/news/view/published-data-api)

Chess.com separately says that developers who want to authenticate Chess.com
members or build a connected board should contact Chess.com for instructions.
There is no public self-service authentication flow in the PubAPI docs.
[Chess.com PubAPI help](https://support.chess.com/en/articles/9650547-what-is-the-pubapi-and-how-do-i-use-it)

Therefore:

- **Verified:** Chessiro says it imports Chess.com games and automatically
  imports recent games after a supported account is connected.
- **Verified:** onboarding searches
  `/api/chesscom/pub/player/{username}` and its `/stats` child, then sends
  `PATCH /api/user/platform` with the Chess.com username. The dashboard fetches
  monthly games from
  `/api/chesscom/pub/player/{username}/games/{YYYY}/{MM}` and falls back to
  `/api/user-games?platform=chesscom`
  ([onboarding chunk](https://chessiro.com/_next/static/chunks/c3e0b8ab0fa4b965.js),
  [connection helper](https://chessiro.com/_next/static/chunks/c4e6af38c6774816.js),
  [dashboard chunk](https://chessiro.com/_next/static/chunks/3a1dd5a4e7c45ccf.js)).
- **Inference:** the same-origin `/api/chesscom/pub/*` routes are Chessiro
  proxies for the official PubAPI. Their paths and response use match the
  official profile, stats, and monthly-game contracts.
- **Unknown:** whether Chessiro performs server-side ownership verification
  beyond the visible browser flow or has a private agreement with Chess.com.

A UI that accepts a public handle but does not prove ownership should not imply
that the user authenticated with Chess.com. Anyone can read the same PubAPI
data.

## What ChenChess does today

### Identity and current web import

ChenChess authenticates its own Player through Firebase email/password or
Google. That provider linking joins Firebase credentials for the same
ChenChess identity; it does not link Lichess or Chess.com accounts
([Firebase auth provider](../../apps/central-host/src/auth/FirebaseAuthProvider.tsx#L112-L167)).

The current review parser accepts only one Chess.com game URL, one Lichess game
URL, or pasted PGN. It does not accept a profile URL and it requires the review
side to be explicit when the source URL does not encode it
([review request parser](../../apps/central-host/src/review-session/reviewRequest.ts#L9-L128)).
That free-text parser was retired on 2026-08-30 with the `/app/` composer it
served; the Coaching Board lobby takes the same sources as typed fields, and
only `extractCompletedPgn` survives at that path.

Single-game Lichess import uses the anonymous official export URL with clocks,
provider evaluations, accuracy, and generated annotations disabled. It asks
Lichess to include the opening
([Lichess import request](../../services/coach-engine/src/lichess.rs#L95-L102)).

Single-game Chess.com import currently turns a supported public computer,
live, or Daily Game URL into its corresponding Chess.com callback URL
([Chess.com import request](../../services/coach-engine/src/chess_com.rs#L79-L87)).
Those callback endpoints are not part of the official PubAPI documentation.
This is why the accepted ChenChess decision blocks distribution of automated
Chess.com educational integration until written authorization exists
([profile-feed ADR](../adr/0031-resolve-profile-games-as-independent-game-imports.md#L52-L54)).

### Public profile feed

The Coach Engine already implements the account-data reader that Chessiro's
product journey suggests, but it deliberately models a public profile rather
than a linked credential:

- It parses only exact `https://lichess.org/@/{username}` and
  `https://www.chess.com/member/{username}` profile URLs
  ([profile parsing](../../services/coach-engine/src/profile_game_feed.rs#L59-L113)).
- It accepts a requested count from one through ten and returns ordinary,
  independent one-game review requests
  ([feed contract](../../services/coach-engine/src/profile_game_feed.rs#L45-L57),
  [request projection](../../services/coach-engine/src/profile_game_feed.rs#L115-L135)).
- For Lichess it requests a bounded, newest-first NDJSON stream of finished
  games and omits moves and tags during discovery
  ([Lichess discovery request](../../services/coach-engine/src/profile_game_feed.rs#L158-L169)).
- For Chess.com it reads the official archive list, then fixed-origin monthly
  archives, at most twelve months, serially. It excludes Daily and non-standard
  games
  ([Chess.com discovery requests](../../services/coach-engine/src/profile_game_feed.rs#L171-L190),
  [archive traversal](../../services/coach-engine/src/profile_game_feed.rs#L411-L478)).
- It matches the supplied profile handle against the white or black player to
  infer review side, then lets the existing one-game import boundary handle
  each result independently
  ([Lichess projection](../../services/coach-engine/src/profile_game_feed.rs#L371-L408),
  [accepted design](../adr/0031-resolve-profile-games-as-independent-game-imports.md#L24-L50)).

Repository search found no caller of `ProfileGameFeed` outside
`profile_game_feed.rs`. The shipped web parser likewise has no profile source.
The capability is implemented and tested as an engine module, but it is not a
Player-facing connection or automatic synchronization journey.

## Comparison

| Question | Chessiro | ChenChess now |
| --- | --- | --- |
| Product identity | Google or email, per Chessiro's public instructions | Firebase email/password or Google |
| Chess.com "connection" | Typed username saved to the Chessiro user; public profile, stats, and monthly-game proxy calls | No linked account; dormant public-profile reader uses official PubAPI |
| Lichess "connection" | Lichess OAuth binds the username; public user-game export supplies history; Play authority remains unresolved | No delegated token; public profile discovery and anonymous single-game export |
| Ownership proof | Lichess OAuth proves control; none is visible for Chess.com | None for public profile URLs; the caller selects a public profile |
| Automatic import | Chessiro says recent games import automatically | No web workflow or scheduled sync; feed returns a bounded snapshot when called |
| Import cardinality | Recent list followed by per-game selection | Current web command owns one game; profile feed returns independent one-game requests |
| Provider credentials | Lichess OAuth credential during sign-in, retention unknown; no Chess.com credential visible | None for chess providers |
| Chess.com source | Same-origin proxy shaped like official PubAPI | Official PubAPI for discovery, undocumented callbacks for current single-game fetch |

## Recommendation for ChenChess

Do not add one generic `ConnectedChessAccount` model. Add the smallest model
for the product promise.

1. **Ship public profile following first.** Let a Player attach an exact
   Lichess or Chess.com profile URL to their ChenChess account. Name it
   "public game profile" in the UI. State that the profile's public games are
   readable without provider authorization.
2. **Prove ownership only if the feature needs it.** Public coaching imports do
   not. If ChenChess later displays an ownership badge or writes to a provider,
   add a provider-specific verification flow rather than treating a typed
   username as proof.
3. **Use Lichess OAuth only for delegated work.** Playing through ChenChess,
   reading private data, or proving account control justifies OAuth. Public
   game discovery does not. Request the narrowest scopes and keep the token
   lifecycle separate from imported-game retention.
4. **Keep discovery separate from import.** Preserve the existing profile-feed
   design: discovery emits stable one-game requests, and each request passes
   independently through canonical validation, provenance, persistence, and
   review. Add scheduling, last-seen state, and partial-failure handling in a
   durable worker, not in `GameImporter`.
5. **Close the Chess.com source gap before launch.** Monthly PubAPI records
   already contain final PGN. Either carry that documented PGN into the normal
   import boundary with PubAPI provenance, or obtain written authorization for
   the callback endpoints. Do not market a Chess.com connection while relying
   silently on an undocumented web callback.
6. **Make revocation promises precise.** Removing a public profile stops future
   discovery but need not erase Player-owned imports. Revoking Lichess OAuth
   stops delegated calls. Deleting imported games is a third action. The UI and
   data model should not collapse those effects into one "Disconnect" button.

This path reaches the useful part of Chessiro's experience, recent games ready
for review, without collecting provider credentials that ChenChess does not
need.

## First-party surfaces inspected

- [Chessiro home and onboarding](https://chessiro.com/home)
- [Chessiro sign-in](https://chessiro.com/auth/signin)
- [Chessiro FAQ](https://chessiro.com/faq)
- [Chessiro privacy policy](https://chessiro.com/privacy)
- [Chessiro terms](https://chessiro.com/terms)
- [Chessiro OpenAPI](https://chessiro.com/openapi.json)
- [Chessiro external-provider status](https://chessiro.com/api/status/external)
- [Lichess official API specification](https://github.com/lichess-org/api/blob/master/doc/specs/lichess-api.yaml)
- [Chess.com official Published-Data API](https://www.chess.com/news/view/published-data-api)
