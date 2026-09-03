# Plan 005: Gate only affected Railway and Firebase release units

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report; do not improvise. When done, update this plan's status row in
> `plans/README.md` unless a reviewer told you they maintain the index.
>
> **Drift check (run first)**:
>
> ```sh
> jj diff --stat \
>   --from fb44b51b --to @ -- \
>   AGENTS.md \
>   README.md \
>   package.json \
>   bun.lock \
>   turbo.json \
>   flake.nix \
>   firebase.json \
>   storage.rules \
>   apps/central-host/package.json \
>   apps/central-host/railway.json \
>   services/coach-engine/railway.json \
>   services/maia/railway.json \
>   tooling/scripts/package.json \
>   tooling/scripts/tsconfig.json \
>   tooling/scripts/release-proof.ts \
>   tooling/scripts/release-proof.test.ts \
>   tooling/scripts/central-topology.test.ts \
>   docs/adr/0022-run-ci-before-github-sync.md \
>   docs/local-ci.md \
>   docs/central-hosting.md
> ```
>
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding. A
> load-bearing mismatch is a STOP condition.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED
- **Depends on**: Plan 003
- **Category**: performance, DX, release correctness
- **Planned at**: commit `fb44b51b`, 2026-07-28
- **Execution status**: DONE
- **Execution revision**: Firebase Hosting target removed after the maintainer
  clarified that Railway hosts every web portal; Firebase deploys only Storage
  Security Rules.
- **Execution scope revision**: the maintainer approved updating the Review
  Session contract generator after its drift check proved that it owns the SDK
  package manifest and complete generated directory.

## Why this matters

`release:proof` currently runs one repository-wide Turbo union before every
push to `main`. A TypeScript-only Coach App or web change therefore enters the
Rust task graph, even though Railway already uses per-service watch paths and
would not redeploy Coach Engine. Conversely, the broad proof is not an exact
deployment proof: it does not build Railway's Docker images, and it does not
exercise Firebase Storage rules.

Separate synchronization from release validation. A push with no
Railway-deployable change should do no release work. A push that will trigger a
Railway autodeploy should run only the gate for the affected `web`, `api`, or
`maia` service. The explicit Firebase Storage deployment should run only its
Storage Rules gate.

Keep the complete credential-free proof for exceptional whole-repository
audits and Apple Silicon Local Pipeline Runtime certification. It is valuable;
it is just the wrong default before every GitHub synchronization.

This distinction must be agent-facing policy, not only implementation detail.
An agent asked only to synchronize a bookmark with remote `main` must not infer
that synchronization authorizes or requires a release. It first determines
whether the changed-path plan selects a Railway service. No selected service
means no release gate and no full proof. A selected service means the push will
cause Railway to deploy, so only that service's scoped gate runs.

## Current state

### One broad command owns every push

`tooling/scripts/release-proof.ts:44-59` defines one platform-neutral step:

```ts
export const PLATFORM_NEUTRAL = [
  [
    "turborepo-verification",
    [
      "bun",
      "run",
      "turbo",
      "run",
      "check",
      "test",
      "lint",
      "build",
      "typecheck",
    ],
  ],
] as const
```

`tooling/scripts/release-proof.ts:366-370` always runs that complete list:

```ts
export async function runPlatformNeutral(): Promise<void> {
  const environment = cleanEnvironment()
  for (const [name, command] of PLATFORM_NEUTRAL) {
    await runStep(name, command, { environment })
  }
}
```

`docs/local-ci.md:41-47`, `README.md:88-95`, and `AGENTS.md:17-24` require the
proof immediately before every push. ADR 0022 records the same decision.

### The broad categories expand into redundant work

The current Turbo dry graph for
`check test lint build typecheck` contains 44 task nodes. Its Rust portion
includes:

- `cargo build --package=chen-chess-coach-engine --locked`;
- `cargo check --workspace --locked`;
- `cargo fmt --all -- --check`;
- `cargo clippy --workspace --all-targets -- -D warnings`;
- `cargo test --workspace`;
- review-session contract drift;
- review-session recording integrity; and
- deterministic evaluation.

The web and Coach App packages each run `check`, `typecheck`, and `build`, even
though `check` and `typecheck` are identical and `build` starts with the same
TypeScript project build. Release gates should express guarantees, not preserve
duplicated task-category names.

The following dry command already proves that an explicit web filter excludes
Rust:

```sh
bun run turbo run lint test build \
  --filter='@chenchess/central-host...' \
  --filter='@chenchess/coach-app...' \
  --dry=json
```

