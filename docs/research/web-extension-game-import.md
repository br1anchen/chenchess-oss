# Browser extension trigger and remote game import research

Research date: 2026-07-26

This note evaluates a browser extension that adds a **Review in ChenChess**
button after a completed game on Lichess, Chess.com, Take Take Take, or
Duolingo. It distinguishes documented platform behavior from proposed
ChenChess design.

## Finding

Build the extension as a **trigger and locator capture**, not as a PGN
crawler. The extension may determine that the current page looks like a
completed game and capture its canonical URL after a Player click. The hosted
ChenChess Game Import module must fetch or accept the PGN, enforce provider
limits, verify that the game is completed and supported, and build the existing
`ImportedGame`.

This split gives all callers—web, browser extension, Coach App, ChatGPT, and
Claude—one trusted import seam. It also avoids putting platform credentials,
rate-limit policy, PGN parsing, or fair-play enforcement into an untrusted
content script.

The recommended rollout is:

1. Ship a Chromium Manifest V3 extension for Lichess first.
2. Add Chess.com import through its documented PubAPI and user-exported PGN,
   but do not ship Chess.com DOM injection or page extraction without written
   authorization.
3. Treat Take Take Take as a partner or upstream-Lichess integration, not a
   scraping target.
4. Do not extract Duolingo games without a documented export or partnership.
5. Keep ChatGPT or Claude browser control as a user-guided fallback, never the
   primary ingestion mechanism.

## Platform decision matrix

| Platform       | Documented game source                                                                               | Automatic in-page button                                                                             | Immediate post-game review                                               | Recommendation                                                           |
| -------------- | ---------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ | ------------------------------------------------------------------------ |
| Lichess        | Anonymous single-game export API                                                                     | Technically feasible; no PGN DOM crawl is needed                                                     | Usually, subject to API availability                                     | **Build first**                                                          |
| Chess.com      | Public monthly archives contain final PGN and exact game URL; the site also offers user PGN export   | Technically feasible, but current terms restrict unauthorized modifications and automated extraction | PubAPI freshness is not guaranteed; manual PGN is the immediate fallback | **Build backend import; obtain written approval before DOM integration** |
| Take Take Take | Its public site says the playzone is powered by Lichess; no public game-export API was found         | Technically possible but has no documented DOM or export contract                                    | Unconfirmed                                                              | **Use linked Lichess history or a partner API; do not crawl**            |
| Duolingo       | Public product material confirms web mini- and full-length games; no public PGN/export API was found | Technically possible, but the terms prohibit scraping/data extraction                                | No supported source                                                      | **Partner-only; do not ship an extractor**                               |

The ability to modify a page with a content script does not establish
permission to extract its data. Browser automation by ChatGPT or Claude does
not change that conclusion.

## Proposed end-to-end design

The browser page is a **trigger surface**. It is not necessarily the
authoritative game source. For example, a click on Take Take Take may lead the
Player to select the latest game from a linked Lichess account. The resulting
provenance is Lichess; “triggered from Take Take Take” is separate interaction
context.

The flow is:

```text
completed-game page
  -> isolated content script sees a terminal-state hint
  -> content script appends one extension-owned button
  -> Player clicks Review in ChenChess
  -> extension worker validates and canonicalizes sender.tab.url
  -> worker stores a short-lived, one-use capture under an opaque nonce
  -> worker opens the exact ChenChess /import/from-extension page
  -> authenticated ChenChess page consumes the capture from the extension
  -> web app sends RemoteGameUrl to the central Game Import module
  -> provider adapter fetches documented PGN or returns a typed fallback
  -> central validation builds ImportedGame and starts Game Review
```

### Why the web app should consume the extension capture

The extension does not need a ChenChess access token or permission to call the
ChenChess backend:

1. Store `{nonce, canonicalPageUrl, expiresAt}` in `chrome.storage.session`.
   Session storage is memory-backed and is not exposed to content scripts by
   default.
2. Open
   `https://<chenchess-host>/import/from-extension?intent=<opaque-nonce>`.
   The URL contains only a random nonce, not the game URL or PGN.
3. Declare the exact ChenChess origin in `externally_connectable`.
4. The authenticated web page calls `chrome.runtime.sendMessage` with the
   production extension ID and nonce.
5. The extension verifies the external sender origin and nonce, returns the
   capture once, and deletes it.
6. The web app uses its normal ChenChess session to import and start the
   review.

