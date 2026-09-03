# Run CI before GitHub synchronization

## Status

Superseded by ADR 0024.

## Context

ChenChess is maintained in Jujutsu and has a complete credential-free release
proof backed by its Turborepo task graph. Running the same checks again on every
GitHub push consumed Actions budget while splitting ownership between local and
hosted task lists. The private repository's current GitHub plan does not make
branch protections or rulesets available, so a committed status check cannot
enforce the local gate remotely.

The Apple Silicon Local Pipeline Runtime also needs native installation and
live-runtime certification. Orchestrating that machine through a self-hosted
Actions job adds remote machinery without improving the proof itself. Publishing
the Maia container is different: it needs GHCR package write access and hosted
Linux amd64/arm64 builders.

## Decision

`scripts/release-proof.ts`, backed by Turborepo, is the authoritative local CI.
Maintainers run it through `nix develop --command bun run release:proof`
immediately before pushing with Jujutsu. Remote selection and authentication
remain machine-specific; the repository does not standardize SSH, HTTPS, or an
environment-specific credential helper.

Jujutsu 0.33 has no native pre-push or pre-sync hook and deliberately suppresses
Git's `pre-push` hook. A maintainer may define a repository-local
`jj checked-push` alias that runs the proof and then `jj git push`, but the alias
is local convenience rather than a committed transport wrapper.

GitHub Actions is reserved for explicitly dispatched deployment or distribution
work. The only current workflow manually publishes the Maia runtime from
`main`. Apple Silicon certification runs directly on the local Apple Silicon
host against the downloaded digest-pinned runtime manifest. Its JSON report and
the separate manual Codex judgment are retained with the release issue.

Direct `jj git push` can bypass this process because the current repository plan
cannot enforce it remotely. ChenChess accepts that single-maintainer trust
boundary and documents the bypass explicitly.

## Consequences

The normal local process gates native Apple Silicon compilation and behavior,
but the former three-OS hosted matrix is retired. Linux compilation is
exercised by explicit deployment and container builds. Windows portability is
no longer claimed as per-change verified. Continuous multi-platform coverage
would require a separately approved local runner, VM, or future remote-rules
design.

GitHub Actions remains enabled for manual publication, and Railway's GitHub
integration remains independent of this decision.
