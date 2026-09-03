# Game Review wall-time attribution

Research dates: 2026-07-17 to 2026-07-18

## Finding

The chess fact pipeline does not explain the reported 15-minute Coach Skill run. On the affected Apple Silicon machine, an exact 66-ply `importGame` took 24.786 seconds inside an already-running JSONL session. Three warm process-level runs took 25.226 to 25.933 seconds, with a 25.306-second median. A cached-cold run took 31.703 seconds, including 6.38 seconds to create the Maia container, load the cached model, and reach health.

Using 900 seconds for the reported total, the cached-cold fact path accounts for 31.703 seconds, or 3.5 percent. The remaining 868.297 seconds, about 14 minutes 28 seconds, sits above the CLI fact path. It includes Language Layer generation, tool-loop scheduling, validation work, and any unrecorded wait in the original conversation. The original run has no timestamped transcript or event log, so the evidence cannot split that residual further.

The practical conclusion is firm despite that limit. Later pipeline tickets should use 24.8 seconds for warm 66-ply fact collection and 31.7 seconds for cached-cold fact collection. They should not use 15 minutes as the pipeline baseline.

A same-day bounded-analysis follow-up changed the full-Game pipeline to analyze Engine Positions with eight workers, restore ply order, then run the single-request Maia phase serially. Three final warm release-binary runs took 10.069 to 12.338 seconds from process launch through `gameImported`, with a 10.078-second median. The event-reported pipeline time ranged from 9.684 to 10.361 seconds, with a 9.687-second median. This cuts the matched 66-ply pipeline by about 61 percent from the 24.786-second command-boundary baseline. Three runs of the canonical 84-ply Game produced a 10.092-second event median, within 92 milliseconds of the rough 10-second target.

A 2026-07-18 follow-up made the Maia service and the pipeline's separate Maia phase four-wide. Three warm 66-ply runs took 6.219 to 6.842 seconds through `gameImported`, with a 6.559-second median. The event-reported pipeline ranged from 5.843 to 6.448 seconds, with a 6.182-second median. This is 36 percent below the 9.687-second bounded-engine baseline and reaches the map's 3-to-8-second full-Game provider target on the certified local runtime.

The next follow-up kept one Stockfish 18 UCI process alive in each of eight Game Review slots. The adapter divides the 67 Positions into deterministic contiguous groups of nine or eight, initializes each process once, and sends repeated `position fen` and `go depth 16` commands without clearing its 16 MiB hash. Five warm runs took 4.946 to 5.593 seconds through `gameImported`, with a 5.011-second median. Event-reported pipeline time ranged from 4.554 to 4.661 seconds, with a 4.595-second median. This is 26 percent below the four-wide Maia baseline while preserving the pinned depth, thread, hash, and terminal-search policy.

## Runtime and test Game

The measurement used installed unit `0.1.0-local-coach.3` with Stockfish 18 at depth 16, one thread, and 16 MiB hash. Maia was `maia2==0.9/rapid` on CPU. The host reported ARM64 and 10 logical CPUs. Docker reported AArch64, 10 CPUs, and 7.653 GiB memory. The pinned Maia image was ARM64, so Docker did not emulate another architecture.

The persistent-session measurement used the same installed Stockfish 18 binary and the threaded Maia image from runtime unit `0.1.0-local-coach.4`, pinned at `sha256:66007c96d3aeed8fc5f1816611c1cfe0b1d74aa0943350f8dd96cf25bb550298`. The image ran in a task-specific container against the existing verified model volume, leaving the active installed runtime unchanged.

