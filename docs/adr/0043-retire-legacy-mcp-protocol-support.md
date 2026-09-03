# Retire legacy MCP protocol support

## Status

Accepted.

This decision supersedes the protocol-conformance clauses in ADR 0025 and the
two-revision examples in ADR 0027. Those documents remain historical records of
the release design that existed while ChenChess supported both MCP eras.

## Context

The Central Host supported both sessionful MCP `2025-11-25` and stateless MCP
`2026-07-28`. The legacy path owned a process-local session map, idle timers,
GET and DELETE lifecycle routes, per-request credential rebinding, a deployment
feature flag, three fixture profiles, and a two-phase restart certification.

Issue #338 demonstrated the cost of that state: a process restart invalidated
the host's session identifier, and a long-lived session could retain an access
token beyond its lifetime without a separate request-local workaround. Result
#336 then measured both supported hosts—ChatGPT and Claude—negotiating exact
`2026-07-28` successfully.

Keeping the older path would preserve substantial runtime and certification
complexity for no measured host. It would also keep process lifetime in a
Player-visible protocol contract even though the modern transport has no such
dependency.

## Decision

ChenChess supports exact MCP `2026-07-28` only.

- `/mcp` is POST-only and stateless. It has no process-local MCP session map,
  idle timer, session identifier, or GET/DELETE lifecycle.
- The server uses the SDK's modern handler with `legacy: "reject"`.
- A `2025-11-25` initialize request receives typed JSON-RPC `-32022` with
  `supported: ["2026-07-28"]`.
- The protocol is not controlled by an environment flag.
- The local fixture has one strict-modern profile. Its E2E gate proves the
  retired revision is rejected and a fresh stateless client can read the same
  addressed card resource after a fixture restart.
- Live conformance brackets a real staging service restart with strict-modern
  clients. Certification accepts only modern host observations and a modern
  conformance arm whose requested and negotiated revisions were observed
  separately after reconnect.
- OAuth dynamic client registration remains supported. OAuth client lifecycle
  and MCP transport revision are independent contracts.

## Consequences

Issue #338's session-loss and stale-session-token failure modes disappear by
construction. A Central Host restart can interrupt an individual HTTP request,
but it cannot invalidate protocol state retained by a host; the next request is
self-contained and authenticated with its own bearer token.

Clients limited to `2025-11-25` can no longer connect. The rejection is explicit
and names the sole supported revision, so this is a deliberate compatibility
break rather than silent fallback behavior.

The rollback target is the immediately preceding Central Host release. Rolling
back restores the dual-era implementation and its feature flag; no persisted
MCP session state or schema migration is involved.
