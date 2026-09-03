# Plan 002: Restructure to `apps/` + `services/` + `packages/`

## Status

- **State**: TODO
- **Priority**: P2
- **Effort**: L
- **Risk**: MED
- **Depends on**: nothing blocking. Phase 0 can start immediately.

Ref: https://turborepo.dev/docs/guides/tools/rust
Follows the Turborepo migration (landed — turbo 2.10.5).

> Cross-references to `.claude/turborepo-migration-plan.md` and
> `.claude/architecture-simplification-plan.md` below point at the **untracked** local `.claude/`
> working directory, per Plan 001. They are context for the author, not resolvable from a clean
> checkout.

Directive: **no root-level packages unless necessary**; organise by function and domain;
prioritise clean seams; **no declared dependency edges that nothing verifies**. Convex is out of
scope (being phased out).

## What the two runtime files actually are

Asked directly, because the answer decides their placement.

**`THIRD_PARTY_NOTICES.md`** — a legal compliance artifact, not config. It collects the license
obligations of the bundled third-party components: Stockfish 18 (GPL v3), Maia-2 and its
`rapid_model.pt` (MIT, CSSLab), the digest-pinned Python 3.11 base image (PSF), and the PyTorch
dependency tree. Its own opening line says the notices _"travel with the installed runtime and
inside the Maia container"_ — so it ships to two destinations and is owned by neither.

**`manifest.template.json`** — the pinned description of one distributable runtime unit: Stockfish
version + archive URL + SHA + which member of the tarball is the binary, the Maia image, the
`maia2==0.11.0` package, the model and config SHAs, and the port. The publish workflow fills in
`unitVersion` and the image digest and emits a concrete manifest.

## You are right that this should not sit under `local_runtime` — and the evidence is sharper than the intuition

Today the manifest is **decisively local-only**, and not by accident:

```json
"target":    { "os": "macos", "arch": "aarch64" },
"stockfish": { "url": ".../stockfish-macos-m1-apple-silicon.tar",
               "binaryMember": "stockfish/stockfish-macos-m1-apple-silicon" }
```

Central hosting shares **none** of it. `docker-compose.yml` and the Railway configs build Maia from
`maia-service/Dockerfile` rather than pulling the published digest-pinned image, and the API image
installs Stockfish with `apt-get install stockfish`. Grepping `docs/central-hosting.md`, the compose
file, and all three Railway configs turns up zero references to the manifest, to `ghcr.io`, or to
the published image.

So "we probably use the same config for the remote runtime as well" is a **direction, not the
current state** — and that is precisely the argument for not burying these files under
`apps/api/src/local_runtime/`. Doing so would harden a coupling you are planning to break, and it
would make the api crate the owner of something intended to outgrow it.

Equally, I should not invent the shared local/remote config structure now, because it does not
exist yet and I would be guessing its shape.

**So `runtime/` stays at root, unchanged, pending that decision.** It is configuration and legal
content, not a package, so it does not violate the no-root-packages rule — same class as `docs/`.
The two files stay together because they describe the same thing: what is in the distributed unit,
and what that obligates legally.

The related code smell — the module being called `local_runtime` while owning
`RUNTIME_MANIFEST_SCHEMA_VERSION`, `RuntimeManifest`, `RuntimeTarget`, `StockfishPackage`,
`MaiaPackage` — is real, and is a **code** question, not a directory question. Noted as a follow-up
below rather than pre-empted by a file move.

## `skills/` stays top level

Placed as you asked, with one consequence stated once and not belaboured.

`skill.rs:7-24` `include_str!`s the four markdown files and `install()` writes them into the user's
skill directory — the mechanism ADR 0012 requires so the installed skill survives the checkout
disappearing. So a compile-time edge to the api crate exists **in the code today**, whatever the
directory layout says.

Per the Rust guide, _"external files require manual declaration via `env` and `inputs`"_ — turbo
will not hash markdown outside the crate. I am **not** adding that `inputs` glob: you are right
that it is an edge nothing verifies, and it would rot silently.

The residual risk is narrow. Cargo tracks `include_str!` through dep-info, so any local
`cargo build` is correct, and `release:proof` builds release binaries fresh. The only exposure is
turbo restoring a cached api artifact after skill markdown changed — on a branch switch or a second
machine. With CI offboarded and one contributor, that is a small, bounded window.

