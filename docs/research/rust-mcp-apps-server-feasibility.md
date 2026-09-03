# Rust MCP Apps server feasibility

Research date: 2026-07-26

## Question

How much additional engineering and operational cost would ChenChess incur by
moving the Coach MCP server, MCP Apps resource integration, and OAuth surface
into the Rust backend, eliminating the Node runtime?

The deployed Node prototype is evidence that the product flow and the
ChatGPT/Claude contract work. It is not treated as a production architecture
constraint.

## Conclusion

Eliminating the Node **runtime** is technically feasible. MCP Apps itself is no
longer a strong reason to retain a Node server:

- the official Rust MCP SDK supports Streamable HTTP, tools, resources,
  arbitrary `_meta`, extension capability negotiation, progress, and
  cancellation;
- the server half of MCP Apps is mostly normal MCP resources plus metadata;
- the browser-side Coach App can continue to use the official TypeScript
  `@modelcontextprotocol/ext-apps` library and be compiled at build time. A
  JavaScript build tool does not imply a Node production process.

The expensive part is the OAuth **authorization server**, not MCP Apps. The
current Node path delegates a large, security-sensitive protocol surface to
OpenID-certified `oidc-provider`. The official Rust MCP SDK's OAuth support is
primarily client-side; its server example is demonstrative rather than a
production authorization-server framework. Reproducing the current cross-host
contract in Rust means owning authorization-code + PKCE, CIMD/DCR, resource
binding, token/refresh rotation, revocation, consent and interaction sessions,
metadata, JWKS rotation, and durable records.

Planning estimate for one experienced engineer, excluding Review Engine
business logic and Coach App UI design:

| Work                                                                              |        Rust-only effort |
| --------------------------------------------------------------------------------- | ----------------------: |
| MCP Streamable HTTP adapter, tool/resource registration, progress/cancellation    |       4–7 engineer-days |
| MCP Apps metadata, resource serving, CSP/domain handling, text fallback           |       2–4 engineer-days |
| ChatGPT/Claude direct interoperability and regression suite                       |       4–7 engineer-days |
| Production OAuth authorization server with Firebase-backed login                  |     15–30 engineer-days |
| OAuth security hardening, failure recovery, key rotation, and conformance testing |      5–10 engineer-days |
| **Total**                                                                         | **6–11 engineer-weeks** |

The estimated premium over a thin Node MCP/OAuth adapter using the existing
`oidc-provider` and MCP Apps SDK paths is roughly **3–6 engineer-weeks**,
followed by higher continuing security and protocol-maintenance ownership.
These are planning estimates inferred from the required surface and the current
codebase, not vendor estimates.

**Recommendation:** retain Node as a thin public protocol adapter for v1. It
owns Streamable HTTP and MCP Apps host compatibility, OAuth endpoints supplied
by `oidc-provider`, the login/consent interaction shell, and an isolated
Firestore database for OAuth protocol records. Rust owns Firebase identity
verification, all Coach application data, Review Sessions, every domain
operation, persistence policy, and authorization. The adapter exchanges only
versioned command/result DTOs with Rust; it never owns or reconstructs the
Review Session aggregate. During login, it delegates Firebase ID-token
verification to a narrow Rust identity endpoint rather than importing the
Firebase Admin SDK.

Moving only MCP Apps into Rust would cost roughly 2–4 engineer-weeks after
cross-host testing while leaving Node alive for OAuth. That is a reasonable
future seam simplification, but a poor runtime-removal step by itself. Remove
the Node runtime only after either:

1. a focused Rust OAuth implementation passes both hosts, protocol conformance,
   restart/key-rotation tests, and security review; or
2. an established authorization provider can preserve the canonical Firebase
   `uid` without ChenChess implementing an authorization server.

If a single production process is worth the additional security ownership,
all-Rust is a valid deliberate investment. It is not a low-cost cleanup.

## Target shapes

### Recommended v1: thin Node MCP/OAuth protocol adapter

```text
apps/central-host — one public HTTPS origin
├── Vite-built client-rendered React web product
└── thin Node protocol adapter
    ├── /mcp and MCP Apps UI resources
    ├── /authorize, /token, /register, interactions
    ├── isolated `coach-oauth` Firestore database
    └── /api/* routing to the private Coach Engine

services/coach-engine — private Rust application service
├── Review Session application layer and deterministic Game Review Engine
├── Stockfish orchestration and Maia adapter
├── Firebase ID-token verification (including the OAuth login bridge)
├── structured Firestore persistence adapter
├── access-token verification and domain authorization
└── versioned Coach command/result API

apps/coach-app
└── client-rendered React + @modelcontextprotocol/ext-apps bundle
    (build-time JavaScript only)

services/maia
└── private Python model service

packages
├── @chenchess/coach-engine-sdk
└── @chenchess/ui
```

