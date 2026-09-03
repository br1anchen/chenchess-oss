# Coach App product and implementation specification

Decision date: 2026-07-26. Resolves
[Lock the Coach App product and implementation specification](#69)
on the
[Design and prove the cross-host Coach App](#62)
map.

## Status and authority

This is the decision-complete specification for the first non-production Coach
App implementation. Where it conflicts with earlier Coach App research, this
document and [ADR 0023](../adr/0023-run-coach-engine-behind-thin-node-protocol-adapter.md)
take precedence.

The verified staging prototype remains evidence for host and OAuth feasibility,
not an implementation baseline. Its source and live service are disposable.
Repository restructure issue
#118 must be refined to the
target boundaries below and completed before product implementation.

## Outcome

ChenChess exposes the same grounded interactive Review Session
through three delivery surfaces:

- the web application, whose configured LLM Explainer remains its Language
  Layer;
- the Coach App installed in ChatGPT;
- the same Coach App installed in Claude.

The ChatGPT or Claude host model is the Coach App's Language Layer. The Coach
App never invokes the web application's language provider. Every surface uses
the same Rust-owned chess facts, Review Session transitions, publication
validation, authorization, persistence policy, cancellation, and failure
semantics.

The first version has one public origin, two client-rendered React products,
one thin public Node adapter, one private Rust Coach Engine, and one private
Maia service. It uses Firebase Authentication and two IAM-isolated Firestore
databases. It uses no application blob storage and no SSR.

## Product contract

### Core journey

1. The Player signs in with Firebase through the web product or through the
   Coach OAuth login/consent interaction started by ChatGPT or Claude.
2. The Player supplies either a public Lichess game URL or raw PGN. In a Coach
   App host this happens in chat; the iframe does not duplicate the host
   composer with an import form. The standalone web product collects the same
   natural-language request in one conversation composer, not separate source,
   Review Side, Elo, or session controls.
3. Coach Engine validates and normalizes the completed standard-chess Game,
   resolves Review Side and Elo Profile inputs, persists a short-lived Game
   Import Record, and returns an opaque Game Import ID.
4. The conversational `review_game` workflow immediately starts a Review
   Session after import. Session start prepares all Automatic Critical Moments
   as equal chronological peers under the existing atomicity boundary; the
   Player never copies an ID or presses a separate start control. Optional
   intent enrichment is computed ephemerally when an unpublished moment is
   opened.
5. The Coach App iframe displays graphical context only: interactive board,
   real-game evaluation graph, Critical Moment picker, and factual evaluation
   labels. Selecting a moment redraws locally first and updates host model
   context. The standalone web product composes the same graphical primitives
   with its adjacent conversation view.
6. The Player discusses a plan, asks follow-ups, and may request a one-shot
   Player Plan Evaluation in the surrounding host chat (or the web conversation
   view), not an iframe composer. Board and Critical Moment
   selection, position inspection, Alternative Move preview, and cancellation
   remain interactive graphical actions. Canonical coaching content still
   renders only from successful Coach Engine-validated results.
7. A valid Review Session may resume after reconnect or process loss using its
   opaque Review Session ID. There is no session listing, browsing, or chat
   synchronization.

### Surface parity and differences

The board, evaluation graph, Critical Moment picker, responsive layout,
interaction hooks, presentation models, and ephemeral graphical behavior are
shared through `@chenchess/ui`. Conversation and import composition remain
surface-specific: host chat for Coach Apps and the adjacent conversation view,
including its initial review request, for web.

The web controller calls Coach Engine through
`@chenchess/coach-engine-sdk` and supplies Firebase credentials. The Coach App
controller implements the same host-neutral UI action interface through the MCP
Apps bridge. Host detection, MCP methods, Firebase, routing, and persistence
never enter the shared UI package.

Web continues to author prose through its existing LLM Explainer and Grounding
Gate. A Coach App supplies Coach Engine facts to the host model and must submit
host-authored Review Moment Comments and Coach Turns for Rust validation before
the workspace renders them. Native host narration may discuss returned facts
but cannot become canonical workspace content.

### Session ownership across surfaces

Review Session authorization binds to Firebase Player ID, not the creating
surface. The same authenticated Player who possesses the opaque identifier may
resume a session from web, ChatGPT, or Claude. The product does not list,
discover, automatically hand off, or synchronize host conversation around that
session.

Web stores only the current Review Session ID in Player-scoped local storage and
clears it on sign-out or terminal expiry. Coach App tool results retain the ID
in host-visible result state for widget or conversation reuse. Node stores no
host-conversation mapping. If a host discards both its tool history and widget
state, the still-valid checkpoint is intentionally undiscoverable.

## Architecture

```text
public HTTPS origin
└── apps/central-host — Vite React SPA + thin Node server
    ├── static web assets
    ├── /api/* same-origin routing
    ├── /mcp and MCP Apps resources
    ├── OAuth discovery, DCR, authorize, token, revoke, consent, JWKS
    ├── built apps/coach-app HTML resource
    └── coach-oauth Firestore database
             │
             │ versioned commands/results + end-user bearer
             ▼
services/coach-engine — private Rust service + Stockfish workers
    ├── Firebase ID-token and Coach JWT verification
    ├── Player/object authorization and compute admission
    ├── Review Session application layer and persistence
    ├── deterministic Game Review Engine
    ├── default Firestore database
    └── private Maia adapter
             │
             ▼
services/maia — private Python model service
```

These are three separate Railway services, not a multi-process container:

- `apps/central-host` is the only public service;
- `services/coach-engine` is private and co-locates its Stockfish worker pool;
- `services/maia` is private and independently sized.

Each has its own health check and restart boundary. Node reaches Coach Engine
and Coach Engine reaches Maia through Railway private networking.

### Node boundary

Node owns only:

- static web and built Coach App resource serving;
- same-origin `/api` byte routing;
- Streamable HTTP and MCP Apps negotiation;
- MCP tool/resource metadata and result-envelope projection;
- best-effort progress and cancellation relay;
- OAuth authorization-server protocol through `oidc-provider`;
- Firebase login and consent pages that collect, but do not verify, a Firebase
  ID token;
- access/refresh token issuance, rotation, revocation, and JWKS publication;
- an `oidc-provider` adapter to the isolated `coach-oauth` Firestore database.

Node never owns or reconstructs a Review Session aggregate, decides Player
authorization, calls Stockfish or Maia, authors coaching, writes Coach
application collections, or implements a web business endpoint.

### Coach Engine boundary

Coach Engine owns:

- Firebase ID-token verification and the OAuth login identity bridge;
- Coach JWT verification and every Player/object authorization decision;
- Game Import, Review Session, account/preference, retention, publication, and
  all other coaching business operations;
- the Game Review Engine and its Grounding Gate;
- Review Session recovery, persistence policy, aggregate revisioning, and
  idempotency;
- Stockfish orchestration, Maia integration, admission, deadlines, rate limits,
  and caches;
- all default-database Firestore adapters;
- the versioned Coach command/result interface.

The Game Review Engine remains the deterministic inner module that creates
facts and admits grounded outputs. Coach Engine is the broader application
service.

## Repository and module boundaries

The target monorepo topology is:

```text
apps/
  web/                  public React product and thin Node adapter
  coach-app/            ChatGPT/Claude MCP Apps React client

services/
  coach-engine/         private Rust application service
  maia/                 private Python model service

packages/
  coach-engine-sdk/     @chenchess/coach-engine-sdk
  ui/                   @chenchess/ui

tooling/
  scripts/              repository verification and operational tooling
```

The private workspace identities are:

| Path                        | Package identity                       |
| --------------------------- | -------------------------------------- |
| `apps/central-host`         | `@chenchess/central-host`              |
| `apps/coach-app`            | `@chenchess/coach-app`                 |
| `services/coach-engine`     | Rust package `chen-chess-coach-engine` |
| `services/maia`             | `@chenchess/maia`                      |
| `packages/coach-engine-sdk` | `@chenchess/coach-engine-sdk`          |
| `packages/ui`               | `@chenchess/ui`                        |
| `tooling/scripts`           | `@chenchess/scripts`                   |

The root `chen-chess-coach` package remains a private workspace coordinator,
not a deployable. Package renames are part of the structural migration so
future task selectors describe the accepted product boundaries.

`@chenchess/coach-engine-sdk` contains Rust-generated command/result types,
schemas and decoders, the typed Coach Engine client, account/preference and
retention operations, auth-neutral credential injection, and shared error,
retry, and authorization outcomes. Web binds a Firebase ID-token supplier; Node
forwards a Coach OAuth bearer. Coach App delegates authorization to its host and
does not store or refresh tokens.

The SDK does not implement Firebase Authentication, `oidc-provider`, login,
consent, cookies, or token storage. `@chenchess/ui` does not implement HTTP,
MCP, Firebase, OAuth, routing, durable persistence, or domain authorization.

Both frontend products compile the shared React source into separate Vite
builds. Neither uses SSR in the first version. The Coach App build is a
self-contained, versioned HTML resource with bundled React, UI code, CSS, and
visual assets.

## Coach command and MCP interface

The existing Review Session command executor remains the deep seam. Rust owns
the source contract and generates the TypeScript SDK. The private HTTP
projection accepts one authenticated, versioned command envelope and returns
accepted/progress/terminal events through the existing JSON/NDJSON event model.
Primitive MCP tools remain restricted projections of one Coach Engine command.
`review_game` is the deliberate orchestration exception: it executes
`ImportGame`, takes only the returned owned Game Import ID, then executes
`StartReviewSession`, returning the final ready-session result. This removes a
UI-only manual transition without weakening either command boundary.

### Tool surface

The Coach MCP Server exposes a compact review surface plus app-only
cancellation and operational tools.

| Tool                            | Rust command projection                               | Visibility  |
| ------------------------------- | ----------------------------------------------------- | ----------- |
| `review_game`                   | `ImportGame` then `StartReviewSession`                | model + app |
| `import_game`                   | low-level `ImportGame` recovery/private projection    | app only    |
| `start_review_session`          | low-level `StartReviewSession` recovery projection    | model + app |
| `open_review_moment`            | `OpenReviewMoment`                                    | model + app |
| `publish_review_moment_comment` | `PublishReviewMomentComment`                          | model + app |
| `inspect_position`              | `InspectPosition`                                     | model + app |
| `evaluate_player_plan`          | stateless `EvaluatePlayerPlan` prepare/admit workflow | model + app |
| `explore_alternative_move`      | `ExploreAlternativeMove`                              | model + app |
| `request_coach_turn`            | admitted `StartCoachTurn`/`PublishCoachTurn` workflow | model + app |
| `resume_review_session`         | `ResumeReviewSession` read projection                 | model + app |
| `cancel_operation`              | `CancelOperation`                                     | app only    |

Player plan discussion remains ordinary host conversation. The optional
`evaluate_player_plan` tool performs one stateless, engine-grounded comparison;
it does not create session intent state.

### Typed handles, not client state

There is no universal `stateToken` and no client-carried aggregate revision.
Each command carries only the minimal typed handles required by that operation:
Game Import ID, Review Session ID, target ply or Review Moment, operation ID,
publication fence, and the operation's actual input.

Coach Engine resolves authoritative facts, evidence, exploration, and
publication state from the Review Session. Widget state and web local state are rendering
caches, never restoration or publication authority. Publication fences and
operation IDs express domain staleness; callers do not send an
`expectedRevision`.

### Result channels

| Channel             | Contract                                                                                                                                                                                   |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `content`           | Compact moment-scoped facts and actionable outcomes required by the host model or text-only fallback. Includes needed opaque handles; never includes raw PGN or whole-game evidence dumps. |
| `structuredContent` | Complete model-safe redraw data for the current result. Treat as model-visible on every host.                                                                                              |
| `_meta`             | Optional UI-only bulk or paging data. Never required for correctness because host delivery behavior is not a portable authority.                                                           |

Canonical cards render only from successful Coach Engine results. MCP
transport faults use MCP errors; domain unavailable, conflict, stale,
interrupted, rejected, and expired outcomes are normal typed results with
recovery guidance.

## Authentication and authorization

### Web profile

The browser signs in with Firebase Authentication and supplies a current
Firebase ID token through the SDK. Node routes the request without interpreting
identity. Coach Engine verifies signature, issuer, Firebase project audience,
expiry, revocation where required, and nonempty `sub`; that exact Firebase
`uid` is Player ID.

### Coach App profile

The one public origin is both OAuth authorization server and exact protected
MCP resource. The accepted common host profile remains:

- MCP 2025-11-25 and MCP Apps 2026-01-26;
- authorization code with PKCE `S256`;
- public DCR clients as the common registration path;
- exact RFC 8707 `/mcp` resource/audience binding;
- one `coach:review` scope;
- RS256 `at+jwt` access tokens with a ten-minute lifetime;
- rotating refresh tokens with the accepted 14-day non-production policy;
- refresh-family invalidation on reuse and RFC 7009 revocation;
- asymmetric JWKS publication and overlap-based signing-key rotation.

During login, the Node interaction page sends the Firebase ID token to a narrow
Coach Engine identity endpoint. Coach Engine verifies it and returns the
canonical `uid`; Node uses that value as the Coach token `sub`. A Firebase token
is never a Coach MCP bearer.

Node performs the public OAuth resource-server checks needed for discovery and
host behavior. Coach Engine independently validates token type, signature,
issuer, exact audience, scope, timestamps, key ID, and subject before executing
any command. Object ownership is checked again for every import, session,
moment, operation, preference, and artifact reference.

Revoking a grant immediately prevents refresh but an already issued
self-contained JWT may remain usable for at most its ten-minute lifetime plus
accepted clock skew. The product does not claim immediate access-token recall.

### Firestore isolation

The Firebase project contains two Firestore databases:

| Database      | Writer                       | Data                                                                                               |
| ------------- | ---------------------------- | -------------------------------------------------------------------------------------------------- |
| default       | Coach Engine service account | Coach application data                                                                             |
| `coach-oauth` | Node service account         | OAuth clients, interactions, codes, grants, consent, refresh families, replay and revocation state |

Per-database IAM conditions enforce the separation. Browser clients have direct
access to neither database. Firestore Security Rules are not treated as a
server-service isolation boundary.

Private signing keys remain Railway secrets, not Firestore documents or source
files.

## Persistence and state

All durable application data is structured JSON in Firestore. There is no
Cloud Storage application-data path, opaque aggregate blob, binary snapshot, or
compute-local durable volume.

### Game Import Record

A successful import atomically persists one Player-owned Game Import Record and
returns its opaque ID. The record contains the normalized canonical Game,
selected source metadata, Review Side inputs, and required provenance. A
Lichess import retains its canonical URL and game ID. A pasted import does not
retain the original PGN text after normalization.

The record has a fixed 24-hour lifetime from import; reads do not extend it.
Session start copies the complete Game state required for recovery into the
Review Session Checkpoint. An active session therefore does not depend on the
import record remaining valid. A still-valid import record may start a new
session after an idle-expired session.

### Review Session Checkpoint

A Review Session is transient operational state, not Saved Game or review
history. Coach Engine nevertheless checkpoints the authoritative aggregate so
acknowledged work survives reconnect, process loss, and deployment restart.

The normalized document family contains:

- a root with Player owner, schema version, internal aggregate revision,
  creation time, last successful Player activity, logical deadlines, immutable
  purge time, and aggregate status;
- Review Moment documents;
- normalized evidence documents and references;
- Alternative Move Exploration documents;
- canonical publication and authoring-provenance documents;
- short-lived operation and idempotency documents.

It contains the complete canonical Game, normalized positions and move
sequence, structural metadata needed by the product, objective evidence, and
exploration/publication state. It excludes the original pasted PGN and nonessential identity-bearing
headers. Lichess source provenance remains available for traceability and
re-import.

Each state-changing command:

1. authenticates and authorizes the Player and addressed objects;
2. loads the current checkpoint documents and internal revision;
3. revalidates the command against current domain state;
4. atomically writes the affected documents and next root revision;
5. acknowledges success only after the transaction commits.

A storage failure leaves the previous revision authoritative and returns a
retryable unavailable outcome. There is no acknowledged memory-only fallback.
Storage contention retries from newly loaded state; domain fences and operation
IDs, not the storage revision, produce caller-visible stale/conflict outcomes.
Old aggregate revisions are not retained.

### Lifetime and cleanup

A Review Session has:

- six hours of idle lifetime;
- 24 hours of absolute lifetime from session creation.

Only a successful Player-initiated domain command refreshes the idle deadline.
Passive reads, `resume_review_session`, polling, rejected requests, and retries
do not. Coach Engine enforces both deadlines synchronously from checkpoint
metadata. Expiry is independent of Firebase or Coach token expiry, and a
refreshed credential for the same Player may resume a still-valid session.

Every document in the checkpoint family receives the same immutable absolute
`purgeAt` time. Firestore TTL performs eventual physical cleanup without
assuming that deleting a root cascades into subcollections. Logically expired
data remains inaccessible while TTL is pending. An expired session cannot be
resurrected; URL data may be submitted again, while raw PGN must be supplied
again after its normalized records are gone.

### Schema evolution

Every checkpoint has `schemaVersion`. Before production MVP, checkpoint data is
disposable: implement only the current decoder and delete incompatible staging
data rather than building migrations.

Production release readiness requires additive evolution by default and an
expand-contract sequence for breaking changes. The production reader supports
the current and immediately previous versions for at least the bounded session
lifetime, upgrades prior state on use, preserves a readable rollback window,
and fails closed on unknown versions without partial restoration.

### Process-loss recovery

Recovery occurs at acknowledged command boundaries; Stockfish and Maia process
state is never serialized.

- Completed acknowledged transitions restore exactly.
- Session creation remains absent unless its atomic final commit completed.
- An asynchronous operation persists its operation ID and publication fence
  before returning accepted.
- On restore, an operation marked active without a live process handle becomes
  interrupted, closes its fence, discards partial output, and permits retry.
- Idempotency and publication records reject late or duplicate completions.

Coach Engine may maintain a bounded in-memory hot cache, but Firestore remains
the authority and every command must tolerate cache loss.

### Evaluation artifacts and preferences

The default database also stores structured Review Snapshots and each Player's
Artifact Retention Preference. Cloud Storage is not used. The established default-on disclosure,
pseudonymization, 12-month retention window, opt-out, withdrawal, Dataset
Admission, and tombstone rules remain unchanged.

The non-production MVP provides web sign-out, web retention preferences, and
per-host OAuth grant revocation/relinking. Turning retention off prevents new
artifacts and deletes non-admitted artifacts for that Player.

Self-service Firebase account deletion is a production gate. Its production
design is an idempotent Coach Engine-owned saga: block new Player commands,
delete Coach application data, ask Node's internal OAuth adapter to revoke all
grants for the Player ID, and delete the Firebase identity only after the
preceding steps succeed. Retries resume recorded deletion progress.

## Grounding and validation

The existing facts-before-prose architecture remains mandatory:

- Stockfish is objective chess authority.
- Maia supplies human-likely move evidence at the Elo Profile.
- Rule Extraction and selection are deterministic and versioned.
- A Language Layer may interpret and phrase returned evidence but cannot invent
  positions, lines, evaluations, probabilities, projected plans, or links.
- `publish_review_moment_comment` is one atomic validation-and-publication
  command. There is no validation-only tool.
- Coach Engine resolves objective facts from the authoritative Review Session,
  validates the kind-aware Grounding Ledger and publication fence, and returns
  only canonical output or deterministic safe rendering.
- `publish_coach_turn` remains specific to Alternative Move Assessment.
- Unsubmitted host chat is not workspace state.

Every UI-enabled result also has a complete textual fallback. A host without
the Apps extension may deliver the grounded text journey, but visual parity is
required only in ChatGPT and Claude hosts that negotiate MCP Apps.

## CSP and frontend delivery

The Coach App uses a single predeclared, versioned
`text/html;profile=mcp-app` resource. Its React, `@chenchess/ui`, CSS, board
assets, and fonts are bundled into the resource. Product data travels through
the MCP Apps bridge; the iframe performs no direct Coach Engine fetch and embeds
no external frame.

The resource declares the smallest standard `_meta.ui.csp`, with no external
connect, resource, frame, or base-URI domains unless a later feature proves one
necessary. Host-specific CSP aliases or dedicated-domain values are data
computed by the Node adapter, not separate Coach App implementations. The web
SPA has its own ordinary HTTP CSP and does not inherit the MCP iframe policy.

## Operating policy

The accepted centralized compute policy remains in force, with ownership moved
from the old Review Engine service name to Coach Engine:

| Limit                                   | First-version value                                             |
| --------------------------------------- | --------------------------------------------------------------- |
| Stockfish workers per Coach Engine cell | 8 single-threaded workers, depth 16, 16 MiB hash                |
| Simultaneous engine leases              | 1                                                               |
| Engine queue                            | 4 waiting, 30-second queue deadline                             |
| Coach Turn pool                         | Player 4 active / 8 waiting / 2-second queue; Local Coach 1 / 1 |
| Maia review batches                     | 1 per replica, FIFO                                             |
| In-flight reviews                       | 1 per Player deployment-wide                                    |
| Imports                                 | 10 accepted per Player per 10 minutes                           |
| Commands                                | 120 per Player per minute across surfaces                       |
| Engine cache                            | in-process 256 MiB exact-key LRU, no TTL                        |
| Lichess cache                           | 1,024 success-only completed games, no TTL                      |
| Execution deadline                      | `clamp(30s, 120s, 2 × modeled duration)`                        |
| Resource release                        | within 5 seconds                                                |

Admission keys by Player/principal class, never by web, ChatGPT, or Claude
surface. Explicit `CancelOperation` is the cancellation authority; disconnect
is not cancellation. Host cancellation notifications and progress are
best-effort relays.

Staging uses one warm Coach Engine cell and one warm Maia replica. More than one
Coach Engine cell, shared admission/rate coordination, shared engine cache,
production compute budgets, and production autoscaling are later
capacity-planning decisions.

## Implementation sequence

1. **Repository restructure (#118).** Establish the target functional buckets,
   rename the private Rust deployable to Coach Engine, extract
   `@chenchess/coach-engine-sdk` and `@chenchess/ui`, relocate Maia and tooling,
   remove the disposable staging prototype and unused Convex/Better Auth
   scaffolding, and preserve current non-prototype Player behavior. Do not add
   Firebase, checkpoint, OAuth, or MCP product behavior in this structural
   slice.
2. **Coach Engine foundation.** Implement Firebase verification, the default
   Firestore adapters, Game Import Records, durable Review Session Checkpoints,
   preference/artifact persistence, current schema version, recovery semantics,
   the versioned command interface, and generated SDK.
3. **Full-stack web origin.** Replace static Nginx ownership with the thin Node
   server, serve the Vite SPA, route `/api`, bind web Firebase/account flows,
   configure the `coach-oauth` database, and implement the `oidc-provider`
   adapter.
4. **Coach App delivery.** Build `apps/coach-app`, the 15 MCP tool projections,
   MCP resource delivery, host authorization, shared UI controller, and
   conservative result/CSP contract.
5. **Cross-host acceptance and hardening.** Exercise the complete journey and
   negative matrix in ChatGPT and Claude, then record observed optional host
   behaviors without making them portable requirements.

Each phase must leave its accepted seams verifiable. #118 runs before product
implementation; no feature ticket should depend on legacy paths that #118 is
about to remove.

## Acceptance proof

The implementation is accepted only when all of the following pass:

### Repository and contract

- The refined #118 topology and exact package identities are present.
- Rust generation and committed `@chenchess/coach-engine-sdk` artifacts pass
  contract drift checks.
- Both Vite clients type-check, test, and build against package exports.
- The authoritative `nix develop --command bun run release:proof` passes.
- Docker builds prove root-context workspace dependency resolution for all
  three services.

### Authentication

- ChatGPT and Claude independently discover, register through DCR, authorize
  with PKCE, link the intended Firebase Player, call `/mcp`, refresh, revoke,
  and relink.
- Firebase and social-provider tokens are rejected as Coach MCP bearers.
- Wrong issuer, audience, type, scope, expiry, key, redirect, PKCE verifier,
  reused code, and reused refresh token fail with the specified outcomes.
- Key rotation preserves the overlap window.
- OAuth protocol state survives Node restart.

### Product journeys

- Each host completes one URL-import Review Session through canonical Review
  Moment publication and one interactive Coach Turn.
- Each host imports pasted PGN using a Player-chosen chat or inline method.
- The web application completes the same facts/session journey through its own
  controller and Language Layer.
- Shared components exhibit equivalent board, timeline, review-card,
  exploration, cancellation, and responsive behavior.

### Persistence and isolation

- A successful import survives Coach Engine restart before session start.
- An acknowledged Review Session transition survives restart and resumes from
  the same opaque ID.
- A process loss during active compute yields interrupted with no partial or
  duplicate publication, then permits retry.
- Idle and absolute expiry, non-refreshing reads, logical fail-closed behavior,
  and eventual TTL cleanup are tested.
- Two Firebase test Players run overlapping operations across both hosts.
  Cross-Player import, session, moment, operation, preference, and artifact IDs
  are rejected.
- The same Player may explicitly resume a known ID across surfaces without any
  listing or automatic handoff.
- Default and `coach-oauth` database service accounts cannot access the other
  database.

### Grounding and operations

- Host-authored canonical comments and Coach Turns cannot render before Rust
  admission; stale fences, invalid ledgers, and unsupported facts fail closed.
- Text fallback contains enough facts for a non-UI host without leaking raw PGN
  or bulk evidence.
- One full two-Player overlap run proves queue fairness and separates queue time
  from provider time.
- Timeout, explicit cancellation, Node disconnect, Firestore outage, Maia
  failure, and Coach Engine restart preserve one terminal outcome and the
  five-second release budget.
- Logs and retained proof artifacts contain no credentials, raw PGN, FEN,
  Player wording, or review content beyond deliberately redacted test
  fixtures.

## Host-specific gaps

These behaviors must be observed and documented but do not block the portable
MVP:

- whether each host renders progress notifications;
- whether each host emits cancellation notifications reliably;
- partial tool-input streaming;
- durable widget-instance behavior;
- Claude model visibility of `structuredContent`;
- host-specific display modes, borders, dedicated domains, and visual polish;
- future negotiated MCP or MCP Apps protocol revisions.

The product assumes `structuredContent` is model-visible, never depends on
progress or partial input, uses explicit cancellation and backend deadlines,
and reconstructs any widget from a self-sufficient result or
`resume_review_session`.

## Out of scope

- production rollout, directory submission, marketplace review, public support,
  production budgets, or broad operations;
- SSR for either React product;
- removing Node before the Rust-only parity gates pass;
- a second host-specific Coach App implementation;
- a local Coach MCP server or tunnelling local compute to consumer hosts;
- redesigning or removing the web application's LLM Explainer;
- Saved Games, permanent Review Sessions, session listing, browsable review
  history, synchronized chat, or automatic cross-host continuation;
- self-service account deletion before the production release gate;
- Cloud Storage or another application blob store;
- multi-cell coordination, shared-cache selection, GPU Maia, GKE, or other
  production scale-out work;
- automatic model fine-tuning or prompt mutation;
- ChatGPT or Claude behavior not supported by their negotiated common MCP Apps
  contract.

## Reconciled earlier decisions

- ADR 0002's Better Auth/Convex provider choice is superseded. Firebase
  Authentication supplies identity; `oidc-provider` supplies Coach OAuth; Coach
  Engine validates both token profiles.
- ADR 0006's public Rust/static-server layout is superseded. Its Vite,
  co-located Stockfish, separate Maia, and Railway multi-service choices remain.
- ADR 0020's no-version rule for transient state is superseded only for
  Firestore-backed Review Session Checkpoints. Non-durable internal traces and
  generated outputs remain unversioned.
- ADRs 0003, 0005, 0009, 0010, 0019, and 0021 remain authoritative for facts,
  provider seams, Language Layer limits, agent validation, kind-aware
  publication, and chronological Review Session preparation. ADR 0026
  supersedes the intent-lifecycle and Intent Response Record portions of ADRs
  0017 and 0018 while retaining uncertain inline hypotheses and Review Snapshot
  retention.
- The earlier client-carried `stateToken`, in-memory-only Review Session,
  12-tool count, Firebase Hosting, public Rust API, Cloud Storage artifact path,
  and prototype-preservation assumptions are superseded.
