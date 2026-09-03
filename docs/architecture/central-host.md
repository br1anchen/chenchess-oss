# central-host (apps/central-host)

The public Node composition layer (ADR 0006, 0032): one origin serving every
browser surface and relaying `/api` to Coach Engine. It holds no domain logic —
every chess fact it serves came out of Coach Engine's contract.

## Request routing (`server.ts` → `createWebOrigin`)

```text
createWebOrigin
  /health                      → admitHealthRequest (boot id)
  /api/*                       → byte relay to COACH_ENGINE_BASE_URL
                                 (source-IP header on beta-access paths only)
  Firebase auth helper paths   → relay to firebaseAuthHelperOrigin
  everything else              → static Vite assets from staticRoot
```

The relay forwards bytes without parsing. The web app authenticates with
Firebase ID tokens that Coach Engine verifies — central-host never validates
identity itself (ADR 0002).

## WebMCP

There is no remote MCP endpoint here. The Coaching Board registers its own
tools in the page through `navigator.modelContext`, so a model looking at the
page drives the same board the Player is looking at, under the Player's own
session. `src/coaching-board/useCoachingBoardTools.ts` is the tool surface, and
`modelContextPolyfill.ts` fills the API in on browsers that lack it.

The hosted deployment that once exposed these tools to third-party model hosts
over an authenticated `/mcp` endpoint, with its own OAuth authorization server,
is not part of this snapshot.

## Browser surfaces (`src/pages/`, Astro static output)

`astro build` writes `dist/`; `server.ts` serves it and is never behind Astro
(`docs/adr/` and issue #424). Application surfaces stay client-rendered React
mounting into `#root`; marketing surfaces render at build time.

```text
src/pages/index.astro           → the static root page, no JavaScript
src/pages/app/                  → src/main.tsx: the Coaching Board
src/pages/login/, join/         → auth + beta invitation redemption
src/pages/privacy|terms|support → src/public/*.tsx, no JavaScript
src/pages/404.astro             → branded miss
src/pages/robots.txt.ts, sitemap.xml.ts
```

`bun run build` runs `verify:public-build` over `dist/` — the public pages must
stay readable without JavaScript, carry their own canonical, reference nothing
off-origin, and keep the application roots out of the sitemap.

Shared presentation lives in `@chenchess/review-projection` and
`@chenchess/ui`, not here.

## Topology

Three processes: the public central-host origin, and the private coach-engine
and maia services it reaches over loopback. `bun run local:up` runs all three.