These are three separate Railway services rather than one multi-process
container. `apps/central-host` is the only public service. `services/coach-engine`
(including its co-located Stockfish workers) and `services/maia` expose only
private-network endpoints, with independent health checks, restarts, and
resource sizing.

Node translates MCP calls into versioned Rust commands and projects Rust
results back into MCP result envelopes, but it does not own Review Session
state or transition rules. During login, it asks Rust to verify the
browser-supplied Firebase ID token and return the canonical Firebase `uid`; it
then issues a resource-bound Coach access token whose `sub` is that identity.
Rust verifies and authorizes the resulting token for every domain command.

### Rust-only runtime

```text
one public HTTPS origin / one Rust process
├── web business API
├── /mcp and MCP Apps resources
├── OAuth authorization server and interactions
├── Firebase identity and structured Firestore adapters
└── Review Session / Review Engine

apps/coach-app
└── React + @modelcontextprotocol/ext-apps bundle (build artifact)
```

This removes a process and an internal routing seam. It does not remove the
JavaScript frontend toolchain.

## Capability assessment

### MCP transport and tool execution: covered in Rust

The official Rust SDK, `rmcp`, provides a Tower
`StreamableHttpService` that mounts in Axum, including stateless operation for
modern and legacy protocol versions. Its server API exposes tools, resources,
typed schemas, notifications, progress, and cancellation. The current
ChenChess backend already uses Axum, Tokio, Serde, Schemars, and Tower HTTP, so
this is an incremental transport adapter rather than a new platform.
[Rust SDK Streamable HTTP](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v2.2.0/README.md#stateless-streamable-http),
[Rust SDK notifications](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v2.2.0/README.md#notifications),
[current backend dependencies](../../services/coach-engine/Cargo.toml)

`rmcp` models arbitrary tool and resource `_meta`, and its stable released line
models the `extensions` capability map, including
`io.modelcontextprotocol/ui`. Those are the primitives the MCP Apps server half
needs.
[Rust tool metadata](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v2.2.0/crates/rmcp/src/model/tool.rs),
[Rust resource metadata](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v2.2.0/crates/rmcp/src/model/resource.rs),
[Rust extension capabilities](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v2.2.0/crates/rmcp/src/model/capabilities.rs)

The Rust SDK maintainers closed their MCP Apps implementation tracker with the
explicit position that `rmcp` already supports Apps well: SDK responsibility is
representing the `ui://` resources and metadata; the host/view iframe bridge
belongs elsewhere.
[Rust SDK MCP Apps tracker](https://github.com/modelcontextprotocol/rust-sdk/issues/891)

Remaining Rust work is application integration:

- map the existing Review Session command router directly into MCP tool
  handlers instead of calling the NDJSON web projection;
- derive the authenticated Player from the MCP access token;
- fold Review Engine events into `content`, `structuredContent`, and `_meta`;
- relay progress and cancellation through `rmcp`;
- derive tool definitions from the same Rust-owned schemas exported through
  `@chenchess/coach-engine-sdk`, rather than authoring a second contract.

This work is moderate but has low protocol invention risk.

### MCP Apps server integration: small manual layer

The stable MCP Apps specification defines the server half as:

- a predeclared `ui://` resource;
- `text/html;profile=mcp-app` content;
- `_meta.ui.resourceUri` on tools;
- `_meta.ui.visibility`, CSP, domain, and presentation metadata;
- capability detection through `extensions["io.modelcontextprotocol/ui"]`;
- meaningful textual tool results for clients without UI.

[MCP Apps stable specification](https://github.com/modelcontextprotocol/ext-apps/blob/v1.7.5/specification/2026-01-26/apps.mdx)

The official TypeScript server helpers are convenience wrappers, not a hidden
runtime protocol. `registerAppTool` normalizes current and legacy resource URI
metadata; `registerAppResource` defaults the MIME type; `getUiCapability` reads
one entry from the extension map. Rust can reproduce these few rules with
typed constructors and golden JSON tests.
[MCP Apps server helpers](https://github.com/modelcontextprotocol/ext-apps/blob/v1.7.5/src/server/index.ts)

The important host-facing UI logic remains in the browser bundle. The official
MCP Apps package supplies `App`, the `postMessage` transport, React hooks, and
host auto-detection as JavaScript modules. `apps/coach-app` should continue to
depend on that package. Rust only serves the compiled HTML bundle as an MCP
resource.
[MCP Apps package surface](https://github.com/modelcontextprotocol/ext-apps/blob/v1.7.5/package.json),
[Claude cross-host guidance](https://claude.com/docs/connectors/building/mcp-apps/cross-compatibility)

Therefore, dropping the Node runtime does **not** require porting the iframe
bridge, React bindings, or ChatGPT host compatibility code to Rust.

### Host documentation and ecosystem maturity: Rust needs more direct testing

OpenAI's current server guide names its TypeScript and Python SDK paths and
uses Node examples. Anthropic likewise points builders to TypeScript and Python
examples even though `rmcp` is an official MCP SDK.
[OpenAI MCP server guide](https://developers.openai.com/plugins/build/mcp-server),
[Anthropic connector guide](https://claude.com/docs/connectors/building)

This is an ecosystem/documentation gap, not a wire-protocol gap. It raises the
cost of diagnosis because host-specific examples, helper APIs, and community
reports will usually assume TypeScript. The production acceptance suite must
therefore exercise:

- initialize/version negotiation on ChatGPT and Claude;
- stateless Streamable HTTP requests and SSE responses;
- tool listing metadata and app-only visibility;
- `ui://` discovery/read and exact resource metadata;
- progress and cancellation, while treating host delivery as optional;
- OAuth discovery, linking, refresh, revocation, and relinking;
- a complete two-Player Review Session in each host.

The Rust server should pin the stable MCP 2025-11-25 contract for v1 and avoid
depending on the 2026-07-28 release candidate until both hosts negotiate it.
The Rust SDK is already developing against that draft, demonstrating healthy
maintenance but also active protocol churn.
[Rust SDK version support](https://github.com/modelcontextprotocol/rust-sdk#readme),
[MCP 2026-07-28 release candidate](https://github.com/modelcontextprotocol/modelcontextprotocol/releases/tag/2026-07-28-RC)

### OAuth resource-server duties: covered or straightforward in Rust

Rust can serve Protected Resource Metadata, return the required
`WWW-Authenticate` challenge, verify JWT signature/issuer/audience/expiry/scope,
and attach the Player identity to request context. The current backend already
implements strict Coach-token verification, including `at+jwt`, `kid`, `iss`,
`aud`, `sub`, scope, and `jti`.
[current Rust token validation](../../services/coach-engine/src/auth.rs),
[MCP authorization specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization)

These resource-server duties should move with `/mcp` into Rust regardless of
which runtime issues the token.

### OAuth authorization-server duties: the material gap

Both hosts require more than a login callback:

- authorization-code flow with mandatory S256 PKCE;
- Protected Resource and Authorization Server discovery;
- exact MCP resource/audience binding;
- CIMD and/or DCR client registration;
- form-encoded token exchange;
- public-client refresh-token rotation or sender constraint;
- consent and interaction state;
- issuer/JWKS publication, signing-key rotation, expiry, revocation, and
  relinking behavior.

Claude adds concrete interoperability rules, including a real `401` discovery
response, public-client CIMD requirements, loopback redirect handling for
Claude Code, form-encoded token requests, and refresh-token rotation.
[Claude connector authentication](https://claude.com/docs/connectors/building/authentication)

OpenAI says authenticated MCP servers are expected to follow OAuth 2.1 and the
MCP authorization specification, and strongly recommends using an established
identity provider instead of implementing authentication from scratch.
[OpenAI authentication guide](https://developers.openai.com/plugins/build/auth)

The Node prototype uses `oidc-provider`, which implements DCR, PKCE, revocation,
resource indicators, refresh flows, JWT access tokens, metadata, and
experimental CIMD support, and is OpenID Certified. This removes a large amount
of protocol and security ownership even though ChenChess still supplies
Firebase login, consent UI, storage, scopes, and policy.
[`oidc-provider` implemented standards and certification](https://github.com/panva/node-oidc-provider#implemented-specs--features),
prototype provider configuration (`prototypes/firebase-mcp-oauth/server.mjs` — historical; that prototype was removed after this was written)

The official Rust MCP SDK documents broad OAuth **client** support. Its bundled
server example implements an in-memory teaching authorization server and
explicitly omits production behavior such as refresh support; it is not a
replacement for `oidc-provider`.
[Rust SDK OAuth support](https://github.com/modelcontextprotocol/rust-sdk/blob/main/docs/OAUTH_SUPPORT.md),
[Rust SDK example authorization server](https://github.com/modelcontextprotocol/rust-sdk/blob/main/examples/servers/src/complex_auth_streamhttp.rs)

Rust has useful OAuth components, but no drop-in library found in the reviewed
primary sources matches the proven Node provider's MCP-relevant production
surface:

- `oauth2` and the `rmcp` auth module are client libraries;
- `oxide-auth` is a server framework with PKCE, but applications still provide
  storage, policy, HTTP integration, consent, and modern MCP extensions;
- Rauthy is a production Rust identity provider with OIDC DCR, but adopting it
  introduces another service/identity system and does not by itself prove the
  MCP resource-binding, CIMD, and canonical Firebase-`uid` contract;
- Nazo Auth Server 0.1.0 is newly OpenID-certified, but it is an
  AGPL-licensed standalone identity product requiring PostgreSQL and Valkey.
  Its own documentation places modular external-provider login on the roadmap,
  so it cannot currently act as a drop-in Firebase-backed library.

[`oauth2` crate scope](https://docs.rs/oauth2/latest/oauth2/),
[`oxide-auth` server framework](https://docs.rs/oxide-auth/latest/oxide_auth/),
[Rauthy capabilities](https://sebadob.github.io/rauthy/),
[Nazo Auth Server](https://github.com/nazozero/NazoAuth),
[OpenID-certified Rust providers](https://openid.net/certification/certified-openid-connect-implementations-2/)

This is why the all-Rust estimate is dominated by OAuth engineering,
conformance, and security review rather than by MCP tools.

### Firebase integration: possible in Rust, less convenient

Firebase does not publish an Admin SDK for Rust. Its supported server SDK list
is Node.js, Java, Python, Go, C#, and experimental Dart. Firebase explicitly
documents third-party JWT verification for unsupported server languages,
including the exact issuer, audience, signature, and key-cache rules. Thus
canonical Firebase identity verification in Rust is supported as a protocol,
but ChenChess owns the implementation and revocation behavior.
[Firebase Admin SDK language matrix](https://firebase.google.com/docs/admin/setup),
[Firebase third-party ID-token verification](https://firebase.google.com/docs/auth/admin/verify-id-tokens#verify_id_tokens_using_a_third-party_jwt_library)

Firestore remains accessible through the
[Firestore REST API](https://firebase.google.com/docs/firestore/use-rest-api)
using service-account credentials. ChenChess durable application state is
structured JSON, so the v1 design does not add an opaque blob format or a Cloud
Storage application-data path. Aggregates that should not fit in one Firestore
document are normalized across one mutable, transactionally updated document
family. A root document carries Player ownership, schema version, aggregate
revision, and expiry; subcollections carry structured Review Moments, evidence,
explorations, publications, and short-lived idempotency records. State-changing
commands compare and advance the root revision while updating only affected
documents. That revision remains internal to the Rust persistence
implementation rather than entering MCP or Node-facing product inputs; commands
are revalidated against current domain state after storage contention, with
publication fences and operation identities carrying semantic staleness.
Old aggregate revisions are not retained. This still adds more adapter and
emulator/fake work than the official Node Admin SDK, and the cost exists in
either recommended shape because persistence policy and Review Session
ownership live in Rust.

## Boundary implications

### What belongs in Coach Engine

- all Game Review Engine and Review Session commands, state, transitions, recovery,
  persistence policy, and authorization;
- all web business endpoints;
- Firebase token verification and structured Firestore adapters;
- artifact retention and publication;
- access-token verification.

### What remains in the v1 Node protocol adapter

- Streamable HTTP lifecycle and host-specific transport compatibility;
- MCP tool/resource registration and MCP Apps metadata;
- projection between MCP envelopes and versioned Rust command/result DTOs;
- progress/cancellation relay without owning the underlying operation;
- OAuth authorization-server metadata and endpoints;
- DCR/CIMD processing supplied by the provider;
- authorization/consent interaction protocol;
- access/refresh token issuance, rotation, revocation, and JWKS;
- an isolated `coach-oauth` Firestore database for OAuth clients,
  authorization codes, grants, interactions, refresh tokens, and revocations;
- Firebase login page integration used only to collect an ID token; a narrow
  Rust identity endpoint verifies it and returns the canonical Firebase `uid`.

That boundary is narrow: it serializes versioned commands and results, never
the Review Session aggregate or persistence representation.

### What remains TypeScript but not a Node runtime

- the `apps/coach-app` React source;
- `@modelcontextprotocol/ext-apps` browser bridge;
- the shared controlled React presentation package `@chenchess/ui`, with no
  Firebase, HTTP, MCP, host, routing, OAuth, or persistence dependency;
- frontend build/test tooling;
- `@chenchess/coach-engine-sdk`, containing generated contracts and decoders, a
  typed Coach Engine client, account/preference/retention operations,
  auth-neutral credential injection, and common outcome handling.

Firebase Authentication behavior, `oidc-provider`, cookies, consent, and token
storage remain application adapters rather than SDK responsibilities. The web
application supplies a Firebase ID token, the Node adapter forwards a Coach
OAuth bearer token, and the Coach App relies on host-managed MCP authorization.
The SDK does not collapse these distinct credential flows.

The Firebase project contains two Firestore databases with distinct service
accounts and per-database IAM conditions. Coach Engine alone accesses the
default database containing Coach application data. The Node service alone
accesses `coach-oauth`. Browser clients have direct access to neither database.
Collection naming is not treated as an isolation control because privileged
server clients bypass Firestore Security Rules.

The Node adapter serves the built Coach App HTML under a versioned `ui://`
resource identifier in v1. A future Rust-only server can embed the same
immutable artifact without changing the browser application.

Neither frontend uses SSR in the first version. The Node process serves the
Vite-built web assets and returns the built Coach App HTML as an MCP `ui://`
resource; dynamic product state arrives through typed Coach Engine or MCP tool
results rather than server-rendered markup.

## Decision gates for removing Node

Do not remove the Node protocol adapter merely because a Rust MCP transport
spike works. Require these proofs first:

1. **Protocol parity:** PRM, RFC 8414/OIDC metadata, authorization-code + S256
   PKCE, DCR and CIMD, RFC 8707 resource binding, refresh rotation, revocation,
   and JWKS rotation.
2. **Identity parity:** successful Firebase login always maps the same Firebase
   `uid` into the Coach access token `sub`; account switching cannot reuse the
   previous Player authorization.
3. **Failure safety:** authorization codes are single-use and short-lived;
   interaction and consent records expire; refresh reuse is detected; failed
   rotation is recoverable; logs contain no tokens.
4. **Restart proof:** authorization, refresh, revocation, client registration,
   and key rotation survive process restart using durable storage.
5. **Host proof:** fresh install, relink, refresh, revoke, and complete Coach
   journey pass in both ChatGPT and Claude.
6. **Security review:** threat-model and review redirect validation, CIMD
   fetching/SSRF, DCR abuse/rate limits, CSRF, PKCE, token audience, signing
   keys, cookie settings, and Firebase token replay/revocation.

Until those gates pass, the Node authorization adapter is a deliberate security
and interoperability dependency, not an architectural home for business logic.

## Implementation sequence

1. Refine and execute repository restructure issue #118 first. Establish
   `apps/central-host`, `services/coach-engine`, `services/maia`,
   `packages/coach-engine-sdk`, `packages/ui`, and tooling ownership; remove the
   disposable staging prototype and unused Convex/Better Auth scaffolding while
   preserving current non-prototype Player behavior. Do not add Firebase,
   checkpoint, OAuth, or MCP product behavior in this structural slice.
2. Build the Coach Engine foundation: Firebase identity verification,
   structured Firestore adapters, durable Review Session checkpoints, the
   versioned command API, and the Coach Engine SDK.
3. Turn `apps/central-host` into the full-stack public origin: serve the web product,
   route `/api`, integrate web identity/account flows, and host the isolated
   OAuth protocol store.
4. Build `apps/coach-app` and the Node MCP/OAuth projections: tools, resources,
   host authorization, and shared controlled UI integration.
5. Run cross-host acceptance and hardening: complete journeys in ChatGPT and
   Claude, two-Player isolation, restart restoration, expiry, refresh,
   revocation, relinking, and failure behavior.

Only portable product, data, and security behavior blocks MVP in both hosts:
installation, OAuth, import, review, coaching, validated publication,
restoration, expiry, revocation, and Player isolation. Treat
`structuredContent` as model-visible unless a host contract proves otherwise.
Progress rendering, delivery of cancellation notifications, partial tool-input
streaming, persistent widget instances, and host-specific visual polish are
optional enhancements. Coach Engine deadlines and the explicit cancellation
operation remain authoritative when a host drops best-effort notifications.
