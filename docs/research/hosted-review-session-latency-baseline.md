# Hosted Review Session latency baseline

> **Baseline date:** 2026-07-28  
> **Instrumentation added:** 2026-07-29 for issue #164  
> **Benchmark:** canonical `Synthet1` Review Session, reviewed as Black  
> **Privacy:** timings, counts, byte sizes, opaque operation handles, and a
> generated trace handle only

This is the before-state for the performance work under #157. It keeps the
observed hosted timings separate from first-party measurements: ChatGPT and
Claude do not currently expose every model-planning and token-generation
boundary to an MCP server.

## Observed hosted baseline

The shared ChatGPT run used a 65-position review and the then-current
six-moment result. The canonical fixture now produces seven moments, so future
comparisons must use all seven and must not silently treat these historical
samples as the new scenario population.

Each row below has one observed hosted sample. Therefore p50, p95, p99, and max
are the same value; the distribution is deliberately reported as sparse rather
than implying statistical confidence.

| Scenario                                      |   n |  p50 |  p95 |  p99 |  max |
| --------------------------------------------- | --: | ---: | ---: | ---: | ---: |
| Initial review and Critical Moment frame      |   1 |  76s |  76s |  76s |  76s |
| Recovery after invalid publication            |   1 |  51s |  51s |  51s |  51s |
| Discuss `12.Ne2`                              |   1 |  53s |  53s |  53s |  53s |
| Discuss `15.f3`                               |   1 | 100s | 100s | 100s | 100s |
| Show all Critical Moments from a warm process |   1 |  22s |  22s |  22s |  22s |
| Simple material-balance follow-up             |   1 |  75s |  75s |  75s |  75s |

The initial backend trace explained about 34.4 seconds of the 76-second turn:

| First-party stage                         | Observed value |
| ----------------------------------------- | -------------: |
| Review facts, 65 cached evaluations       |          611ms |
| Maia work, concurrent sum                 |        2,330ms |
| Maia median / maximum                     |    33ms / 60ms |
| Game import persistence after facts       |        4,315ms |
| Six initial intent contexts               |       12,699ms |
| Initial checkpoint after domain creation  |       14,182ms |
| Initial checkpoint writes                 |            360 |
| Latest observed mutation persistence tail |        7,649ms |

The 22-second warm “show all” path performed no Stockfish, Maia, or Firestore
write. It is evidence that host planning, redundant tool work, transfer, app
startup, and rendering need independent spans.

## Payload and bundle baseline

The historical payload measurements were:

| Artifact                       | Before-state size |
| ------------------------------ | ----------------: |
| Coach App HTML                 |   1,185,893 bytes |
| Coach App HTML, gzip           |     468,736 bytes |
| Game Import Firestore document |     514,297 bytes |
| Review Session root document   |     515,577 bytes |
| Six moment documents           |     151,636 bytes |
| 353 evidence documents         |  ~1,144,518 bytes |
| Initial checkpoint REST body   |            ~1.8MB |

The instrumented build is 1,188,721 raw bytes and 470,348 gzip bytes. The build
now prints an attribution report for:

- MCP Apps runtime;
- React and React DOM;
- `react-chessboard`;
- `chessops`;
- motion;
- shared UI;
- selector, graph, arrows, and handoff code;
- inline CSS;
- branded and other inlined image assets;
- uncategorized application and HTML overhead.

The report accounts for every raw HTML byte. Compressed category values are
proportional estimates; only the whole-resource raw and gzip values are exact.
Runtime parse and first-execution cost comes from the app performance marks,
not a bundle-size proxy.

## Model tool-surface comparison

The original #157 hosted observation recorded 11 model-visible tools. Its
baseline report enabled catalog byte telemetry but did not retain one concrete
catalog-byte sample. To keep the contraction comparison reproducible at the
current MCP schema generator version, issue #178 normalized that 11-tool
before-state from the exact pre-contraction catalog at commit `5fc802a81018`:
exclude only the two compound tools introduced during the migration and
measure the current equivalents of the original 11 definitions.

