# ADR 0007: Maia-2 as a separate HTTP runtime

## Status

Accepted.

## Decision

Run the official [CSSLab Maia-2](https://github.com/CSSLab/maia2) Python package as a separate, self-hosted service. Pin the package to `maia2==0.11.0`, default to its `rapid` model on CPU, and expose a small private-network HTTP contract:

```text
POST /v1/predict
{ position, playerElo, opponentElo, limit }

200
{ moves: [{ uci, probability }], winProbability }
```

The Rust `HumanMoveModel` interface remains provider-neutral. `MaiaHttpAdapter` maps the per-review Elo Profile to both Maia-2 self and opponent Elo for MVP because the product does not yet ask for an opponent profile. It sorts probabilities, assigns ranks, and converts transport, non-success status, malformed JSON, and invalid candidate data into recoverable adapter errors.

The Python service loads `model.from_pretrained(type="rapid", device="cpu")`, prepares position-wise inference once, and calls `inference.inference_each` with FEN and Elo values. These APIs follow the official Maia-2 position-wise inference example.

## Bounded concurrency refinement (2026-07-18)

The private service uses Python's threaded HTTP server and caps prediction inference at four concurrent calls against one shared model. It caps PyTorch CPU intra-operation parallelism at two threads and inter-operation parallelism at one. The `/v1/predict` request and response stay unchanged.

The full-Game pipeline sends the Maia-only phase four requests at a time after its Stockfish phase completes. The service does not add a batch endpoint because no current adapter calls one, and bounded concurrency removes the measured single-request bottleneck without creating a second contract. The measurement harness compares every concurrent response with its serial response and fails on any drift.

Runtime unit `0.1.0-local-coach.4` publishes this service as `maia-runtime@sha256:66007c96d3aeed8fc5f1816611c1cfe0b1d74aa0943350f8dd96cf25bb550298`. The digest-pinned manifest passed isolated install, doctor, live certification, rollback, and cleanup; the recorded evaluation corpus carries the same image provenance.

## Inference correctness upgrade (2026-07-19)

Upgrade the service package from Maia-2 `0.9` to `0.11.0`. Releases through `0.10.0` incorrectly flattened the two-dimensional indices returned by position-wise legal-move masking, which could leak vocabulary index zero (`a1h8`) as a zero-probability candidate. Maia-2 `0.11.0` selects indices from the single position row and uses masked softmax, so position-wise inference returns only legal moves.

The upstream rapid checkpoint and model-architecture config retain their existing SHA-256 values. Runtime-manifest schema v3 separates the package identity (`maia2==0.11.0`) from the model type (`rapid`) and retains the independent model/config artifact hashes. Installation and `doctor` verify all four identities against the immutable image and model volume. Maia-2 `0.11.0` packages the architecture config instead of downloading it beside the checkpoint, so provisioning copies that package resource into the model volume before verifying the unchanged config digest.

Runtime unit `0.2.0-local-coach.4` publishes the schema-v3 service as `maia-runtime@sha256:ab3b6dc16b75c3602f2e6c4002dc0f99ef77c8c042641cffea66fc1c23482972`. Its health response, runtime configuration, evidence provenance, evaluation fixtures, and installed volume markers report the package and model as separate identities.

## Packaging

- `maia-service/Dockerfile` owns Python and PyTorch dependencies.
- `docker-compose.yml` runs Maia on the private Compose network and points Rust at `http://maia:8080`.
- A named volume mounted at `MAIA_MODEL_DIR=/models` preserves the downloaded model and config.
- The first start requires network access to download pretrained weights. Later starts reuse the cache.
- The provided image installs PyTorch's CPU wheel. GPU deployments require a GPU-enabled PyTorch image/wheel plus the appropriate container device/runtime configuration before setting `MAIA_DEVICE=gpu`.
- The service has no public port or authentication in Compose. Central Host deployments must keep it private or add network-layer authentication.

## Consequences

Maia-2 remains independently deployable and replaceable, and Rust does not acquire Python/PyTorch dependencies. Model loading can be slow and failures remain possible, so the backend must treat the adapter as recoverable rather than making Maia health a startup invariant.