### The embedding is deliberate, and I was wrong to propose removing it

An earlier draft of this plan suggested shipping the markdown as files in the release unit instead
of compiling it in. **Withdraw that.** It was motivated by a turbo caching inconvenience, which is
a weak reason to loosen a version lock. The skill is not documentation — it is the agent-facing
interface to the CLI, and it names the exact surface it drives:

```
chenchess review-session --jsonl
chenchess validate-review --review-event-file … --review-start-event-file … --draft-file …
chenchess validate-practice --review-event-file …
gameImported.review.practiceSelection        (JSON field path)
```

If the markdown and the binary can drift, the agent invokes flags that no longer exist and reads
fields that have moved — a failure that surfaces as incoherent coaching, not a crash. `include_str!`
makes that drift **structurally impossible**: the binary that writes the skill out is the binary the
skill drives. That is the same reason ADR 0012 stages and verifies the whole unit atomically rather
than updating pieces in place.

So the coupling is real and worth keeping. What is genuinely questionable is not the embedding — it
is that **nothing verifies the lock**. `skill.rs:33` hashes the installed files, which detects
tampering after install but says nothing about whether the markdown matches the CLI's actual
argument surface.

**Better fix, and it is the one that answers the earlier objection about unverifiable edges:** add a
drift test in the api crate that parses the flags and subcommands named in the embedded markdown and
asserts each one exists in the clap definition. That converts an edge nothing checks into an edge
that fails loudly, and it guards the risk that actually matters — semantic drift — which a turbo
`inputs` glob never protected against anyway. It also works wherever the markdown lives, so it does
not constrain the directory decision. Follow-up below.

## The seam measurement (do this before believing any layout)

I checked whether the Rust crate's domains are separable today. **They are not.**

```
rule_extractor.rs   -> review_session_contract   (5 refs)
review_facts.rs     -> review_session_contract   (6 refs, + review_session_board)
game_import.rs      -> review_session_contract::*
lichess_import.rs   -> review_session_contract
```

The chess domain depends on `review_session_contract` — `Color`, `GameRef`, `GameReview`,
`PositionSnapshot` are its type vocabulary. The contract is the **bottom** layer, so the only
correct split is `review-contract ← chess-domain ← api`. But that subsystem is exactly what
`.claude/architecture-simplification-plan.md` is actively cutting (7,554 → 6,737 LOC, ongoing).
**The lowest layer is the unstable one**, so the crate split waits.

## Target layout

Projected state after Phases 0–7.

```
apps/
  web/                          chen-chess-coach-web        (was frontend/)
    src/  index.html  vite.config.ts  tsconfig*.json
    components.json  default.conf.template  .env.example
    Dockerfile  railway.json  package.json
  api/                          chen-chess-coach-api        (was backend/)
    src/  tests/  test_support/  certification-fixtures/
    evaluation/                 fixtures corpus comparisons gotham  (was evaluation/)
    .env.example
    Cargo.toml  Dockerfile  railway.json
services/
  maia/                         maia-service                (was maia-service/)
    app.py  test_app.py  requirements.txt  licenses/
    Dockerfile  railway.json  package.json
packages/
  review-session-contract/      @chenchess/review-session-contract
    src/  fixtures/  schema/  package.json
tooling/
  scripts/                      @chenchess/scripts          (was scripts/)
    release-proof.ts  local-smoke.ts  central-topology.test.ts  gotham/
    package.json  tsconfig.json
skills/
  chenchess-coach/              *.md  — generated artifact under follow-up 3
runtime/
  manifest.template.json  THIRD_PARTY_NOTICES.md  README.md
                                pending the local/remote decision (follow-up 1)
docs/  plans/  .claude/  .learn/  .github/
Cargo.toml  Cargo.lock  rust-toolchain.toml
package.json  bun.lock  turbo.json  tsconfig.json
docker-compose.yml  flake.nix  .dockerignore  .gitignore
AGENTS.md  README.md  CONTEXT.md  tickets.md
```

Dissolved from root: `contracts/`, `evaluation/`, `scripts/`, `deploy/`, `convex/`.
Root keeps only buckets, tool-owned config, and documentation — **no package owns source at root**.

`turbo ls` → 6 packages:

