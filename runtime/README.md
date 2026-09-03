# Local Pipeline Runtime

The Local Pipeline Runtime packages the Rust CLI, Stockfish, the Maia-2 Python service, and Maia model storage as one versioned user installation. Docker is the only host runtime prerequisite; Maia-2 and PyTorch stay inside the published container.

Player-facing installation, privacy, resource, limit, and recovery guidance is in [`docs/local-coach.md`](../docs/local-coach.md). Runtime and container license obligations are collected in [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).

## Publish a runtime manifest

After the local proof passes and `main` is pushed, manually run the **Publish
Maia runtime** GitHub Actions workflow with the complete unit version:

```sh
gh workflow run publish-maia-runtime.yml \
  --repo <owner>/<repo> \
  --ref main \
  -f unit_version=<version>
```

It builds `services/maia/Dockerfile` for Linux amd64 and arm64, publishes
`maia-runtime`, and uploads `runtime-manifest.json` with the
resulting multi-platform image digest. The workflow also pins the official
Stockfish 18 Apple Silicon archive and checksum from
`manifest.template.json`.

Publication is not certification. The proof that runs a published runtime end
to end on an Apple Silicon host, and the certification report it writes, are
release machinery this snapshot does not carry.

The template is deliberately not installable: its image field is replaced only after the registry returns an immutable digest. Runtime-manifest schema v3 records the Maia-2 package (`maia2==0.11.0`) and model type (`rapid`) independently. Installation and `doctor` verify the actual package installed in the immutable image, both unchanged artifact hashes, and separate package/model markers inside the model volume.

## Install and diagnose

```sh
chenchess runtime install --manifest /path/to/runtime-manifest.json
chenchess runtime doctor
chenchess runtime maia-status
chenchess runtime maia-stop
chenchess runtime maia-start
```

The default per-user locations are:

- CLI link: `~/.local/bin/chenchess`
- Codex skill link: `~/.agents/skills/chenchess-coach`
- Claude Code skill link: `~/.claude/skills/chenchess-coach`
- configuration: `~/.config/chenchess/runtime.json`
- versioned units and provider assets: `~/.local/share/chenchess/`
- runtime state: `~/.local/state/chenchess/`

Installation rejects unsupported targets, mutable image tags, incompatible schemas, and checksum failures before activation. It explicitly pulls the pinned Maia image, provisions the versioned `rapid` model volume, starts the chenchess-labelled service, and waits for health before reporting success. `doctor` verifies the installed hashes, Stockfish UCI handshake, Docker/image/model readiness, schema version, Maia health, and exact provider identities.

The installer stores one canonical `chenchess-coach` skill inside the versioned runtime unit and links both host discovery paths to it. The embedded copy does not refer to the source checkout. It uses the same managed CLI in Codex and Claude Code, leaves command approval to the host, and removes temporary Review Session events and draft JSON after each review.

## Run installed reviews

Installed `review-session --jsonl` always uses the active runtime unit. It rejects `STOCKFISH_*`, `MAIA_BASE_URL`, and Maia model environment overrides. The first `importGame` command starts the provisioned Maia container if needed and leaves it running for warm reuse. Reviews never pull images or download model files.

Only one live review or live Pipeline Evaluation may own the runtime. A concurrent caller exits with a `runtime busy` diagnostic. SIGINT and SIGTERM cancel a review, terminate its Stockfish process, release the runtime, and leave Maia running.

The v1 Game limit is 400 plies. Set `CHENCHESS_MAX_GAME_PLIES` to a positive whole number only for development exercises. The CLI rejects invalid or oversized PGN before it starts a provider. Live evaluation tolerances, time budgets, progress cadence, and the Apple Silicon certification command are documented in `services/coach-engine/evaluation/README.md`; each certification report records the compiled limits it used.
