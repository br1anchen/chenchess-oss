# ADR 0025: Automate staging and promote versioned production releases

## Status

Accepted for staging autodeploy from `main`, `jj checked-push`, Railway
Skipped Builds remaining disabled for the web unit, and the ban on routine
`railway up`. Production promotion (disabled autodeploy, SemVer GitHub
Release tags, and `release:central-host` as the required path) is superseded
by ADR 0052.

## Context

ADR 0024 established changed-path Railway release gates, but the live Railway
project was still deployed from workstation uploads. Those deployments carried
CLI caller metadata rather than a GitHub repository and commit, so a push to
`main` could not trigger the documented watch scopes. The repository-local
`jj checked-push` alias had also drifted back to the complete
`release:proof`.

Workstation uploads are useful for diagnosis, but they are a poor routine
release interface: Railway cannot identify the GitHub commit, a clean checkout
is only a maintainer convention, and production cannot name or reproduce a
release from source provenance alone.

The Central Host does not yet have a production release history. It has no
Central Host tags or GitHub Releases, and its web build embeds
environment-specific Vite configuration. Automatically deploying every
`main` commit to production would therefore conflate continuous staging with
an intentional product release.

## Decision

The repository uses a hybrid deployment model.

Staging connects every Railway release unit to the GitHub repository and
automatically deploys the watched units from `main`. Every maintainer push that
can select a Railway unit goes through the repository-maintained
`checked-push` module via the repository-local `jj checked-push` alias. The
module plans the exact bookmark range, runs only the selected gates, and calls
`jj git push` only after they pass.

This remains a trusted-maintainer interface. Jujutsu does not invoke Git's
`pre-push` hook, direct `jj git push` remains a bypass, and the current private
GitHub plan cannot require checks on `main`. Railway Wait for CI stays disabled
until the repository intentionally restores a push-triggered GitHub workflow;
an absent check is not a gate.

Production may connect to the same repository for source provenance, but
autodeploy stays disabled. A production release:

1. has a unique `central-host-v<major>.<minor>.<patch>` GitHub Release tag;
2. resolves to one immutable Git commit;
3. plans and runs scoped gates from the previous production revision to that
   commit;
4. validates one privacy-safe staging certification artifact that proves the
   exact API, Maia, and web deployment IDs and revision, reviewed topology,
   first-party and payload gates, MCP conformance, and
   independent ChatGPT and Claude journeys; Daily Coaching candidates also
   enumerate their 18 conformance journeys, five correctness invariants,
   deployed checks, and completed beta soak;
5. deploys each selected production release unit by that exact commit SHA; and
6. records the prior production revision as the rollback target.

`bun run release:central-host` owns version validation, immutable source
resolution, staging-certification validation, changed-path selection, and gate
execution. The certification is validated against the resolved candidate
before even a list-only manifest is emitted. It prepares a release; it
deliberately does not mutate GitHub or Railway. Creating the GitHub Release and
deploying its exact SHA remain explicit operator actions.

Routine releases do not use `railway up`. It remains an emergency or diagnostic
upload and never establishes production provenance. Railway Skipped Builds
stays disabled for the web release unit because its Vite output contains
environment-specific build values; production rebuilds the named source
revision with production configuration.

## Consequences

Staging continuously exercises the same GitHub source path that production
releases use, while production changes only at a named promotion point.
Railway deployment history becomes attributable to Git commits instead of
workstation snapshots.

The checked-push module gives the local gate one small, testable interface and
keeps the Jujutsu alias shallow. It does not pretend to be remote enforcement.
A future move to a GitHub plan with protected branches can add a
push-triggered scoped workflow and Railway Wait for CI without changing the
release-target registry.

Production builds may repeat work already performed in staging. That is
intentional for the web release unit because build-time environment values
make cross-environment image reuse unsafe.

The staging certification is release evidence, not a repository fixture. It
contains only allowlisted revisions, deployment IDs, measurements, counts,
protocol observations, and pass/fail facts. Short-lived OAuth state used by
live conformance remains outside the artifact and outside the repository.