| Package                              | Location                           |
| ------------------------------------ | ---------------------------------- |
| `chen-chess-coach-web`               | `apps/central-host`                |
| `chen-chess-coach-api`               | `apps/api`                         |
| `maia-service`                       | `services/maia`                    |
| `@chenchess/review-session-contract` | `packages/review-session-contract` |
| `@chenchess/scripts`                 | `tooling/scripts`                  |
| `chenchess-rust`                     | synthetic (workspace metadata)     |

### What the follow-ups would further change

Not part of the eight phases; listed so the destination is legible.

```
apps/api/src/
  skill.rs                      promoted out of local_runtime/       (follow-up 3)
  skill/templates/*.md          authored templates, crate source     (follow-up 3)
  runtime/                      was local_runtime/                   (follow-up 2)
    manifest.rs                 unit description — the shared-with-remote candidate
    paths.rs                    RuntimePaths — the only genuinely local policy
    installer.rs  activation.rs  docker.rs  process.rs  state.rs  manager.rs
                                host-agnostic: acquire, verify, stage, activate, supervise
skills/chenchess-coach/         becomes generated output             (follow-up 3)
runtime/                        resolved by follow-up 1
```

**`packages/` has exactly one member.** Membership rule: two or more consumers across a language or
deployable boundary. `review-session-contract` qualifies — api generates it, web consumes it.
Nothing else does. The bucket stays because the deferred crate split adds two more.

### Why each directory lands where it does

| Was                                      | Now                                 | Reason                                                                                                                                   |
| ---------------------------------------- | ----------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `contracts/` + `frontend/src/generated/` | `packages/review-session-contract/` | One Rust→TS artifact written to two unrelated trees, imported via `../../../../`. Crosses the language boundary.                         |
| `evaluation/`                            | `apps/api/evaluation/`              | **Not shared.** `fixtures` 11 refs (all api), `gotham` 1, `corpus` 2 (api CLI + the turbo task invoking it), `comparisons` 0.            |
| `skills/`                                | unchanged                           | Top level. Becomes a _generated artifact_ directory under follow-up 3, which is what makes top-level right rather than merely tolerated. |
| `runtime/`                               | unchanged                           | Pending the local/remote convergence decision. Not a package.                                                                            |
| `deploy/railway/*.json`                  | co-located `railway.json`           | Each deployable owns its deploy config. Not deployed today, so a pure file move.                                                         |
| `scripts/`                               | `tooling/scripts/`                  | Not product. Becoming a package retires four `//#` root tasks.                                                                           |

### The payoff

Once `scripts/` becomes a package and convex is gone, **the root `package.json` owns no source** —
no `dependencies`, no `//#` tasks, just workspace config, devDeps, and delegating scripts.
`turbo.json` loses the broad root-task `inputs` lists. The scripts package retains only the
package-task external inputs that its topology tests and contract typecheck actually read.

## Sequencing

1. **Extract before moving**, so files move once.
2. **Fold crate-owned content into the crate before relocating the crate**, so the relocation does
   not touch paths that are about to become internal.

| Phase | Change                                                | Coupling                                |
| ----- | ----------------------------------------------------- | --------------------------------------- |
| 0     | Widen workspace globs                                 | none                                    |
| 1     | `packages/review-session-contract`                    | rust paths are workspace-root-relative  |
| 2     | Fold `evaluation/` into `backend/`                    | manifest-relative literals              |
| 3     | `apps/central-host` + `railway.json`                  | Docker                                  |
| 4     | `apps/api` + `railway.json`                           | Cargo members, Docker, 5 `include_str!` |
| 5     | `services/maia` + `railway.json`; `deploy/` dissolves | Docker, workflow                        |
| 6     | `tooling/scripts`; root empties out                   | turbo root tasks                        |
| 7     | Cleanup + docs                                        | —                                       |

Under jj a plain `mv` suffices; rename detection is content-based.

## Phase 0 — widen globs

```json
"workspaces": ["apps/*", "services/*", "packages/*", "tooling/*", "frontend", "maia-service"]
```

Legacy entries stay until their phase so every intermediate state installs green.

## Phase 1 — `packages/review-session-contract`

```
packages/review-session-contract/
  package.json   # @chenchess/review-session-contract, private, TS source (no build step)
  src/           # <- frontend/src/generated/review-session/*
  fixtures/      # <- contracts/review-session/fixtures/*
  schema/        # <- contracts/review-session/*.json
```

