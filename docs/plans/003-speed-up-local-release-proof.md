# Plan 003: Make the local release proof fast, observable, and process-safe

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
>   --from d0c14cff --to @ -- \
>   tooling/scripts/release-proof.ts \
>   tooling/scripts/release-proof.test.ts \
>   services/coach-engine/Cargo.toml \
>   services/coach-engine/tests/chenchess_cli.rs \
>   services/coach-engine/tests/review_session_transports.rs \
>   docs/local-ci.md
> ```
>
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding. A
> load-bearing mismatch is a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: none
- **Category**: perf, correctness, DX
- **Planned at**: commit `d0c14cff`, 2026-07-28
- **Execution status**: DONE on 2026-07-28

## Execution history

The plan was implemented and reviewed in isolated Jujutsu change
`lstmknkmnsnv`, then integrated into `main` after approval.

- Before the final process-lifecycle review revisions, the complete cold proof
  passed in 155.15 seconds; Turbo took 138.078 seconds with 0 of 27 executable
  tasks cached.
- In that same pre-revision state, a complete warm proof passed with Turbo
  taking 82.782 seconds and 21 of 27 executable tasks cached.
- Two other full-proof attempts ended when
  `chenchess-rust#review-session-contract-drift` exited 137. The second
  occurrence triggered this plan's explicit STOP condition.
- Independent review passed the 24 release-proof tests, both focused Rust
  integration binaries, the complete Cargo workspace suite, the Cargo
  no-run target audit, scripts format/lint/typecheck, and the 44-task Turbo dry
  graph.
- After operator-directed resumption, 95 contract-drift invocations completed
  without reproducing exit 137, including three forced full graphs with 0 of 27
  tasks cached. No evidence-backed source fix was justified.
- The exact final-source proof then passed with 27 of 27 tasks successful and
  24 cached in 94.302 seconds, followed by a fully cached 27-of-27 warm proof
  whose release-proof step took 0.966 seconds.
- Independent final review passed the exact proof again with 27 of 27 tasks
  cached; Turbo took 0.648 seconds, the release-proof step took 0.694 seconds,
  and total wall time including Nix setup was 53.07 seconds. No owned
  descendant survived.

## Why this matters

The authoritative local CI command took 192.44 seconds in a measured run even
though 25 of 27 executable Turbo tasks were cache hits. The same run spent
51.49 seconds in user CPU time and 141.02 seconds in system time. macOS
`syspolicyd` reached approximately one full core while Gatekeeper evaluated
newly created, ad-hoc-signed Rust executables carrying
`com.apple.provenance`.

A minimal local probe established the load-bearing trigger:

- launching the existing `target/debug/chenchess --help` took 267 ms;
- copying that same binary to a fresh temporary path and launching it exceeded
  five seconds in three consecutive probes; and
- removing `com.apple.provenance` from a throwaway copy reduced that launch to
  1.24 seconds.

The repository must not solve this by globally removing security metadata or
disabling Gatekeeper. It can avoid unnecessary executable copies, stop Cargo
from generating empty test harnesses, and ensure a failed or interrupted proof
does not leave Turbo and Cargo descendants consuming resources. The proof
should also report per-step wall time so a future regression identifies its
stage without another system-wide investigation.

## Current state

### The release proof launches one broad Turbo union without owning its process group

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

`tooling/scripts/release-proof.ts:171-211` directly spawns and awaits the
immediate child. It does not create an owned process group, propagate parent
termination, clean up descendants, or report elapsed time:

```ts
const child = Bun.spawn([...command], {
  cwd: ROOT,
  env: environment,
  stdin: inputText === undefined ? "ignore" : new Blob([inputText]),
  stdout: capture ? "pipe" : "inherit",
  stderr: capture ? "pipe" : "inherit",
})
// ...
const [exitCode, capturedStdout, capturedStderr] = await Promise.all([
  child.exited,
  stdout,
  stderr,
])
```

During diagnosis, a failed invocation left the complete
`release-proof -> turbo -> cargo test` descendant tree alive for more than 30
minutes after failure output had been reported. Repeated proof attempts then
contended with the leaked work.

Bun 1.3.13's checked-in type declarations support the required ownership seam:
`Bun.spawn` accepts `detached: true` on POSIX, which creates a new session and
process group. The implementation can therefore terminate the owned group
without matching process names or touching unrelated user processes.