Its executable nodes are only Coach App, web, UI, and Coach Engine SDK tasks.
The equivalent explicit API filter selects only `chenchess-rust` and its Cargo
package.

Do not use Turbo's repository-wide `--affected` result as the release-unit
selector. In this colocated Jujutsu history, a known Coach App/web-only range
also selected the root Rust workspace through root-package and dependency
effects. The repository already has a more precise deployment boundary:
Railway watch paths.

### Railway already knows which service will deploy

The checked-in Config-as-Code files define these watch paths:

| Service | Current watch paths                                                                                                       |
| ------- | ------------------------------------------------------------------------------------------------------------------------- |
| `web`   | `apps/coach-app/**`, `apps/central-host/**`, `packages/coach-engine-sdk/**`, `packages/ui/**`, `package.json`, `bun.lock` |
| `api`   | `services/coach-engine/**`, `runtime/**`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`                               |
| `maia`  | `services/maia/**`                                                                                                        |

Railway skips deployments whose changes do not match a service's watch paths.
Because GitHub autodeploy remains enabled, a matching push is itself a release
action and still needs a pre-push trust gate. A nonmatching push is only
synchronization and should skip release validation.

The current watch paths also have coverage gaps relative to Docker build
inputs:

- `services/coach-engine/Dockerfile` copies `skills/`, but the API watch paths
  do not include `skills/**`;
- `services/maia/Dockerfile` copies
  `runtime/THIRD_PARTY_NOTICES.md`, but the Maia watch paths do not include it;
- `apps/central-host/Dockerfile` copies `services/maia/package.json` and
  `tooling/scripts/package.json` for the frozen workspace install, but the web
  watch paths do not include either manifest; and
- `apps/central-host/Dockerfile` copies the Coach App provider recording from
  `services/coach-engine/evaluation/fixtures/Synthet1/`, but the web watch paths
  do not include that file.

These must be corrected before the same scopes can safely select release gates.

### Firebase Storage Rules need their own gate

Railway hosts every web portal; Firebase Hosting is not a supported release
product. The stale Hosting configuration must be removed from `firebase.json`.
Cloud Storage Security Rules remain an explicitly deployed Firebase product,
and the broad release proof does not run a Storage rules emulator test.
Firebase supports a Storage-specific partial deployment and predeploy hook; a
failed predeploy hook cancels the deployment. Use that native boundary rather
than asking a generic proof to guess what the operator intends to deploy.

### The full proof also owns runtime certification

`tooling/scripts/release-proof.ts:793-1121` contains the Apple Silicon Local
Pipeline Runtime certification: release CLI build, digest-pinned Maia runtime
installation, live Review Session journeys, warm reuse, rollback, report
generation, and clean uninstall. That certification is not a Railway or
Firebase gate. Preserve it and its process-lifecycle protections from Plan 003.

## Target design

### Selection modes

Add `tooling/scripts/release-gate.ts` with two mutually exclusive modes:

1. **Changed Railway mode**

   ```sh
   bun run release:gate -- \
     --platform railway \
     --from <current-remote-main> \
     --to @
   ```

   Resolve changed paths with direct-argv Jujutsu execution:
   `jj diff --from <from> --to <to> --name-only`. Map those paths to Railway
   release units. Run each selected unit once. If no unit is selected, print a
   stable no-op message and exit 0.

2. **Explicit target mode**

   ```sh
   bun run release:gate -- --target firebase-storage
   bun run release:gate -- --target railway-central-host
   bun run release:gate -- --target railway-coach-engine
   bun run release:gate -- --target railway-maia
   ```

   Explicit targets always run. Do not silently skip an explicitly requested
   deployment gate because a Git comparison appears empty.

Both modes support `--list`, which prints the selected target IDs, matching
paths/reasons, and exact commands as JSON without running them.

Reject mixed modes, unknown targets, missing revision arguments, a failed
Jujutsu diff, absolute paths, `..` path traversal, and malformed diff output
with usage exit 2. A path-selection error must never become a successful
no-op.

### Release-unit registry

Create `tooling/scripts/release-targets.ts` as the single typed registry used by
the planner and its tests. Keep command argv arrays rather than shell strings.

| Target                 | Deployment inputs                                                              | Required local guarantees                                                                                                                                 |
| ---------------------- | ------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `railway-central-host` | Railway `web` watch paths, including exact Docker `COPY` inputs                | Coach App/web/UI/SDK lint and tests; Coach App and full web client+server production builds; central-topology contract                                    |
| `railway-coach-engine` | Railway `api` watch paths, including `skills/**`                               | Rust format, clippy, workspace tests, contract drift, recording integrity, deterministic evaluation, release-mode server build; central-topology contract |
| `railway-maia`         | Railway `maia` watch paths, including the runtime notice copied into its image | Maia Python unit tests; central-topology contract                                                                                                         |
| `firebase-storage`     | `storage.rules`, the `storage` section of `firebase.json`                      | credential-free Storage emulator tests for anonymous and authenticated deny behavior                                                                      |

Railway watch paths and the registry must not drift. Extend
`tooling/scripts/central-topology.test.ts` to:

- compare each Railway target's deployment-input patterns with the matching
  `railway.json` `watchPatterns`;
- prove every local Docker `COPY` source is covered by at least one watch
  pattern for that service;
- accept exact-file patterns as well as directory globs; and
- fail with the uncovered Docker source in the assertion message.

Keep documentation, plans, evaluations that are not copied into an artifact,
and unrelated tooling outside all release-unit scopes. A new Docker input
cannot silently escape the registry: changing the Dockerfile selects its
service, and the topology test must fail until the new source is covered.

### Exact target commands

Use focused Turbo filters and Cargo's existing incremental cache. Do not pass
the global `check test lint build typecheck` union to a target.

For `railway-central-host`:

```sh
bun run turbo run lint test build \
  --filter='@chenchess/central-host...' \
  --filter='@chenchess/coach-app...'
bun run turbo run check \
  --filter='@chenchess/ui' \
  --filter='@chenchess/coach-engine-sdk'
bun test tooling/scripts/central-topology.test.ts
```

Add a real SDK `check` script and minimal `tsconfig.json` if needed. The app
builds already typecheck their imported source, while the focused shared
package checks cover files not reached by current app imports. Do not also run
the duplicate app `check` and `typecheck` scripts. Update the Review Session
contract generator as the source of truth for both generated files.

For `railway-coach-engine`:

```sh
bun run turbo run lint test --filter='chenchess-rust'
cargo build --release --locked \
  -p chen-chess-coach-engine \
  --bin chen-chess-coach-engine
bun test tooling/scripts/central-topology.test.ts
```

Remove `chenchess-rust#build` from the dependencies of
`chenchess-rust#test`: `cargo test --workspace` already compiles the test
profile, and the explicit final command proves the exact release-mode server
binary. Keep the deterministic-evaluation, contract-drift, and
recording-integrity dependencies. Do not invoke the redundant workspace
`check` task in this release target; clippy, tests, and the release build cover
compilation.

For `railway-maia`:

```sh
bun run turbo run test --filter='@chenchess/maia'
bun test tooling/scripts/central-topology.test.ts
```

Railway's Docker build and configured health check remain provider-side gates.
Do not add a local multi-platform PyTorch image build to every Python change.
The repository gate proves service behavior and deployment topology; Railway
proves the image build before replacing the healthy deployment.

For `firebase-storage`, add a focused test under
`tooling/scripts/storage-rules.test.ts` and run it through the Storage emulator
with a demo project ID. Cover anonymous and authenticated read/write attempts;
all four must be denied by the current rules. Use the checked-in
`storage.rules` through `firebase.json`; do not read production credentials or
contact the production project.

### Process and credential handling

Extract Plan 003's process ownership, interruption, cleanup, timing, and
credential-cleaning seam from `release-proof.ts` into
`tooling/scripts/release-process.ts`. Both proof and gate must use the same
implementation. Preserve:

- POSIX child process groups and bounded TERM/KILL cleanup;
- direct-child behavior on Windows;
- SIGINT/SIGTERM propagation and conventional exit codes;
- captured-output behavior;
- per-step wall-time diagnostics; and
- removal of hosted-provider credentials and local Stockfish/Maia overrides.

Do not duplicate a simpler `Bun.spawn` loop in the new gate and regress the
descendant cleanup fixed by Plan 003.

### Invocation boundaries

Keep `release:proof` as an explicit full-repository proof and the entry point
for `--runtime-manifest` Apple Silicon certification. Remove it from routine
push instructions.

Rename the active `AGENTS.md` and `README.md` sections from "Local CI before
GitHub synchronization" to "Scoped validation and release gates" (or an
equally explicit name). Both files must state the same decision procedure:

1. During implementation, run focused checks for the code actually changed.
   This is ordinary engineering validation, not a release gate.
2. If the task is only to synchronize a bookmark with remote `main`, do not run
   `release:proof`.
3. Before that synchronization, inspect the Railway changed-path plan. If it
   selects no Railway release unit, do not run `release:gate` either; proceed
   with synchronization using the configured remote.
4. If the plan selects one or more Railway units, the push is also a deployment
   trigger because autodeploy is enabled. Run only the selected unit gates.
5. The Firebase Storage gate runs only from its explicit partial-deployment
   command through the Storage predeploy hook. Firebase Hosting is not used.
6. Run the complete `release:proof` only when the operator explicitly requests
   a whole-repository proof, when publishing/certifying the Local Pipeline
   Runtime, or when another documented release procedure names it.
7. Never use `release:proof` as a fallback merely because target selection is
   uncertain. A selection or revision-resolution error is a STOP condition to
   diagnose, not permission to run the most expensive command.

Include one positive no-release example:

```text
Changed paths: docs/**, plans/**, or other nondeployable files
Requested action: synchronize bookmark to remote main
Action: no release gate; no release:proof
```

Include one deployment example:

```text
Changed paths: apps/coach-app/**
Requested action: synchronize bookmark to remote main
Action: railway-central-host gate only, because that push triggers Railway web deploy
```

Update the optional local Jujutsu alias so it runs changed Railway selection
before pushing:

```sh
nix develop --command bun run release:gate -- \
  --platform railway \
  --from <current-remote-main> \
  --to @
jj git push --bookmark <bookmark>
```

On this checkout, `<current-remote-main>` is `main@codex-https`. Keep the script
revision-parameterized rather than hard-coding a remote name into source.
Direct `jj git push` remains an explicit trust-boundary bypass. A
non-fast-forward push after a successful gate must be rejected by the remote;
fetch/rebase and rerun the gate rather than reusing stale proof.

Add the Firebase Storage hook and remove stale Hosting configuration:

```json
{
  "storage": {
    "predeploy": "bun run release:gate -- --target firebase-storage"
  }
}
```

Add a pinned root script that uses explicit partial deployment and project
selection:

```text
deploy:firebase:storage -> firebase deploy --project chenchess --only storage
```

Do not expose a Firebase Hosting or bare all-products repository script.
Firebase Storage rules do not have an application-style rollback, so the
operation must remain deliberate.

Do not put test commands in Railway `preDeployCommand`. Railway runs that
command from the already-built runtime image in a separate predeployment
container; these images intentionally lack the repository test toolchains and
sources. The local changed-path gate plus Railway's build and health check is
the existing single-maintainer trust model.

## Commands you will need

| Purpose                   | Command                                                                                                     | Expected on success                                                                                                  |
| ------------------------- | ----------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| Inspect state             | `jj status`                                                                                                 | existing Plan 003 working-copy changes remain visible and untouched                                                  |
| Record source             | `jj log -r @ --no-graph -T commit_id`                                                                       | prints the revision under test                                                                                       |
| Gate unit tests           | `nix develop --command bun test tooling/scripts/release-gate.test.ts tooling/scripts/release-proof.test.ts` | planner, process, CLI, and existing proof tests pass                                                                 |
| Topology tests            | `nix develop --command bun test tooling/scripts/central-topology.test.ts`                                   | every Docker input is covered by its service watch scope                                                             |
| Scripts typecheck         | `nix develop --command bun x tsc --project tooling/scripts/tsconfig.json`                                   | exits 0                                                                                                              |
| TS-only historical plan   | `nix develop --command bun run release:gate -- --platform railway --from c907db0c- --to c907db0c --list`    | selects `railway-central-host`; JSON contains no Cargo command                                                       |
| Rust-only historical plan | `nix develop --command bun run release:gate -- --platform railway --from b2638c55- --to b2638c55 --list`    | selects `railway-coach-engine` only                                                                                  |
| No-op plan                | focused unit/CLI fixture with docs-only changed paths                                                       | selects no release units and exits 0 with a stable no-op message                                                     |
| Agent policy              | `nix develop --command bun test tooling/scripts/release-gate.test.ts`                                       | proves `AGENTS.md` and `README.md` forbid full proof for sync-only/no-target work and document the Railway exception |
| Web gate                  | `nix develop --command bun run release:gate -- --target railway-central-host`                               | all selected web tasks pass; no Rust task appears                                                                    |
| API gate                  | `nix develop --command bun run release:gate -- --target railway-coach-engine`                               | all Rust behavior gates and release server build pass; no frontend task appears                                      |
| Maia gate                 | `nix develop --command bun run release:gate -- --target railway-maia`                                       | Python and topology tests pass; no Cargo or frontend build appears                                                   |
| Storage gate              | `nix develop --command bun run release:gate -- --target firebase-storage`                                   | Storage emulator tests pass against the demo project; no production access                                           |
| Full proof plan           | `nix develop --command bun run release:proof -- --list`                                                     | complete proof and Apple Silicon certification metadata remain available                                             |
| Full proof regression     | `nix develop --command bun run release:proof`                                                               | exits 0 after extraction to the shared process module                                                                |
| Diff                      | `jj diff --stat`                                                                                            | only in-scope files and plan status changed                                                                          |

## Suggested executor toolkit

- Use the `onevcat-jj` skill for all local version-control operations. This is
  a colocated Jujutsu repository; do not use Git staging, commits, stashes, or
  checkouts.
- Railway watch-path behavior:
  <https://docs.railway.com/deployments/monorepo>
- Railway GitHub autodeploy behavior:
  <https://docs.railway.com/deployments/github-autodeploys>
- Firebase partial deploys and predeploy hooks:
  <https://firebase.google.com/docs/cli>
- Firebase Security Rules emulator testing:
  <https://firebase.google.com/docs/rules/unit-tests>

## Scope

**In scope**:

- `tooling/scripts/release-process.ts` (create)
- `tooling/scripts/release-targets.ts` (create)
- `tooling/scripts/release-gate.ts` (create)
- `tooling/scripts/release-gate.test.ts` (create)
- `tooling/scripts/storage-rules.test.ts` (create)
- `tooling/scripts/release-proof.ts`
- `tooling/scripts/release-proof.test.ts`
- `tooling/scripts/central-topology.test.ts`
- `tooling/scripts/package.json`
- `tooling/scripts/tsconfig.json`
- `package.json`
- `bun.lock`
- `turbo.json`
- `flake.nix` only if the credential-free Storage emulator needs a pinned Java
  runtime not already available in `nix develop`
- `apps/central-host/package.json`
- `packages/coach-engine-sdk/package.json`
- `packages/coach-engine-sdk/tsconfig.json` (create if required)
- `services/coach-engine/src/bin/generate_review_session_contract.rs`
- `apps/central-host/railway.json`
- `services/coach-engine/railway.json`
- `services/maia/railway.json`
- `firebase.json`
- `storage.rules` only if a test exposes an unintended current rule behavior;
  do not broaden access merely to make a test pass
- `docs/adr/0022-run-ci-before-github-sync.md`
- `docs/adr/0024-gate-affected-release-units.md` (create)
- `AGENTS.md`
- `README.md`
- `docs/local-ci.md`
- `docs/central-hosting.md`
- `plans/README.md` for final status only

**Out of scope**:

- Disabling Railway GitHub autodeploy or replacing it with a new GitHub Actions
  deployment pipeline. Changed-path local gating is compatible with the
  repository's current no-push-CI decision.
- Railway variables, secrets, service IDs, project IDs, or live environment
  mutation.
- Any Firebase production deployment while implementing this plan.
- Apple Silicon certification behavior, live-provider journeys, runtime
  manifests, or certification fixtures, except import changes required by the
  shared process helper extraction.
- Dropping deterministic evaluation, contract drift, recording integrity, or
  release-mode compilation from the API release gate.
- Adding a Docker build to every target. Railway already blocks deployment on
  build failure and health-check failure; local image smoke can be proposed
  later if a real escape is observed.
- Reworking ordinary per-change developer validation. This plan changes the
  release boundary, not the expectation that implementers run focused tests
  for code they modify.
- GitHub remote synchronization or publishing.

## Jujutsu workflow

- Plan 003 is currently present in local change `lstmknkm` and is not yet at
  `main@codex-https`. Execute this plan only after Plan 003's source is the
  intended parent.
- Start a new Jujutsu change for this work; do not amend the completed Plan 003
  change.
- Use a logical description such as
  `perf: gate only affected release units`.
- Do not create a bookmark, push, deploy, or publish unless the operator
  explicitly requests it.
- Preserve all unrelated working-copy changes.

## Steps

### Step 1: Record the superseding release decision

Create `docs/adr/0024-gate-affected-release-units.md`. Mark it Accepted and
state explicitly:

1. routine GitHub synchronization is not a release gate;
2. a push that matches a Railway watch scope is a deployment action because
   Railway autodeploy is enabled;
3. only affected Railway units are gated before that push;
4. Firebase uses explicit product-specific gates and partial deploys;
5. the full proof remains for whole-repository audit and Local Pipeline Runtime
   certification; and
6. the local gate remains a bypassable single-maintainer trust boundary.

Update ADR 0022's status to `Superseded by ADR 0024` and retain its historical
context. Do not rewrite it as though the earlier decision was never made.

**Verify**:

```sh
rg -n 'Superseded|synchronization|Railway|Firebase|release:proof|trust boundary' \
  docs/adr/0022-run-ci-before-github-sync.md \
  docs/adr/0024-gate-affected-release-units.md
```

Expected: both the old and new decisions are discoverable, with no
contradictory Accepted pre-push mandate.

### Step 2: Add failing planner and scope-contract tests

Before implementing the gate, add `tooling/scripts/release-gate.test.ts` tests
for:

- one file that selects each individual target;
- a path that selects multiple targets where appropriate;
- docs/plans-only changes selecting nothing;
- duplicate matching paths selecting a target once;
- path normalization and rejection cases;
- invalid/missing revisions failing rather than no-oping;
- explicit targets running without a diff;
- stable `--list` JSON;
- the known `c907db0c-..c907db0c` TypeScript range selecting only
  `railway-central-host`;
- the known `b2638c55-..b2638c55` Rust range selecting only `railway-coach-engine`; and
- no Cargo argv anywhere in web, Hosting, or Maia plans.

Extend `central-topology.test.ts` with the registry/watch-path and Docker
`COPY`-coverage assertions. The initial tests should expose the five known
watch-path gaps.

**Verify**:

```sh
nix develop --command bun test \
  tooling/scripts/release-gate.test.ts \
  tooling/scripts/central-topology.test.ts
```

Expected before Steps 3-5: focused failures identify the missing planner and
the exact uncovered Docker inputs; unrelated existing topology tests pass.

### Step 3: Extract the shared process seam without behavior change

Move only the generic subprocess types and functions from
`release-proof.ts` to `release-process.ts`. Update imports and preserve every
Plan 003 process test. Do not move certification-domain validation or JSON
helpers.

Keep `release-proof.ts`'s CLI output, plan JSON, full platform-neutral command,
runtime certification, exit codes, and timing diagnostics byte-for-byte
compatible where tests assert them.

**Verify**:

```sh
nix develop --command bun test tooling/scripts/release-proof.test.ts
nix develop --command bun run release:proof -- --list
```

Expected: all existing proof tests pass and the list still contains the full
Turbo union plus Apple Silicon certification metadata.

### Step 4: Implement pure release-unit selection and gate execution

Implement the typed registry, path matcher, CLI parser, Jujutsu diff reader,
stable plan JSON, deduplication, and sequential target execution. Reuse the
shared process seam and clean environment.

The selector must be pure: given normalized changed paths and a platform, it
returns selected target IDs plus matching reasons. Keep filesystem and process
work at the CLI boundary so tests do not need a live repository for every
case.

Run selected targets sequentially. This avoids two release units contending
for the same Cargo/Bun caches and makes failure ownership clear. Within a
Turbo command, retain Turbo's safe task parallelism.

**Verify**:

```sh
nix develop --command bun test tooling/scripts/release-gate.test.ts
nix develop --command bun x tsc --project tooling/scripts/tsconfig.json
nix develop --command bun run release:gate -- \
  --platform railway \
  --from c907db0c- \
  --to c907db0c \
  --list
```

Expected: tests and typecheck pass; the historical plan names only
`railway-central-host` and contains no `cargo` command.

### Step 5: Align Railway scopes and remove target-local redundancy

Add the missing Docker input patterns to the three `railway.json` files and
the typed registry. Implement the exact web, API, and Maia command lists from
"Exact target commands".

Add the SDK check configuration. Remove only the redundant debug build
dependency from `chenchess-rust#test`; preserve its integrity and deterministic
evaluation dependencies. Do not weaken the complete `release:proof`.

**Verify**:

```sh
nix develop --command bun test tooling/scripts/central-topology.test.ts
nix develop --command bun run release:gate -- --target railway-central-host --list
nix develop --command bun run release:gate -- --target railway-coach-engine --list
nix develop --command bun run release:gate -- --target railway-maia --list
```

Expected: topology passes; each JSON plan contains only its target's executable
commands. `railway-central-host` and `railway-maia` contain no Cargo argv.

### Step 6: Add the Firebase Storage gate and enforce it at deploy time

Remove the unsupported Firebase Hosting configuration. Add the credential-free
Storage emulator test, pinned Firebase tooling, emulator configuration, the
Storage `predeploy` hook, and one explicit partial-deploy script.

Do not test this step by deploying to the live Firebase project.

**Verify**:

```sh
nix develop --command bun run release:gate -- --target firebase-storage
nix develop --command bun test tooling/scripts/release-gate.test.ts
```

Expected: Storage passes four deny tests against a demo emulator project;
config tests prove the Storage partial deployment invokes its hook and that no
Firebase Hosting target or deploy script remains.

### Step 7: Replace the universal push instruction

Update `AGENTS.md`, `README.md`, `docs/local-ci.md`, and
`docs/central-hosting.md`:

- ordinary work uses focused implementation checks;
- synchronization to remote `main` alone does not require a release gate;
- agents must not run `release:proof` merely because they are about to push;
- the Railway changed-path plan is inspected before synchronization;
- the optional checked-push path runs changed Railway gates;
- a no-target result means synchronize without `release:gate` or
  `release:proof`;
- a selected Railway target means the push is a deployment trigger and only
  that target's gate runs;
- the explicit Firebase Storage deploy script carries its own hook;
- `release:proof` is the full audit/runtime-certification command, not the
  routine push command;
- direct push remains a bypass;
- Railway autodeploy/build/health-check ownership is explicit; and
- Railway owns all web hosting while Firebase Storage Rules remain a deliberate
  non-application release.

Do not claim that Railway or Firebase is remotely enforcing the local
single-maintainer gate.

Add documentation-contract assertions to `release-gate.test.ts`. Prefer
checking for required policy phrases and the absence of the old exact
unconditional instruction over snapshotting whole Markdown files. The test
failure must name the file whose policy drifted.

**Verify**:

```sh
rg -n 'release:proof|release:gate|checked-push|firebase|Railway|immediately before' \
  AGENTS.md README.md docs/local-ci.md docs/central-hosting.md
```

Expected: no active instruction requires the full proof before every push;
every deployment path names its selected gate; and both agent-facing files
explicitly say that sync-only/no-target work runs no release gate.

### Step 8: Run each target and retain the full-proof regression

Run all commands in "Commands you will need". Record for each target:

- selected task count;
- cold wall time;
- warm wall time;
- whether Cargo ran;
- whether a production artifact was built; and
- exact source revision.

Then run the full proof once to verify the process-module extraction and task
dependency adjustment did not weaken the exceptional whole-repository path.
Do not deploy or publish.

**Verify**:

```sh
jj diff --stat
```

Expected: only in-scope implementation, tests, docs, and plan status changed.

## Validation record

All target runs were local and credential-free. “Cold” below means the first
passing timed invocation for that target in this execution; existing language
and package-manager caches were not deleted. No command deployed, pushed, or
mutated a live provider.

| Target                 | Selected work                                          | Cold wall | Warm wall | Cargo | Production artifact                            | Timed source revision                                                                              |
| ---------------------- | ------------------------------------------------------ | --------: | --------: | ----- | ---------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| `railway-central-host` | 3 commands: 11 Turbo tasks plus topology               |    12.19s |     7.67s | no    | web client/server and self-contained Coach App | `f9e81d4c91ac406d8aecba925c6beae49f130315`                                                         |
| `railway-coach-engine` | 3 commands: 6 Turbo tasks, release build, and topology |  2356.91s |   544.37s | yes   | optimized `chen-chess-coach-engine` server     | `296224b2618560df93f701095419d467f9a27a40`                                                         |
| `railway-maia`         | 2 commands: 1 Turbo task plus topology                 |    10.25s |     7.54s | no    | no; Railway owns the image build               | `296224b2618560df93f701095419d467f9a27a40`                                                         |
| `firebase-storage`     | 1 emulator step with 4 rules tests                     |    11.99s |     9.32s | no    | rules only                                     | cold: `f9e81d4c91ac406d8aecba925c6beae49f130315`; warm: `296224b2618560df93f701095419d467f9a27a40` |

The complete proof initially exposed that the Storage emulator test was also
discovered by the ordinary scripts test suite. The test now skips outside the
explicit emulator boundary and still runs all four assertions inside the
Storage gate. At final source revision
`7ec1905f8be9bd74a87d293677bd7f38e52ad90d`:

- the corrected Storage gate passed again in 8.740s of gate wall time;
- `release:proof` passed 28 of 28 tasks in 1764.997s;
- 59 focused release-gate, release-proof, and topology tests passed;
- scripts TypeScript and formatting checks passed;
- `bun install --frozen-lockfile` reported no changes; and
- `nix flake check` passed for the host system.

## Test plan

- `tooling/scripts/release-gate.test.ts`
  - pure path-to-target mapping;
  - multiple-match deduplication;
  - explicit target vs changed-path mode;
  - CLI usage errors;
  - failed ref resolution;
  - stable plan JSON;
  - credential cleaning;
  - historical TS-only and Rust-only ranges;
  - command-language exclusions per target;
  - Firebase hook-to-target mapping;
  - `AGENTS.md` and `README.md` contain the sync-only no-gate policy;
  - neither file retains an unconditional `release:proof`-before-push
    instruction; and
  - both files explain that a Railway-matching push is a deployment trigger and
    therefore runs only the selected Railway gate.
- `tooling/scripts/release-proof.test.ts`
  - all existing process lifecycle, timing, environment, plan, and runtime
    certification tests remain;
  - shared helper extraction adds no behavior regression.
- `tooling/scripts/central-topology.test.ts`
  - registry equals Railway watch patterns;
  - every local Docker `COPY` input is watched;
  - exact files and directory globs both work;
  - uncovered inputs produce actionable failures.
- `tooling/scripts/storage-rules.test.ts`
  - anonymous read denied;
  - anonymous write denied;
  - authenticated read denied;
  - authenticated write denied.
- Existing web, Coach App, UI, SDK, Rust, and Maia suites run only through
  their owning release targets.

## Done criteria

- [x] ADR 0024 supersedes ADR 0022's universal pre-push gate.
- [x] `release:proof` remains functional but no active instruction requires it
      before every push.
- [x] `AGENTS.md` explicitly tells agents not to run `release:proof` merely to
      synchronize a bookmark with remote `main`.
- [x] `README.md` documents the same sync-only/no-target behavior for human
      maintainers.
- [x] Agent documentation requires focused implementation checks without
      conflating them with deployment gates.
- [x] A docs-only Railway changed-path plan is a successful no-op.
- [x] A no-target synchronization path runs neither `release:gate` nor
      `release:proof`.
- [x] A Railway-matching synchronization runs only the selected Railway unit
      gates because that push triggers autodeploy.
- [x] The historical TypeScript-only range selects `railway-central-host` and no Cargo
      command.
- [x] The historical Rust-only range selects `railway-coach-engine` and no frontend or
      Maia command.
- [x] Every Railway Docker `COPY` source is covered by its service watch paths
      and typed release registry.
- [x] `railway-central-host`, `railway-coach-engine`, and `railway-maia` gates all pass.
- [x] `firebase-storage` passes credential-free emulator rule tests.
- [x] Firebase Hosting has no target, config, or deploy script; Railway owns
      every web portal.
- [x] The Firebase Storage partial deployment has a failing-closed predeploy
      hook.
- [x] `nix develop --command bun run release:proof` still exits 0.
- [x] A failed or interrupted gate leaves no owned descendants.
- [x] No live Railway, Firebase, GitHub, or package-registry mutation occurred
      during implementation.
- [x] `plans/README.md` marks Plan 005 DONE only after every criterion passes.

## STOP conditions

Stop and report back if:

- Plan 003 is not the actual parent source or its process-lifecycle excerpts
  have drifted.
- Railway GitHub autodeploy is disabled or the live service watch paths differ
  from checked-in Config-as-Code. The changed-push boundary would no longer
  match the deployment boundary.
- Firebase Storage rules are intended to permit an operation. Resolve the
  authorization policy before changing the deny-all tests.
- Storage emulator tests require production credentials or contact a
  production Firebase project.
- A Docker build input cannot be represented safely by Railway watch paths.
- The API gate cannot produce the release-mode server without adding live
  credentials.
- Focused gates expose a real cross-target contract that requires broader
  validation. Report the exact dependency and revise the target registry
  rather than silently restoring the global union.
- Any verification fails twice after a reasonable fix attempt.
- Implementation appears to require a file outside the in-scope list.

## Maintenance notes

- Treat `release-targets.ts` and Railway watch paths as one contract. Any
  Docker `COPY`, package dependency, or deployment surface change must update
  both in the same change.
- Reviewers should scrutinize false-negative selection more than false
  positives. A needless focused gate costs time; a missed release unit can
  deploy unvalidated source.
- Explicit target mode is the escape hatch for manual redeploys, ref-only
  deployments, and uncertainty. It must never consult a diff and decide to
  skip.
- Do not add new global task categories to every target. Add the narrow
  guarantee to the owning target and evidence its dependency.
- If the project later enables GitHub push CI and Railway "Wait for CI", the
  pure selector and target registry can be reused remotely. That is a separate
  deployment-policy decision, not required here.
- The full proof remains intentionally expensive. Optimize it independently
  only when whole-repository audit or Local Pipeline Runtime certification is
  the actual bottleneck.
