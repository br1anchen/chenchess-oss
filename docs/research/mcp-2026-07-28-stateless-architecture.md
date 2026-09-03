# MCP 2026-07-28 stateless architecture

Research date: 2026-07-29

## Scope

This note is the primary-source evidence base for reviewing the ChenChess MCP
work in progress under issue #157. It covers the final MCP `2026-07-28`
specification, current official SDK migration guidance, and the latest public
OpenAI and Anthropic/Claude MCP documentation available on the research date.
It does not assess the ChenChess implementation itself.

## Conclusion

MCP `2026-07-28` is a breaking lifecycle and transport revision, not merely an
optional stateless mode. Modern requests no longer use `initialize`,
`notifications/initialized`, `Mcp-Session-Id`, HTTP GET/DELETE session
endpoints, or resumable SSE. Each request is self-contained and independently
versioned; cross-request state is represented by explicit application handles
or, for multi-round-trip retries, integrity-protected `requestState`.
[MCP release post](https://blog.modelcontextprotocol.io/posts/2026-07-28/),
[base protocol](https://modelcontextprotocol.io/specification/2026-07-28/basic),
[Streamable HTTP](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http)

The migration-safe target is a **dual-era endpoint** until ChatGPT and Claude
publicly document and pass direct interoperability tests for the modern
revision. The specification explicitly permits servers to serve both modern
per-request and legacy initialization-based clients. Current OpenAI and Claude
documentation recommends Streamable HTTP, but neither vendor's public MCP
server documentation found in this review explicitly claims client support for
protocol `2026-07-28`; both still contain guidance tied to the prior lifecycle.
[MCP version compatibility](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning),
[OpenAI server guide](https://developers.openai.com/plugins/build/mcp-server),
[Claude connector guide](https://claude.com/docs/connectors/building)

Official SDK support is available. TypeScript v2 provides modern, dual-era HTTP
serving and migration shims, but its client remains legacy by default unless
version negotiation is enabled. Rust `rmcp` released stable v3.0.0 late on
2026-07-28 and its current documentation says it implements `2026-07-28` while
remaining compatible with older versions.
[TypeScript 2026 revision guide](https://ts.sdk.modelcontextprotocol.io/v2/migration/support-2026-07-28),
[Rust SDK README](https://github.com/modelcontextprotocol/rust-sdk),
[Rust SDK v3.0.0 release](https://github.com/modelcontextprotocol/rust-sdk/releases/tag/rmcp-v3.0.0)

## Normative modern protocol contract

### Requests are independent

The server MUST process every request independently and MUST NOT infer protocol
version, client capabilities, identity, conversation, thread, or task from a
connection, process, or prior request. Related operations MUST carry an
explicit identifier on every request. An open stdio process or HTTP connection
is not a conversation/session boundary.
[Base protocol: Statelessness](https://modelcontextprotocol.io/specification/2026-07-28/basic#statelessness)

Every client request MUST include these fields in `params._meta`:

- `io.modelcontextprotocol/protocolVersion`
- `io.modelcontextprotocol/clientCapabilities`

`io.modelcontextprotocol/clientInfo` is optional, but clients SHOULD include it
on every request. Missing required fields are JSON-RPC `-32602`; over HTTP the
status MUST be `400`. Servers SHOULD include
`io.modelcontextprotocol/serverInfo` in every successful result's `_meta`.
Neither self-reported identity value may be trusted for behavioral or security
decisions.
[Base protocol: per-request fields](https://modelcontextprotocol.io/specification/2026-07-28/basic#_meta)

### Discovery and version negotiation

There is no negotiation handshake. Servers MUST implement `server/discover`,
but clients MAY skip it and call an application RPC directly. An unsupported
version produces `UnsupportedProtocolVersionError` (`-32022`) with the server's
supported versions; the client SHOULD select a mutually supported version and
retry.
[Versioning: Protocol Version Negotiation](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning#protocol-version-negotiation),
[Discovery](https://modelcontextprotocol.io/specification/2026-07-28/server/discover)

Optional extensions are declared in the per-request capabilities
`extensions` map. If only one side supports an extension, it MUST fall back to
core behavior or reject the request as the extension specifies. MCP Apps
continues to use `io.modelcontextprotocol/ui`; Tasks now uses the official
`io.modelcontextprotocol/tasks` extension.
[Versioning: Extension Negotiation](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning#extension-negotiation),
[2026-07-28 changelog](https://modelcontextprotocol.io/specification/2026-07-28/changelog)

### Streamable HTTP

The server exposes one MCP endpoint accepting POST. Every JSON-RPC request is a
new POST, and the response is either one `application/json` object or a
request-scoped `text/event-stream` stream. Clients MUST advertise and support
both response types. The final response SHOULD terminate a request-scoped SSE
stream.
[Streamable HTTP: Sending and Receiving Messages](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http)

The modern revision removes:

- the standalone GET notification stream;
- DELETE session termination;
- `Mcp-Session-Id`;
- `Last-Event-ID` and SSE event resumption/redelivery; and
- independent server-to-client JSON-RPC requests.

If a response stream breaks, the in-flight request is lost and the client must
retry it as a new request with a new request ID. A modern-only server should
return `405` for legacy GET/DELETE, ignore incoming legacy session/resumption
headers, and never mint or echo a session ID.
[Streamable HTTP: Earlier revisions](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http#earlier-streamable-http-revisions),
[2026-07-28 changelog: major changes](https://modelcontextprotocol.io/specification/2026-07-28/changelog#major-changes)

Closing a request's SSE response stream is the HTTP cancellation signal. The
server SHOULD stop the work promptly and MUST NOT send further messages for
that request. The modern HTTP core does not send
`notifications/cancelled`.
[Streamable HTTP: Cancellation](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http#cancellation)

Long-lived change notifications use a client-initiated
`subscriptions/listen` request whose response is an SSE stream. Request-scoped
progress and log notifications remain on the originating request's response
stream. If deployments need an event produced on one instance to reach a
listen stream held by another, the application still needs a shared pub/sub
mechanism; protocol statelessness does not provide that application
distribution layer.
[Message patterns: Subscribe and Notify](https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns#subscribe-and-notify),
[TypeScript SDK: sessions, state, and scaling](https://ts.sdk.modelcontextprotocol.io/v2/serving/sessions-state-scaling)

### Required HTTP routing headers

Every modern HTTP POST MUST carry `MCP-Protocol-Version` and `Mcp-Method`.
`tools/call`, `resources/read`, and `prompts/get` also require `Mcp-Name`.
The protocol version header must match the request `_meta`, and servers that
process the body MUST validate all mirrored header/body values. Missing,
malformed, or mismatched values produce HTTP `400` with `HeaderMismatch`
(`-32020`).
[Streamable HTTP: request metadata and validation](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http#request-metadata)

Tools may mark primitive input properties with `x-mcp-header`, requiring
clients to mirror their values as `Mcp-Param-*` headers. Client and server
implementations must apply the specified validation and Base64 sentinel
encoding rules. This is relevant even if ChenChess does not initially use
custom routing headers because a conforming modern client must understand
them.
[Streamable HTTP: custom headers from tool parameters](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http#custom-headers-from-tool-parameters)

The existing Streamable HTTP security rules remain: servers MUST validate a
present `Origin` and return `403` for an invalid origin; local servers SHOULD
bind to localhost; servers SHOULD authenticate connections.
[Streamable HTTP: Security & Endpoint](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http#security-endpoint)

### Results, MRTR, and explicit state

Every successful modern result MUST carry `resultType`. Ordinary results use
`"complete"`; an unrecognized result type is invalid. For backward
compatibility, clients MUST treat a result from an earlier server that omits
the field as `"complete"`.
[Base protocol: ResultType](https://modelcontextprotocol.io/specification/2026-07-28/basic#resulttype)

Servers no longer issue `roots/list`, `sampling/createMessage`, or
`elicitation/create` as independent JSON-RPC requests. A server needing input
returns `resultType: "input_required"` with `inputRequests`; the client retries
the original method with `inputResponses` and echoes any `requestState`.
Only `prompts/get`, `resources/read`, and `tools/call` may return this core
`InputRequiredResult`.
[MRTR specification](https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/mrtr)

Protocol statelessness does not require the application or database to be
stateless. General cross-call state should use explicit, server-minted domain
handles passed as ordinary tool arguments. `requestState` is specifically an
MRTR continuation token; because the client can modify it, a server that uses
it for authorization, resource access, or business decisions MUST
integrity-protect and verify it. Binding it to the principal, original
operation, arguments, and a short expiry is recommended. Strict single-use
replay prevention still requires server-side storage.
[Release post: No handshake or sessions](https://blog.modelcontextprotocol.io/posts/2026-07-28/#no-handshake-or-sessions),
[MRTR: Server Requirements](https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/mrtr#server-requirements)

### Caching and deterministic lists

`server/discover`, list operations, and `resources/read` now return `ttlMs` and
`cacheScope` (`public` or `private`). Servers SHOULD return tools in a
deterministic order. These hints allow clients to cache catalogs without
making catalogs connection-specific.
[2026-07-28 changelog: cacheable results](https://modelcontextprotocol.io/specification/2026-07-28/changelog#minor-changes)

### Authorization and deprecations

The revision hardens authorization as follows:

- authorization servers SHOULD include RFC 9207 `iss`, and clients MUST
  validate a present issuer before code redemption;
- DCR clients must set an appropriate `application_type`;
- persisted client credentials MUST be keyed to the issuer and MUST NOT be
  reused with a different authorization server; and
- Dynamic Client Registration is deprecated in favor of CIMD, while remaining
  available for backward compatibility.

[2026-07-28 changelog: authorization](https://modelcontextprotocol.io/specification/2026-07-28/changelog#minor-changes),
[deprecated features](https://modelcontextprotocol.io/specification/2026-07-28/deprecated)

Roots, Sampling, Logging, HTTP+SSE, and DCR are deprecated rather than
immediately removed. The feature lifecycle guarantees a minimum twelve-month
window, and new implementations should not adopt deprecated features.
[Deprecated features registry](https://modelcontextprotocol.io/specification/2026-07-28/deprecated),
[feature lifecycle](https://modelcontextprotocol.io/community/feature-lifecycle)

## SDK support and migration

### TypeScript

The final release names TypeScript, Python, Go, and C# as Tier 1 SDKs supporting
`2026-07-28`.
[MCP release: SDKs](https://blog.modelcontextprotocol.io/posts/2026-07-28/#sdks)

TypeScript v2's `createMcpHandler(factory)` serves the modern revision per
request and, by default, also serves legacy traffic through the established
legacy-stateless behavior. The factory constructs a fresh `McpServer` for each
request. `legacy: "reject"` makes the endpoint modern-only. Existing code that
connects a hand-built server directly to an older transport entry does not
become modern merely because the dependency was upgraded.
[TypeScript 2026 serving guidance](https://ts.sdk.modelcontextprotocol.io/v2/migration/support-2026-07-28#server-over-http-createmcphandler),
[TypeScript legacy clients](https://ts.sdk.modelcontextprotocol.io/v2/serving/legacy-clients)

TypeScript clients preserve the legacy initialization handshake by default.
They must opt into `ClientOptions.versionNegotiation` (for example,
`mode: "auto"`) to probe modern and fall back. The SDK also provides a legacy
shim so handlers written with `inputRequired(...)` can serve 2025-era
connections, and it handles modern envelope fields, routing headers,
`resultType`, and conservative cache defaults internally.
[TypeScript 2026 migration guide](https://ts.sdk.modelcontextprotocol.io/v2/migration/support-2026-07-28)

Projects using `@modelcontextprotocol/sdk` v1 require the documented v2
migration: v2 splits client, server, core, and runtime adapter packages and
requires Node.js 20 or newer. This is not a drop-in dependency-only upgrade.
[TypeScript v1-to-v2 migration](https://ts.sdk.modelcontextprotocol.io/v2/migration/upgrade-to-v2)

### Rust

The release post initially described Rust support as beta, but stable
`rmcp-v3.0.0` was published later on 2026-07-28. The current official README
states that v3 implements stable `2026-07-28` and remains compatible with
`2025-11-25` and earlier.
[MCP release: SDK status](https://blog.modelcontextprotocol.io/posts/2026-07-28/#sdks),
[rmcp releases](https://github.com/modelcontextprotocol/rust-sdk/releases),
[rmcp README](https://github.com/modelcontextprotocol/rust-sdk)

For a modern request, `rmcp` automatically omits sessions, GET/DELETE streams,
and resumption. Its service factory runs per request, so shared database pools
or caches belong in clonable application handles captured by the factory;
per-request handler memory cannot provide cross-call continuity. The
`legacy_session_mode` setting controls only older protocol revisions.
[rmcp: Stateless Streamable HTTP](https://github.com/modelcontextprotocol/rust-sdk#stateless-streamable-http)

## OpenAI behavior documented on 2026-07-29

OpenAI's current server guide requires a stable public HTTPS Streamable HTTP
endpoint and recommends the official TypeScript or Python SDK. It still
describes server `instructions` as arriving "during initialization" and links
that behavior to the `2025-06-18` lifecycle specification. The page does not
name `2026-07-28`, modern request metadata, `server/discover`, or dual-era
behavior.
[OpenAI: Build an MCP server](https://developers.openai.com/plugins/build/mcp-server)

The Responses API documentation says its MCP tool can connect to Streamable
HTTP or legacy HTTP/SSE servers. It retains an `mcp_list_tools` item in model
context to avoid re-fetching the tool list, but does not state that this is the
new MCP cache-hint mechanism. OpenAI does not retain the remote authorization
value, so the API caller must include it in every Responses API creation
request. These are OpenAI API behaviors, not proof of `2026-07-28` wire
support by the remote MCP client.
[OpenAI: MCP and Connectors](https://developers.openai.com/api/docs/guides/tools-connectors-mcp)

OpenAI's current authentication guide explicitly targets the MCP
`2025-11-25` authorization spec. It already prefers CIMD when configured, still
supports DCR, performs authorization-code + PKCE S256, sends resource-bound
tokens, attaches a bearer token to subsequent MCP requests, and requires the
server to verify issuer, audience, expiry, and scopes on every invocation.
Those behaviors are compatible with the new direction, but the page does not
document the new RFC 9207 issuer-response validation or declare
`2026-07-28` support.
[OpenAI: Authentication](https://developers.openai.com/plugins/build/auth)

**OpenAI uncertainty:** a search through the official OpenAI developer-docs
index on 2026-07-29 found no page explicitly documenting ChatGPT, Codex, or the
Responses API as a `2026-07-28` MCP client. This is an absence-of-documentation
finding, not evidence that OpenAI clients lack support. Treat modern host
support as unconfirmed until OpenAI publishes it or a real-host interoperability
test records the handshake-free request shape.

## Anthropic/Claude behavior documented on 2026-07-29

Claude's connector guide supports Streamable HTTP and legacy HTTP+SSE and says
the latter is being deprecated. Its authentication support list names the
`2025-03-26`, `2025-06-18`, and `2025-11-25` authorization specifications, not
`2026-07-28`. It supports tools, prompts, resources, text/image tool results,
and text/binary resources, while resource subscriptions, sampling, and
advanced/draft capabilities are not supported on the documented hosted
connector surface.
[Claude: Building custom connectors](https://claude.com/docs/connectors/building)

Claude Code recommends remote HTTP, accepts `streamable-http` as an alias for
its `http` transport configuration, and calls legacy SSE deprecated. That page
does not identify a negotiated MCP protocol revision.
[Claude Code: MCP](https://code.claude.com/docs/en/mcp)

Claude's current lazy-authentication guide still explicitly accommodates
"stateful Streamable HTTP sessions" and places authentication before the
session transport handler. This shows that legacy sessionful servers remain a
documented compatibility case; it does not establish that modern stateless
requests are unsupported.
[Claude: Lazy authentication](https://claude.com/docs/connectors/building/lazy-authentication)

Claude currently supports both DCR and CIMD. It selects CIMD only when the
authorization-server metadata advertises both
`client_id_metadata_document_supported: true` and `"none"` in
`token_endpoint_auth_methods_supported`; otherwise it falls back to DCR.
Claude uses PKCE S256 and refreshes tokens reactively on `401`, with a
proactive refresh window up to five minutes before expiry. Therefore a
cross-host server should keep DCR compatibility during the deprecation window
unless host testing proves CIMD-only onboarding works everywhere.
[Claude: Authentication for connectors](https://claude.com/docs/connectors/building/authentication)

**Anthropic uncertainty:** an official-domain documentation search on
2026-07-29 found no Claude or Claude Code page explicitly claiming
`2026-07-28` client support or documenting `server/discover`, per-request
capability metadata, MRTR, or the new standard HTTP headers. This is
documentation lag/absence, not proof of missing runtime support. A modern-only
cutover would therefore be speculative without direct real-host evidence.

## Compatibility and rollout implications for issue #157

These are protocol-level implications for the implementation review, not
findings about the current ChenChess code:

1. Prefer one dual-era `/mcp` endpoint backed by an SDK's documented modern
   entry point. Do not implement a separate custom protocol dialect.
   [Version compatibility](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning#backward-compatibility-with-initialization-based-versions)
2. Keep domain state behind explicit, principal-scoped identifiers. Do not
   move durable Review Session or operation state into transport sessions or
   unsigned client-carried continuation data.
   [Base protocol: Statelessness](https://modelcontextprotocol.io/specification/2026-07-28/basic#statelessness),
   [MRTR server requirements](https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/mrtr#server-requirements)
3. Test both JSON and request-scoped SSE responses, stream-close cancellation,
   retry after broken streams, required header/body validation, modern version
   errors, and legacy fallback. These behaviors are normative.
   [Streamable HTTP](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http)
4. Preserve CIMD and DCR during host migration. Add the new issuer validation,
   DCR `application_type`, and issuer-bound credential rules wherever
   ChenChess acts as an OAuth client; verify every access token on every
   server request regardless of transport era.
   [2026 authorization changes](https://modelcontextprotocol.io/specification/2026-07-28/changelog#minor-changes),
   [OpenAI authentication](https://developers.openai.com/plugins/build/auth),
   [Claude authentication](https://claude.com/docs/connectors/building/authentication)
5. Do not make a modern-only production cutover until ChatGPT and Claude real
   host tests show the intended era and all required flows. The public vendor
   documentation does not yet provide that guarantee.

## Source index

- [MCP 2026-07-28 release](https://blog.modelcontextprotocol.io/posts/2026-07-28/)
- [MCP 2026-07-28 specification](https://modelcontextprotocol.io/specification/2026-07-28)
- [MCP 2026-07-28 changelog](https://modelcontextprotocol.io/specification/2026-07-28/changelog)
- [Base protocol](https://modelcontextprotocol.io/specification/2026-07-28/basic)
- [Versioning and compatibility](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning)
- [Streamable HTTP](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http)
- [Discovery](https://modelcontextprotocol.io/specification/2026-07-28/server/discover)
- [Multi Round-Trip Requests](https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/mrtr)
- [TypeScript SDK 2026 migration](https://ts.sdk.modelcontextprotocol.io/v2/migration/support-2026-07-28)
- [TypeScript SDK v1-to-v2 migration](https://ts.sdk.modelcontextprotocol.io/v2/migration/upgrade-to-v2)
- [Rust SDK](https://github.com/modelcontextprotocol/rust-sdk)
- [OpenAI MCP server guide](https://developers.openai.com/plugins/build/mcp-server)
- [OpenAI MCP authentication](https://developers.openai.com/plugins/build/auth)
- [OpenAI Responses API MCP tool](https://developers.openai.com/api/docs/guides/tools-connectors-mcp)
- [Claude connector guide](https://claude.com/docs/connectors/building)
- [Claude connector authentication](https://claude.com/docs/connectors/building/authentication)
- [Claude Code MCP](https://code.claude.com/docs/en/mcp)
