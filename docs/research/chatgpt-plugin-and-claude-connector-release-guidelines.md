# ChatGPT plugin and Claude connector release guidelines

Research date: 2026-07-30

This note records the current first-party publication, review, and change
management rules for shipping one remote MCP service with optional interactive
UI through OpenAI's and Anthropic's public directories. It intentionally does
not design the ChenChess release pipeline. Runtime portability details remain
in [MCP Apps cross-host contract research](./mcp-apps-cross-host-contract.md).

Sources are limited to current OpenAI, Anthropic, and Model Context Protocol
documentation. Where a vendor does not define a release requirement, the gap is
called out rather than filled with an assumed convention.

## Executive constraints

1. **The current OpenAI publication unit is a plugin.** A plugin may be
   MCP-only, skills-only, or combine both; UI is optional. The public listing
   lives in the universal Plugins Directory shared by ChatGPT and Codex. New UI
   should start with the open MCP Apps UI standard, adding ChatGPT-specific
   extensions only when required. This is not the legacy `ai-plugin.json`
   publication model. [OpenAI plugin architecture](https://developers.openai.com/plugins/concepts/plugins),
   [OpenAI plugin documentation index](https://developers.openai.com/plugins)
2. **The Claude publication unit for this MVP is a directory connector.** It is
   a remote MCP server; if it returns interactive UI, Anthropic calls it an MCP
   App. A Claude plugin is a separate installable bundle of skills and connector
   references, mainly for Claude Code and Cowork, and is not required to list a
   remote MCP server across Claude surfaces.
   [Anthropic: what to build](https://claude.com/docs/connectors/building/what-to-build),
   [Anthropic connector submission](https://claude.com/docs/connectors/building/submission)
3. **OpenAI snapshots the reviewed MCP contract; Claude reads the live
   contract.** OpenAI metadata changes require a new scan, review, approval, and
   publication. Anthropic says tool additions, changes, and removals need only a
   live server deployment and are visible on the next connection, with no
   scheduled re-review.
   [OpenAI ongoing maintenance](https://developers.openai.com/plugins/deploy/app-review#ongoing-maintenance),
   [Anthropic after publishing](https://claude.com/docs/connectors/building/after-publishing)
4. **Do not use ChatGPT or Claude client version strings as release gates.**
   MCP implementation names and versions are self-reported and intended for
   display, logging, and debugging, not behavior or security decisions.
   Anthropic additionally warns that Claude's exact `clientInfo.name` and
   `clientInfo.version` vary by surface, request path, and release.
   [MCP 2026-07-28 metadata](https://modelcontextprotocol.io/specification/2026-07-28/basic#_meta),
   [Anthropic connector testing](https://claude.com/docs/connectors/building/testing#detect-claude-as-the-client)
5. **Only OpenAI requires submission release notes.** OpenAI asks whether the
   submission is initial or an update, what changed, what the plugin does, and
   any reviewer setup or credential context. Anthropic documents no remote
   connector version, changelog, release-note field, format, or SemVer rule.
   [OpenAI submission](https://developers.openai.com/plugins/deploy/submission#submit),
   [Anthropic submission portal fields](https://claude.com/docs/connectors/building/submission#what-to-expect-in-the-portal)

## Keep the version domains separate

| Version domain                                  | Source of truth                                                 | Vendor meaning                                                                                                                    |
| ----------------------------------------------- | --------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| ChenChess service and UI versions               | ChenChess release artifacts                                     | Product provenance. Neither vendor prescribes the scheme for a remote MCP service or its UI bundle.                               |
| MCP protocol version                            | Date-based MCP revision such as `2026-07-28`                    | Wire compatibility, negotiated independently of product releases.                                                                 |
| MCP `serverInfo.version` / `clientInfo.version` | Self-reported implementation metadata                           | Observability only; not a compatibility, authorization, or rollout contract.                                                      |
| OpenAI plugin package `version`                 | `.codex-plugin/plugin.json` when distributing a packaged plugin | Required package identity field. OpenAI examples are SemVer-shaped, but the publication docs do not define bump semantics for it. |
| OpenAI reviewed MCP version                     | Metadata snapshot stored by the submission portal               | The contract ChatGPT and Codex use until an approved replacement is published.                                                    |
| Claude directory connector version              | None documented                                                 | Claude connects to a live endpoint and refreshes its tool surface on the next connection.                                         |
| MCP Apps extension version                      | The MCP Apps extension specification                            | UI protocol compatibility; independent of the ChenChess UI artifact version and the MCP core revision.                            |

The current MCP protocol revision is `2026-07-28`, which uses date-based
identifiers for the last backward-incompatible change and allows multiple
versions to coexist. Each request declares its protocol version; unsupported
requests return the versions the server supports.
[MCP versioning](https://modelcontextprotocol.io/docs/2026-07-28/learn/versioning)

Host adoption must still be proved rather than inferred from the MCP project's
“current” label. Anthropic's connector page currently names auth-spec support
through `2025-11-25`, and OpenAI's authentication guide also targets the
`2025-11-25` authorization contract. The safe cross-host conclusion is to
negotiate capabilities, retain older-revision compatibility where the chosen
SDK supports it, and test every claimed revision on the real host surfaces.
[Anthropic protocol support](https://claude.com/docs/connectors/building#transport-authentication),
[OpenAI authentication](https://developers.openai.com/plugins/build/auth)

## OpenAI: submission and review

### What is submitted

OpenAI's portal accepts skills-only, MCP-only, and skills-plus-MCP plugins.
MCP-backed submissions submit the production MCP server directly; they do not
publish a reference to an already registered integration. A public submission
requires:

- a verified individual or business identity and matching publisher details;
- `Apps Management` write permission (`api.apps.write`) for the submitter;
- a public production MCP URL, authentication configuration, working reviewer
  credentials when needed, and domain verification;
- listing name and descriptions, brand assets, website, support, privacy, and
  terms URLs;
- starter prompts, at least five positive and three negative test cases,
  country availability, attestations, and release notes; and
- a current successful tool scan.

[OpenAI submission requirements](https://developers.openai.com/plugins/deploy/submission),
[OpenAI MCP review requirements](https://developers.openai.com/plugins/deploy/app-review)

The default endpoint shape is **Universal**: one fixed MCP URL for every user.
Template URLs for per-workspace endpoints are limited to trusted developers
with prior OpenAI approval. The portal verifies control of the MCP host (or an
allowed parent host) by reading the exact token from
`/.well-known/openai-apps-challenge`.
[OpenAI MCP submission fields](https://developers.openai.com/plugins/deploy/submission#mcp),
[OpenAI template URLs](https://developers.openai.com/plugins/deploy/app-review#template-mcp-server-urls)

Submission starts review but does not publish. After approval, the developer
chooses when to publish. Only one version of an MCP integration may be
published at a time and only one may be in review at a time. Review has no
published SLA. Rejected versions can be corrected and resubmitted; an
in-review version must be cancelled before changing what reviewers evaluate.
[OpenAI public publishing flow](https://developers.openai.com/plugins/deploy/submission#public-publishing-flow),
[OpenAI review and approval](https://developers.openai.com/plugins/deploy/app-review#review-and-approval)

### Tool and security metadata

The scan captures tool names, titles, descriptions, input and output schemas,
security schemes, annotations, `_meta`, linked UI resource metadata and CSP,
plus MCP server `instructions`. Submission justifications explain the values;
they cannot override what the server advertises.
[OpenAI scanned metadata](https://developers.openai.com/plugins/deploy/app-review#metadata-stored-during-tool-scanning)

Every tool must accurately supply:

- `readOnlyHint`;
- `openWorldHint`; and
- `destructiveHint`.

Names and descriptions must be narrow, truthful, non-promotional, and aligned
with actual side effects. Inputs and responses must be minimized; raw chat
history, authentication secrets, debug payloads, internal identifiers, and
unnecessary personal data are not acceptable. Retry behavior and irreversible
side effects must be explicit. Iframes declared through `frameDomains` receive
extra manual scrutiny and are often not approved for broad distribution.
[OpenAI plugin guidelines](https://developers.openai.com/plugins/app-guidelines)

### Authentication and domain considerations

Authenticated servers are expected to implement MCP OAuth 2.1:

- protected resource metadata and authorization-server discovery;
- audience-bound `resource` propagation;
- authorization code with PKCE S256;
- CIMD, DCR, or a predefined client as configured for the submission;
- the production callback
  `https://chatgpt.com/connector/oauth/{callback_id}`; and
- per-tool `securitySchemes` plus runtime
  `_meta["mcp/www_authenticate"]` challenges to trigger account linking.

OpenAI currently prefers CIMD for scale but supports DCR. The MCP 2026-07-28
spec formally deprecates DCR in favor of CIMD, so new authorization
infrastructure should not make DCR its only long-term registration path.
[OpenAI authentication](https://developers.openai.com/plugins/build/auth),
[MCP 2026-07-28 authorization changes](https://blog.modelcontextprotocol.io/posts/2026-07-28/#authorization)

### How OpenAI versions changes

OpenAI treats discovered MCP metadata as a versioned API contract:

| Change                                                                                                                                                             | OpenAI-required publication action                                                              |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------- |
| Tool list/name/title/description, input or output schema, annotations, security schemes, tool `_meta`, visibility, UI resource reference, or server `instructions` | Deploy additively, create/update a draft, scan, submit for review, then publish after approval. |
| UI resource URI or linked metadata including CSP                                                                                                                   | Same new-version review flow.                                                                   |
| Backward-compatible UI content at the same already-published resource URI                                                                                          | Deploy directly; no review. ChatGPT may cache resource content for up to one hour.              |
| Server-only fix, live result change, result `_meta`, or business-data update that preserves the published contract                                                 | Deploy directly; no review.                                                                     |
| MCP endpoint path on the same origin                                                                                                                               | New-version review flow.                                                                        |
| MCP origin (`scheme`, host, or port)                                                                                                                               | Create and review a new plugin.                                                                 |

Breaking a published contract in place is unsupported. OpenAI's documented
migration order is additive: keep old tools, fields, and UI resources working;
submit and publish the new metadata; then continue honoring the old contracts.
If a live deployment breaks the published version, roll it back rather than
waiting for review.
[OpenAI published metadata versions](https://developers.openai.com/plugins/deploy/app-review#how-published-mcp-metadata-versions-work)

The release-note field is review material, not a documented end-user changelog.
OpenAI mandates its content but does not mandate a format or say that portal
version numbers must match a product SemVer. A packaged plugin does contain a
`version` field in `.codex-plugin/plugin.json`; the examples use `0.1.0` and
`1.0.0`, but the docs do not specify compatibility rules for bumps.
[OpenAI release notes](https://developers.openai.com/plugins/deploy/submission#submit),
[OpenAI plugin manifest](https://developers.openai.com/plugins/build/plugins#plugin-structure)

## Anthropic: submission and review

### What is submitted

Remote MCP servers, including MCP Apps, are submitted through Claude.ai Admin
settings. Submission requires a Team or Enterprise organization and Directory
management access. Team submissions remain with Owners/Primary Owners;
Enterprise Owners may delegate access with a custom role.
[Anthropic connector submission](https://claude.com/docs/connectors/building/submission#before-you-start)

The portal collects:

- a public HTTPS server URL, Streamable HTTP or legacy SSE transport, and URL
  tenancy shape;
- live-discovered tools, prompts, and resources;
- name, tagline, description, categories, permanent listing slug,
  documentation and privacy URLs, support contact, and company identity;
- auth mode, use cases, read/write behavior, and data-handling declarations;
- fully populated test-account credentials and reviewer instructions; and
- policy attestations and proof that every tool has been exercised.

MCP Apps additionally require three to five PNG screenshots, at least 1000 px
wide, cropped to the app response with each paired prompt supplied separately.
[Anthropic portal fields](https://claude.com/docs/connectors/building/submission#what-to-expect-in-the-portal),
[Anthropic MCP App assets](https://claude.com/docs/connectors/building/submission#carousel-screenshots-mcp-apps)

Anthropic's directory distinguishes:

- **Verified:** Anthropic performed quality and security review.
- **Community:** automated checks passed, but no in-depth Anthropic review.
- **Custom:** user-added URL, not reviewed.

All three use the same connector technology. Directory inclusion is optional
for use; it adds discoverability and a review label.
[Anthropic connector verification](https://claude.com/docs/connectors/verification)

Anthropic publishes no review-time SLA. Status and reviewer feedback live in
the submissions dashboard. Public documentation is required by publication,
and reviewers require a populated test account. There is no separate Claude
connector staging environment: the documented test path is a custom connector
against the real Claude runtime, plus MCP Inspector validation.
[Anthropic review criteria](https://claude.com/docs/connectors/building/review-criteria),
[Anthropic connector testing](https://claude.com/docs/connectors/building/testing)

### Tool and security metadata

Anthropic requires:

- tool names no longer than 64 characters;
- a human-readable `title` and applicable `readOnlyHint` or
  `destructiveHint`;
- separate read and write tools rather than a catch-all tool whose method
  parameter mixes safe and unsafe operations;
- narrow descriptions free of prompt injection, hidden instructions,
  cross-tool interference, and unrelated promotion;
- successful behavior for valid calls, actionable validation errors, and
  task-sized responses; and
- a server domain and APIs the publisher owns or is legitimately authorized
  to proxy.

[Anthropic pre-submission checklist](https://claude.com/docs/connectors/building/review-criteria)

For one portable tool surface, the stricter combined rule is: keep every name
within 64 characters, split read/write operations, provide a title, and set
all three OpenAI annotations accurately even though Anthropic's checklist names
only the applicable read/destructive hints.

### Authentication and endpoint considerations

Authenticated directory services must use OAuth. Claude supports DCR and CIMD
out of the box; Anthropic-held client credentials and custom connections
require coordination. Pure machine-to-machine `client_credentials` is not
supported because every connection requires user consent.

The interoperable path requires:

- PKCE S256;
- `401` plus `WWW-Authenticate: Bearer resource_metadata="..."` (Claude does
  not honor that challenge on a `200` response);
- a protected-resource `resource` matching the exact MCP URL, including path;
- form-encoded token requests and correct refresh-token handling; and
- hosted callback `https://claude.ai/api/mcp/auth_callback`, plus the documented
  loopback callbacks if Claude Code must connect.

[Anthropic connector authentication](https://claude.com/docs/connectors/building/authentication)

### How Anthropic handles changes

Anthropic treats the MCP server as a live API. Tool additions, modifications,
and removals are deployed directly, require no resubmission, and appear when
Claude next connects. There is no scheduled re-review.
[Anthropic after publishing](https://claude.com/docs/connectors/building/after-publishing)

Listing metadata can be edited in the submissions dashboard and remains pending
until approved. A display-name change affects existing users and explicitly
requires re-review. The directory slug is permanent.
[Anthropic listing management](https://claude.com/docs/connectors/building/managing-your-listing#edit-your-listing)

Changing the directory endpoint has migration consequences: existing
connections continue using the endpoint they installed, appear as custom when
they no longer match the listing, and must be removed and re-added to move to
the new URL, including reauthentication. An endpoint change therefore does not
upgrade already connected users.
[Anthropic endpoint changes](https://claude.com/docs/connectors/directory#when-a-connectors-endpoint-changes)

Anthropic documents no remote-connector SemVer field, release notes, changelog
format, deprecation window, or versioned-endpoint requirement. Maintaining an
operator changelog and backward-compatibility policy may be prudent, but it is
not an Anthropic submission rule.

## Cross-host release-note evidence

For an initial OpenAI submission, the minimum vendor-required note answers:

- What does the plugin do?
- Is this the initial submission or an update?
- What changed from the previous submitted version?
- What do reviewers need to know about credentials, fixtures, expected data,
  or setup?

For an update, the same note should identify every reviewed-contract change
that caused resubmission. A separate public/user changelog is not required by
OpenAI's submission page and Anthropic specifies no release-note artifact at
all. Therefore a single ChenChess-authored release note can serve both
operators and OpenAI reviewers, but its public visibility, format, and
service/UI version fields are product decisions rather than vendor mandates.

## Unconfirmed or explicitly absent

- No official page found defines how an OpenAI portal draft version is numbered
  or requires it to equal `.codex-plugin/plugin.json.version`.
- No official page found mandates SemVer for a remote MCP service or MCP Apps UI
  on either directory.
- No official Anthropic page found requires release notes or a connector
  changelog for initial submission or live tool-surface updates.
- Neither vendor publishes a stable ChatGPT/Claude consumer-client version that
  a third-party server may safely use as a compatibility or rollout key.
- Neither vendor's current connector page commits every production surface to
  the current `2026-07-28` MCP revision; cross-host support must be observed in
  real surface tests.