```json
"exports": {
  ".": "./src/index.ts",
  "./fixtures/*": "./fixtures/*.json",
  "./schema/*": "./schema/*.json"
}
```

Ship TS source directly (Turborepo internal-package pattern) — Vite and vitest resolve it under
`moduleResolution: "Bundler"`.

- `frontend` gains `"@chenchess/review-session-contract": "workspace:*"`.
- `@/generated/review-session` → package name (~8 files).
- `../../../contracts/…/fixtures/X.json` → `@chenchess/review-session-contract/fixtures/X` (~5 files).
- `generate_review_session_contract.rs:19-20` → `packages/review-session-contract/{schema,src}`.
- `session.rs:676`, `capture_review_session_recording.rs:76` fixture paths.
- Root `tsconfig.json` `paths.@review-session`; `turbo.json` scripts-task `inputs`.

**Verify:** `generate_review_session_contract -- --check` reports no drift. Load-bearing — it proves
codegen and the committed tree agree.

## Phase 2 — fold `evaluation/` into the crate

`mv evaluation backend/evaluation`, while the crate is still at `backend/`, so the manifest-relative
literals shorten now and survive Phase 4 untouched:
`practice.rs:352`, `lichess_import_tests.rs:489`, `capture_review_session_recording.rs:623`,
`certification/{review_session.rs:539,web_journey.rs:87,255}`.

Workspace-root-relative uses → `backend/evaluation/…`: `capture_review_session_recording.rs:22,68`,
`generate_review_session_contract.rs:121`, `gotham.rs:27`, `session.rs:678`,
`release-proof.ts:620,861,948`, `turbo.json:114`.

Re-root `.gitignore`: `evaluation/**/last-accepted.diff`, `evaluation/gotham/{raw,runs,evaluations}`.

**Freebie found while measuring:** `.dockerignore` excludes `target`, `node_modules`, `.git`, `.jj`
— but _not_ `evaluation/`. Every service builds with `context: .`, so ~102 MB of gotham data is
already shipped to the Docker daemon on every build today. Folding evaluation into `backend/` makes
it worse (it enters `COPY backend ./backend`), so fix `.dockerignore` in this phase.

**Verify:** `cargo test --workspace`, `git status` clean of gotham output, `docker compose build api`.

## Phase 3 — `apps/central-host`

`mv frontend apps/central-host`; `mv deploy/railway/web.railway.json apps/central-host/railway.json`.

- Rewrite every web Dockerfile path for the new location, including the final-stage `dist` source:
  `apps/central-host/package.json`, `apps/central-host`, `/app/apps/central-host`, `/app/apps/central-host/dist`, and
  `apps/central-host/default.conf.template`.
- The web build now consumes a workspace package, so its Docker dependency layer must copy
  `packages/review-session-contract/package.json` before `bun install --frozen-lockfile`, and its
  source layer must copy `packages/review-session-contract/` before `bun run build`. Without both,
  the workspace symlink resolves to a path that does not exist in the image.
- Keep copying `maia-service/package.json` in this phase; Phase 5 changes that manifest path to
  `services/maia/package.json`.
- `docker-compose.yml` `web.build.dockerfile`; `railway.json` `dockerfilePath` + `watchPatterns`.
- `scripts/central-topology.test.ts:17,51,55` and its `deploy/railway` assertions.
- Delete the stray empty `frontend/@/components/ui/`.

`@/*` aliases are package-relative — unchanged.

**Verify:** `docker compose build web`.

## Phase 4 — `apps/api`

`mv backend apps/api`; `mv deploy/railway/api.railway.json apps/api/railway.json`.

Evaluation is already internal. The root-owned skill and notice files are not: there are **nine**
`include_str!` literals that reach them, and every one gains a level:

- `local_runtime/skill.rs:7-24` — `../../../skills/…` → `../../../../skills/…` (4 literals)
- `tests/coach_skill.rs:1-4` — `../../skills/…` → `../../../skills/…` (4 literals)
- `local_runtime.rs:363` — `../../runtime/…` → `../../../runtime/…`

