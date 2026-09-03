# Compose one Central Host from browser surfaces and Coach App artifacts

ADR 0055 supersedes this decision's combined `/join` ownership by separating
Firebase identity at `/login`, Beta admission at `/join`, and the authorized
Player home at `/dashboard`.

## Status

Accepted.

## Context

The public deployment has grown beyond an authenticated Vite application. It
must serve a public Landing Page, the authenticated web product, OAuth and MCP
protocols, API relay traffic, legal pages, and fixture-only UI previews from one
origin. The repository also contains a Coach App whose `ui://` resources must
remain self-contained HTML artifacts for MCP hosts.

The old `apps/central-host` name obscures this deployment boundary. Its Vite development
server also made the authenticated application the default fallback, so a
nested `/preview/*` request could initialize Firebase even though previews are
public. Separately, `apps/coach-app` maintained production, move-sequence, and
preview Vite applications, allowing preview behavior to diverge from the exact
MCP artifacts.

Splitting every browser surface onto a separate Railway service and subdomain
would create deployment, cookie, CORS, CSP, and operational boundaries before
those surfaces have independent scale or ownership needs.

## Decision

Rename `apps/central-host` and its workspace package to `apps/central-host` and
`@chenchess/central-host`. The Central Host remains the only public Railway
service and keeps Node 22 with Express-compatible protocol handling. It owns
the public origin, Vite browser build, OAuth and MCP protocols, health endpoint,
and private Coach Engine relay.

Use one Vite project with entries only at browser bootstrap and trust
boundaries:

- `/` is a minimal public Landing Page.
- `/app/*` is the authenticated web product. It consumes the Firebase session
  established by the identity surface without rendering sign-up or sign-in
  journeys itself.
- `/join/*` owns Firebase sign-up, sign-in, email verification, provider
  linking, and invitation redemption journeys.
- `/preview/*` is one public, non-indexed Preview Catalog entry. Its explicit
  registry routes to fixture-only studies without authentication, product
  backends, storage, or real Player data.

Reserve `/backoffice` for its domain surface, but do not add an empty entry
before it is implemented. `/join` becomes a browser entry with the beta access
work tracked by issue #203; identity journeys arrive before invitation
redemption. Preview routes are owner-qualified:
`/preview/ui/*`, `/preview/web/*`, and `/preview/coach-app/*`. UI-package
specimens live in `packages/ui/src/preview`, web layout studies live in
`apps/central-host/src/preview`, and Coach App fixtures and harnesses live in
`apps/coach-app/src/preview`. The outdated shared-workspace preview is removed.

Keep `apps/coach-app` as a separate, non-deployable artifact-producing
workspace. One public build command performs one single-file Vite pass per MCP
resource, then writes:

```text
apps/coach-app/dist/manifest.json
apps/coach-app/dist/resources/review-session.sha256-<digest>.html
apps/coach-app/dist/resources/move-sequence.sha256-<digest>.html
```

The manifest binds each `ui://` URI to its MIME type, file, digest, and preview
fixture identifier. Each filename and URI contains the SHA-256 digest of the
exact HTML bytes, while the manifest records the source revision supplied by
the deployment build. MCP resource reads and Central Host previews resolve
artifacts through this manifest and one `COACH_APP_ARTIFACT_ROOT`. During
local development a Coach App watcher rebuilds this same directory. The
Preview Catalog loads the exact HTML as a Vite virtual module and renders it
as a sandboxed iframe `srcdoc`; there is no public `/preview-assets` route and
no separate Coach App preview build.

Rename the Railway service `coach-oauth` to `central-host` in place and rename
repository release targets to `railway-central-host`,
`railway-coach-engine`, and `railway-maia`. The repository contains only the
new `apps/central-host/railway.json`; there is no temporary `apps/central-host`
compatibility configuration. Railway's service directory/config path must be
updated after the repository change is pushed.

Local development uses `bun run dev` for Coach Engine plus Central Host and
`bun run dev:central-host` for the Node-owned public origin, Vite middleware,
and Coach App artifact watcher. Public Landing Page and Preview Catalog routes
must work without Firebase or OAuth configuration. Authenticated `/app` and
identity `/join` may show configuration guidance when public Firebase settings
are absent; protocol routes remain unavailable until their server
configuration is present.
Production startup stays fail-closed for required protocol configuration.

## Consequences

- One origin preserves simple OAuth, MCP, cookies, CSP, and deployment
  operations while browser bootstraps enforce their own trust boundaries.
- `/preview/*` cannot accidentally fall through to the Firebase-authenticated
  application entry.
- Firebase identity controls remain isolated to `/join`, while `/app` consumes
  the resulting session for the authenticated product.
- Previewed Coach App UI is byte-for-byte the resource returned to MCP hosts,
  with fixture behavior supplied by the parent preview harness.
- Coach App source ownership remains independent without adding another
  deployable service.
- The direct repository rename creates an intentional Railway handoff: a push
  cannot deploy the Central Host successfully until the service config path is
  changed from `apps/central-host/railway.json` to
  `apps/central-host/railway.json`.
- Separate services or subdomains remain available later if measured scaling,
  security, or team-ownership needs justify the additional boundaries.
