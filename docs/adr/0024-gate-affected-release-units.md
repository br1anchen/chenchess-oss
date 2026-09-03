# ADR 0024: Gate affected release units

## Status

Accepted. The environment-specific deployment policy is refined by ADR 0025.

## Context

ChenChess has three Railway release units (`web`, `api`, and `maia`) with
GitHub autodeploy enabled and one explicitly deployed Firebase product (Cloud
Storage Security Rules). Railway hosts every web portal; Firebase Hosting is
not a release product. The former local policy ran the
complete repository proof before every GitHub synchronization. That conflated
ordinary source synchronization with deployment, made nondeployable changes
pay for release work, and still did not prove the exact Railway or Firebase
deployment surfaces.

Railway watch paths already define which pushed paths trigger each service.
Firebase supports a Storage-specific partial deploy and predeploy hook. Those
are the release boundaries the local gates should follow.

## Decision

Routine GitHub synchronization is not itself a release gate. Before
synchronization, maintainers inspect the changed-path Railway plan. A push
matching a Railway watch scope is a deployment action because Railway
autodeploy is enabled, so only the affected Railway release units are gated
before that push. A push matching no Railway scope runs no release gate.

Firebase Storage Rules releases use one explicit command with a local gate
attached through a predeploy hook. An explicit target always runs even when no
source comparison would select it. There is no Firebase Hosting target because
Railway owns all web hosting.

The complete credential-free `release:proof` remains available for an
explicit whole-repository audit and for publishing or certifying the Apple
Silicon Local Pipeline Runtime. It is not the fallback for an uncertain
changed-path selection; selection or revision errors fail closed and must be
diagnosed.

The local gates remain a bypassable, single-maintainer trust boundary.
Railway and Firebase do not remotely enforce them. Direct synchronization or
deployment can bypass the process, and that bypass is accepted and documented
rather than presented as a remote guarantee.

## Consequences

Nondeployable work can synchronize without release validation. Railway work
pays only for the services the push will deploy, while shared inputs can select
multiple units. Firebase Storage remains a deliberate, separately gated
release.

The checked-in Railway watch paths and the typed local release-target registry
form one contract. Docker build inputs must be covered by that contract.
Railway still owns provider-side image builds and health checks; Firebase still
owns the actual product deployment. The exceptional full proof remains
intentionally broader and more expensive than the routine scoped gates.