Also re-root every manifest-relative reference to the Phase 1 shared contract package:
`CARGO_MANIFEST_DIR/../packages/review-session-contract/…` becomes
`CARGO_MANIFEST_DIR/../../packages/review-session-contract/…`. Grep both `include_str!` and
`CARGO_MANIFEST_DIR` after the move; compile-time literals and runtime `Path::join` calls both exist.

The remaining edits point _at_ the crate:

- `Cargo.toml`: `members = ["apps/api"]`. `[workspace.metadata] name` unchanged.
- `apps/api/Dockerfile`: `COPY backend ./backend` → `COPY apps/api ./apps/api`; `COPY skills ./skills`
  unchanged.
- `docker-compose.yml` dockerfile + `env_file`; `railway.json` `dockerfilePath` + `watchPatterns`.
- `scripts/central-topology.test.ts:16,53,70`; `release-proof.ts` and `turbo.json` evaluation paths.
- Re-root `.gitignore` / `.dockerignore` gotham entries once more.

`-p chen-chess-coach-api` and all `chenchess-rust#*` task keys are name-derived — **no change**.
`target/` stays at the workspace root.

_(`local_runtime.rs:190,193` — `third_party_notices()` and `skill()` — resolve against the
**installed unit root**, not the repo. See the `/licenses/...` layout the maia Dockerfile writes.
Leave them alone.)_

**Verify:** `cargo build --workspace`, `cargo test --workspace` (the latter compiles
`tests/coach_skill.rs` and proves all nine root-file literals), `docker compose build api`.

## Phase 5 — `services/maia`, and `deploy/` goes away

`mv maia-service services/maia`; `mv deploy/railway/maia.railway.json services/maia/railway.json`;
`rmdir deploy/`. **Keep the package name `maia-service`** so `maia-service#test` still binds.

Dockerfile `COPY` lines, `docker-compose.yml`, `publish-maia-runtime.yml` (`file:`),
`apps/central-host/Dockerfile` (`COPY maia-service/package.json`), turbo `inputs`.
`COPY runtime/THIRD_PARTY_NOTICES.md` is unchanged — `runtime/` stays at root.

## Phase 6 — `tooling/scripts`, and root empties out

`mv scripts tooling/scripts` + manifest `@chenchess/scripts`:

```json
"dependencies": {
  "@chenchess/review-session-contract": "workspace:*"
},
"scripts": {
  "test": "bun test .",
  "typecheck": "bun x tsc --project tsconfig.json",
  "lint": "bun x prettier --check .",
  "format": "bun x prettier --write ."
}
```

Add `tooling/scripts/tsconfig.json`; it owns the moved source include. Replace the
`@review-session` import in `local-smoke.ts` with `@chenchess/review-session-contract`, backed by
the dependency above. Root `tsconfig.json` then drops its scripts `include` and alias.

Delete `//#scripts-test`, `//#scripts-typecheck`, `//#scripts-lint`, and `//#scripts-format`.
The ordinary package task names replace them, so update every caller:

- Root `test`, `typecheck`, `lint`, and `format` scripts invoke only the corresponding ordinary
  Turbo task; remove the extra `scripts-*` positional task names.
- Root `release:proof` and `smoke:local` point to
  `tooling/scripts/{release-proof,local-smoke}.ts`.
- `release-proof.ts`'s `PLATFORM_NEUTRAL` command removes `scripts-test`, `scripts-lint`, and
  `scripts-typecheck`; `@chenchess/scripts` now participates through ordinary `test`, `lint`, and
  `typecheck`.
- Update `release-proof.test.ts` assertions and spawned command paths to the same surface.

Do **not** discard the verified external cache edges when dissolving the root tasks. Package-local
hashing does not cover files that `central-topology.test.ts` reads outside `tooling/scripts/`.
Define `@chenchess/scripts#test` inputs using `$TURBO_DEFAULT$` plus `$TURBO_ROOT$/…` globs for:

- `.github/workflows/**`;
- `apps/{api,web}/{Dockerfile,railway.json}`, `apps/central-host/default.conf.template`;
- `services/maia/{Dockerfile,railway.json}`;
- `skills/chenchess-coach/SKILL.md`, `docs/central-hosting.md`, and root `package.json`.

Likewise, `@chenchess/scripts#typecheck` includes
`$TURBO_ROOT$/packages/review-session-contract/src/**`, because `local-smoke.ts` type-checks directly
against that source package and the contract intentionally has no build task. These are declared
edges that the tasks actually verify; retaining them prevents a restored cache hit from hiding a
broken topology or contract.