| Surface                                  | Model tools | Input schema bytes | Description bytes |
| ---------------------------------------- | ----------: | -----------------: | ----------------: |
| #157 normalized 11-tool baseline         |          11 |             30,744 |             3,244 |
| #177 migration bridge before contraction |          13 |             32,364 |             3,461 |
| #178 contracted user-intent surface      |           7 |              4,597 |             2,271 |
| #178 legacy visibility rollback          |          13 |             32,364 |             3,461 |

The contracted surface removes 4 of the original 11 model choices and 85.0% of
their normalized input-schema bytes. The final seven choices are `review_game`,
`start_review_session`, `discuss_review_moment`,
`publish_review_moment_comment`, `evaluate_player_plan`,
`resume_review_session`, and `render_move_sequence`. Telemetry now measures
input and output schemas separately and reports their sum only for definitions
whose MCP Apps visibility includes `model`; registered app-only compatibility
and control tools do not inflate that metric.

Strict output contracts add real catalog cost, so outcome profiles omit
terminal variants that a tool cannot return. The contracted catalog measured
61,833 output-schema bytes after this profiling, down from 75,956 bytes for the
first strict-union implementation. Treat that value as an observation rather
than a pinned contract; compare it through catalog telemetry whenever tool
selection latency changes.

The app-only handlers remain registered for historical cards and in-flight
operations. They are app-only on every deployment; the visibility rollback
switch this baseline was measured against has since been retired.

## Trace contract

One generated `trace:review-session:<uuid>` handle correlates boundaries without
embedding a Player, Game, session, or provider identifier.

| Boundary    | Captured measurements                                                                                                                                                                                          |
| ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Node MCP    | request admission, authentication, handler dispatch, handler return, caller kind, normalized failure, retry, timeout, result bytes, input/output tool-schema bytes, description bytes, resource raw/gzip bytes |
| Node → Rust | request bytes, first response byte, terminal event, response bytes, total time, retry and server-compound classification                                                                                       |
| Rust        | command total and response bytes, queue wait/depth, engine lease occupancy, worker queue, cache hit/miss/coalescing counts, per-moment authoring time                                                          |
| Providers   | Maia request/response bytes, candidate count, wall time; existing Stockfish cache/batch spans                                                                                                                  |
| Firestore   | transport wall time/status/response bytes, commit body bytes/write count, checkpoint read/write document counts and encoded bytes                                                                              |
| App         | boot, connection, host result, redraw projection, skeleton, meaningful frame, picker, board, arrows, graph, handoff, app tool calls, model-context updates, resource bytes, viewport/motion/host category      |

Host message arrival, host model tool-selection time, and assistant first/final
token time remain `null` until a host exposes them. Server handler dispatch is
reported separately and must not be presented as model tool-selection latency.
Every MCP completion explicitly records `latencyScope=mcp_request_only`,
`hostModelPlanningMeasured=false`, and
`hostResponseGenerationMeasured=false`.

### Diagnose a slow hosted reply

Use the same prompt on both hosts only as a symptom comparison; then correlate
each run with its ChenChess trace.

1. Compare the Player-observed wall time with `mcp.return`. A large gap when
   `mcp.return` is small is outside the ChenChess request clock: host model
   planning before `tools/call`, host scheduling, or response generation after
   the result.
2. If `mcp.return` is high, subtract `mcp.toolRequestReceived` from
   `mcp.handlerSelected` to estimate Central Host dispatch, then subtract
   `mcp.handlerSelected` from `mcp.return` to estimate handler execution.
3. Inspect the correlated `engineCalls`. A large first-byte or total time
   assigns the delay to Coach Engine or its downstream providers. Multiple
   calls with `callerKind=server-compound` are intentional orchestration and
   expose their individual cost.
4. Compare cold and warm samples. A cold-only first-byte spike points to
   process/provider startup. Similar warm MCP traces with a much slower
   ChatGPT wall time than Claude point to the ChatGPT host/model path, not a
   slower MCP response.

