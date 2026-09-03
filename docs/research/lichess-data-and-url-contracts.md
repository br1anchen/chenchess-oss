# Lichess data and URL contracts

Status note: written 2026-07-13 during the Lichess interactive-review design. Retained as the dated evidence record behind the Lichess import contract.

Checked 2026-07-13 against Lichess's current public documentation, official source, and a small number of live requests. This note uses three evidence labels:

- **Documented** means the public API reference or an official policy states the behavior.
- **Observed** means a live first-party endpoint behaved this way on the check date. It is evidence, not a promise.
- **Inferred** means a product or engineering consequence derived from the first two.

## Bottom line

Anonymous URL import is viable. Anonymous Opening Explorer use is not.

The supported v1 foundation is an eight-character Lichess game ID, an optional `/white` or `/black` orientation, and the anonymous single-game export endpoint. One JSON export request can return the required eligibility fields and the full PGN. Opening identification is available from that export.

The map's no-OAuth boundary conflicts with current Opening Explorer access. Lichess changed the Explorer in March 2026: API calls now require an OAuth token, are limited to 25 requests per minute, and stop at 50 plies. Anonymous `/lichess` and `/masters` requests returned `401` during this investigation. [Lichess's announcement](https://lichess.org/@/thibault/blog/the-opening-explorer-now-requires-authentication/FSWh9Zg3), the [current Explorer specification](https://github.com/lichess-org/api/tree/a0f82d031107e270f56d6bebbbdac4265926b90e/doc/specs/tags/openingexplorer), and the [April 2026 changelog](https://lichess.org/page/changelog) agree on the authentication change.

Lichess Practice has public pages and source-backed JSON behavior, but no documented public API for catalog discovery, theme search, or lesson matching. Runtime dependence on those internal routes would be fragile.

## Game URLs and review side

### Public game-page forms

**Source-backed.** Lichess routes public game pages as:

```text
https://lichess.org/{gameId}
https://lichess.org/{gameId}/white
https://lichess.org/{gameId}/black
```

`gameId` is eight characters. The bare route defaults to White. The side-qualified route passes the selected color to the watcher controller, which constructs that color's game point of view. Replayable completed games then render through the analysis replay. [Official routes](https://github.com/lichess-org/lila/blob/4e076cfaceaeebb19437edbc657a7371b3e841ff/conf/routes#L391-L402), [watcher controller](https://github.com/lichess-org/lila/blob/4e076cfaceaeebb19437edbc657a7371b3e841ff/app/controllers/Round.scala#L114-L170)

**Observed.** The bare, `/white`, and `/black` forms for `Synthet1` all returned `200` HTML.

**Important semantic boundary.** `/white` and `/black` select board and replay orientation. They do not prove which participant submitted the URL, whose account is active, who won, or whose turn it is. ChenChess may adopt the suffix as the requested Review Side, but that is a ChenChess convention built on Lichess orientation semantics.

Lichess also routes a 12-character `GameFullId`, composed of the eight-character game ID and a four-character player ID. It resolves a player point of view and is used by player actions. Treat it as a capability-bearing URL, not a share URL. Do not retain or log the last four characters. [ID definition](https://github.com/lichess-org/lila/blob/4e076cfaceaeebb19437edbc657a7371b3e841ff/modules/core/src/main/id.scala#L14-L39), [player route](https://github.com/lichess-org/lila/blob/4e076cfaceaeebb19437edbc657a7371b3e841ff/app/controllers/Round.scala#L73-L78)

### What the v1 parser should accept

**Inferred.** Accept only HTTPS URLs on `lichess.org` whose path is exactly an eight-character root game ID, optionally followed by `white` or `black`. Strip query parameters and fragments after parsing. Reject 12-character full IDs with a message asking for the public share URL.

Study, broadcast, puzzle, and Practice resources have separate namespaces such as `/study/...`, `/broadcast/...`, `/training/...`, and `/practice/...`. They are not single-game URLs and should be rejected before calling game export. The official route table defines these as different resource types. [Study, broadcast, puzzle, and Practice routes](https://github.com/lichess-org/lila/blob/4e076cfaceaeebb19437edbc657a7371b3e841ff/conf/routes)

Do not extract an arbitrary eight-character segment from one of those URLs. Resource IDs in other namespaces are not game IDs.

## Single-game export

### Endpoint and authentication

**Documented.** `GET https://lichess.org/game/export/{gameId}` exports one game. Its OpenAPI operation declares `security: []`, so no account or token is required. The path ID must be exactly eight characters. The documented response formats are PGN and JSON, selected with `Accept: application/x-chess-pgn` or `Accept: application/json`. The success response permits cross-origin access. [Single-game export specification](https://github.com/lichess-org/api/blob/a0f82d031107e270f56d6bebbbdac4265926b90e/doc/specs/tags/games/game-export-gameId.yaml)

The Lichess server also routes `/game/export/{gameId}.pgn`, and it worked in the live check, but the public OpenAPI contract names the extensionless endpoint. Prefer the documented form. [Export routes](https://github.com/lichess-org/lila/blob/4e076cfaceaeebb19437edbc657a7371b3e841ff/conf/routes#L506-L513)

### Recommended request

Use one JSON request and include the PGN in it:

```http
GET /game/export/{gameId}?pgnInJson=true&opening=true&clocks=false&evals=false&accuracy=false&division=false
Accept: application/json
```

This avoids a second request. It also gives ChenChess typed eligibility fields before PGN parsing. The JSON schema requires `id`, `rated`, `variant`, `speed`, `perf`, `createdAt`, `lastMoveAt`, `status`, and `players`; it can include `winner`, `opening`, `moves`, `pgn`, clocks, analysis, and phase division. [Game JSON schema](https://github.com/lichess-org/api/blob/a0f82d031107e270f56d6bebbbdac4265926b90e/doc/specs/schemas/GameJson.yaml)

**Observed.** For `Synthet1`, that request returned `variant: "standard"`, `status: "mate"`, player data, SAN moves, opening `{ "eco": "A00", "name": "Saragossa Opening", "ply": 4 }`, and a complete PGN string.

The query defaults are `true` for moves, tags, clocks, evaluations, opening, and phase division. `pgnInJson`, accuracy, and literate annotations default to `false`. Request only what the import path uses. Lichess analysis may be absent even when `evals=true` because the field is included only when analysis exists.

PGN tag examples include Site, players, result, ratings, Variant, TimeControl, ECO, Opening, and Termination. The PGN response is documented as a string, however, so the example's exact tag set is not a schema guarantee. Use JSON fields for eligibility and PGN for the existing review pipeline. [PGN schema and example](https://github.com/lichess-org/api/blob/a0f82d031107e270f56d6bebbbdac4265926b90e/doc/specs/schemas/GamePgn.yaml)

### Completed standard-game boundary

**Documented.** The export endpoint also serves ongoing games, delayed by three moves to deter cheating. It does not enforce ChenChess's completed-game rule. The response status enum includes `created`, `started`, `aborted`, `mate`, `resign`, `stalemate`, `timeout`, `draw`, `outoftime`, `cheat`, `noStart`, `unknownFinish`, `insufficientMaterialClaim`, and `variantEnd`. [Export behavior](https://github.com/lichess-org/api/blob/a0f82d031107e270f56d6bebbbdac4265926b90e/doc/specs/tags/games/game-export-gameId.yaml), [status enum](https://github.com/lichess-org/api/blob/a0f82d031107e270f56d6bebbbdac4265926b90e/doc/specs/schemas/GameStatusName.yaml)

**Inferred.** V1 must validate at least:

- `variant === "standard"`;
- status is not `created`, `started`, `aborted`, or `noStart`;
- PGN and moves are present, and the PGN result is not `*`.

This rejects ongoing games and non-games even though export succeeds. `variant` rejects Chess960 and every other variant. Keep the accepted terminal-status set explicit in the later import-contract decision, since imported standard games may use `unknownFinish`.

Study, broadcast, and puzzle URLs should not be sent to this endpoint. If they contain an eight-character resource ID, export will usually return a game `404`, but that is accidental and not type-safe.

### Errors

The OpenAPI file specifies only the `200` response, not a structured error schema.

**Observed.** An unknown eight-character ID returned:

- `404 {"error":"Not found"}` when `Accept: application/json` was sent;
- a `404` HTML page when PGN was requested.

Always request JSON so the normal not-found case has the observed small JSON body, but do not couple correctness to its exact text. Unsupported `Accept` values may fall back to PGN instead of returning `406`; send only documented media types.

## Opening Explorer

### Authentication is now mandatory

**Documented.** `/lichess`, `/masters`, and `/player` on `https://explorer.lichess.org` all declare OAuth2. No OAuth scope is requested, but a valid bearer token is still required. Lichess's March 2026 announcement says every API Explorer request must include an OAuth token. It sets a rate of 25 requests per minute and a maximum depth of 50 plies. [Announcement](https://lichess.org/@/thibault/blog/the-opening-explorer-now-requires-authentication/FSWh9Zg3), [current endpoint specs](https://github.com/lichess-org/api/tree/a0f82d031107e270f56d6bebbbdac4265926b90e/doc/specs/tags/openingexplorer)

**Observed.** Anonymous requests to `/lichess` and `/masters` returned nginx `401`. An anonymous `/player` request behaved inconsistently in one check, but that contradicts both the current specification and the announced policy. Do not build on that exception.

**Product consequence.** A no-OAuth v1 cannot depend on live Explorer statistics. The map must choose one of these routes later:

1. Keep no OAuth. Use the anonymous game export's opening name and ECO, and postpone database statistics.
2. Add a server-held Lichess token. This adds secret storage, token rotation, a shared 25-request-per-minute budget, and an external-service failure mode.
3. Add user OAuth. This directly changes the agreed v1 scope and onboarding.

The first route preserves the agreed scope. It still provides first-class Lichess import and opening identification.

### Endpoints and filters

**Documented.** The Explorer accepts a root FEN and a comma-separated `play` sequence of legal UCI moves. `play` continues from `fen`; it is needed to find an opening name when the root position itself is not an exact named position.

`/lichess` aggregates rated Lichess games and supports:

- `variant`, default `standard`;
- `speeds`;
- rating bands `0`, `1000`, `1200`, `1400`, `1600`, `1800`, `2000`, `2200`, and `2500`;
- month-based `since` and `until`;
- `moves`, default 12;
- up to 4 top games and 4 recent games, with some queries permitting 8 recent games;
- optional monthly history.

[Lichess-games Explorer specification](https://github.com/lichess-org/api/blob/a0f82d031107e270f56d6bebbbdac4265926b90e/doc/specs/tags/openingexplorer/lichess.yaml)

`/masters` supports FEN, UCI play, year bounds, 12 moves by default, and up to 15 top games. [Masters Explorer specification](https://github.com/lichess-org/api/blob/a0f82d031107e270f56d6bebbbdac4265926b90e/doc/specs/tags/openingexplorer/masters.yaml)

`/player` requires a username and color, then supports variant, FEN, UCI play, speeds, casual/rated modes, month bounds, move count, and up to 8 recent games. It streams NDJSON while on-demand indexing proceeds, may send empty keepalive lines, and may emit multiple deduplicated updates. New games are indexed at most once per minute; ongoing games are revisited at most daily. [Player Explorer specification](https://github.com/lichess-org/api/blob/a0f82d031107e270f56d6bebbbdac4265926b90e/doc/specs/tags/openingexplorer/player.yaml)

The player endpoint is unnecessary for URL-only v1.

### Response meaning

The Lichess and Masters responses contain:

- optional opening `{eco, name}`;
- total `white`, `draws`, and `black` game outcomes from the queried position;
- common moves with UCI, SAN, average rating, and outcome counts;
- optional opening after each move;
- top or recent example games, depending on endpoint and request.

[Lichess response schema](https://github.com/lichess-org/api/blob/a0f82d031107e270f56d6bebbbdac4265926b90e/doc/specs/schemas/OpeningExplorerLichess.yaml), [Masters response schema](https://github.com/lichess-org/api/blob/a0f82d031107e270f56d6bebbbdac4265926b90e/doc/specs/schemas/OpeningExplorerMasters.yaml)

These are descriptive game counts. They are not evaluations, best-move claims, critical-moment labels, themes, or lesson recommendations. Per-move outcome counts can include games that later transpose to the same position. The Explorer's own source describes the counts as game outcomes, not objective move quality. [Official Explorer README](https://github.com/lichess-org/lila-openingexplorer/blob/38bddd031a30a3d17dad041d20baf86fcb91e038/README.md#public-http-api)

**Inferred.** Explorer data may explain frequency, rating cohort, and where recorded play becomes sparse. Stockfish and ChenChess's deterministic review facts must remain the authority for objective move quality.

### Explorer errors and caching

The public spec documents `200` only. The official server source maps malformed position or move input to plain-text `400`, a full player-index queue to `503`, and upstream request failure to `500`. A reverse proxy may replace these bodies. [Explorer error implementation](https://github.com/lichess-org/lila-openingexplorer/blob/38bddd031a30a3d17dad041d20baf86fcb91e038/src/api/error.rs)

The server internally caches Lichess queries for up to two hours and Masters queries for up to four hours, with ten-minute idle expiry. Those are implementation settings, not client freshness promises, and the service emits no documented cache contract. [Explorer cache setup](https://github.com/lichess-org/lila-openingexplorer/blob/38bddd031a30a3d17dad041d20baf86fcb91e038/src/main.rs#L148-L177)

## Lichess Practice

### What exists

**Source-backed, not public OpenAPI.** Lichess routes:

```text
/practice
/practice/{sectionId}
/practice/{sectionId}/{studySlug}
/practice/{sectionId}/{studySlug}/{studyId}
/practice/{sectionId}/{studySlug}/{studyId}/{chapterId}
/practice/load/{studyId}/{chapterId}
```

[Practice routes](https://github.com/lichess-org/lila/blob/4e076cfaceaeebb19437edbc657a7371b3e841ff/conf/routes#L347-L355)

`GET /practice` content-negotiates JSON. Anonymous `Accept: application/json` returned sections and studies with `id`, `slug`, and `name`. The controller and serializer confirm that shape. [Practice controller](https://github.com/lichess-org/lila/blob/4e076cfaceaeebb19437edbc657a7371b3e841ff/app/controllers/Practice.scala), [JSON serializer](https://github.com/lichess-org/lila/blob/4e076cfaceaeebb19437edbc657a7371b3e841ff/modules/practice/src/main/JsonView.scala)

Study and chapter IDs are the lookup keys. The full-page controller ignores the supplied section and slug, then emits a canonical URL for the real lesson. This explains why a path with a valid study ID and invented slugs returned `200` in a live check. Do not rely on ignored slugs; use the canonical section, slug, and IDs from a reviewed catalog. [Practice controller](https://github.com/lichess-org/lila/blob/4e076cfaceaeebb19437edbc657a7371b3e841ff/app/controllers/Practice.scala)

### What is missing

There are no Practice paths in the public Lichess OpenAPI specification. The catalog JSON exposes no theme taxonomy, semantic search, game-review mapping, descriptions, or chapter IDs. `/practice/load/...` returns internal study and analysis data when both IDs are already known, but it is not a documented public API.

The current hard-coded catalog contains checkmates, fundamental and advanced tactics, pawn endgames, and rook endgames. It has no opening lessons. [Current Practice catalog source](https://github.com/lichess-org/lila/blob/4e076cfaceaeebb19437edbc657a7371b3e841ff/modules/practice/src/main/PracticeSections.scala)

**Product consequence.** Opening identification cannot by itself produce a matching Lichess Practice recommendation. A safe v1 can maintain a small, reviewed allowlist that maps ChenChess teaching themes to canonical Lichess Practice study URLs. If no exact mapping exists, show no recommendation. Do not scrape Practice HTML or call the internal loader at review time. Lichess explicitly asks developers to request a public endpoint instead of using web scraping or browser automation. [API Tips](https://lichess.org/page/api-tips)

Deep links work today, but Lichess gives no stability guarantee for the catalog, its IDs, or those web routes. Validate the allowlist periodically and treat a broken link as an optional recommendation failure, never a review failure.

## Operating rules

### Rate limits and retry

**Documented.** Lichess's general rules are:

- make only one API request at a time;
- after `429`, stop for a full minute before resuming, then reduce request frequency;
- exact limits vary and can change.

[API reference introduction](https://github.com/lichess-org/api/blob/a0f82d031107e270f56d6bebbbdac4265926b90e/doc/specs/lichess-api.yaml#L42-L50), [API Tips](https://lichess.org/page/api-tips)

No `Retry-After` header or exact game-export quota is promised. Explorer has the separate announced 25-request-per-minute authenticated limit.

**Inferred retry policy.** Serialize outbound Lichess calls. Treat `404` as terminal and `401` as a configuration or scope error. For `429`, pause the whole Lichess client for at least 60 seconds. Bounded exponential backoff with jitter for timeouts and `5xx` is reasonable for idempotent GETs, but that is ChenChess policy, not a Lichess guarantee.

### Caching

**Observed.** The checked game-export responses had no `Cache-Control`, ETag, or Last-Modified header. Practice pages and the Practice loader used `no-cache, no-store, must-revalidate`. Explorer exposes no documented HTTP freshness metadata.

**Inferred.** Cache a completed game only under an app-owned key containing game ID and request flags. The move list is final, but later Lichess analysis, accuracy, or opening metadata can change. Since v1 Review Sessions are in-session only, a session cache and request de-duplication are enough. A Practice allowlist should be versioned and checked out of band rather than fetched for every review.

### Attribution, licenses, and robots

Lichess Terms allow personal and commercial use of its services subject to applicable licenses and discretionary caps. The service and its licensing may change. [Terms of Service](https://lichess.org/terms-of-service)

Lichess's downloadable database exports are CC0 and may be reused without permission. Official curated opening-name data is also public-domain data. These statements apply to those published datasets; the single-game API documentation does not state a separate response-data license or attribution rule. [Open database](https://database.lichess.org/), [chess-openings copyright](https://github.com/lichess-org/chess-openings#copyright)

No current API-specific attribution mandate was found. Still, preserving the original `Site` URL and displaying a plain `Source: Lichess` link gives the learner provenance and a route back to the game. That is a product recommendation, not a claimed license requirement. Lichess logos, UI assets, and source code have separate licenses.

`robots.txt` disallows crawler indexing of `/game/export/` and `/api/`. This does not contradict the documented API invitation; robots rules govern crawlers, while API Tips encourages normal API clients and discourages scraping. Use the API with an identifying client and do not crawl HTML. [robots.txt](https://lichess.org/robots.txt), [API Tips](https://lichess.org/page/api-tips)

## Facts later tickets can rely on

- Anonymous import from an eight-character game URL is supported by a documented endpoint.
- Bare game URLs mean White orientation. `/white` and `/black` are orientation hints, not identity claims.
- One JSON export with `pgnInJson=true` can provide eligibility metadata, opening identification, moves, and PGN.
- The export endpoint will return ongoing and variant games. ChenChess must reject them after inspecting JSON.
- Live Opening Explorer statistics require OAuth as of March 2026. This conflicts with no-OAuth v1.
- Explorer outcomes and popularity are context, not objective chess evaluation.
- Lichess Practice has no stable public discovery or matching API, and its current catalog has no opening lessons.
- A curated, optional Practice-link allowlist is safer than runtime dependency on internal Practice routes.
- Serialize requests, wait at least one minute after `429`, and do not assume server-provided cache TTLs or structured error bodies.
