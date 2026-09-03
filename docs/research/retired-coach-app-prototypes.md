# Retired Coach App prototypes

Retirement date: 2026-07-28

Issue #155 removed the
executable Convex authentication backend and the disposable Firebase/MCP
staging service after the replacement journey in
#153 was accepted. This is
a provenance index, not an active deployment runbook.

The following redacted records remain the durable evidence:

- [Convex + Better Auth authorization research](./convex-better-auth-mcp-authorization.md)
  records the rejected provider path and the protocol constraints it exposed.
- [Firebase-backed Coach MCP authorization](./firebase-backed-coach-mcp-authorization.md)
  records the PKCE, DCR, resource binding, refresh rotation, revocation, JWKS,
  login/consent separation, and cross-host feasibility findings.
- [Cross-host MCP Apps contract](./mcp-apps-cross-host-contract.md) records the
  portable ChatGPT and Claude boundary.
- The issue 153 staging acceptance record (withheld from this snapshot) records
  redacted host, restart, isolation, and web-journey evidence.
- [Coach App product and implementation specification](./coach-app-product-and-implementation-specification.md)
  is the authoritative replacement design.

The retired source owned a combined staging process, a client-carried state
token, a twelve-tool projection, Firebase Hosting, and a Cloud Storage artifact
path. None is an active build or deployment input. The accepted system retains
Firebase Authentication and two IAM-isolated Firestore databases: Coach Engine
owns application data in the default database, while the thin Node origin owns
OAuth protocol records in `coach-oauth`.

## Shared Coach App workspace prototype

Retirement date: 2026-07-31

`apps/central-host/src/prototypes/coach-app-workspace/` explored three structurally
different workspace shapes for
#68 — `A` board plus coach
rail, `B` guided single-review flow, `C` evidence desk — against the pinned
`Synthet1` provider recording, with the MCP Apps bridge simulated rather than
wired. The web application stopped importing it once `apps/coach-app` became the
dedicated owner, so the source was removed; the accepted direction it produced
is the durable part:

- The Player selected `A` — board plus coach rail.
- Keep the graphical Position and move controls as the primary workspace
  surface.
- Put the canonical Review Moment Comment, Move Intent conversation state, and
  Alternative Move result in one adjacent coach rail.
- Keep Lichess URL and private pasted-PGN import together above the workspace.
- Keep provider-recording proof, recovery behavior, and host capability
  differences subordinate to the review rather than making them the main
  layout.
- Collapse to one column at narrow host widths without changing the interaction
  model.
- Use one portable core for both hosts. ChatGPT widget-state resume is optional
  polish; Claude instance supersession redraws from a self-sufficient result.

OAuth and bridge negotiation, host progress and cancellation emission, partial
tool input, Claude `structuredContent` visibility, and real widget supersession
were never provable from the recording and remain staging proofs.

This record intentionally contains no credentials, Player identifiers, source
PGN, Player wording, operational tokens, or unredacted staging data.
