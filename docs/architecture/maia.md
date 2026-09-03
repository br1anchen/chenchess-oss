# maia (services/maia)

Maia-2 human-move inference behind a minimal private HTTP service (ADR 0007).
Coach Engine's `MaiaHttpAdapter` is its only client.

```text
app.py
  ThreadingHTTPServer
    parse_request        # {position FEN, player_elo, opponent_elo, limit}
    BoundedSemaphore(4)  # MAX_CONCURRENT_PREDICTIONS
    torch: 2 intra-op / 1 inter-op threads
    → ranked move probabilities + RuntimeIdentity {package, model}

model_artifacts.py       # prepare/finalize pinned model files (maia2==0.11.0, "rapid")
```

Design choices worth keeping:

- **Stdlib HTTP server, no framework** — the surface is one prediction
  endpoint; dependencies stay at torch + maia2.
- **Pinned runtime identity** — package and model versions are reported with
  every prediction so evaluation fingerprints (ADR 0049) can detect drift.
- **Bounded concurrency** — the semaphore and thread caps keep one container
  predictable on shared Railway CPU.

Locally it runs as a Docker container managed by the `chenchess` CLI's
runtime manager (`local_runtime/docker.rs`); in deployment it is a private
Railway service reached at `maia.railway.internal`.