Chrome documents external messages from allowlisted web pages and recommends
validating the sender. This handoff avoids game URLs in query strings, avoids
extension OAuth in the first release, and gives login/resume behavior to the
existing web surface. [Chrome external messaging](https://developer.chrome.com/docs/extensions/develop/concepts/messaging#external-webpage),
[externally connectable manifest](https://developer.chrome.com/docs/extensions/reference/manifest/externally-connectable),
[session storage](https://developer.chrome.com/docs/extensions/reference/api/storage#property-session)

If ChenChess later needs a review fully inside an extension side panel, add a
dedicated OAuth authorization-code/PKCE client then. Do not rely on
cross-origin web cookies from the extension worker.

## Manifest V3 extension design

### Two permission modes

Offer two explicit modes:

- **Toolbar mode:** the Player clicks the extension action. `activeTab` grants
  temporary access to the current tab, sufficient to capture a supported URL
  and start the handoff. It cannot make an automatic button appear later
  without another Player gesture.
- **Automatic button mode:** during onboarding or extension settings, the
  Player enables an individual supported site. Request that origin through
  `optional_host_permissions`, then use `chrome.scripting` to register the
  site-specific content script. Remove the registration when permission is
  revoked.

`activeTab` is temporary and follows a Player invocation. Persistent automatic
injection requires site access. Optional permissions let a Player enable only
the sites they want rather than granting all supported origins at install
time. [Chrome activeTab](https://developer.chrome.com/docs/extensions/develop/concepts/activeTab),
[Permissions API](https://developer.chrome.com/docs/extensions/reference/api/permissions),
[Scripting API](https://developer.chrome.com/docs/extensions/reference/api/scripting)

For the proposed handoff, the first manifest needs only:

- `activeTab` for toolbar capture;
- `scripting` for an opted-in automatic content script;
- `storage` for the short-lived capture;
- `optional_host_permissions` for origins actually supported in that release;
  and
- an exact ChenChess origin under `externally_connectable`.

It does not need `tabs`, `cookies`, `webRequest`, `debugger`, `<all_urls>`,
platform API hosts, ChenChess backend host permission, or
`web_accessible_resources`.

### Content script behavior

Keep the script in Chrome's default isolated world. It should:

- recognize only allowlisted HTTPS origins and documented/known game URL
  shapes;
- use `MutationObserver` to handle single-page-application transitions and
  post-game UI appearing after the initial load;
- observe only the presence of a terminal-result hint, not board squares,
  clocks, move lists, application state, cookies, or network requests;
- coalesce observer callbacks and mount at most one button;
- append a fixed-position extension root to `document.body`, preferably with a
  Shadow DOM root so page styles and rerenders do not corrupt it;
- build UI with DOM methods and `textContent`, never `innerHTML`;
- remain silent—no import, engine request, or background polling—until the
  Player clicks; and
- treat a platform DOM result as a hint only. The server is the authority on
  completion.

Platform DOM selectors are private implementation details and will break. Keep
each provider's selector and route recognition in a small bundled
`PageTriggerAdapter`; fixture-test it and fail closed. A selector failure should
hide the automatic button while toolbar mode remains available. Remote
configuration may disable a bundled adapter, but must not deliver executable
selector or extraction code because Manifest V3 requires reviewable packaged
code. [Chrome content scripts](https://developer.chrome.com/docs/extensions/develop/concepts/content-scripts),
[Manifest V3 and remote-hosted code](https://developer.chrome.com/docs/extensions/develop/migrate/remote-hosted-code)

### Message trust

The content script and page DOM are untrusted:

- Ignore a page-supplied provider name.
- Prefer `sender.tab.url` over a URL included in the content-script message.
- Strictly parse the origin and route in the extension worker, and repeat the
  parsing on the ChenChess server.
- Accept only a small versioned `captureCurrentGame` message. Reject unknown
  fields and bound every string.
- Bind a capture nonce to the exact ChenChess destination, expire it after a
  few minutes, and consume it once.
- Validate the external web-page sender against the exact production ChenChess
  origin.

Chrome explicitly warns that content scripts are less trustworthy than the
extension worker and that privileged messages must be validated. [Chrome
extension security guidance](https://developer.chrome.com/docs/extensions/develop/security-privacy/stay-secure),
[message passing](https://developer.chrome.com/docs/extensions/develop/concepts/messaging)

## Central Game Import module

The current command contract exposes `LichessUrl`, `PastedPgn`, and
`LocalPgnFile` in
[`operations.rs`](../../services/coach-engine/src/review_session_contract/operations.rs).
The implementation dispatches the Lichess variant directly in
[`game_import.rs`](../../services/coach-engine/src/game_import.rs), while provenance,
progress, provider errors, and review-side resolution also contain
Lichess-specific names.

Do not add one top-level command variant for every website. Deepen the existing
Game Import module behind a source-neutral interface:

```rust
enum GameInputSource {
    RemoteGameUrl { url: String },
    PastedPgn { pgn: String },
    LocalPgnFile { path: String },
}

enum RequestedReviewSide {
    Selected { review_side: ReviewSide },
    FromSourceIdentity,
    Required,
}
```

`FromSourceIdentity` can use a side-qualified Lichess URL or match the players
in a fetched PGN to an authenticated Player's linked platform handle. The
client must not supply an authoritative username; linked-account identity
comes from ChenChess-owned Player data.

Inside the module, two real remote sources justify an adapter seam:

```rust
trait RemoteGameAdapter {
    async fn fetch(
        &self,
        request: RemoteGameRequest,
    ) -> Result<CompletedPgnEnvelope, RemoteGameError>;
}
```

The module, not callers, owns:

- origin recognition and canonical URL parsing;
- fixed provider endpoint construction and SSRF protection;
- linked-account lookup needed to resolve an official archive;
- provider serialization, caching, cooldowns, and response deadlines;
- response-size limits and PGN parsing;
- exact game-URL/player/result cross-checks;
- completed standard-game eligibility;
- provenance and digests; and
- typed retry/fallback behavior.

`CompletedPgnEnvelope` should contain the platform, canonical game URL,
canonical provider game ID, PGN bytes, capture time, provider-contract version,
and response metadata needed for provenance. It should not expose raw provider
response shapes to Review Session code.

Generalize externally visible Lichess-specific contract names only where the
new source genuinely varies:

- `ImportProgressStage::WaitingForRemoteSource`;
- `CommandRejectionReason::InvalidRemoteGameUrl`;
- a typed `RemoteSourceNotYetAvailable` result for a documented archive that
  has not published the game yet; and
- `ImportProvenance::RemotePlatform { platform, canonical_url, ... }`.

Keep the extension trigger origin separate from game provenance. A Take Take
Take click that ultimately imports a Lichess game must not claim that the PGN
came from Take Take Take.

This is a deep module: deleting it would force URL validation, provider policy,
completion checks, and provenance back into every caller. Its interface is
also the natural test surface.

## Platform capture paths

### Lichess

Use the existing anonymous single-game
`GET /game/export/{gameId}` gateway. The official schema supports PGN/JSON,
requires an eight-character game ID, exposes game status, and allows
cross-origin access, although ChenChess should continue fetching server-side
for shared serialization, caching, and validation. The existing
[operating-limits research](review-session-operating-limits.md) already records
the exact request shape and local policy.

Lichess asks integrations to use its API rather than scrape or automate its
browser interface. It also prohibits external assistance during an ongoing
game. Therefore the content script may notice a result and capture the URL,
but the backend must still reject `created` and `started`, and the extension
must never call the review engine during play. [Lichess single-game export
specification](https://github.com/lichess-org/api/blob/master/doc/specs/tags/games/game-export-gameId.yaml),
[Lichess API tips](https://lichess.org/page/api-tips),
[Lichess fair play](https://lichess.org/page/fair-play)

**Release decision:** automatic button and toolbar capture are reasonable for
v1. Fetch PGN through the existing server adapter; do not read it from the DOM
or `window.lichess`.

### Chess.com

Chess.com's PubAPI is read-only and public. Its completed monthly archive
endpoint is:

```text
GET https://api.chess.com/pub/player/{username}/games/{YYYY}/{MM}
```

Each finished game includes the final `pgn`, exact game `url`, `end_time`, FEN,
players, result, and rules. There is also a monthly multi-game PGN endpoint.
No documented endpoint fetches one game by its numeric page ID. Requests should
be serial, use cache validators, handle `429`, and identify the client.
[Chess.com Published-Data API](https://www.chess.com/news/view/published-data-api)

For a just-finished game, a ChenChess adapter can:

1. load the authenticated Player's linked Chess.com username;
2. request the current UTC monthly archive, and the previous month only near a
   month boundary;
3. match the canonical current-page URL exactly;
4. require a final result and supported rules; and
5. import that entry's PGN.

The PubAPI documentation contains both 12-hour and 24-hour maximum-refresh
statements and promises no immediate publication time. Treat a miss as “not
yet available,” not “invalid game,” and offer the official user-export path.
Chess.com documents copying or downloading PGN from the game/share UI.
[Chess.com PGN export help](https://support.chess.com/en/articles/8705305-how-do-i-get-a-pgn-of-my-game)

There is a material release constraint. The current User Agreement prohibits
unauthorized third-party software designed to modify the service, automated
retrieval/data mining, and automated or AI use for educational tools or game
databases without prior authorization. The Fair Play Policy also prohibits
tools and browser extensions that analyze positions during play.
[Chess.com User Agreement](https://www.chess.com/legal/user-agreement),
[Chess.com Fair Play Policy](https://www.chess.com/legal/fair-play)

**Release decision:** implement official PubAPI and pasted-PGN support in
ChenChess, but seek written Chess.com authorization before distributing a
Chess.com content script or automated educational integration. Never scrape
the move list, intercept private XHR, click the site's PGN UI
programmatically, or analyze while the game is ongoing.

### Take Take Take

Take Take Take's public homepage says its playzone is “Powered by Lichess,” and
its sign-in surface supports Lichess and Chess.com accounts. No public
single-game API, PGN export contract, or developer documentation was found.
[Take Take Take homepage](https://taketaketake.com/),
[Take Take Take sign-in](https://auth.taketaketake.com/)

The supported near-term experience is therefore:

- a Take Take Take trigger opens ChenChess;
- ChenChess shows recent games from the Player's linked Lichess account using
  the documented
  [Lichess user-games endpoint](https://github.com/lichess-org/api/blob/master/doc/specs/tags/games/api-games-user-username.yaml);
  and
- the Player selects the just-completed game.

This is an inference from the public “Powered by Lichess” statement, not proof
that a Take Take Take page URL contains a Lichess game ID or that every game is
immediately exported upstream. Validate it with a manual product spike and ask
Take Take Take for a partner game locator/export contract.

**Release decision:** no automatic page PGN extraction. A public DOM button
should wait for partner permission; a generic toolbar handoff may offer linked
Lichess recent-game selection without reading Take Take Take game data.

### Duolingo

Duolingo's current Chess announcement confirms that the web course includes
mini- and full-length matches, games against Oscar, and games with friends or
matched players. It does not document a PGN export or public game API.
[Duolingo Chess announcement](https://blog.duolingo.com/chess-course/)

A manual **Review in ChenChess** click is materially different from an
unattended crawler: it is one Player-directed export of one completed game.
The earlier conclusion should therefore not be read as “a browser extension
necessarily violates the terms.” The contract language nevertheless does not
limit its restrictions by volume, ownership claim, or whether a human starts
the extraction. It prohibits scraping and similar extraction of Service
Content and says Duolingo owns materials generated through educational
activities except where the terms authorize use. Whether a narrowly
user-initiated game export falls within those clauses is a contract/legal
question, not a technical finding. This note does not provide legal advice.
[Duolingo Terms](https://www.duolingo.com/terms)

There is also first-party evidence that the intended learning use is
reasonable. Duolingo tells chess learners to record their games and explains
that a saved game can be run through an engine to find mistakes; another
Duolingo article explicitly recommends engine analysis for intermediate
players. Neither article documents a Duolingo export interface or overrides
the Terms, but they support the Player benefit rather than an anti-competitive
or bulk-data purpose.
[Duolingo chess notation](https://blog.duolingo.com/chess-notation/),
[Duolingo chess improvement guide](https://blog.duolingo.com/how-to-get-better-at-chess/)

#### Observed web implementation

The public web bundles, inspected on the research date, establish a viable
private integration:

- the web client defines authenticated same-origin routes for creating,
  updating, listing, and fetching matches under
  `/chess/1/{userId}/matches` and
  `/chess/1/{userId}/matches/{matchId}`;
- a completed player-versus-player match is represented with `moveHistory`,
  `boardFen`, `endCondition`, and `outcome`, and the client refetches the exact
  match through `GET /chess/1/{userId}/matches/{matchId}`;
- bot-match updates also send `moveHistory` and use the same generic match
  path family; and
- the visible player-versus-player UI exposes accessible **Previous move** and
  **Next move** controls, but not a textual move list. The board is rendered
  separately, so DOM or ARIA scraping alone is not a sound export path.

These are reverse-engineered implementation facts, not a documented API
contract. The hashed assets can change at any deployment.
[Duolingo web app bundle](https://d35aaqx5ub95lt.cloudfront.net/js/app-b99c4894.js),
[Duolingo web PvP bundle](https://d35aaqx5ub95lt.cloudfront.net/js/7103-939111de.js),
[Duolingo web bot-match bundle](https://d35aaqx5ub95lt.cloudfront.net/js/1913-03a8eb85.js)

#### Narrow user-initiated capture design

A proof of concept can avoid page-state spelunking and network interception:

1. Request optional access only to `https://www.duolingo.com/*`. Do nothing
   during play.
2. After a terminal-result hint appears, add **Review in ChenChess**. Do not
   fetch until the Player clicks it.
3. On click, inspect same-origin `PerformanceResourceTiming` entries for the
   most recent path matching the strict Duolingo match shape. Normal bot and
   PvP play already issue requests whose URL contains both `userId` and
   `matchId`; no response body, cookie, bearer token, Redux store, or WebSocket
   needs to be observed. Fail closed if the exact locator is absent from the
   timing buffer.
4. From the content script, make one same-origin credentialed `GET` for that
   exact match. Content-script requests execute on behalf of the page origin
   and remain subject to its same-origin policy.
5. Accept only a completed response with bounded `moveHistory`, final
   `boardFen`, `endCondition`, and `outcome`. Send that small capture to the
   central Game Import module, replay the moves, generate PGN, and reject it
   unless the replayed final position and result agree.
6. Mark provenance as a versioned `DuolingoPrivateWebContract`, discard the
   raw response after import, and never retry or enumerate other matches in
   the background.

[Resource Timing Level 2](https://www.w3.org/TR/resource-timing-2/),
[Chrome cross-origin request model](https://developer.chrome.com/docs/extensions/develop/concepts/network-requests)

Validate two unknowns with a Player-owned test account before implementation:
whether the direct `GET` also returns a completed Oscar match, and whether
`moveHistory` is UCI, SAN, or another stable move encoding. If either check
fails, stop at a manual move/PGN fallback; do not escalate to intercepting
Duolingo's WebSocket, patching `fetch`, or reaching into private Redux/Webpack
state.

Duolingo's Privacy Policy separately offers access to a copy of held personal
information through its Data Vault and an export right for personal
information supplied to Duolingo. That is an official fallback for a data
request, but the policy does not promise that a response contains chess move
history or is immediate enough for post-game review.
[Duolingo Privacy Policy](https://www.duolingo.com/privacy),
[Duolingo Data Vault](https://drive-thru.duolingo.com/)

**Release decision:** build the narrow flow as a private, disabled-by-default
technical spike. A public extension should enable it only after product/legal
review or written Duolingo permission resolves use of the undocumented match
endpoint. The manual click, completed-game gate, single exact fetch, and
ephemeral handling substantially reduce fair-play and privacy risk; they do
not by themselves create a documented platform authorization.

## Fair-play and privacy invariants

The extension must be deliberately useless during an ongoing game:

- no board or move-list extraction;
- no Stockfish, Maia, opening, tablebase, or coaching request;
- no background PGN/API polling;
- no button until a bundled terminal-result heuristic fires;
- one Player click before any handoff; and
- a server-authoritative completed-game check before analysis.

This is required even on Lichess, whose ongoing export is delayed rather than
fully unavailable. A delay is not permission to analyze.

Chrome treats page content, website URLs, browsing activity, cookies, and
request/response data as user data, even when processing is local. The Web
Store listing and privacy policy must prominently describe the single purpose:
“When you click Review in ChenChess on a supported completed chess game, the
extension sends that game's public locator to ChenChess.” Request the narrowest
permissions, transmit only over HTTPS, never place PGN/player names/game URLs
in telemetry, and delete the pending capture after consumption or expiry.
[Chrome user-data FAQ](https://developer.chrome.com/docs/webstore/program-policies/user-data-faq),
[Chrome user-data policy](https://developer.chrome.com/docs/webstore/program-policies/user-data),
[Chrome privacy guidance](https://developer.chrome.com/docs/extensions/develop/security-privacy/user-privacy)

## Why ChatGPT or Claude browser control is not the import transport

A ChatGPT or Claude browser agent can help a Player copy a canonical URL or
user-exported PGN and then call the ChenChess Coach tool. It should not be the
system that discovers hidden application state or drives provider UI:

- selectors and UI sequences are less deterministic than a small extension;
- the agent may have access to unrelated signed-in page data;
- page content can attempt prompt injection;
- every invocation costs latency and model capacity;
- it cannot provide a durable post-game button; and
- it does not bypass platform terms or fair-play rules.

OpenAI and Anthropic both document risks and user-control requirements for
browser operation. [OpenAI computer-use safety guidance](https://learn.chatgpt.com/docs/computer-use#safety-guidance),
[Claude in Chrome](https://support.claude.com/en/articles/12012173-get-started-with-claude-in-chrome)

Use ChatGPT/Claude as another caller of the central `RemoteGameUrl` or
`PastedPgn` interface after the Player supplies permitted input. Do not create
a separate “agent crawler” import path.

## Implementation slices

### Slice 1: deepen Game Import

- Introduce `RemoteGameUrl` and source-neutral provenance/progress/rejections.
- Move current Lichess behavior behind the first `RemoteGameAdapter`.
- Reuse the existing PGN parse, eligibility, review-side, digest, and snapshot
  construction path.
- Add fixed-origin adapter contract tests, ongoing-game rejection, malformed
  response, response-size, redirect, cooldown, and provenance tests.

### Slice 2: credential-free extension handoff

- Add the authenticated `/import/from-extension` web route.
- Implement exact-origin external messaging, opaque nonce, session storage,
  TTL, and single consumption.
- Start with toolbar `activeTab` capture against strict Lichess URL fixtures.
- Test forged internal/external messages, wrong origins, expired/replayed
  nonces, and URLs containing credentials, non-HTTPS schemes, ports, or
  unsupported paths.

### Slice 3: opted-in Lichess button

- Request only `https://lichess.org/*` at a Player gesture.
- Register an isolated, bundled content script.
- Implement an idempotent MutationObserver and Shadow DOM button against saved
  public HTML fixtures.
- Run a manual live smoke test after completed games. Do not automate live
  gameplay or rely on a production site crawler in CI.

### Slice 4: Chess.com official-source import

- Add linked Chess.com handle storage.
- Add a serialized/cached PubAPI monthly-archive adapter with exact URL match.
- Model “not yet available” and offer pasted PGN with official export
  instructions.
- Do not enable the Chess.com content script until written authorization and a
  legal/product review resolve the current terms.

### Slice 5: partnerships

- Ask Take Take Take for a canonical Lichess locator or completed-game PGN
  endpoint and permission to add the button.
- Ask Duolingo for a user-directed completed-game export and integration
  permission.
- Add an adapter only after a documented contract exists; do not preserve a
  crawler as a fallback.

## Verification strategy

- **Extension unit/fixture tests:** route recognition, terminal hint, one
  mount, remount after SPA rerender, click message, no move extraction.
- **Extension security tests:** sender validation, malformed messages, exact
  origins, nonce TTL/replay, session-storage access level.
- **Backend adapter tests:** canonicalization, fixed endpoint construction,
  rate-limit/caching policy, exact game identity, completed status, supported
  variant, bounded PGN, provenance.
- **End-to-end test:** fixture platform page -> extension handoff -> signed-in
  web route -> fake provider adapter -> existing Game Review. Do not make live
  platform automation a required CI dependency.
- **Canary/manual checks:** one supported public game per bundled page adapter
  before publishing an extension update.
- **Policy release check:** permission diff, Web Store disclosure/privacy
  policy, platform authorization, and fair-play no-network-before-completion
  evidence.

## Explicit uncertainty

- Provider DOM selectors and SPA routes are not public stability contracts.
- Chess.com's current PubAPI publication latency for a just-finished game is
  not guaranteed, and its documentation contains inconsistent cache-refresh
  statements.
- Current Chess.com terms create a material authorization question even when
  the documented PubAPI, rather than DOM scraping, supplies the PGN.
- Take Take Take's public “Powered by Lichess” statement does not document how
  to map a Take Take Take page to an upstream game.
- No public Take Take Take or Duolingo completed-game API/export contract was
  found.
- Duolingo's ownership language may restrict even Player-reconstructed game
  data; partnership/legal review is required.
- This design targets Chromium Manifest V3. Firefox/Safari packaging and
  website-to-extension handoff behavior need a separate compatibility spike.
- Chrome Web Store review outcome and each platform's future terms cannot be
  guaranteed by technical design.
