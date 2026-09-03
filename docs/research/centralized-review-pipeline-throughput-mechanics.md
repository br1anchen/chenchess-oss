# Centralized Game Review throughput mechanics

Research date: 2026-07-18. Resolves [Design the centralized review pipeline throughput mechanics](#80) on the [Speed up Game Review analysis wall time](#72) map and supplies measured constants to [Set the centralized multi-session compute policy](#66).

## Decision

Run a full Game Review as one asynchronous backend operation with two ordered provider phases:

1. lease up to eight long-lived, single-threaded Stockfish workers as a group, restore or compute every required Position, and collate the evidence in ply order;
2. send the Game's ordered move Positions to one bounded Maia batch operation, using four concurrent CPU inference slots on the measured deployment shape.

The operation uses the existing `accepted -> progress* -> terminal` Review Session event stream. Progress counts completed Positions in the active provider phase and emits a heartbeat while the count is unchanged. Queue wait and provider execution have separate deadlines. Explicit cancellation or an execution deadline removes the queued lease request or undispatched lane work, stops active provider work, and fences late results from publication.

An exact Engine Analysis cache may satisfy Stockfish Positions before the worker lease. Its key is the canonical FEN plus every field of `EngineProvenance`. Cache entries are cross-Player because they contain only Position evidence and provider provenance, never a Player, source Game, or Review Session. Correctness does not depend on cache availability or retention.

This is the throughput mechanism, not the centralized compute policy. Pool size, the number of simultaneous leases, admission, queue ordering and length, fairness, per-Player budgets, rate limits, and cache capacity or retention remain decisions for [Set the centralized multi-session compute policy](#66).

Keep one canonical Game Review pipeline. `ReviewFactsService` continues to enumerate Positions, sequence provider phases, restore ply order, and build facts. The centralized Engine Analysis adapter owns cache lookup and worker leasing behind the existing bulk-analysis seam; the centralized Human Move Model adapter owns the one batch call. The operation runner owns progress, deadline, and cancellation. No central-host branch belongs in Rule Extraction or evidence assembly.

## Why this reaches the 3–8 second target

The measurements below come from [Game Review wall-time attribution](game-review-wall-time-attribution.md); the fixed provider settings, Game limit, and deadline baselines come from [Review Session operating limits](review-session-operating-limits.md). The measured 66-ply resignation fixture requires 67 Stockfish calls and 66 Maia calls. The current local runtime already implements the same provider ordering with eight persistent Game-scoped Stockfish sessions followed by four-wide Maia:

| Measurement                                 |                Result |
| ------------------------------------------- | --------------------: |
| Full event-reported pipeline median         |               4.595 s |
| Process start through `gameImported` median |               5.011 s |
| Summed Stockfish time median                |              11.508 s |
| Persistent-session Stockfish call median    |                137 ms |
| Four-wide Maia throughput                   |      24.81 requests/s |
| Stockfish child cleanup p95 / maximum       | 305.78 ms / 355.85 ms |

The conservative planning model does not assume every centralized search receives the 137 ms warm-session median. At 300 ms per Stockfish Position, eight workers need `ceil(67 / 8) * 0.300 = 2.7` seconds. Maia needs `66 / 24.81 = 2.66` seconds on the measured four-wide CPU service. Sequential phases therefore model to 5.36 seconds before non-provider assembly, inside the 3–8 second server-side provider target. The observed 4.595-second local pipeline is stronger end-to-end evidence for this shape than the arithmetic alone.

The target covers active provider time for an admitted 66-ply job. It excludes queue wait, cold model provisioning, image download, Language Layer prose, and host tool-loop latency. Policy must size admission against a cold cache; cache hits are upside, not required capacity.

## Stockfish pool and review lease

The deployment owns a pool of already-started Stockfish 18 processes. Each worker is permanently configured with the accepted policy: depth 16, `Threads=1`, `Hash=16` MiB, and `go depth 16`. A worker that fails its handshake, crashes, or times out is terminated, reaped, and replaced before it returns to the pool.

A Game Review requests `min(8, cache_miss_count)` workers as one lease. The admission policy decides when that lease starts; the throughput layer does not silently start the measured eight-wide job with fewer workers. Once leased:

1. send `ucinewgame` and `isready` to every worker so no previous Player's transposition-table history crosses the lease boundary;
2. partition the ordered cache misses into deterministic contiguous lanes, differing in length by at most one Position;
3. let each worker analyze its lane serially, retaining its 16 MiB hash between Positions in that lane;
4. collect results by original Position index and preserve the current earliest-ply provider-error precedence;
5. return healthy workers only after every lane has completed or cancellation cleanup has reaped its work.

The global scheduler queues review-lease requests, not unrelated individual Positions. After admission, each leased worker owns one private lane of Position work for that Game. Arbitrary cross-Game Position interleaving would make one Player's warm transposition table affect another Player's result and would discard the measured session-reuse behavior. The group lease is therefore the mechanical unit exposed to the policy layer.

This structure retains the benefit proven by [Reuse one Stockfish session per Game Review](#77): one handshake per lane and a warm hash across related Positions. It also retains deterministic lane assignment for a fixed set of misses. Cache state can change which Positions are searched, so mixed hit/miss certification must use the existing live-evaluation contract: exact moves, ranks, categories, selected plies, and provenance, with the accepted 15-centipawn numeric tolerance. Neither single-threaded search nor the cache implies bit-identical evaluations across different warm-hash histories.

## Maia batch operation

The centralized Review Engine job defined here is the concrete caller for one private batch operation; the local runtime had no such caller when [Serve Maia predictions concurrently](#76) correctly omitted it.

The Maia service should run behind an ASGI server with one loaded, digest-pinned model. Its internal batch request is deliberately narrow:

- one ordered list of canonical FENs from a single Game;
- one required Elo Profile shared by those Positions;
- the existing fixed top-five result contract;
- one ordered result or one indexed failure for each input Position.

On the certified CPU shape, the service executes waves of at most four predictions with PyTorch pinned to two intra-op threads and one inter-op thread, matching the measured 24.81 requests/second configuration. Cancelling the batch request stops dispatch of later waves; a synchronous torch call already running may finish its current wave, but its result is discarded. The endpoint removes per-Position Rust-to-Python round trips as an orchestration concern without claiming tensor batching that has not been measured. A later GPU implementation may replace the inference executor with true tensor batches behind the same narrow contract only after digest, parity, latency, memory, and cancellation certification.

Stockfish and Maia remain separate phases on CPU, preserving the [Local Pipeline Runtime refinement](../adr/0003-game-review-pipeline.md). Measured provider overlap made the 66-ply pipeline slower: 12.485 seconds with eight Stockfish workers and 23.109 seconds with four, versus 9.687 seconds for the then-current sequential phases. A deployment with GPU Maia may reconsider overlap only with equivalent full-Game evidence; it is not part of the 3–8 second capacity claim.

Status note (2026-07-29): #175
supersedes this design's earlier omission of a Maia cross-Player cache.
Successful predictions now use a bounded 64 MiB process-local LRU keyed by
canonical FEN (including side to move), exact Elo, output limit, and pinned
Maia package/model/image/config identities. The cache contains no Player,
request, conversation, or session identity; failures are never cached, and
capacity planning still assumes zero hits.

## Exact Engine Analysis cache contract

### Key

Parse and canonicalize the six-field FEN once at Game-import validation. Do not remove counters or otherwise normalize it lossy: the half-move clock participates in chess rules, and an exact key is safer than a broader transposition key.

The logical key is:

```text
StockfishEvidenceCacheKey {
  canonical_fen,
  engine_version,
  engine_binary_sha256,
  depth,
  threads,
  hash_mib
}
```

These provider fields are exactly the current [`EngineProvenance`](../../services/coach-engine/src/engine_analysis.rs#L47-L53) fields. The full logical key and provenance remain in the value so a cache adapter can verify them on read.

The value contains one typed `EngineAnalysis`, the exact `EngineProvenance`, and a content digest over the canonical key and typed analysis. The cache adapter is an untrusted-data boundary: it decodes once, verifies the digest, requires exact key/provenance equality, requires `analysis.depth == provenance.depth`, and returns trusted evidence to the pipeline. A malformed or mismatched entry is a miss and an observable cache-integrity fault, never a fallback value published as evidence.

### Certification and invalidation

`EngineProvenance` pins the facts that select the accepted engine execution contract: Stockfish version, exact binary digest, depth, threads, and hash. The existing evidence-integrity and provider-recording checks already reject drift in those fields. A cache hit with matching provenance can therefore enter the same recording and live-evaluation certification path as newly computed evidence.

This certifies provider identity and settings, not bit-for-bit replay across arbitrary transposition-table histories. Cached and newly computed facts still pass the existing structural gates and 15-centipawn live tolerance before a runtime is released.

Invalidation is namespace-based:

- a different Stockfish version or binary changes the key;
- a different depth, thread count, or hash size changes the key;
- a changed typed evidence shape that cannot be decoded is treated as a miss;
- failed, timed-out, cancelled, terminal-without-analysis, and malformed results are never stored.

Deploys need not scan or delete old entries for correctness. Cache writes are atomic per immutable Position entry and may survive a later job cancellation; the Game Review itself is published only after all ordered evidence and downstream validation succeed. If a shared cache that survives deployments is introduced later, its deployment-owned namespace can include the code revision; that need does not justify a versioned evidence struct today.

The cache may be bounded and per API instance in a one-instance deployment. It is non-authoritative, so a cold deploy or an instance-local miss only spends compute. If the centralized policy chooses horizontal API replicas and wants deployment-wide hit rates, it must choose a shared cache backend; the key and validation contract do not change.

### Opening-repetition estimate

No production corpus has measured a cross-Player hit rate yet. Capacity planning must use zero hits. The exact-FEN mechanics still give useful bounds for a 67-Position review after the cache is warm:

| Repetition case                | Engine hits | Hit rate |
| ------------------------------ | ----------: | -------: |
| Standard initial Position only |      1 / 67 |     1.5% |
| Same first 4 plies             |      5 / 67 |     7.5% |
| Same first 8 plies             |      9 / 67 |    13.4% |
| Same first 12 plies            |     13 / 67 |    19.4% |
| Exact Game re-review           |     67 / 67 |     100% |

The 1.5–19.4 percent figures are opening-prefix scenarios, not a traffic forecast. Instrument `engine_cache_hits / engine_positions` by deployment and by prefix depth before using a nonzero assumption in policy. Exact re-reviews are the only justified near-100-percent case.

## Job, progress, deadline, and cancellation semantics

### Asynchronous operation

Keep the existing [Review Session command seam](coach-mcp-seam.md). `ImportGame` is accepted with its `operationId`, and the Review Engine owns the live job while the transport streams sequenced events. The Coach MCP adapter may relay progress when the host supports it and folds the one terminal event into the tool result. Host progress support is optional; correctness never depends on it.

An HTTP or MCP disconnect is not cancellation. The explicit `CancelOperation` command is the cancellation authority. V1 does not require a durable job broker, polling API, or resumable event log: those are not needed by the current command caller, and a process restart may terminate in-memory work under the existing unavailable/start-new-operation recovery model.

### Progress

The `RunningGameReview` stage gains typed provider detail:

```text
phase: engineAnalysis | humanMoveModel | buildingReview
completedPositions: integer
totalPositions: integer
cacheHits: integer (engineAnalysis only)
```

`completedPositions` advances once for every cache hit or completed provider result, even when work completes out of ply order. It never decreases and never exceeds `totalPositions`. The engine total includes the final Position for a nonterminal Game; Maia covers move Positions only. Ordered collation remains a terminal assembly concern.

Emit progress after every completed Position and repeat the current value at least once per second while a provider call is active. That makes a 400-ply Game visibly alive even during a slow Position. `buildingReview` has no synthetic percentage; its heartbeat continues until the typed Game Review is published or the operation terminates.

### Deadlines

Queue wait and execution use different clocks:

- after source validation and the fail-open cache lookup determine the miss set, the queue deadline starts when the job requests its complete Stockfish worker lease and ends when that lease is admitted; its duration and busy/queue outcome belong to the centralized compute policy;
- the execution deadline starts when the lease is admitted, or immediately after an all-hit lookup that needs no lease, and covers both provider phases, ordered assembly, and fact validation;
- the existing 30-second per-Position provider timeout remains a subordinate fail-safe.

Size the execution deadline from the measured p95 model, then add a two-times integration margin:

```text
modeled_seconds =
  ceil(engine_position_count / 8) * 0.685
  + human_position_count / 24.81

execution_deadline = clamp(30 seconds, 120 seconds, 2 * modeled_seconds)
```

Both Position counts are the full pre-cache counts, so cache hits never shorten the safety deadline. This produces the 30-second floor for the 67/66 call case. For the maximum nonterminal 400-ply Game, 401 Stockfish and 400 Maia Positions model to about 51.1 seconds and receive about 102.2 seconds, below the 120-second cap. The cap is valid only while `MAX_GAME_PLIES` remains 400 and must be re-derived if the limit or provider measurements change.

If the execution deadline expires during Stockfish, terminate with the existing Stockfish timeout reason; if it expires during Maia, use the Maia timeout reason. Expiry during ordered assembly or fact validation needs a typed `reviewDeadline` reason in the next contract revision rather than falsely blaming a provider. Queue expiry remains `queueDeadline`. User cancellation remains `cancelled`, not a timeout. Exactly one terminal outcome may pass the publication fence.

### Cleanup

Cancellation and deadline handling share one structured cleanup path:

1. mark the operation terminal-pending so no new Position is dispatched;
2. remove its queued lease request or undispatched private-lane work;
3. cancel the in-flight Maia batch and discard any late response;
4. terminate and reap every Stockfish process in the lease rather than returning uncertain state to the pool;
5. replace those workers asynchronously and prevent late facts from crossing the publication fence;
6. emit the one typed terminal event after cleanup ownership is established.

The release budget remains five seconds. The current eight-process probe cleared all Stockfish children at 305.78 ms p95 and 355.85 ms maximum across 60 samples, so the budget has measured headroom. Centralized release proof must repeat the probe through the actual queue, batch call, and transport rather than inheriting the local number as a service guarantee.

## Constants handed to centralized compute policy

| Constant                     | Value and use                                                                              |
| ---------------------------- | ------------------------------------------------------------------------------------------ |
| 66-ply provider shape        | 67 Stockfish Positions, 66 Maia Positions                                                  |
| Stockfish lease              | `min(8, cache_miss_count)` exclusive workers; the measured target assumes 8 misses/workers |
| Stockfish configuration      | version 18, depth 16, `Threads=1`, `Hash=16` MiB, `go depth 16`                            |
| Measured Stockfish work      | 11.508 worker-seconds median summed per 66-ply review; 137 ms median per call              |
| Conservative Stockfish model | 300 ms per Position; 2.7 s wall for 67 Positions over 8 lanes                              |
| Stockfish planning p95       | 685 ms per Position for deadline sizing                                                    |
| Maia CPU shape               | 4 concurrent predictions, 2 intra-op / 1 inter-op torch threads, 24.81 requests/s          |
| 66-ply active-provider model | about 5.36 s with sequential phases and zero cache hits                                    |
| Observed full local pipeline | 4.595 s event median; 5.011 s including process startup                                    |
| Maximum-size zero-hit model  | about 31.4 s at the 300 ms engine model, or 51.1 s using the 685 ms planning p95           |
| Hash memory floor            | 16 MiB per Stockfish worker, 128 MiB for eight hashes, excluding process overhead          |
| Cleanup                      | 305.78 ms p95 / 355.85 ms max to clear eight local children; 5 s release budget            |
| Cache planning assumption    | 0% for admission capacity; measured initial-Position floor is 1/67 after warm-up           |

For a pool of `W` healthy Stockfish workers, at most `floor(W / 8)` zero-hit 66-ply engine phases can hold measured-width leases simultaneously. That is a mechanical ceiling, not the recommended deployment concurrency: CPU, Maia capacity, queue targets, fairness, and memory still determine the policy.

## Observability and release proof

Every job should record, keyed by `operationId` and without Player-supplied PGN or FEN values in logs:

- queue wait, active execution, Stockfish wall time, summed Stockfish call time, Maia wall time, assembly time, and total pipeline time;
- requested and granted Stockfish workers, cache hits/misses/integrity faults, lane lengths, Maia batch size, and provider call counts;
- progress-heartbeat gaps, deadline phase, cancellation-to-terminal time, child-process cleanup time, and worker replacement count;
- engine and Maia provenance identifiers already present in evidence.

The centralized implementation is not certified by this design. Release proof must run at least:

1. the 66-ply fixture cold-cache and warm-cache at idle, recording p50, p95, and maximum;
2. the accepted six-case live corpus with exact structural gates, provenance, and the 15-centipawn/0.02 numeric tolerances;
3. mixed cache-hit patterns to exercise deterministic miss lanes and cache-entry boundary validation;
4. overlapping reviews at every concurrency the centralized policy permits, measuring queue wait separately from active provider time;
5. cancellation while queued, during Stockfish, during Maia, and immediately before publication, proving one terminal event and no surviving child process;
6. the 400-ply limit, proving monotonic progress and the derived operation deadline;
7. worker crash, malformed cache value, Maia indexed failure, and deploy-cold recovery.

The 3–8 second target is accepted only if the centralized 66-ply active-provider distribution meets it under the concurrency policy's certified load. The local 4.595-second median and the 5.36-second conservative model justify the architecture; they do not substitute for the centralized measurement.
