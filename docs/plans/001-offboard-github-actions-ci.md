# Plan 001: Move CI to the local release proof

## Status

- **State**: DONE
- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: `66da2f6f` (TypeScript orchestration) and `7496b1dc`
  (Turborepo task graph)
- **Planned at**: `7496b1dc`, 2026-07-21
- **Refined at**: `a11ee4ca`, 2026-07-22

## Goal

Stop spending GitHub Actions budget on checks that the local development host
can run before each push. Keep GitHub Actions only for work that needs GitHub's
deployment credentials or hosted multi-platform builders.

## Final design

### Local CI owns verification

`bun run release:proof` is the single authoritative credential-free gate. Its
Turborepo task graph covers:

- Rust formatting, linting, tests, and production builds;
- frontend linting, typechecking, tests, and production builds;
- Maia service and repository script checks; and
- deterministic Pipeline Evaluation without hosted language-model credentials.

The proof removes hosted-provider credentials and local provider overrides from
its subprocess environment. Docker is required only when a digest-pinned runtime
manifest is supplied for Apple Silicon live-runtime certification.

### Jujutsu owns synchronization

After pointing the intended bookmark at `@`, the maintainer runs the proof and
then pushes with ordinary Jujutsu commands:

```sh
jj bookmark set <bookmark> -r @
bun run release:proof
jj git push --bookmark <bookmark>
```

The repository does not prescribe a remote transport or credential helper.
Developers use the SSH or HTTPS remote appropriate to their machine and
execution environment.

Jujutsu 0.33 has no native pre-push or pre-sync lifecycle hook and deliberately
does not invoke Git's `pre-push` hook. A repository-local `jj checked-push`
alias may combine the last two commands for convenience, but direct
`jj git push` remains an explicit bypass. This is an accepted single-maintainer
trust boundary, not remotely enforceable CI.

### GitHub Actions owns manual publication only

Delete the push-triggered CI workflow and the self-hosted Apple Silicon
certification workflow. Keep only `publish-maia-runtime.yml`, dispatched
manually from `main`, because it needs `GITHUB_TOKEN` package-write access and
hosted Linux amd64/arm64 builders.

Run Apple Silicon certification directly on the supported local host after
downloading the publication artifact's digest-pinned runtime manifest. Retain
the generated certification report and separate manual Codex judgment with the
release issue.

### Local workspace files stay local

`.claude/` contains checkout-local operator and agent material. Exclude it in
`.git/info/exclude`, remove it from the current tracked tree, abandon unpublished
`.claude/`-only changes, and preserve the files in the working directory as
ignored local files. Do not rewrite already-published history solely to purge
older copies, and do not add the pattern to the committed `.gitignore` unless
ignoring `.claude/` becomes a project-wide policy.

## Implementation

1. Add `build` to the platform-neutral `release:proof` Turborepo task union.
2. Extend release-proof tests to characterize the complete task union and the
   one-workflow manual-publication topology.
3. Delete `.github/workflows/ci.yml` and
   `.github/workflows/certify-local-runtime.yml`.
4. Keep `.github/workflows/publish-maia-runtime.yml` on `workflow_dispatch`,
   guard it to `main`, and update its pinned checkout action.
5. Remove the transport-specific GitHub synchronization script, package entry,
   and tests. Authentication workarounds belong to their execution environment,
   not the repository workflow.
6. Document the local proof, normal Jujutsu push, optional local alias, manual
   publication, direct-push trust boundary, and reduced cross-platform
   guarantee.
7. Keep `.claude/` ignored locally and absent from the revision being pushed.

## Platform guarantee

The local proof runs natively on the supported Apple Silicon development and
runtime-certification host. Explicit publication and multi-platform container
builds continue to exercise Linux. Windows portability is no longer claimed as
verified on every change. Restoring per-change multi-platform coverage would
require a separately approved local runner, VM, or remote-rules design.

Railway's GitHub integration is independent of GitHub Actions CI and remains
unchanged.

## Verification

Run:

```sh
bun test scripts/release-proof.test.ts
bun run typecheck
bun run lint
bun run release:proof
```

Confirm:

- `release:proof` includes `build` and every other required Turborepo task once;
- `publish-maia-runtime.yml` is the only workflow;
- the remaining workflow is manual, guarded to `main`, and contains no hosted
  language-provider credential;
- no package script or source file defines a GitHub synchronization wrapper;
- repository documentation does not prescribe SSH, HTTPS, or a credential
  helper;
- `jj checked-push --help` runs the proof and reaches Jujutsu's push command
  without contacting or mutating a remote; and
- `.claude/` is absent from `@`, its unpublished file-only sibling is abandoned,
  and its local files remain present and ignored.

Do not publish a runtime or mutate a GitHub remote merely to validate this
migration.
