# Promote production by fast-forwarding a protected `prod` bookmark

## Status

Accepted.

This decision supersedes the production-promotion clauses of ADR 0025
(disabled Railway autodeploy, `central-host-v*` GitHub Release tags, and
`release:central-host` as the required promotion path). It declines ADR 0027's
SemVer Release Tuple and GitHub Release as the promotion record. Staging
autodeploy from `main`, `jj checked-push`, the ban on routine `railway up`,
and Railway Skipped Builds remaining disabled for `central-host` stay accepted.

## Context

ADR 0025 used a hybrid model: staging autodeploys from `main`, and production
stays disconnected from that push path. Promotion was a SemVer GitHub Release
plus an explicit Railway `serviceInstanceDeployV2` of that SHA. ADR 0027 then
proposed a second SemVer (`coach-app-v*`) and a Release Tuple note under
`docs/releases/`.

That machinery does not match how this monorepo is operated:

- No workspace package is published for other repositories to consume. A
  SemVer tag does not name an installable artifact.
- GitHub Actions is not ChenChess CI. Required status checks would invent a
  remote gate the repository has already declined (ADR 0022, ADR 0024).
- A GitHub Release is a second source of truth next to the git object that
  Railway already deploys.
- #400 already created remote `prod`, pointed Railway production
  (`central-host`, `coach-engine`, `maia`) at that bookmark, and left staging
  on `main`. Landing to `main` must not ship production.

The remaining gap is the written contract: protect `prod`, name the deployed
revision by git SHA, and stop telling operators to cut a GitHub Release.

## Decision

Production promotion is a fast-forward of the protected `prod` bookmark to a
certified `main` SHA. That SHA is the release version.

### Watch scopes

| Railway environment | Git bookmark | Autodeploy |
| ------------------- | ------------ | ---------- |
| staging             | `main`       | enabled    |
| production          | `prod`       | enabled    |

Production autodeploy is enabled from `prod`. Railway Wait for CI stays
disabled. There is no push-triggered GitHub workflow that production waits
on.

### What `prod` may contain

`prod` is never a development line. It points only at a commit that already
exists on `main`. The maintainer does not author unique commits on `prod`,
does not open pull requests whose base is `prod`, and does not
`jj spr land` onto `prod`. Cloud Agents never push `prod`. They may
advance `main` only when the user explicitly authorizes `jj spr land` or
`jj checked-push` (`jj-pr-boundary`).

Advancing `prod` is a Jujutsu bookmark move to the exact certified SHA,
then a push of that bookmark only:

```sh
jj bookmark set prod -r <certified-40-character-sha>
jj git push --bookmark prod
```

Do not use the GitHub merge button, a merge commit, or a raw `git merge` of
`main` into `prod`. Those create history Jujutsu did not author. "Merge
`main` to `prod`" means `prod` fast-forwards to that `main` SHA.

### Protection

A repository ruleset named `prod is promotion-only` targets
`refs/heads/prod` with `enforcement: active` and no bypass actors. It denies
deletion and non-fast-forward updates, and requires no status check, no
GitHub Actions run, and no pull request.

It does **not** carry a per-ref push restriction, so the clause above about
restricting who may push is satisfied by repository access rather than by the
ruleset: the maintainer is currently this repository's only collaborator.
That is a weaker and differently-scoped control, and it lapses silently the
first time a collaborator, deploy key, or app installation gains write
access. Adding one is the trigger to revisit this and add a ruleset `update`
rule whose bypass list names the maintainer.

Rulesets are a paid feature on a private repository. This one could not be
created until the account moved to GitHub Pro, which is why earlier revisions
of this decision recorded the protection as pending.

Linear history follows from fast-forward-only updates, not from a required
merge queue. The ruleset deliberately omits a linear-history rule: `main`
carries historical merge commits, and a promotion range containing one is a
normal fast-forward that the rule would refuse.

A ruleset cannot tell an authorized promotion from an accidental push, so
`tooling/scripts/checked-push.ts` refuses `--bookmark prod` outright. Gating
a push of new work is what that script is for; promotion is a fast-forward to
a SHA that is already certified, so a gate run at push time would measure the
wrong thing and its passing would read as permission.

### Version identity

The production version is the 40-character git SHA that `prod` points to.
Railway deployment metadata already names that SHA. Private Cargo and
workspace-package versions remain build metadata and need not advance.

Do not create `central-host-v*` or `coach-app-v*` tags as the promotion
record. Do not create a GitHub Release as the promotion record.
The superseded `release:central-host` helper and `docs/releases/` SemVer notes
from ADR 0025 / ADR 0027 have been removed.

Host-specific Coach App publication (a ChatGPT plugin review versus Claude's
live connector contract) stays a later, host-owned concern. It is not a git
identity.

### Before `prod` moves

1. The candidate SHA is on `main` and has deployed to staging.
2. Staging is green for that SHA (the three Railway units that the change
   selected).
3. The privacy-safe staging certification for that exact SHA is reviewed and
   records that production remained unchanged. Certification remains release
   evidence, not a repository fixture.
4. Dual review agrees to promote that SHA.

Changed-path Railway gates still run when `main` moves (`jj checked-push` or
the maintainer's pre-`jj spr land` plan). Promotion does not invent a second
gate suite.

### Rollback

`prod` moves only forward. To undo a production SHA, revert or fix on
`main`, certify the new SHA on staging, then fast-forward `prod` to that new
SHA. Do not force-push `prod` backward.

Routine releases do not use `railway up`. It remains an emergency or
diagnostic upload and never establishes production provenance. Railway
Skipped Builds stays disabled for `central-host` because its Vite output
embeds environment-specific build values; production rebuilds the named SHA
with production configuration.

## Consequences

Staging and production share one GitHub repository and diverge only by which
bookmark Railway watches. A push to `main` cannot ship production. The
deployed production version is the SHA already visible in Railway, git, and
the certification artifact.

Operators stop maintaining SemVer tags and GitHub Releases that no consumer
installs.

Protecting `prod` was an operator action on GitHub, done once. It does not
restore Actions as CI.