### Rust integration tests create six avoidable executable copies

`services/coach-engine/tests/chenchess_cli.rs:532-605` creates one fixture per
four tests. Each fixture copies Cargo's already-built CLI to a new temporary
path:

```rust
let cli_path = root.join("chenchess-test");
fs::copy(env!("CARGO_BIN_EXE_chenchess"), &cli_path)
    .expect("test CLI should be copied outside the installed runtime");
```

The Cargo-provided executable is already outside the installed Local Pipeline
Runtime, so the copy does not establish an additional product invariant.

`services/coach-engine/tests/review_session_transports.rs:53-61` and
`:130-140` make two more temporary copies before exercising JSONL and PTY
transport behavior:

```rust
let cli_path = cli_dir.join("chenchess-test");
fs::copy(env!("CARGO_BIN_EXE_chenchess"), &cli_path)
    .expect("test CLI should be copied outside the installed runtime");
let mut child = Command::new(&cli_path);
```

These tests need temporary directories for the command FIFO and other
ephemeral files. They do not need a new executable inode or path.

### Cargo auto-discovers four binary targets with empty test harnesses

`services/coach-engine/Cargo.toml` has no explicit `[[bin]]` target metadata, so
Cargo auto-discovers every file under `src/bin/` and builds a test harness for
each by default. The current workspace test output confirms zero tests in:

- `generate_review_session_contract`;
- `prototype_chronological_selector` (removed after this plan was written);
- `prototype_coach_oauth_verify` (removed after this plan was written); and
- `prototype_positive_highlights` (removed after this plan was written).

Do not disable test harnesses for these targets, which currently contain real
tests:

- the default `chen-chess-coach-engine` binary;
- `chenchess`;
- `capture_review_session_recording`; and
- `measure_review_session_primitives`.

### The broad Rust task graph is a secondary cost, not yet a justified rewrite

Turbo's experimental Cargo workspace graph currently expands the full proof
into eight Cargo commands: build, check, format, clippy, test, two integrity
commands, and deterministic evaluation. On the diagnosed checkout, clippy and
workspace tests were the two cache misses; they ran concurrently and took 80
seconds and 46.72 seconds respectively.

Do not remove any verification category: ADR 0022 and `docs/local-ci.md` make
this proof the authoritative local CI gate. Do not blindly serialize the whole
Turbo graph either; that can replace resource contention with the sum of every
task's wall time. First land the executable-churn and process-ownership fixes,
then measure the remaining graph on a healthy host.

## Commands you will need

| Purpose            | Command                                                                                              | Expected on success                                                  |
| ------------------ | ---------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| Inspect state      | `jj status`                                                                                          | existing unrelated working-copy changes remain visible and untouched |
| TypeScript tests   | `nix develop --command bun test tooling/scripts/release-proof.test.ts`                               | all release-proof tests pass                                         |
| Focused Rust tests | `nix develop --command cargo test --workspace --test chenchess_cli --test review_session_transports` | both integration test binaries pass                                  |
| Cargo target check | `nix develop --command cargo test --workspace --no-run`                                              | no test executables are produced for the four zero-test binaries     |
| Turbo graph        | `nix develop --command bun run turbo run check test lint build typecheck --dry=json`                 | exits 0 and still contains every required verification category      |
| Full proof         | `nix develop --command bun run release:proof`                                                        | exits 0 with no hosted-provider credentials                          |
| Timed proof        | `/usr/bin/time -lp nix develop --command bun run release:proof`                                      | exits 0 and prints total plus per-step timing                        |
| Diff               | `jj diff --stat`                                                                                     | only in-scope files and the plan status are changed by this work     |

## Suggested executor toolkit

- Use the `diagnosing-bugs` skill if the focused timing still reproduces a
  process-launch stall after the executable copies are removed.
- Use the `onevcat-jj` skill for all version-control operations. This is a
  colocated Jujutsu repository; do not use Git staging, commits, stashes, or
  checkouts.

## Scope

**In scope**:

- `tooling/scripts/release-proof.ts`
- `tooling/scripts/release-proof.test.ts`
- `services/coach-engine/Cargo.toml`
- `services/coach-engine/tests/chenchess_cli.rs`
- `services/coach-engine/tests/review_session_transports.rs`
- `docs/local-ci.md`
- `plans/README.md` for the final status update only