Every telemetry input is allowlisted or structural. The collector rejects a
trace containing fields named `pgn`, `fen`, `playerText`, `evidence`,
`providerPayload`, `firestoreDocument`, credentials, passwords, or tokens.
Telemetry must never contain chess content, raw provider bodies, Firestore
documents, OAuth material, email addresses, or copied conversation text.

## Reproduce

1. Build the exact iframe and retain the printed attribution:

   ```bash
   bun --cwd apps/coach-app run build
   ```

2. Run each benchmark against the deployed Node and Rust revisions and save
   their stderr logs as newline-delimited JSON. Use only the generated
   telemetry events; do not export request bodies, provider bodies, database
   documents, or host conversation content.

3. Aggregate named scenarios:

   ```bash
   bun run review-session:baseline -- \
     --input warm-initial=/path/to/warm-initial.ndjson \
     --input cold-initial=/path/to/cold-initial.ndjson \
     --input warm-restore=/path/to/warm-restore.ndjson \
     --input cold-restore=/path/to/cold-restore.ndjson \
     --input warm-show-all=/path/to/warm-show-all.ndjson \
     --input cold-show-all=/path/to/cold-show-all.ndjson \
     --input warm-follow-up=/path/to/warm-follow-up.ndjson \
     --input cold-follow-up=/path/to/cold-follow-up.ndjson \
     --output /path/to/review-session-baseline.json
   ```

The report emits count, p50, p95, p99, and max for every observed stage and byte
metric, plus duplicate-call, cancellation, cancellation-effectiveness, timeout,
and render-fallback rates. An empty or sparse stage stays visibly sparse.

Use the sanitized repository recording under
`services/coach-engine/evaluation/fixtures/Synthet1/`; never copy the shared
host conversation into a fixture or telemetry file.

For every applicable scenario, record warm and cold Node/Rust processes,
ChatGPT and Claude, desktop and constrained mobile viewports, normal and
reduced motion, successful rendering, and an intentionally failed app resource
load. At minimum exercise:

- initial seven-moment review with warm and cold engine caches;
- warm “show all” and restore after process restart;
- one automatic-moment discussion;
- one conversational plan discussion with an optional Player Plan Evaluation;
- one alternative plus Coach Turn;
- app selection, graph selection, handoff, and read-only transition;
- plain-text fallback when the app cannot load.

Host-level elapsed time is recorded beside, not merged into, the correlated
first-party trace.

## Provisional budgets

These are comparison budgets for later #157 slices, not claims that the
before-state passes.

| Boundary or event                           | Provisional p95 |
| ------------------------------------------- | --------------: |
| Local input feedback                        |           200ms |
| Skeleton/loading state                      |           500ms |
| Meaningful first content                    |            1.5s |
| Warm Review Manifest read                   |           300ms |
| App result to first meaningful paint        |           500ms |
| App result to interactive picker            |            1.5s |
| Synchronous mutation persistence            |            1.5s |
| Warm manifest after game facts              |            1.5s |
| ChenChess-owned simple intent exchange      |              5s |
| User-visible “show all” on a supported host |              5s |
| User-visible simple follow-up               |             15s |
| Initial Review Manifest / one moment detail |       100KB raw |
| Incremental update                          |        25KB raw |
| Coach App entry resource                    |      150KB gzip |

Before optimization, the hosted initial turn is 5.1× the 15-second simple
follow-up target, “show all” is 4.4× its 5-second target, mutation persistence
is 5.1× its 1.5-second target, and the iframe is 3.1× its 150KB gzip target.

Issue #180 reviewed the post-optimization resource attribution. The official
MCP Apps runtime plus React accounts for about 151.5KB gzip before ChenChess
product behavior, so the 150KB row remains a visible product objective rather
than a release gate. The #180 build was about 244KB gzip and reported the
150KiB objective as unmet.

This is not permission for silent growth. `verifyBundle.ts` verifies artifact
integrity, `reportBundle.ts` reports the exact size and provisional objective,
and the exact deployed resource size enters staging certification as an
observation. The measurement remains reviewable and meeting the original
150KiB objective remains desirable, but neither value blocks a production
release.