The Maia container had no `NanoCpus`, quota, cpuset, or memory limit. `chenchess runtime maia-status` reported 332.9 MiB in use while idle. The pinned 811,001,909-byte image and named model volume were already present. Runtime startup uses `--pull=never`, so a review cannot pull the image. The cached-cold measurement includes container creation, cached model load, and health readiness. It includes neither image download nor model provisioning. [docker.rs](../../services/coach-engine/src/local_runtime/docker.rs#L15-L53)

The test Game used plies 1 through 66 of the canonical `Synthet1` fixture, ending after `33...d3` and recorded as a normal Black win by resignation. Review Side was Black and Elo Profile was 1246. This gives the same provider call count as any 33-move Game whose last board is not checkmate. The exact positions differ from the unrecorded 15-minute Game, so this is a call-count match rather than a replay of that conversation. [canonical PGN](../../services/coach-engine/evaluation/fixtures/Synthet1/lichess-export.pgn)

## Measurements

### Fact pipeline

| Boundary                                                                       | Runs |  Minimum |   Median | P95 or maximum |
| ------------------------------------------------------------------------------ | ---: | -------: | -------: | -------------: |
| Warm CLI process launch through `gameImported`                                 |    3 | 25.226 s | 25.306 s |       25.933 s |
| Warm `importGame` write through `gameImported` in a long-lived JSONL process   |    1 | 24.786 s | 24.786 s |       24.786 s |
| Cached-cold CLI process launch through `gameImported`                          |    1 | 31.703 s | 31.703 s |       31.703 s |
| Cached Maia container start through healthy                                    |    1 |   6.38 s |   6.38 s |         6.38 s |
| Bounded 66-ply warm release process launch through `gameImported`              |    3 | 10.069 s | 10.078 s |       12.338 s |
| Bounded 66-ply event-reported pipeline                                         |    3 |  9.684 s |  9.687 s |       10.361 s |
| Bounded 84-ply warm release process launch through `gameImported`              |    3 | 10.402 s | 10.472 s |       11.188 s |
| Bounded 84-ply event-reported pipeline                                         |    3 | 10.022 s | 10.092 s |       10.801 s |
| Four-wide Maia 66-ply warm release process launch through `gameImported`       |    3 |  6.219 s |  6.559 s |        6.842 s |
| Four-wide Maia 66-ply event-reported pipeline                                  |    3 |  5.843 s |  6.182 s |        6.448 s |
| Persistent Stockfish 66-ply warm release process launch through `gameImported` |    5 |  4.946 s |  5.011 s |        5.593 s |
| Persistent Stockfish 66-ply event-reported pipeline                            |    5 |  4.554 s |  4.595 s |        4.661 s |

The JSONL probe started the installed process, allowed eight seconds for warm runtime setup, then started its clock immediately before writing `importGame`. It stopped the clock when the terminal `gameImported` event arrived. The operation emitted 31 events and returned 66 plies. The process-level runs used [`measure_review_session_primitives`](../../services/coach-engine/src/bin/measure_review_session_primitives.rs).

The cached-cold delta was 6.397 seconds relative to the warm median. That agrees within 17 milliseconds with the standalone 6.38-second Maia startup. CLI bootstrap and run-to-run noise account for the small difference.

### Provider calls

The 66-ply Game has a nonterminal final board because it ended by resignation. The pipeline therefore makes 67 Stockfish calls and 66 Maia calls. The baseline ran Stockfish and Maia together for one Position, waited for both, then advanced to the next ply. The first bounded follow-up started up to eight independent Stockfish calls, restored original ply order, then kept Maia serial because the pinned service handled one request at a time. The next implementation kept the provider phases separate but ran the Maia phase four-wide against the threaded service. The persistent implementation retains those phases and replaces 67 Stockfish process lifecycles with eight deterministic Game-scoped sessions.

The prescribed primitive benchmark used six Positions. Stockfish used three repetitions per Position at depths 12, 14, 16, and 18. Maia used ten repetitions per Position at Elo 1200 and 1900.

| Provider               | Sample   |   Minimum |    Median |       P95 |   Maximum |
| ---------------------- | -------- | --------: | --------: | --------: | --------: |
| Stockfish 18, depth 16 | 18 calls | 197.56 ms | 279.67 ms | 684.78 ms | 684.78 ms |
| Maia rapid, Elo 1200   | 60 calls |  19.89 ms |  29.08 ms | 136.17 ms | 280.88 ms |
| Maia rapid, Elo 1900   | 60 calls |  19.20 ms |  25.80 ms |  34.37 ms |  83.38 ms |

Stockfish is within the 2026-07-15 envelope of 291 to 391 ms median and 630 to 782 ms p95. Maia medians improved from 36.58 to 40.02 ms. The Elo 1200 sample had a few larger outliers than the earlier 65.44 ms p95, but no call exceeded 281 ms. Neither provider shows a slowdown capable of producing minutes of extra wall time.

The three final 66-ply runs preserved the same call counts. Stockfish per-call medians ranged from 439 to 606 milliseconds under eight-process contention, while the sum of the 67 call durations ranged from 32.960 to 42.350 seconds. Bounded overlap reduced the Stockfish wall-time contribution despite that higher per-call cost. Serial Maia totals ranged from 4.179 to 5.622 seconds, with per-call medians of 51 to 65 milliseconds. Keeping Maia out of the Stockfish phase avoided the 1.49-second median observed when four concurrent requests queued behind the single-threaded service during an intermediate prototype. An ADR-compliant prototype with provider phases overlapped was also rejected: its 66-ply medians were 12.485 seconds with eight Stockfish workers and 23.109 seconds with four.

The 2026-07-18 isolated Maia comparison used 60 calls at Elo 1200. Against the old single-threaded image, width four took 6.161 seconds at 9.74 requests per second; request median and p95 were 279 and 915 milliseconds because calls queued. The threaded image completed the same work in 2.418 seconds at 24.81 requests per second; request median and p95 were 145 and 329 milliseconds. Every concurrent payload matched its serial payload. The full-Game runs recorded 66 Maia calls with summed per-call durations of 8.905 to 9.584 seconds; those durations overlap within the four-wide phase.

Across the five persistent-session runs, the sum of 67 Stockfish call durations ranged from 11.357 to 11.560 seconds, with an 11.508-second median. Per-call medians ranged from 136 to 143 milliseconds, with a 137-millisecond median across runs; per-run maxima were 570 to 598 milliseconds. The previous fresh-process implementation summed to 32.960 to 42.350 seconds with per-call medians of 439 to 606 milliseconds. Session reuse removes 65 to 73 percent of summed engine time through one handshake per slot and warm transposition tables.

The harness hashed the typed Game Review, not the imported Game. The three final 66-ply runs and one installed sequential-baseline run all produced `7a7303fb2dc32c967e15c05f600d2fbfec2542be17d310c1075710f7a675b7be`, which directly verifies unchanged facts and ordering for this fixture. The three 84-ply runs were also internally stable at `34d89c3c87cd8d5928e1716ddc7b6d69452be9c1120fc6b98dcdccb5fb73ad49`.

The later four-wide Maia runs used a newer `main` revision whose teaching-fact schema had already changed. On that common revision, one run against the old Maia image and all three runs against the threaded image produced `a3b68e0b74b88ded431bcee3cb5abeb587da28ac02ba3d13a03d981bb1c79b2b`. This isolates service concurrency and proves that it did not change Game Review facts or ordering.

All five final persistent-session runs produced `5299a68fd51ea13bea9518b01f7dccd20cb1f28bc303fe1ca5869bcdde8f8db2`. This differs from the fresh-process digest because a warm transposition table can change exact numeric evaluations at a fixed depth. The accepted six-case live corpus reported zero differences under the existing 15-centipawn tolerance while still requiring exact best moves, ranks, categories, selected plies, and provenance.

A 60-sample pooled cancellation probe terminated a 66-ply import after all eight Stockfish processes started. The CLI exited in 1.81 milliseconds median, 22.55 milliseconds p95, and 36.54 milliseconds maximum. All eight engine PIDs disappeared in 4.74 milliseconds median, 305.78 milliseconds p95, and 355.85 milliseconds maximum, below the five-second cleanup budget.

At the new medians, 67 sequential Stockfish calls model to 18.738 seconds. The 66 Maia calls model to 1.919 seconds at Elo 1200, but most of that time overlaps Stockfish. The gap between the 18.738-second Stockfish model and the observed 24.786-second import includes position-dependent search variation, runtime checks, JSON transport, PGN and contract work, and rule extraction. It is not all idle overhead.

### Reported skill run

This attribution uses the reported total of roughly 900 seconds and the measured cached-cold fact path.

| Phase                                               | Wall time | Share of reported total |
| --------------------------------------------------- | --------: | ----------------------: |
| Cached Maia startup                                 |   6.380 s |                    0.7% |
| `importGame` through `gameImported`                 |  24.786 s |                    2.8% |
| CLI bootstrap and measurement difference            |   0.537 s |                    0.1% |
| Language Layer and unrecorded conversation overhead | 868.297 s |                   96.5% |

The last row is a residual, not a direct hosted-model timing. It rules out chess providers as the cause of the 15-minute observation. It does not distinguish model inference, agent reasoning, tool scheduling, draft repair, validation, or human wait. A future run needs host timestamps around each agent turn to make that split.

## Planning constants

Use these values for later tickets on the Game Review performance map:

- Local 66-ply warm fact collection: 24.8 seconds at the JSONL command boundary, or 25.3 seconds including warm process setup.
- Local 66-ply cached-cold fact collection: 31.7 seconds.
- Cached Maia startup: 6.38 seconds. Image pull and model provisioning are separate installation costs and were zero in this run.
- Per-Position Stockfish depth-16 planning median and p95: 280 ms and 685 ms.
- Per-Position Maia planning median: 29 ms near Elo 1200. Treat 281 ms as the observed maximum until a larger run replaces it.
- Provider call count for a 66-ply resignation: 67 Stockfish and 66 Maia.
- Eight Stockfish sessions divide 67 calls into three groups of nine and five groups of eight. The 11.508-second median sum is about 1.44 seconds per slot before tail imbalance and pipeline costs.
- Bounded-engine 66-ply baseline: 9.687 seconds median inside the event-reported pipeline and 10.078 seconds median including release-process startup.
- Current four-wide Maia 66-ply warm fact collection: 6.182 seconds median inside the event-reported pipeline and 6.559 seconds median including release-process startup.
- Current persistent-session 66-ply warm fact collection: 4.595 seconds median inside the event-reported pipeline and 5.011 seconds median including release-process startup.
- Current bounded 84-ply warm fact collection: 10.092 seconds median inside the event-reported pipeline and 10.472 seconds median including release-process startup.
- Current eight-session Stockfish per-call median: 137 milliseconds across the five persistent-session runs. The separate Maia phase completes 60 isolated requests in 2.418 seconds at width four and 24.81 requests per second.
- Pooled cancellation: 22.55 milliseconds p95 for CLI exit and 305.78 milliseconds p95 for all eight Stockfish PIDs to disappear, against a five-second cleanup budget.

The current implementation meets the map's local requirement of well under 60 seconds and its 3-to-8-second provider target for this 66-ply Game on the certified local runtime. Deterministic Stockfish session reuse adds about 1.6 seconds of median headroom below the target ceiling. Timing provenance remains useful because it prevents another Language Layer delay from being misdiagnosed as provider time.

## Rerun commands

Primitive distributions:

```sh
cargo run -p chen-chess-coach-api --bin measure_review_session_primitives -- \
  --stockfish ~/.local/share/chenchess/units/0.1.0-local-coach.3/bin/stockfish \
  --depths 12,14,16,18 \
  --stockfish-repeats 3 \
  --cancellation-repeats 20 \
  --maia-base-url http://127.0.0.1:38271 \
  --maia-repeats 10 \
  --maia-concurrency 4
```

Warm process-level import using the checked-in 66-ply fixture:

```sh
cargo run -p chen-chess-coach-api --bin measure_review_session_primitives -- \
  --skip-stockfish \
  --review-command target/release/chenchess \
  --review-pgn backend/evaluation/fixtures/Synthet1/game-66ply.pgn \
  --review-elo 1246 \
  --review-side black \
  --review-repeats 3
```

Replace `game-66ply.pgn` with `lichess-export.pgn` to rerun the 84-ply acceptance case.

When measuring a development binary, set `STOCKFISH_PATH` to the pinned installed binary and `MAIA_BASE_URL` to the running pinned service. The harness includes each run's event-reported provider timing in `reportedTimingByRun`.

For the cached-cold measurement, run `chenchess runtime maia-stop` first. Do not remove the image or model volume. Time `chenchess runtime maia-start` separately to isolate container and cached-model startup.