**Out of scope**:

- `turbo.json` and the set of proof categories. Graph consolidation requires
  a separate before/after benchmark after this plan lands.
- `flake.nix`, Nix derivations, and Rust toolchain versions. A no-op
  `nix develop --command true` took only 2.68 seconds.
- Global or repository-wide `xattr` removal, Gatekeeper changes, code signing
  identities, notarization, and corporate endpoint-security configuration.
- Product/runtime executable installation and copying in
  `services/coach-engine/src/local_runtime.rs`; those copies are real product
  behavior, not test-fixture duplication.
- Docker-based Apple Silicon live-runtime certification. The ordinary
  credential-free proof must be fast and correct before certification is
  revisited.
- Unrelated existing working-copy changes under `apps/coach-app`,
  `apps/central-host/railway.json`, or `tooling/scripts/central-topology.test.ts`.

## Jujutsu workflow

- Work in the current operator-selected change or in a dedicated Jujutsu
  workspace if the operator provides one. Do not create a bookmark for
  local-only work.
- Before every edit, inspect `jj status` and preserve the existing unrelated
  changes.
- Use logical change descriptions matching the repository's recent style, for
  example `perf: avoid redundant CLI executable scans` or
  `fix: terminate release proof process groups`.
- Do not push or create a bookmark unless the operator explicitly requests it.

## Steps

### Step 1: Add deterministic process-ownership regression tests

In `tooling/scripts/release-proof.test.ts`, add a POSIX-only integration test
for the process seam before changing `runStep`:

1. Start a small Bun child through `runStep`.
2. Have that child start a long-lived grandchild and communicate the
   grandchild PID through a temporary file or captured stdout.
3. Make the immediate child exit non-zero.
4. Assert that `runStep` rejects.
5. Poll `process.kill(pid, 0)` for a short bounded interval and assert the
   grandchild no longer exists.
6. Clean the temporary directory in `finally`.

Add a second focused test for elapsed-time reporting. Inject a clock or expose
a small formatter rather than using a real multi-second sleep. Assert both
successful and failed steps report their name and a non-negative duration.

The tests must never send a signal to the current test runner's process group.
They must own a distinct child group and use the exact recorded PID/group ID.

**Verify**:

```sh
nix develop --command bun test tooling/scripts/release-proof.test.ts
```

Expected before Step 2: the new descendant-cleanup test fails for the current
implementation while existing tests remain green.

### Step 2: Give every release-proof step explicit process ownership

Refactor `tooling/scripts/release-proof.ts` around one private process-lifecycle
helper used by `runStep` and `runCoachSkillReviewJourney`:

1. On POSIX, spawn external step commands with `detached: true`; the direct
   child becomes leader of a new session/process group.
2. On Windows, retain direct-child termination behavior. Do not claim or
   emulate process-group guarantees that Bun does not provide there.
3. Register scoped `SIGINT` and `SIGTERM` handlers while a step is active.
   Forward the signal to the owned POSIX process group, or to the direct child
   on Windows.
4. In `finally`, remove every scoped signal handler.
5. After any direct-child exit, check whether the owned group still contains
   descendants and terminate them. Also terminate the group when the direct
   child fails, the parent is interrupted, output decoding throws, or
   validation after spawn fails. Send `SIGTERM`, wait for a short bounded grace
   interval, then use `SIGKILL` only for a group that still exists.
6. Treat `ESRCH` as already cleaned. Propagate every other process-management
   error instead of hiding it.
7. Preserve current stdout/stderr capture and replay behavior.
8. Preserve exit-status semantics, including expected-failure steps and
   conventional interrupted exits.

Use monotonic time (`performance.now()`) around every `runStep`. Emit one
stable diagnostic on both success and failure:

```text
release proof: <step-name> finished in <seconds>s
```

Do not add a metrics library or a persisted timing file.

**Verify**:

```sh
nix develop --command bun test tooling/scripts/release-proof.test.ts
```

Expected: the descendant-cleanup and timing tests pass, and the existing 18+
release-proof tests remain green.

### Step 3: Execute the Cargo-provided CLI directly in Rust integration tests

In `services/coach-engine/tests/chenchess_cli.rs`:

1. Set `RuntimeFixture::cli_path` directly from
   `env!("CARGO_BIN_EXE_chenchess")`.