Consider letting the `chenchess` CLI default `--corpus-dir` to its own packaged corpus, so
`release-proof.ts` stops passing a path into another package's interior.

`release-proof.ts` and `central-topology.test.ts` compute `ROOT` relative to their own files; change
the calculation for the extra directory level. Then grep `import.meta.dir`, `../package.json`,
`.github/workflows`, and spawned `scripts/…` command paths in both source and tests—several test
paths are relative without using the `ROOT` constant.

**Verify:** `bun run release:proof` end to end — the real gate for the whole restructure.

## Phase 7 — cleanup

- Drop legacy workspace glob entries.
- `turbo ls` → `chen-chess-coach-web`, `chen-chess-coach-api`, `maia-service`,
  `@chenchess/review-session-contract`, `@chenchess/scripts`, `chenchess-rust`.
- Docs sweep: `README.md`, `AGENTS.md`,
  `docs/{central-hosting,local-self-hosting,local-ci,local-coach}.md`, `runtime/README.md`,
  `evaluation/README.md`, `CONTEXT.md`.
- **Leave `.learn/*.md` and prior `.claude/*-plan.md` alone** — dated records.
- First `turbo run build` is a full cache miss. Expected.

## Follow-ups this restructure deliberately does not decide

1. **Does the remote runtime share the local runtime's config?** Today it shares none of it —
   different Stockfish acquisition (tarball vs `apt`), different Maia acquisition (published digest
   vs built from source), and a manifest hard-pinned to `macos/aarch64`. If they should converge,
   the manifest needs splitting into a portable unit description plus a platform-specific
   acquisition block, and `runtime/` gets a real structure. That decision belongs in an ADR, not in
   a file move.
2. **Rename `local_runtime` → `runtime`, and split out `manifest` and `paths`.**
   **Not blocked on (1)** — an earlier draft said it was, on the reasoning that ~2,300 lines of
   installer machinery were inherently local and only ~58 lines of manifest types were not. That
   read the volume rather than the parameterisation, and it was wrong.

   `RuntimePaths` is a one-field struct:

   ```rust
   pub struct RuntimePaths { home: PathBuf }
   ```

   `RuntimeInstaller`, `activation`, `docker`, `process`, `state`, and `manager` all operate on
   `RuntimePaths` / `RuntimeUnitPaths` and never touch `home` directly; `InstallRequest` takes
   `paths` as an input. **The installer is already host-agnostic** — it installs a pinned unit at a
   given root. The entire local assumption is ~40 lines: `from_environment()` reading `$HOME`, and
   six path derivations (`.local/bin`, `.config`, `.local/state`, `.local/share`, `.agents/skills`,
   `.claude/skills`).

   So the module is not local; only its **path policy** is. Target:

   ```
   apps/api/src/runtime/
     manifest.rs   RuntimeManifest, RuntimeTarget, StockfishPackage, MaiaPackage,
                   RUNTIME_MANIFEST_SCHEMA_VERSION      — the shared unit description
     paths.rs      RuntimePaths — user-install layout policy (the only local part)
     installer.rs  activation.rs  docker.rs  process.rs  state.rs  manager.rs
                   acquire, verify, stage, activate, supervise — host-agnostic
   ```

   If central hosting adopts the unit it supplies a **different paths policy**, not a different
   module. That is the seam (1) is really asking about, and it already exists in the code — just
   mislabeled. Doing this rename _first_ also gives (1) a much smaller surface to decide over.

   The one irreducibly local remainder is `codex_skill_link()` / `claude_skill_link()` — a central
   host does not install a skill into an agent's home directory. Two methods, and follow-up 3 moves
   skill installation out regardless.

