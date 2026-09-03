# Review Session operating limits research

Research date: 2026-07-15

This note uses current primary Lichess documentation, API schemas, service terms, and server source. It also records local measurements for the pinned Stockfish and Maia runtime used to choose Review Session budgets. The final product limits live in the resolution of the Wayfinder ticket that links this asset.

## Finding

Chenchess needs one anonymous, read-only Lichess request: `GET /game/export/{gameId}`. It does not need `POST /api/import`, despite that endpoint's similar name. Lichess publishes no numeric request quota for single-game export. Its published operational guidance is to make only one API request at a time and, after `429`, wait at least one minute while reducing request frequency. Some rate limits require a longer wait. [Lichess API rate-limiting guidance](https://github.com/lichess-org/api/blob/master/doc/specs/lichess-api.yaml#L42-L49), [Lichess API Tips](https://lichess.org/page/api-tips)

The safe v1 shape is therefore one server-side export request per uncached game, with all Lichess calls serialized, concurrent requests for the same game coalesced, and a shared cooldown after `429`. Lichess does not publish a timeout, service-level objective, response-size ceiling, or retry rule for timeouts and `5xx` responses. Chenchess must choose those limits from its own measurements and treat them as local policy, not as a Lichess guarantee.

## Single-game export

`GET https://lichess.org/game/export/{gameId}` accepts an eight-character game ID and requires no authentication. It returns either PGN or JSON according to `Accept`. The JSON form can include the complete PGN in a `pgn` field, which lets Chenchess validate status and variant and obtain the PGN in one request. The documented success response also allows cross-origin access, although Chenchess still benefits from a server-side gateway because that is where global serialization, caching, and URL validation can be enforced. [Export one game API specification](https://github.com/lichess-org/api/blob/master/doc/specs/tags/games/game-export-gameId.yaml#L1-L19), [response formats and CORS](https://github.com/lichess-org/api/blob/master/doc/specs/tags/games/game-export-gameId.yaml#L95-L113)

The narrow request needed by v1 is:

```http
GET /game/export/{gameId}?moves=true&pgnInJson=true&tags=true&clocks=false&evals=false&accuracy=false&opening=true&division=false&literate=false
Accept: application/json
```

`pgnInJson=true` requests the full PGN in the JSON response. Setting the other switches explicitly matters because moves, tags, clocks, evaluations, opening, and game division all default to included, except accuracy and literate annotations. Chenchess needs moves, tags, and the optional opening name; it does not need Lichess clocks, evaluations, accuracy, phase division, bookmarks, or generated prose. Omitting those fields reduces transferred data and keeps Lichess analysis out of Chenchess's own fact boundary. [Export parameters](https://github.com/lichess-org/api/blob/master/doc/specs/tags/games/game-export-gameId.yaml#L20-L94)

The JSON schema requires `id`, `variant`, `status`, player data, and timestamps; `pgn`, `moves`, and `opening` are optional fields whose presence depends on the request and available data. The opening name is therefore enrichment, not a condition for successful import. [Game JSON schema](https://github.com/lichess-org/api/blob/master/doc/specs/schemas/GameJson.yaml)

The variant must be `standard`. Lichess documents `created` and `started` alongside terminal statuses such as `mate`, `resign`, `draw`, and `timeout`. Chenchess should reject `created` and `started` before starting any chess analysis, then apply its own reviewability checks to terminal or aborted games. [Variant keys](https://github.com/lichess-org/api/blob/master/doc/specs/schemas/VariantKey.yaml), [game status values](https://github.com/lichess-org/api/blob/master/doc/specs/schemas/GameStatusName.yaml)

### Ongoing-game safeguard

Lichess deliberately delays ongoing export by three moves to deter cheat bots. Its Terms of Service also prohibit external assistance, including chess engines and move recommendations, during a game in which the user is participating. A delayed export is not permission to analyze an ongoing game. V1 must inspect the returned status and refuse to launch Game Review unless the game is over. This check belongs at the trusted server boundary, not only in the browser or Coach Skill. [Export delay](https://github.com/lichess-org/api/blob/master/doc/specs/tags/games/game-export-gameId.yaml#L4-L9), [Lichess fair-play terms](https://lichess.org/terms-of-service#fair-play-and-community-guidelines)

Use the documented API endpoint rather than scraping a game page or driving a browser. Lichess explicitly asks integrations to request missing API coverage instead of scraping or browser automation. Chenchess already has the needed endpoint. [Lichess API Tips](https://lichess.org/page/api-tips)

## The unrelated Lichess import endpoint

`POST /api/import` writes one form-encoded PGN into Lichess and returns a new Lichess game ID and URL. Its published limit is 100 imported games per hour for anonymous requests and 200 per hour for OAuth requests. This endpoint creates data on Lichess; it is not how Chenchess reads the game behind a Player-supplied Lichess URL and should not appear in the v1 dependency path. [Import one game API specification](https://github.com/lichess-org/api/blob/master/doc/specs/tags/games/api-import.yaml)

There is a documentation inconsistency worth recording. The import operation's prose gives an anonymous quota, but its OpenAPI `security` block lists OAuth. Current Lichess server source implements the API action with `AnonOrScopedBody` and charges anonymous requests a higher rate-limit cost, which supports the prose claim that anonymous import works. [Lichess importer controller](https://github.com/lichess-org/lila/blob/master/app/controllers/Importer.scala#L24-L46)

The import specification does not promise idempotency or duplicate suppression. If a later product ever uses this write endpoint, it should not automatically replay an ambiguous timeout. That is an inference from the absence of an idempotency contract, not a documented Lichess retry rule.

## Rate limiting, retries, and caching

Lichess states two general rules: make only one API request at a time, and slow down after `429`. The main API reference says a one-minute wait is sufficient in most cases, but some limits require longer; API Tips says to wait a full minute before resuming. Lichess intentionally does not publish endpoint-by-endpoint thresholds because the protective limits vary and change. [API reference](https://github.com/lichess-org/api/blob/master/doc/specs/lichess-api.yaml#L42-L49), [API Tips](https://lichess.org/page/api-tips)

For v1, those rules imply:

- Run at most one outbound Lichess API request at once per deployment or shared egress identity. A per-process lock is insufficient if several instances share an IP. Lichess does not state whether "one request at a time" is keyed by IP, token, or another identity, so deployment-wide serialization is the conservative reading.
- Coalesce simultaneous imports of the same canonical game ID into one in-flight request. Cache only a successful, validated completed-game response, keyed by canonical game ID plus the explicit export representation/version. Do not cache `429`, timeouts, malformed bodies, ongoing games, or other failures as game data.
- On `429`, open a shared Lichess cooldown for at least 60 seconds. Do not send probes during the cooldown. A later idempotent export GET may retry, but repeated minute-by-minute retries are not justified because Lichess says some limits last longer and clients must reduce request frequency.
- Do not retry a user-visible import loop without a bound. The official guidance covers `429`, not connection failures, timeouts, or `5xx`. A single later GET attempt may be a reasonable Chenchess policy because export is read-only, but Lichess does not require or guarantee that behavior.
- Keep the fetch behind the fixed `https://lichess.org/game/export/{gameId}` construction. Do not follow a Player-supplied hostname, scheme, port, path, or redirect as an API target. This is a Chenchess SSRF safeguard, not a Lichess rule.

Lichess publishes no cache lifetime or revalidation contract for single-game export. Completed move data is naturally stable, but optional opening labels and Lichess-generated analysis can change. Because the proposed request excludes evaluations and other generated analysis, a Chenchess-owned cache can be useful, but its retention period must be decided locally. The Terms also reserve Lichess's ability to change or remove services and apply discretionary caps. [Lichess Terms of Service, "Using our services"](https://lichess.org/terms-of-service#using-our-services)

## Usage and attribution

Lichess permits personal and commercial use of its services, including APIs and game databases, subject to its rules and the licenses that apply to each part. It also states that submitted content remains the submitter's, while the submitter grants Lichess a broad non-exclusive license. [Lichess Terms of Service, service use and submitted content](https://lichess.org/terms-of-service)

The open-database page releases its database exports under CC0 and allows research, commercial use, modification, and redistribution without permission. That statement is explicitly about database exports. The same page separately licenses broadcast-game exports under CC BY-SA 4.0. Neither the API reference nor the Terms explicitly says that every individual game returned by `GET /game/export/{gameId}` is a CC0 database export. [Lichess open database](https://database.lichess.org/)

No first-party source reviewed here imposes a specific attribution string for this API call. The licensing boundary for an individual API-exported PGN is nevertheless less explicit than the bulk-database license. V1 should preserve the canonical Lichess game URL and display a plain source link such as `Source: Lichess` with imported-game provenance. That is a conservative product recommendation and good traceability, not a claim that Lichess has published this exact attribution requirement. If Chenchess later republishes bulk game data, annotations, broadcast material, logos, or other Lichess assets, that use needs its own license review.

## Contract Chenchess can safely derive

1. Parse the Player's URL locally, extract one canonical eight-character game ID, and build the fixed Lichess export URL.
2. Make one anonymous JSON export request with `pgnInJson=true` and explicit minimal fields.
3. Serialize all Lichess requests and coalesce identical in-flight game IDs.
4. Require `variant == "standard"`, reject `created` and `started`, require a parseable single-game PGN, and preserve the source URL.
5. After `429`, stop all Lichess calls for at least 60 seconds and reduce request frequency. Treat longer cooldowns as possible.
6. Do not call `POST /api/import`, scrape Lichess pages, or send OAuth credentials for this feature.
7. Choose request deadlines, response byte limits, cache retention, and any non-`429` retry only from Chenchess measurements and operational risk. Lichess supplies no values to inherit.

## Explicit uncertainty

Current first-party sources do not specify:

- a numeric request quota for `GET /game/export/{gameId}`;
- the identity against which the one-request-at-a-time rule is enforced;
- a connect, first-byte, whole-response, or server timeout;
- a response-size or maximum-PGN-size limit;
- a service-level availability or latency objective;
- a stable error schema for not-found, malformed-ID, `429`, or `5xx` responses;
- a guaranteed `Retry-After` header, maximum `429` cooldown, or retry rule for timeouts and `5xx`;
- cache headers, a cache lifetime, or an immutability promise for completed export responses;
- an idempotency contract for `POST /api/import`; or
- a license or mandatory attribution phrase stated specifically for a single game fetched through the export API.

These gaps should stay visible in the v1 specification. They are not evidence for unlimited requests, infinite payloads, immediate retries, or omission of provenance.

## Local measurement method

Measurements ran on 2026-07-15 on the certified Apple Silicon development machine with 10 logical CPUs. The installed unit was `0.1.0-local-coach.1`, with Stockfish 18 and `maia2==0.9/rapid` on CPU. The warmed Maia container reported 388.4 MiB in use while idle. Starting that cached container and reaching health took 6.63 seconds.

[`measure_review_session_primitives`](../../services/coach-engine/src/bin/measure_review_session_primitives.rs) is the rerunnable harness. It uses the production Stockfish and Maia adapters, starting a fresh Stockfish process for each call as the adapter does. It samples six positions covering the initial Position, opening tactics, quiet and complex middlegames, a pawn ending, and forced mate. It also calls the pinned Maia service over its production HTTP adapter shape at 1200 and 1900 Elo.

The main commands were:

```sh
cargo run -p chen-chess-coach-api --bin measure_review_session_primitives -- \
  --stockfish ~/.local/share/chenchess/units/0.1.0-local-coach.1/bin/stockfish \
  --depths 12,14,16,18 \
  --maia-base-url http://127.0.0.1:38271 \
  --maia-concurrency 4

cargo run -p chen-chess-coach-api --bin measure_review_session_primitives -- \
  --skip-stockfish \
  --maia-base-url http://127.0.0.1:38271 \
  --maia-repeats 10 \
  --maia-concurrency 4

cargo run -p chen-chess-coach-api --bin measure_review_session_primitives -- \
  --skip-stockfish \
  --review-command ~/.local/bin/chenchess \
  --review-case backend/evaluation/corpus/positional-black-intermediate.case.json \
  --review-elo 1200 \
  --review-side black \
  --review-repeats 10
```

One live export measurement used the exact minimal request proposed above against the repository's known game `Synthet1`. It returned HTTP 200, 1,882 bytes, first byte in 102 ms, and completed in 102 ms. This single observation checks request shape and scale. It is not a Lichess latency objective.

## Stockfish results

Depth 16 was the useful knee in this small sample.

| Depth | Observed median |      Observed p95 | Difference from depth 18               |
| ----- | --------------: | ----------------: | -------------------------------------- |
| 12    |          213 ms |          1,088 ms | Up to 37 cp; 4 of 6 best moves matched |
| 14    |          240 ms |            389 ms | Up to 15 cp; 4 of 6 best moves matched |
| 16    |   291 to 391 ms |     630 to 782 ms | Up to 10 cp; 6 of 6 best moves matched |
| 18    |   400 to 575 ms | 1,480 to 1,844 ms | Reference                              |

The ranges reflect separate runs with two or three repetitions per Position. They are more honest than pooling small samples gathered under different machine load. The existing 15-centipawn live-evaluation tolerance covers every depth-16 versus depth-18 difference in this sample. Mate outcome and distance stayed identical.

Twenty cancellation probes started a depth-99 search, sent process termination, and waited for reap. Termination took 10.66 ms at p95 and 10.67 ms at maximum. This measures the owned Stockfish process only. Request propagation, queue removal, evidence cleanup, transport acknowledgement, and language-provider abort still need integration measurement.

## Maia results

The isolated warm sample used 60 calls at each Elo. The service returned up to 20 candidates, and the harness computed how much reported probability each prefix retained.

| Elo  |   Median |      p95 |   Maximum | Top-4 mass, min / median | Top-5 mass, min / median |
| ---- | -------: | -------: | --------: | -----------------------: | -----------------------: |
| 1200 | 36.58 ms | 65.44 ms | 212.73 ms |          0.5609 / 0.7874 |          0.6465 / 0.8407 |
| 1900 | 40.02 ms | 54.61 ms |  92.54 ms |          0.6225 / 0.8348 |          0.7153 / 0.8728 |

The original Python service used `HTTPServer`, so one Maia instance processed one request at a time. A 2026-07-18 refinement moved the private endpoint to a threaded server with four prediction slots and capped PyTorch CPU intra-operation parallelism at two threads and inter-operation parallelism at one. Sixty width-four requests at Elo 1200 completed in 2.418 seconds at 24.81 requests per second, versus 6.161 seconds and 9.74 requests per second against the old service. Concurrent payloads matched serial payloads exactly. The timing sample supports a small beam, but it does not prove multi-ply joint mass. The projection policy must record joint probability and abstain when retained mass falls below its gate.

## Existing Game Review cost

Ten warm two-ply Game Review fact runs took 2.07 seconds minimum, 2.12 seconds median, and 2.22 seconds p95. Each run includes runtime ownership and health checks, two paired Stockfish and Maia Position calls, and one final Stockfish Position call. One warm 84-ply repository Game completed in about 26 seconds as observed by the command runner. The current orchestrator walks the Game sequentially by ply, while Stockfish and Maia run together at each Position.

A 2026-07-17 follow-up on installed unit `0.1.0-local-coach.3` measured an exact 66-ply Game at 24.786 seconds inside a warm JSONL session, 25.306 seconds median including warm process setup, and 31.703 seconds from a cached-cold process launch. Cached Maia startup accounted for 6.38 seconds. The reported 15-minute Coach Skill run therefore spent about 96.5 percent of its time above the fact pipeline. See [Game Review wall-time attribution](game-review-wall-time-attribution.md) for the provider distributions, runtime inspection, and limits of that residual.

The bounded-engine implementation measured 9.687 seconds median inside the event-reported pipeline and 10.078 seconds including release-process startup across three warm runs of the same 66-ply fixture. A later four-wide Maia phase reduced those medians to 6.182 and 6.559 seconds. Persistent Stockfish sessions then reduced them to 4.595 and 5.011 seconds across five runs. The final pipeline keeps eight deterministic single-threaded Stockfish sessions, restores ply order, then runs four Maia calls at a time without overlapping provider phases. Its accepted six-case live evaluation had no differences under the pinned 15-centipawn tolerance or exact structural gates. A 60-sample cancellation probe measured 22.55 milliseconds p95 for CLI exit and 305.78 milliseconds p95 for all eight Stockfish PIDs to disappear, below the five-second cleanup limit. Three earlier warm runs of the canonical 84-ply Game measured 10.092 seconds median inside the pipeline and 10.472 seconds including release-process startup.

These results show why a maximum-size Game needs progress and an operation deadline rather than a request that appears idle. They do not measure LLM Game Review prose generation.

## Cost model for unimplemented Review Session work

The interactive operations do not exist yet, so end-to-end claims would be fabricated. Provider-call counts still give useful bounds:

- An Alternative Move whose source Position already has exact Engine Analysis needs one new depth-16 Stockfish call for the resulting Position. The observed p95 was below 0.8 seconds. An exact-cache miss that requires source and result analysis models to less than 1.6 seconds at the observed p95 before queue time.
- An Objective Refutation is one depth-16 Stockfish call when its Position is already canonical and legal.
- A width-four Intent Projection over four future plies creates at most 16 newly analyzed beam nodes. With two Stockfish slots, the observed p95 models to about 6.3 seconds, plus Maia calls. Extending to six future plies creates at most 24 new nodes and models to about 9.4 seconds, plus Maia. This rough bound predates the four-slot service and does not assume projection-level concurrency. Terminal nodes, transpositions, and exact evidence reuse reduce those counts.
- No hosted Language Layer call was measured. Coach Turn generation, structured repair, cancellation acknowledgement, and tool-loop overhead need an integration probe against the chosen provider before release.

This model is suitable for setting conservative v1 admission deadlines. It is not an end-to-end service-level measurement. The release proof should rerun the harness on a larger Position corpus, measure the actual projection implementation, exercise Alternative Move Evaluation during an active Coach Turn, and record p50, p95, maximum, cancellation-to-stop, steer-to-replacement, CPU, and memory under the configured concurrency limits.
