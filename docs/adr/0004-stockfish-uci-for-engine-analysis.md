# Stockfish UCI for Engine Analysis

ChenChess will use a backend-managed Stockfish UCI process as the MVP Engine Analysis provider. Stockfish gives the Game Review an objective, self-hostable source of chess truth, while Maia remains the Human Move Model and the LLM Explainer remains prose-only.

The Rust `EngineAnalyzer` interface accepts a selected FEN position and returns provider-neutral best-move, evaluation, principal-variation, and completed-depth data. Centipawn and mate scores use the side-to-move perspective of the analyzed FEN. The Stockfish adapter owns UCI startup, readiness checks, analysis, output validation, timeout, shutdown, and process errors.

Each analysis uses an isolated Stockfish process. This avoids shared mutable UCI state and makes crashes local to one recoverable request. The later Game Review Orchestrator owns analysis concurrency and candidate selection; it can revisit process pooling if measured startup overhead becomes material.

Local Nix development includes Stockfish. The Rust production image downloads
the official Stockfish 18 generic and AVX2 Linux archives, verifies every
pinned archive and binary SHA-256 digest, and installs its GPL notice beside
the backend. At container start it selects AVX2 only when `/proc/cpuinfo`
advertises the instruction and otherwise retains the generic
`STOCKFISH_PATH=/usr/local/bin/stockfish` fallback. `STOCKFISH_DEPTH` defaults
to 16 when an adapter path is configured. Runtime provenance records the
selected binary digest and rejects cached evidence produced by a different
binary or search configuration.
