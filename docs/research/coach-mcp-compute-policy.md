# Centralized multi-session compute policy

Status note (2026-07-26): the final
[Coach App product and implementation specification](./coach-app-product-and-implementation-specification.md)
supersedes the client-carried-session and old service-naming assumptions.
Admission, cache, deadline, rate, fairness, and measured capacity constants
remain in force under Coach Engine ownership.

Status note (2026-07-29): #175
supersedes the earlier “no Maia cache” choice for retry-safe on-demand
authoring. Exact successful predictions now use a bounded process-local cache
keyed by canonical FEN (including side to move), Elo, candidate limit, and the
pinned Maia package/model/image/config identities. Capacity planning still
assumes zero hits; cache reuse is upside rather than required capacity.

Decision date: 2026-07-20. Resolves [Set the centralized multi-session compute policy](#66) on the [Design and prove the cross-host Coach App](#62) map. Player-confirmed decisions from a grilling session; binds the measured constants in [centralized-review-pipeline-throughput-mechanics.md](./centralized-review-pipeline-throughput-mechanics.md) and builds on [coach-mcp-seam.md](./coach-mcp-seam.md), [coach-mcp-tool-interface.md](./coach-mcp-tool-interface.md), and [review-session-operating-limits.md](./review-session-operating-limits.md).

## TL;DR

Admission stays keyed by principal class, never by surface: CoachApp Players share the web Player pool, so one human is one compute consumer across web, ChatGPT, and Claude. Admission splits by resource class — the existing `coachTurns` pool plus a new `engineLease` gate over the Stockfish review lease. The deployment is two independently scalable tiers: **engine cells** (API instance + co-located Stockfish workers) and **Maia replicas**; staging runs the production topology at one cell and one replica, so promotion turns counts, never redesign. One in-flight review per Player deployment-wide makes FIFO fair. Exact provider caches are bounded process-local LRU stores with no TTL. Rate limits live in the Review Engine keyed by PlayerId. Cancellation and deadline semantics adopt the throughput mechanics wholesale.

## Admission structure

Two decisions fix the shape; both are local to [admission.rs](../../services/coach-engine/src/review_session_processor/admission.rs):

1. **No CoachApp pool.** Pools stay keyed by principal class (`Player` vs `LocalCoach`), not `DeliverySurface`. Player identity is already unified across surfaces (`player:sha256(sub)`), so a per-surface pool would let one Player double their concurrent compute by opening a second host while partitioning a fixed physical resource. Isolation the map's acceptance test needs is per-Player, which a surface split cannot give. A surface bulkhead remains a local `admission.rs` change if production traffic ever justifies one.
2. **Admission gates by resource class.** `CoachAdmission` today gates only Coach Turn generation (web 4 slots / 8 waiting / 2-second queue deadline, local 1/1); that stays. The centralized Stockfish **review lease** — the mechanical unit the throughput design exposes to policy — gets its own `engineLease` admission, because its queue math differs by an order of magnitude from an LLM-call queue and a Player waiting on engine work must not hold a Coach Turn slot. Alternative Move and ephemeral Intent Enrichment engine calls contend under the same engine admission. Light deterministic operations such as position inspection stay ungated.

## Deployment topology and scaling

The two providers have opposite locality requirements, so the deployment is two tiers, not one cell holding both:

- **Engine cell** = one API instance plus its co-located pool of `W` already-started Stockfish 18 workers. The measured economics — one handshake per lane, warm 16 MiB hash across a contiguous lane, 305.78 ms p95 kill-all cleanup — depend on process-local workers. A network-distributed Stockfish pool is **rejected**, not deferred: a round trip per Position overwhelms the 137 ms warm-call median and turns the five-second release budget into a distributed-systems problem. Cells scale horizontally; each contributes `floor(W / 8)` leases behind its own instance-local admission.
- **Maia tier** = independently scaled replicas of the existing ASGI batch service behind one URL. The service is already stateless per batch behind a narrow contract, CPU-certified at 4 slots / 24.81 requests/s, and GPU-swappable later behind the same contract with recertification. Hardware allocation for the two tiers is therefore independent, matching their different CPU/GPU affinities.

**Staging runs the production topology at minimum scale**: one engine cell, one Maia replica, a real service boundary between them (distinct service URLs; same or separate hosts is a hardware knob). Staging's value is real ChatGPT/Claude host behavior plus derisking the production shape — the shape must be production-true, the counts may be minimal. Every measured constant is single-machine (10-logical-CPU certified shape); nothing here claims cluster behavior. At more than one engine cell, the exact Engine Analysis cache must move to a shared backend for deployment-wide hit rates — the key and validation contract do not change — and stateless routing needs no session affinity because sessions are client-carried; only live operations pin to the instance holding them, which the single accepted → terminal event stream already guarantees.

## Policy constants

| Constant                            | Value                                                       |
| ----------------------------------- | ----------------------------------------------------------- |
| Stockfish workers per cell (`W`)    | 8, single-threaded, depth 16, `Hash=16` MiB                 |
| Simultaneous engine leases per cell | `floor(W / 8)` = 1                                          |
| `engineLease` queue                 | 4 waiting, 30-second queue deadline                         |
| `coachTurns` pool (unchanged)       | web 4 / 8 waiting / 2 s; local 1 / 1                        |
| Maia batch gate                     | 1 review batch in flight per replica, FIFO                  |
| In-flight reviews per Player        | 1 deployment-wide (active or queued)                        |
| Import rate                         | 10 accepted imports per Player per 10-minute sliding window |
| Command rate                        | 120 commands per Player per minute, all sessions            |
| Engine cache                        | in-process LRU, 256 MiB bound, no TTL                       |
| Maia cache                          | in-process LRU, 64 MiB bound, success-only, no TTL          |
| Lichess export cache                | LRU 1,024 games, success-only, no TTL                       |
| Execution deadline                  | `clamp(30 s, 120 s, 2 × modeled)` per throughput mechanics  |
| Release budget                      | 5 seconds, re-proved through the full stack                 |

## Queues and busy outcomes

- **Engine lease**: 1 slot, 4 waiters, 30-second queue deadline. Depth 4 is already pathological for the staging population (every session of both test Players importing at once); a fifth requester gets an immediate honest `admissionLimit` rather than a doomed wait. Thirty seconds covers one maximal 400-ply engine phase ahead of a waiter (~20 s at the 300 ms model) or roughly ten typical 66-ply phases. Expiry is `queueDeadline`. Both reasons already exist in the v1 contract; no new vocabulary.
- **Maia batches**: one review batch in flight per replica, FIFO. The engine lease is released before the Maia phase, so overlapping reviews pipeline naturally; serializing batches gets the first review out sooner and the second no later, since interleaving on fixed total throughput is strictly worse for p50. This queue only orders admitted reviews — it can never reject one — and the execution deadline is its backstop. Interactive single predictions (~40 ms) bypass the batch gate and use free slots directly; they are bounded by per-session live-operation conflicts, not by a queue behind a 66-Position batch.
- Domain outcomes remain results, not errors, per the tool-interface note: busy and deadline outcomes surface as `unavailable` with reason and retry guidance in actionable prose.

## Per-Player limits and fairness

- **One in-flight review per Player, deployment-wide.** In-flight means holding or queued for an engine lease. A second concurrent `import_game` from any of that Player's sessions — web, ChatGPT, or Claude — returns an immediate busy result naming the running review and a retry estimate. A human cannot watch two reviews at once; the second request is an impatient retry or a second host tab, and rejecting it fast beats silently doubling compute. Re-import after completion stays allowed as a deliberate fresh import.
- **FIFO, no priority classes.** With the per-Player cap, every engine-queue waiter is a distinct Player, so FIFO already yields round-robin fairness. No surface priority (Q1 makes surfaces indistinguishable), no shortest-job-first (ply-count scheduling starves long Games for a marginal p50 win), no cache-warmth priority (cache is upside, never scheduling input). The two-Player overlap acceptance holds by construction: each Player is at most one queue position behind the other's single lease.
- **Rate limits in the Review Engine, keyed by derived PlayerId.** The adapter stays stateless (seam decision); the backend is the single implementation point admission already lives in, and the web surface inherits the same protection. Ten accepted imports per 10-minute sliding window stops a looping host model from monopolizing the lease and bounds Lichess pressure; 120 commands/minute is unreachable by human-paced coaching across three sessions but caps a runaway model loop — light operations count too, because they are cheap individually, not in an unbounded loop. Windows are in-memory per instance, per-cell at N>1.
- **No compute budgets in staging.** Billing and production capacity planning are out of the map's scope. Instead, per-Player consumed Stockfish worker-seconds and Maia prediction counts are instrumented so production budgets are set from measured data.
- Interactive operations need no per-Player cap beyond existing mechanics: the core's live-operation map and conflict outcomes already bound them per session.

## Caching

The exact Engine Analysis cache **contract** — key, digest-verified untrusted boundary, namespace invalidation, never storing failures — is fixed by the throughput mechanics; this policy sets capacity and retention:

- **Engine cache: in-process LRU bounded at 256 MiB, no TTL.** Entries are ~1–2 KB of typed analysis plus provenance and digest, so the bound holds roughly 10⁵ Positions — ample for staging traffic where exact re-reviews hit 100%. No TTL because entries are immutable facts keyed by exact provenance; staleness is impossible by construction, invalidation is namespace rotation, and LRU handles pressure. The same bound applies to the shared backend once cells exceed one.
- **Maia cache: in-process LRU bounded at 64 MiB, success-only, no TTL.** #175 supersedes the earlier omission so concurrent host retries and on-demand authoring can reuse the same immutable prediction. The key contains canonical FEN (including side to move), exact Elo, candidate limit, and the pinned package/model/image/config identities; it contains no Player, request, conversation, or session identity. Failures never enter the cache, and capacity planning continues to assume a cold cache.
- **Lichess export cache: LRU 1,024 games, success-only, no TTL.** Completed-Game moves are immutable; only cosmetic enrichment can drift and never affects facts. Keyed by canonical game ID plus representation version per the operating-limits contract; 429s, timeouts, and ongoing Games are never cached. Serialized exports, coalescing, and the 60-second 429 cooldown remain as specified there.

## Cancellation and deadlines

Adopted wholesale from the throughput mechanics, with the policy-owned blanks filled:

- Execution deadline `clamp(30 s, 120 s, 2 × modeled_seconds)` on full pre-cache Position counts; phase-attributed timeout reasons; exactly one terminal outcome through the publication fence.
- Queue deadline 30 seconds (`queueDeadline`); cancelling a queued request simply drops the waiter.
- Explicit `CancelOperation` is the only cancellation authority; disconnect is not cancellation. MCP `notifications/cancelled` and the widget cancel button both map to it. Silent abandonment is tolerated because the execution deadline bounds runaway work and the per-Player in-flight cap isolates the cost to the abandoning Player.
- The five-second release budget stands, and **staging must re-prove it through the actual path** — MCP adapter → NDJSON endpoint → queue → lease teardown — as an acceptance gate, not inherit the 305.78 ms local probe as a service guarantee.

## Observability

On top of the throughput mechanics' per-operation record (timings, worker and cache counts, lane lengths, provenance; no Player-supplied PGN or FEN in logs), the policy layer records, keyed by `operationId` and derived PlayerId:

- per admission pool (`engineLease`, `coachTurns`, per-replica Maia gate): occupancy, queue depth, wait p50/p95, and rejections split by reason — `admissionLimit`, `queueDeadline`, and per-Player-busy separately, so an undersized pool is distinguishable from one hot Player;
- rate-limiter rejections per window per Player, naming the limit that fired;
- cache hits, misses, and integrity faults, plus `engine_cache_hits / engine_positions` by opening-prefix depth — required before any nonzero cache assumption enters capacity planning;
- per-Player daily Stockfish worker-seconds and Maia prediction counts, the input to future production budgets;
- for the two-Player overlap acceptance run: queue wait reported separately from active provider time per review, proving the second Player's review pipelines rather than starves.

## Deferred

- Cell count, Maia replica count, shared cache backend selection, and per-Player compute budgets → production capacity planning, out of the #62 map's scope, to be set from the instrumentation above.
- GPU Maia executor behind the unchanged batch contract → requires digest, parity, latency, memory, and cancellation certification per the throughput mechanics.
- Empirical host cancellation and progress behavior → staging proof (#63).
