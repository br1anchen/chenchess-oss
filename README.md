# ChenChess

A chess coach that never invents a move. A Player imports a finished Game;
Coach Engine analyses every position with Stockfish and a Maia-2 human-move
model, selects Critical Moments with a deterministic rule extractor, and
publishes coaching through one validation boundary — so every claim in the
prose is traceable to evidence a chess engine produced.

It is also a WebMCP application: the Coaching Board publishes its own tools to
whatever model is looking at the page, so a model can drive the same board you
are looking at.

> **This repository is a snapshot.** One squashed commit, no history, published
> so the work can be read and run. Development continues privately. Issues are
> off; see [`CONTRIBUTING.md`](CONTRIBUTING.md) for what to do instead.
>
> It is also a *subset*. This snapshot is the self-hostable product: Coach
> Engine, the Coaching Board, and the local stack that runs them. The hosted
> deployment's own parts are not here — no deployment configuration, no
> operator runbooks, no release machinery, no OAuth authorization server, and
> no remote MCP endpoint for third-party model hosts. What is here runs
> entirely on your machine.

## Run the whole thing on your machine

No account, no cloud project, no service-account key. A Firebase
Authentication clone and a Firestore clone run locally, and Coach Engine accepts
the emulator's unsigned tokens only while the emulator address resolves to
loopback (see [ADR 0060](docs/adr/0060-develop-against-local-firebase-emulators.md)).

You need [Nix with flakes](https://nixos.org/download) and Docker. Docker is
only for the Maia-2 human-move model, whose PyTorch runtime does not belong in
a checkout.

```bash
./tooling/nix-develop
bun install --frozen-lockfile
bun run local:up          # five processes, health-gated, ~2 minutes cold
```

Then, in a second terminal inside the same shell:

```bash
bun run local:seed        # creates the Player, grants Beta Access, imports a Game
```

Sign in at <http://127.0.0.1:4173/login> with the address and password the seed
prints. `local:seed` is safe to run again, and you want it after every
`local:up`: the Auth emulator restores its accounts from its export, but the
Firestore emulator's export carries only the default database and this stack
runs on a named one, so a restart comes back with your Player and without their
Games.

To drive the API directly, the seed prints a ready command:

```bash
AUTH_TOKEN=<the token it printed> bun run smoke:local
```

### What is running

| Port   | Process                       |
| ------ | ----------------------------- |
| `4173` | Central Host — the web origin, and the `/api` relay |
| `8787` | Coach Engine — the Rust application service |
| `8080` | Maia-2 human-move inference (Docker) |
| `9099` | Firebase Authentication emulator |
| `8081` | Firestore emulator |

Stockfish runs as a child process over standard input and output, so it has no
port.

`bunfig.toml` rejects packages published in the last seven days
(`minimumReleaseAge = 604800`), so a fresh same-day release fails install with
"no version matching" until the window passes.

## What is in here

- `apps/central-host/` — the Node composition layer and Vite/Astro project. It
  serves the static public pages, the sign-in surfaces, and the authenticated
  Coaching Board, and relays `/api` to Coach Engine.
- `services/coach-engine/` — the private Rust application service. It verifies
  Firebase and Coach access tokens and owns Game Imports, the transient Review
  Sessions above them, authorization, Stockfish orchestration, and application
  Firestore data.
- `services/maia/` — Maia-2 position inference behind a private HTTP service.
- `packages/coach-engine-sdk/` and `packages/ui/` — the Rust-generated command
  contract and the host-neutral shared presentation layer.

Per-module diagrams are in [`docs/architecture/`](docs/architecture/README.md),
the domain vocabulary in [`CONTEXT.md`](CONTEXT.md), and the decisions behind
both in [`docs/adr/`](docs/adr/).

The runtime keeps every chess claim grounded:

1. Import PGN.
2. Analyse every position through the Stockfish and Maia Model Adapters.
3. Select Critical Moments through the deterministic Rule Extractor.
4. Persist the normalized import under an opaque Game Import ID.
5. Start a Review Session over that durable import — transient, keyed by Player
   and Game Import, with nothing to resume — and publish coaching only through
   the Coach Engine validation boundary.

## Developing

```bash
bun run test        # Rust, frontend, Maia, and the repository's own scripts
bun run check       # typecheck
bun run lint
bun run build
```

`build` needs no environment. Central Host serves itself from
`http://127.0.0.1:5173` unless `PUBLIC_SITE_ORIGIN` names another origin, and
an instance that names one is also the one whose public pages become
indexable.

Build Rust through `./tooling/cargo-cached`, never a bare `cargo`: the build
cache only sees prefixed invocations, so a bare `cargo` silently builds
uncached. `./tooling/nix-develop .#vanilla` is the same shell with the cache
kept off `PATH`. See [`docs/rust-build-cache.md`](docs/rust-build-cache.md),
and [`docs/local-coach.md`](docs/local-coach.md) for the `chenchess` CLI that
reviews a Game without the web app.

`bun run keys:test` generates the RSA pair Coach Engine's tests sign with. It
is generated rather than committed, and `turbo run test` runs it for you.

`bun run sweep:targets` reclaims Cargo target space across this checkout and
its sibling worktrees; `--dry-run` first.

## API

Authenticated commands use `POST /api/v1/review-session/commands` with one JSON command envelope. The response is NDJSON and ends in exactly one typed terminal event.

```json
{
  "requestId": "request:<fresh>",
  "operationId": "operation:<fresh>",
  "surface": "web",
  "command": {
    "kind": "importGame",
    "source": {
      "kind": "pastedPgn",
      "pgn": "[Event \"Example\"]\n[Result \"0-1\"]\n\n1. f3 e5 2. g4 Qh4# 0-1"
    },
    "reviewSide": { "kind": "selected", "reviewSide": "both" },
    "eloProfile": { "kind": "playerProvided", "rating": 1450 }
  }
}
```

The completion contains the opaque durable import handle and the grounded
review:

```json
{
  "event": {
    "kind": "completed",
    "result": {
      "kind": "gameImported",
      "gameImportId": "game-import:<opaque>",
      "review": {
        "criticalMoments": [],
        "positionViews": [],
        "evaluationTimeline": [],
        "learningPlan": {
          "selectionPolicyVersion": "learning-plan-selection/v1",
          "resourceCatalogVersion": "learning-resources/2026-07-25",
          "tracks": []
        }
      }
    }
  }
}
```

Send `startReviewSession` with the returned `gameImportId`. The generated
schema, fixtures, decoder, and TypeScript types are published by
`@chenchess/coach-engine-sdk`.

A Player holds at most one in-flight Coach Turn per `gameImportId`. The scope
spans every conversation over that import, so a `startCoachTurn` sent from a
second chat on the same imported game while a turn is running terminates in a
`conflict` event with reason `coachTurnAlreadyActive`, whereas a turn on a
different `gameImportId` runs concurrently. Steering — a `startCoachTurn`
carrying the running turn in `priorTurn` — replaces that turn instead of
conflicting.

A Review Session is transient: no identifier, no durable record, nothing to
resume. A command whose session has been evicted is rejected with
`unknownSession` and succeeds on a plain retry, which rebuilds it from the
durable import.


## Licence

[AGPL-3.0-or-later](LICENSE) for the code. The ChenChess **name** is not
covered by it, and the seal-derived marks and wordmark logos are not in this
repository at all — the application icons here are plain placeholder geometry.
The board and brush textures, the chess pieces and the coaching-value icons are
granted separately under CC BY 4.0, so a fork can keep the visual system and
put its own name on it. The details are in [`TRADEMARKS.md`](TRADEMARKS.md).
