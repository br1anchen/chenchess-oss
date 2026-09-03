# ADR 0023: Run Coach Engine behind a thin Node protocol adapter

## Status

Accepted.

Refined by ADR 0032, which names the public workspace Central Host and defines
its browser surfaces and Coach App artifact boundary.

This ADR supersedes ADR 0002's Convex/Better Auth provider choice, ADR 0006's
public Rust/static-host topology, and ADR 0020's no-schema-version rule only for
durable Review Session Checkpoints. The Vite client choice, Rust token
validation, co-located Stockfish workers, separate Maia service, and the
remaining real-seam contract rules stay in force.

## Decision

ChenChess has one public origin owned by `apps/central-host`. Its
client-rendered React web application and thin Node server are one product. The
Node server serves static web assets, routes `/api` to a private Rust service,
implements the MCP and MCP Apps protocol surface, runs the Coach OAuth
authorization server through `oidc-provider`, and serves the client-rendered
Coach App resource. It contains no coaching, Review Session, persistence
policy, authorization policy, or Firebase identity-verification logic.

The private Rust service is named **Coach Engine**. It verifies Firebase and
Coach access tokens, owns all Player authorization, application persistence,
Review Session lifecycle, admission, Stockfish orchestration, Maia integration,
and coaching business operations. Its deterministic **Game Review Engine**
remains a deep inner module rather than becoming the name of the whole
application service.

The Node adapter and Coach Engine run as separate Railway services. Maia remains
a third private Railway service. Only `apps/central-host` has a public domain; Node
reaches Coach Engine and Coach Engine reaches Maia over Railway's private
network.

Firebase Authentication remains the canonical sign-in system and Firebase
`uid` remains Player ID. Coach Engine alone verifies Firebase ID tokens. The
Node OAuth server issues resource-bound Coach JWTs after asking a narrow Coach
Engine identity endpoint to verify the browser Firebase token. Coach Engine
then independently validates every Coach JWT and authorizes every addressed
object.

Durable data remains structured JSON in Firestore. Coach application data lives
in the default database and is accessible only to Coach Engine. OAuth protocol
records live in a separate `coach-oauth` database and are accessible only to
Node. Separate service accounts receive per-database IAM access. Neither
browser application accesses Firestore directly. Cloud Storage is not an
application-data dependency.

Review Sessions remain transient product state but are durably checkpointed for
crash recovery. That checkpoint is a real persistence seam and therefore has a
schema version even though it is not Saved Game or review-history data.

## Consequences

- Node is retained for the mature MCP Apps and OAuth server ecosystems without
  becoming a second business backend.
- Node and Rust exchange only versioned Coach command/result DTOs. They never
  serialize the Review Session aggregate or Firestore representation between
  processes.
- The browser web product and Coach App remain separate Vite-built React clients
  that share `@chenchess/ui`; neither uses SSR in the first version.
- `@chenchess/coach-engine-sdk` owns the generated TypeScript contract, typed
  client, auth-neutral credential injection, account/retention operations, and
  common outcomes. Provider-specific Firebase and Coach OAuth implementations
  stay in application adapters.
- Removing Node later is an interoperability and security migration, not a
  cleanup. It requires parity for MCP Apps, OAuth registration, refresh,
  revocation, key rotation, restart recovery, and both-host behavior before the
  public seam moves.