3. **Make the Coach Skill a generated artifact owned by the `chenchess` CLI.**

   The skill couples to both the CLI flag surface and the contract vocabulary, and `install()`
   already lives in the CLI. One owner for authoring, generation, and installation is the coherent
   design; a drift test would only detect after the fact what generation prevents by construction.

   _An earlier draft of this plan argued the skill could not be generated because most of it is
   authored judgment rather than derivable fact. That reasoning was wrong, and this repo refutes
   it:_ `generate_review_session_contract.rs:116` writes a prose `README.md` from
   `contract_readme()`, and `review_session_contract/templates/` holds ~24 KB of authored
   TypeScript that the same generator emits. Generation here has never required derivability — it
   requires single ownership.

   Shape, mirroring the contract pipeline exactly:

   - **Templates** — authored markdown with placeholders, inside the crate at
     `apps/api/src/skill/templates/*.md`. Ordinary crate source, so turbo hashes them
     automatically.

     This promotes `skill` to a **crate-level module**: `src/skill.rs` + `src/skill/templates/`,
     out of `local_runtime/` entirely. The skill is its own domain — generated from the CLI
     surface and the contract, then installed. `local_runtime` merely _invokes_ the install during
     unit setup, which is a caller relationship, not ownership. Mechanically: move
     `local_runtime/skill.rs` → `skill.rs`, widen `pub(super) fn install/sha256` to `pub(crate)`,
     and have `local_runtime` call `crate::skill::install`. Consistent with follow-up 2 — the same
     over-scoping of `local_runtime` shows up in both.

   - **Generator** — `generate_coach_skill` bin (or a `chenchess` subcommand) rendering to
     `skills/chenchess-coach/`, with `--check` mode, matching
     `generate_review_session_contract`'s interface.
   - **Drift gate** — `chenchess-rust#coach-skill-drift` in `turbo.json`, beside the existing
     `chenchess-rust#review-session-contract-drift`.
   - **Install** — renders from the embedded templates rather than `include_str!`ing the generated
     output, so no compiled-in file lives outside the crate.

   Injected vocabulary comes from the two machine-readable sources: the clap definition
   (`review-session --jsonl`, `validate-review --review-event-file/--review-start-event-file/--draft-file`,
   `validate-practice`) and the generated JSON Schema (`gameImported.review.practiceSelection`,
   `coachTurnPrepared.facts`, `criticalMoment.objective.lines`, `residualOutcome`,
   `mechanism.payoff`).

   **Implementation requirement, or the guarantee is theatre:** the vocabulary must be genuinely
   parameterised. Prose that hardcodes `--draft-file` in a sentence can still drift, and the
   generator would be markdown with extra steps. Every flag name and contract field path the prose
   references must be a substitution.

   **This supersedes the placement debate.** `skills/` becomes a checked-in _generated artifact_
   directory — legitimately top-level for the same reason `frontend/src/generated/` was
   legitimately inside the web app: it is output, addressed by consumers outside the repo (the
   installed `~/.claude/skills/`). It also retires the last unhashed external `include_str!`, so
   the Phase 4 skill path edits become moot once this lands.

4. **The Rust crate split** — `packages/review-contract` ← `packages/chess-domain` ← `apps/api`.
   After the architecture simplification settles the contract subsystem, and only against a measured
   rebuild-time problem. The only change that would make `experimentalCargoWorkspaces` pay off.

## Risks

- **Docker `COPY` paths** fail at build time, not test time, and GH Actions CI is offboarded
  (`plans/001-offboard-github-actions-ci.md`). Run `docker compose build` explicitly in Phases 2–5.
  With Railway not deployed, this is the only way a path mistake reaches something real.
- **`.gitignore` re-rooting** happens twice (Phases 2 and 4) — miss either and ~95 MB of untracked
  gotham output becomes trackable. `git status` after each move is the check.
- **Undeclared `include_str!` inputs** for `skills/` and `runtime/` — bounded as described above
  (cargo dep-info keeps local builds correct; only a restored turbo cache is exposed). Follow-up 3
  guards the failure that actually matters — skill↔CLI drift — which hashing never covered.
- **Scripts-package cache inputs** remain explicit for files its tests and typecheck read outside
  `tooling/scripts/`. Removing those edges would let `release:proof` restore a passing result for
  obsolete Docker, Railway, workflow, documentation, or contract content.
- Contract extraction moves ~200 generated files; keep it isolated so the diff reads as moves only
  against a clean `--check`.

## Verification ladder

```bash
bun install && bun run check test lint build
```

Per phase: `generate_review_session_contract -- --check` (1), `cargo build --workspace` (2, 4),
`docker compose build <svc>` (2–5), `bun run release:proof` (6 and final).

## Status

No open questions blocking Phase 0. Four follow-ups above are deliberately left as decisions, not
guesses.