2. Remove only the executable copy and its copy-specific expectation text.
3. Keep the unique fixture root, fake Stockfish script, fake Maia server,
   input files, and cleanup behavior unchanged.
4. Keep the two direct `--help` tests on the same Cargo-provided executable.

In `services/coach-engine/tests/review_session_transports.rs`:

1. Start both CLI subprocesses directly from
   `env!("CARGO_BIN_EXE_chenchess")`.
2. Keep the temporary directory for FIFO and transport-owned files.
3. Preserve PTY setup, signals, timeouts, assertions, and directory cleanup.

Do not hard-link and do not strip xattrs. Direct execution is simpler and
preserves the actual contract under test: the Cargo-built CLI runs outside an
installed Local Pipeline Runtime with explicit test providers.

**Verify**:

```sh
nix develop --command cargo test --workspace \
  --test chenchess_cli \
  --test review_session_transports
rg -n 'fs::copy\(env!\("CARGO_BIN_EXE_chenchess"\)' \
  services/coach-engine/tests
```

Expected: all focused Rust tests pass; `rg` returns no matches. On a warm build,
the test execution phase must not leave any individual trivial `--help` test
running for 60 seconds.

### Step 4: Stop generating empty Cargo test harnesses

Add these explicit `[[bin]]` sections to
`services/coach-engine/Cargo.toml`:

```toml
[[bin]]
name = "generate_review_session_contract"
path = "src/bin/generate_review_session_contract.rs"
test = false

[[bin]]
name = "prototype_chronological_selector"
path = "src/bin/prototype_chronological_selector.rs"
test = false

[[bin]]
name = "prototype_coach_oauth_verify"
path = "src/bin/prototype_coach_oauth_verify.rs"
test = false

[[bin]]
name = "prototype_positive_highlights"
path = "src/bin/prototype_positive_highlights.rs"
test = false
```

Those three `prototype_*` bins were later removed; the snippet is historical.

Each section must preserve the current target name and source path. Do not set
`autobins = false`; do not change how production binaries are built. Do not
disable the real unit tests in `chenchess`,
`capture_review_session_recording`, `measure_review_session_primitives`, or the
default server binary.

**Verify**:

```sh
nix develop --command cargo test --workspace --no-run
nix develop --command cargo test --workspace
```

Expected: the workspace passes; Cargo no longer lists test harness executables
for the four named zero-test binaries, while all existing real binary tests
still run.

### Step 5: Document the supported diagnosis boundary

Add a concise troubleshooting subsection to `docs/local-ci.md`:

- the proof now reports per-step wall time;
- a slow `nix develop --command true` indicates Nix/environment setup, while a
  fast no-op shell and slow Rust process launch indicate a host execution-policy
  or endpoint-security problem;
- inspect rather than disable platform security;
- do not globally clear `com.apple.provenance` as a repository workflow;
- after interruption or failure, no `release-proof`, Turbo, Cargo, or Coach
  Engine descendants should remain from that invocation.

Keep ADR 0022's decision intact: the complete credential-free proof remains
the authoritative local CI before Jujutsu synchronization.

**Verify**:

```sh
rg -n 'per-step|provenance|descendant|Gatekeeper|execution-policy' \
  docs/local-ci.md
```

Expected: the troubleshooting boundary is discoverable without prescribing a
security bypass.

### Step 6: Run the complete proof twice and decide whether graph work remains

First verify there is no pre-existing proof/Cargo process from an earlier run.
Do not terminate a process unless its ownership is known.

Run:

```sh
/usr/bin/time -lp nix develop --command bun run release:proof
nix develop --command bun run release:proof
```

The first run exercises changed Rust inputs; the second confirms the warm
Turbo-cache path. Record in the handoff or change description:

- total wall time;
- `turborepo-verification` wall time from the new step diagnostic;
- Turbo task count and cache-hit count;
- Rust test execution time, separate from compilation;
- whether any descendant survived success, failure, or an intentional
  interrupt.

If the proof passes and the avoidable scans/process leaks are gone, do not
change `turbo.json` in this plan. If a healthy-host measurement still shows
multiple concurrently executing Cargo commands as the dominant cost, open a
separate plan to benchmark a Rust-only dependency chain or bounded Turbo
concurrency. Require an alternating before/after benchmark and at least a 15%
median wall-time improvement before accepting a graph change.

**Verify**:

```sh
nix develop --command bun run turbo run \
  check test lint build typecheck --dry=json
jj diff --stat
```

Expected: all proof categories remain in the graph; only in-scope files and the
plan status changed.

## Test plan

### TypeScript orchestration

Extend `tooling/scripts/release-proof.test.ts`, matching its current use of
real Bun subprocesses for CLI-boundary tests and injected runners for cleanup
logic:

- failed immediate child with a live grandchild cleans the full owned group;
- successful step reports a stable elapsed-time diagnostic;
- failed step also reports elapsed time and preserves the original error;
- signal handlers are removed after each step;
- expected-failure steps retain their current semantics.

### Rust CLI behavior

Keep every existing test in:

- `services/coach-engine/tests/chenchess_cli.rs`; and
- `services/coach-engine/tests/review_session_transports.rs`.

The change must alter only executable location/ownership, not assertions,
provider fixtures, JSONL protocol behavior, FIFO behavior, or shutdown
semantics.

### Cargo target behavior

Run the entire workspace suite after adding `test = false`. Confirm the four
empty harnesses disappear and the binaries with real tests still execute those
tests.

### End-to-end release gate

The final acceptance test is the exact trusted command from `AGENTS.md` and
ADR 0022:

```sh
nix develop --command bun run release:proof
```

It must pass without hosted-provider credentials and without leaving
descendants.

## Done criteria

- [x] `nix develop --command bun test tooling/scripts/release-proof.test.ts`
      exits 0.
- [x] The process-ownership test proves a failed direct child cannot leave its
      long-lived grandchild running.
- [x] Every release-proof step reports elapsed wall time on success and failure.
- [x] `rg -n 'fs::copy\(env!\("CARGO_BIN_EXE_chenchess"\)' services/coach-engine/tests`
      returns no matches.
- [x] Both focused Rust integration test binaries pass using Cargo's original
      CLI path.
- [x] The four named zero-test binaries have `test = false`; binaries with real
      tests retain their harnesses.
- [x] `nix develop --command cargo test --workspace` exits 0.
- [x] `nix develop --command bun run release:proof` exits 0 twice, including
      the warm-cache run.
- [x] No process owned by either proof invocation survives its success,
      failure, or intentional interruption.
- [x] The Turbo dry graph still covers check, test, lint, build, and typecheck.
- [x] No global xattrs, Gatekeeper settings, code-signing policy, or corporate
      security configuration changed.
- [x] `jj diff --stat` shows no executor-created changes outside the in-scope
      list.
- [x] The Plan 003 row in `plans/README.md` is updated to DONE.

## STOP conditions

Stop and report back; do not improvise if:

- an in-scope file no longer matches the load-bearing excerpts;
- direct use of `CARGO_BIN_EXE_chenchess` changes any CLI test behavior beyond
  process-launch latency;
- a supposedly zero-test binary contains or gains a real unit test;
- owned POSIX process groups cannot be implemented with the installed Bun
  version without shelling out to process-name matching;
- process-group cleanup risks signaling the parent test runner or an unrelated
  process;
- the focused tests still stall while launching the original Cargo executable;
- macOS execution-policy or endpoint-security behavior remains the dominant
  cost after the redundant copies are removed;
- the full proof fails twice for a reason unrelated to the implementation; or
- completing a step requires modifying an out-of-scope file.

In the host-policy cases, report the timing, `syspolicyd` evidence, and exact
blocked command. Do not disable security controls or clear repository-wide
provenance metadata.

## Maintenance notes

- Any future integration test using `CARGO_BIN_EXE_chenchess` should execute
  that path directly unless the copy/install behavior itself is the contract
  under test.
- Any new `src/bin/*.rs` target with no unit tests should explicitly declare
  `test = false`; remove that setting as soon as tests are added.
- `runStep` and `runCoachSkillReviewJourney` must share one process-ownership
  implementation so their signal and cleanup semantics cannot drift.
- Reviewers should scrutinize process-group IDs, signal-handler removal, and
  escalation from `SIGTERM` to `SIGKILL`; a fast proof is not worth signaling
  an unrelated process.
- Turbo graph consolidation is intentionally deferred. Revisit it only with a
  same-host benchmark after this plan, because serializing independent checks
  can increase total wall time on a healthy machine.
