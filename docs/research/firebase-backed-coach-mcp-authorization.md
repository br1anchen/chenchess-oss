# Firebase-backed Coach MCP authorization façade and staging proof

Status note (2026-07-26): the final
[Coach App product and implementation specification](./coach-app-product-and-implementation-specification.md)
supersedes this prototype's deployment, Firebase-verification placement, and
single-database storage assumptions. Its verified OAuth protocol profile,
token lifetimes, revocation bound, and cross-host evidence remain in force.

Research date: 2026-07-24

This note resolves the protocol and evidence questions in [GitHub issue #71](#71). It builds on the cross-host contract in `mcp-apps-cross-host-contract.md` and supersedes the provider choice—not the security invariants—in `convex-better-auth-mcp-authorization.md`.

## Decision

Use one OAuth authorization-server façade on the same stable Railway origin as the protected Coach MCP resource. Firebase Authentication remains the sign-in system and canonical identity source, while the façade owns the OAuth surface and issues Coach-specific access and refresh tokens. The live prototype proved that standard Firebase Authentication is sufficient for the email/password bridge; upgrading the Firebase project to Identity Platform is not required for this contract.

The smallest common host contract is:

- OAuth authorization code with PKCE `S256`;
- public clients with `token_endpoint_auth_method=none`;
- Dynamic Client Registration (DCR) as the shared registration path;
- one exact protected-resource identifier, including its `/mcp` path;
- JWT access tokens whose `aud` is that exact resource;
- one initial Coach scope;
- short-lived access tokens and rotating refresh tokens;
- RFC 7009 revocation for refresh tokens and grants;
- an asymmetric signing-key set exposed through `jwks_uri`; and
- Firebase `uid` copied unchanged into the Coach token `sub`.

Both ChatGPT and Claude document support for public DCR clients and PKCE. ChatGPT also supports Client ID Metadata Documents (CIMD), and Claude prefers CIMD only when the authorization-server metadata advertises the required support. DCR is the mature intersection and avoids depending on the authorization library's still-experimental CIMD implementation. The app creator must select DCR when configuring the ChatGPT app. Claude falls back to DCR when CIMD is not advertised. ([OpenAI Apps SDK authentication](https://developers.openai.com/apps-sdk/build/auth), [Claude connector authentication](https://claude.com/docs/connectors/building/authentication), [MCP authorization specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization), [`node-oidc-provider` v9.10.0](https://github.com/panva/node-oidc-provider/tree/v9.10.0))

## Boundary and topology

Use stable nonproduction URLs of this form:

```text
OAuth issuer:       https://<stable-railway-host>
Protected resource: https://<stable-railway-host>/mcp
```

The literal deployed values become configuration, but the following invariant is not configurable per host:

```text
resource requested during authorization
= resource requested during token exchange
= access-token aud
= protected-resource metadata resource
= the URL entered in ChatGPT and Claude
```

The `/mcp` path and trailing-slash choice are part of the identifier. Claude explicitly requires the protected-resource metadata `resource` to match the connector URL as entered, including the path. MCP and RFC 8707 require the client to send `resource` to both authorization and token endpoints and require the authorization server to audience-restrict the access token. ([Claude connector authentication](https://claude.com/docs/connectors/building/authentication), [MCP authorization specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization), [RFC 8707](https://www.rfc-editor.org/rfc/rfc8707.html))

The façade should be a small Node service or module using [`node-oidc-provider` v9.10.0](https://github.com/panva/node-oidc-provider/tree/v9.10.0), mounted beside or reverse-proxied with the Rust Coach MCP service. That release supplies the needed standards surface: authorization-server metadata, DCR, PKCE, resource indicators, revocation, refresh-token rotation, JWT access tokens, and JWKS publication. A persistent custom adapter is required; its in-memory adapter is for development only. Run the provider on Node rather than assuming undocumented Bun compatibility.

The same Railway deployment must expose:

- `GET /.well-known/oauth-protected-resource/mcp`, with a root well-known alias for clients that fall back there;
- OAuth authorization-server metadata for the exact issuer;
- authorization, token, registration, revocation, and JWKS endpoints;
- the Firebase-backed login and consent interaction;
- the Streamable HTTP MCP endpoint at `/mcp`; and
- no redirects on metadata or `/mcp` endpoints.

The protected-resource document names the exact `/mcp` resource, the single authorization server, and supported scopes. An unauthenticated `/mcp` request returns a real HTTP `401` with a `WWW-Authenticate: Bearer` challenge containing `resource_metadata` and the required scope. Claude does not initiate authorization from an error embedded in an HTTP `200`. ChatGPT additionally needs its tool `securitySchemes`, resource metadata, and tool-result `_meta["mcp/www_authenticate"]` wired consistently so its UI can initiate linking. ([RFC 9728](https://www.rfc-editor.org/rfc/rfc9728.html), [Claude lazy authentication](https://claude.com/docs/connectors/building/lazy-authentication), [OpenAI Apps SDK authentication](https://developers.openai.com/apps-sdk/build/auth))

## Firebase is the sign-in bridge, not the Coach access-token issuer

Firebase's documented browser flow yields a Firebase ID token. The backend verifies that token and obtains the Firebase `uid`. Firebase documents that token's audience as the Firebase project ID and its issuer as `https://securetoken.google.com/<projectId>`. Identity Platform also distinguishes upstream provider credentials from the Firebase credential produced after sign-in. It does not document the MCP authorization-server surface needed here: protected-resource discovery, OAuth authorization and token endpoints, DCR, or RFC 8707 resource processing. Therefore, as an inference from the documented contracts, a Firebase or social-provider token is not a valid Coach MCP access token. ([Verify Firebase ID tokens](https://firebase.google.com/docs/auth/admin/verify-id-tokens), [Identity Platform user concepts](https://cloud.google.com/identity-platform/docs/concepts-manage-users))

The bridge is:

1. A host starts an authorization request with a registered public client, exact `resource`, requested scope, state, and PKCE challenge.
2. The façade creates a one-time interaction record bound to that request.
3. Its interaction page signs the person in with the Firebase Web SDK.
4. The page posts the Firebase ID token and interaction identifier to the façade over HTTPS.
5. The façade verifies the token with the Firebase Admin SDK, including the revocation check for this sign-in boundary, and obtains `uid`.
6. The façade presents explicit Coach consent, records the grant for that client and `uid`, and completes the authorization-code flow.
7. The token endpoint validates the one-time code, redirect URI, PKCE verifier, client, and exact `resource`, then issues Coach tokens.

The Firebase token is accepted only at step 5. It is never forwarded to `/mcp`, used as the Coach bearer token, logged, or returned to either host. The façade should bind the interaction to an unguessable, short-lived one-time record rather than trusting a browser-supplied account identifier.

Firebase documents `uid` as the unique identifier for a user in a Firebase project, and linked sign-in providers retain the same Firebase user. Use that exact `uid` as `sub` and therefore as the existing Coach Player ID; do not derive it from email, display name, provider subject, DCR client, or host. ([Manage Firebase users](https://firebase.google.com/docs/auth/admin/manage-users), [Link multiple auth providers](https://firebase.google.com/docs/auth/web/account-linking))

Checking Firebase revocation during sign-in does not continuously couple later Coach grants to Firebase session state. Firebase ID tokens are normally valid for about an hour, while Firebase refresh tokens are long-lived until a documented revocation event. If disabling or deleting a Firebase account must immediately terminate existing Coach sessions, an administrative event must also revoke that user's Coach grants; it does not happen automatically. ([Manage Firebase sessions](https://firebase.google.com/docs/auth/admin/manage-sessions))

## Authorization-server contract

### Discovery and registration

Authorization-server metadata must advertise at least:

```json
{
  "issuer": "https://<stable-railway-host>",
  "authorization_endpoint": "https://<stable-railway-host>/<authorization-path>",
  "token_endpoint": "https://<stable-railway-host>/<token-path>",
  "registration_endpoint": "https://<stable-railway-host>/<registration-path>",
  "revocation_endpoint": "https://<stable-railway-host>/<revocation-path>",
  "jwks_uri": "https://<stable-railway-host>/<jwks-path>",
  "response_types_supported": ["code"],
  "grant_types_supported": ["authorization_code", "refresh_token"],
  "code_challenge_methods_supported": ["S256"],
  "token_endpoint_auth_methods_supported": ["none"],
  "scopes_supported": ["coach:review", "offline_access"]
}
```

Do not advertise CIMD support for this prototype. Enable unauthenticated DCR and restrict registered clients to:

- public clients only (`token_endpoint_auth_method=none`);
- authorization-code and refresh-token grants only;
- `response_types=["code"]`;
- known HTTPS host callback patterns, including ChatGPT's documented `https://chatgpt.com/connector/oauth/{callback_id}` and Claude's documented `https://claude.ai/api/mcp/auth_callback`;
- no wildcard or caller-controlled non-host redirect;
- no privileged scope beyond the supported set; and
- sensible metadata size limits and registration rate limits.

DCR is public by design here, not unvalidated. RFC 7591 permits public registration and `none` token authentication subject to authorization-server policy. Persist each generated client because both hosts reuse it after registration. ([RFC 7591](https://www.rfc-editor.org/rfc/rfc7591.html), [OpenAI Apps SDK authentication](https://developers.openai.com/apps-sdk/build/auth), [Claude connector authentication](https://claude.com/docs/connectors/building/authentication))

Exact client metadata submitted by each current host—especially `grant_types`, redirect URI shape, and requested scopes—is live evidence, not something their documentation fully freezes. Capture a redacted registration transcript in staging and fail closed if it falls outside policy.

### Scope and consent

Start with one domain scope:

```text
coach:review
```

It covers the current protected Coach workflow: import the signed-in player's game, create and continue that player's review session, and publish that player's review artifacts. Split it only when the product has a capability that can be meaningfully granted and denied independently.

The consent page must identify Coach, the connecting host/client, the signed-in Firebase account, the `coach:review` capability, and that the connection remains usable through a rotating refresh token until revoked. Consent is recorded per `(Firebase uid, OAuth client, resource, scope set)`, so revoking the ChatGPT grant does not revoke the Claude grant for the same player.

Claude appends `offline_access` when authorization-server metadata advertises it. ChatGPT's documentation requires planning for refresh but does not publish an exact refresh schedule or guarantee precisely how every current client requests `offline_access`. Configure the provider to issue refresh tokens to approved public clients that are registered for the refresh-token grant after explicit persistent-access consent, rather than relying solely on the presence of that scope. Verify both hosts' real authorization and token requests in staging. ([Claude connector authentication](https://claude.com/docs/connectors/building/authentication), [OpenAI Apps SDK authentication](https://developers.openai.com/apps-sdk/build/auth))

### Access tokens

Issue an RS256 JWT access token with a 10-minute lifetime. Its JOSE header includes:

```json
{
  "typ": "at+jwt",
  "alg": "RS256",
  "kid": "<active signing key>"
}
```

Its claims include at least:

```json
{
  "iss": "https://<stable-railway-host>",
  "aud": "https://<stable-railway-host>/mcp",
  "sub": "<Firebase uid>",
  "client_id": "<registered public client>",
  "scope": "coach:review",
  "iat": 0,
  "exp": 0,
  "jti": "<unique token id>"
}
```

Ten minutes is short enough to bound the residual life of a revoked self-contained token, but it avoids creating an access token that Claude may consider immediately eligible for proactive refresh: Claude documents refreshing up to five minutes before expiry and retrying reactively after a `401`. ChatGPT's exact refresh timing is unconfirmed until observed. RFC 9068 supplies the JWT access-token validation profile and requires issuer, audience, expiry, signature, and authorization claims to be checked by the resource server. ([RFC 9068](https://www.rfc-editor.org/rfc/rfc9068.html), [Claude connector authentication](https://claude.com/docs/connectors/building/authentication))

### Refresh and revocation

Use a 14-day inactivity/absolute policy suitable for nonproduction, rotate a public client's refresh token on every successful exchange, and revoke the complete grant on reuse. A refresh exchange must preserve `sub`, client, resource, and the consented scope ceiling. Return standard `invalid_grant` for an invalid, expired, revoked, or reused refresh token.

Enable RFC 7009 revocation. Revoking a refresh token revokes its grant and all refresh descendants. The chosen structured JWT access tokens cannot be directly introspected or recalled by `node-oidc-provider`; RFC 7009 also acknowledges that immediate revocation of a self-contained access token needs additional backend coordination. In the smallest façade, an already-issued access token can therefore remain usable for at most its 10-minute lifetime plus accepted clock skew. The UI and proof report must not claim immediate access-token revocation. If immediate termination becomes a requirement, add an online grant/jti denylist check at `/mcp` or switch to opaque reference tokens plus introspection. ([RFC 7009](https://www.rfc-editor.org/rfc/rfc7009.html), [`node-oidc-provider` changelog](https://github.com/panva/node-oidc-provider/blob/v9.10.0/CHANGELOG.md))

### Persistence

Implement the provider's custom adapter with Firestore for registered clients, interactions, authorization codes, grants/consent, refresh tokens, replay state, and revocation state. Consume authorization codes and rotate refresh-token families transactionally. Firestore retries transactions on concurrent edits and applies their writes atomically. ([Firestore transactions](https://firebase.google.com/docs/firestore/manage-data/transactions))

Every read must enforce the artifact's logical expiry. Firestore TTL is cleanup only: Firebase says deletion is not instantaneous and expired documents can typically remain for up to 24 hours. ([Firestore TTL](https://firebase.google.com/docs/firestore/ttl))

Keep private signing JWKs in Railway-managed secrets, not in Firestore documents or source control. Use Firestore server credentials with least-privilege IAM; Firebase documents that server client libraries bypass Firestore Security Rules and authenticate through Google Application Default Credentials. ([Firestore Security Rules conditions](https://firebase.google.com/docs/firestore/security/rules-conditions))

### Signing-key rotation

Publish only public keys through `jwks_uri`, and include a unique `kid` in each JWT. A safe staged rotation is:

1. Generate K2 without removing K1.
2. Deploy JWKS containing K1 and K2 while continuing to sign with K1.
3. After all resource-server caches have observed K2, make K2 the signing key.
4. Keep K1 published until every K1 access token has exceeded its maximum lifetime plus clock skew and cache margin.
5. Remove K1.

The provider documents a two-stage ordering procedure for introducing signing keys; RFC 8414 exposes the `jwks_uri`, and RFC 7517 defines the key-set format. The retirement overlap above follows from the resource server's obligation to validate unexpired tokens. ([`node-oidc-provider` key configuration](https://github.com/panva/node-oidc-provider/blob/v9.10.0/docs/README.md), [RFC 8414](https://www.rfc-editor.org/rfc/rfc8414.html), [RFC 7517](https://www.rfc-editor.org/rfc/rfc7517.html))

## Review Engine resource-server gate

The current Review Engine gate already checks an RS256 signature and `exp`, `iss`, `aud`, and nonempty `sub`, but the staging façade requires these focused changes:

- replace the hard-coded Convex audience with the exact configured `/mcp` resource;
- replace the static `JWT_JWKS` dependency with authorization-server metadata plus `jwks_uri`, while retaining a last-known-good cache across transient fetch failure;
- select by `kid`, allow one single-flight refresh on an unknown `kid`, and reject if it remains unknown;
- require `typ=at+jwt`, exact `iss`, exact `aud`, valid `iat`/`exp`, and an allowed asymmetric algorithm;
- require `scope` to contain `coach:review`;
- require `sub` to satisfy Firebase UID constraints and continue using it unchanged as Player ID;
- reject Firebase ID tokens and social-provider tokens through the same issuer, audience, type, and scope checks;
- return `401` plus an `invalid_token` bearer challenge for missing or invalid credentials; and
- return `403` plus `error="insufficient_scope"` and the required scope when the bearer token is otherwise valid.

JWT validation authenticates the Player; it does not replace object authorization. Every game, review session, variation, note, and published artifact lookup must still be constrained by the authenticated Player ID.

## Required staging proof

The ticket should not be closed from configuration screenshots alone. Retain a dated, redacted evidence packet with deployment version, exact nonsecret URLs, host/client versions where visible, Firebase project alias, and correlation IDs.

### Direct protocol proof

Before testing either consumer host:

1. Fetch both protected-resource well-known locations, authorization-server metadata, and JWKS from a clean network client.
2. Send an unauthenticated MCP request and record the HTTP `401` bearer challenge.
3. Register two public DCR clients through the same policy, using the current ChatGPT and Claude callback forms.
4. Complete an authorization-code exchange with `S256`, exact `resource`, and one-time code semantics.
5. Decode a redacted access token locally and assert `typ`, `alg`, `kid`, `iss`, exact `aud`, `sub`, `client_id`, `scope`, `iat`, `exp`, and unique `jti`.
6. Prove that a Firebase ID token and an upstream social-provider token are both rejected by `/mcp`.
7. Prove rejection of wrong issuer, wrong audience, missing scope, expired token, malformed/empty subject, unknown key, wrong PKCE verifier, reused authorization code, unregistered redirect, and credentials in a query string.
8. Exchange RT1 for RT2, reuse RT1, and prove `invalid_grant` plus invalidation of the refresh family.
9. Re-authorize, revoke the current refresh token, and prove subsequent refresh fails. Record that its last JWT access token remains usable only until the documented 10-minute bound, unless an online revocation check was deliberately added.
10. Exercise K1-to-K2 rotation: validate a K1 token, publish both keys, mint and validate a K2 token after an unknown-`kid` refresh, then remove K1 only after the overlap and prove expired K1 tokens fail.

Use the MCP Inspector where it helps validate the MCP exchange, but keep a deterministic protocol harness for negative cases and lifecycle assertions. Never retain raw access tokens, refresh tokens, Firebase tokens, authorization codes, PKCE verifiers, private JWKs, PGNs, or review content in logs or artifacts.

### Real-host proof

Use two dedicated Firebase test players, A and B:

1. Connect ChatGPT as A through the complete Firebase login, consent, MCP tool call, and Coach response.
2. Connect Claude as A through the same shared façade and protected resource.
3. Connect at least one host as B while A remains connected in the other host.
4. From both hosts, create and resume player-owned review state and verify the audit trail maps each call to the expected Firebase `uid`, client/grant, resource, scope, and signing `kid`.
5. Force or wait for access-token expiry in each host and observe a successful refresh without another Firebase sign-in. Record redacted token-endpoint fields and token-family identifiers, not token values.
6. Revoke A's ChatGPT grant. Prove its next refresh fails while A's Claude grant and B's grant continue to work. After the JWT residual window, prove the revoked ChatGPT connection receives `401`.
7. Repeat revocation for Claude so both hosts' refresh behavior is evidenced.
8. Attempt cross-player game and review identifiers in automated ownership tests and through the available host surface. Prove A cannot read or mutate B's state, and vice versa.
9. Run overlapping A/B operations and verify there is no shared in-memory “current player,” review session, or host-client state.

Redacted structured server events should include a correlation ID, deployment version, host client identifier or stable hash, pseudonymized `sub`, grant identifier hash, `aud`, scope set, `kid`, outcome, and owned-resource identifier hash. They must not contain credentials or user content.

## What the evidence cannot prove

- A successful host screenshot proves that one current account, client version, deployment, and flow worked. It does not prove support across every ChatGPT or Claude plan, desktop/mobile client, future release, or directory-review environment.
- Screenshots cannot prove PKCE, `resource`, access-token claims, refresh rotation, replay rejection, or signing-key rollover. Those require redacted server events and deterministic protocol tests.
- Neither host exposes its complete credential store. “No credential leakage” means the defined logs, traces, UI, error responses, and retained artifacts were inspected and the negative test matrix passed; it is not an exhaustive proof of host internals.
- OpenAI's public documentation requires a sound refresh/revocation design but does not publish the exact ChatGPT refresh schedule. Label timing and retry behavior as observed evidence, not a portable guarantee.
- Claude documents proactive refresh up to five minutes before expiry and reactive refresh on `401`, but a single observation still does not guarantee all Claude clients behave identically.
- Revoking a refresh token does not immediately invalidate an already-issued self-contained JWT in the smallest design. The residual authorization window is explicit and bounded.
- Firebase-session revocation and Coach-grant revocation are separate systems after the sign-in bridge. Testing one does not prove the other.
- Principal separation at the token layer does not prove application data isolation. Ownership tests at each Coach storage and publication boundary are required.
- A key-rotation drill proves the exercised overlap and cache behavior; it does not prove that future operators will preserve the procedure.

## Prototype status — 2026-07-24

The local fake-identity harness now directly proves:

- both protected-resource discovery locations, authorization-server discovery,
  public DCR restricted to known callback shapes, and the HTTP `401` bearer
  challenge;
- authorization code with exact resource and `S256`, rejection of a wrong PKCE
  verifier, one-time authorization-code semantics, and grant invalidation on
  code replay;
- RS256 `at+jwt` access tokens with exact issuer/audience and
  `coach:review`, accepted by both the MCP gate and the Review Engine gate with the same
  pseudonymized Player fingerprint;
- distinct fake Players A and B through the same MCP tool without exposing the
  underlying IDs;
- refresh rotation, refresh-family invalidation after token replay, RFC 7009
  grant revocation, and the explicit residual access-token window; and
- a Node 22 Railway-shaped Docker image that builds and answers its health
  check.

The Review Engine negative matrix independently rejects wrong issuer, audience, scope,
expiry, subject, token type, key ID, issued-at, and token ID. This was the
pre-deployment state; the live evidence captured next is directly observed
rather than inferred from the local run.

## Live staging result — 2026-07-25

The façade was deployed at
`https://coach-oauth-production.up.railway.app` in the nonproduction Railway
project `chenchess`. Its exact protected resource is
`https://coach-oauth-production.up.railway.app/mcp`. Firebase project
`chenchess` supplied standard Firebase Authentication and Firestore on the
Spark plan. No Identity Platform upgrade or Firebase billing account was
needed for the prototype. The Railway service account was limited to
`roles/datastore.user` and `roles/firebaseauth.viewer`.

Two dedicated Firebase test Players produced stable, distinct, non-secret
fingerprints:

| Surface                           | Player | Observed fingerprint |
| --------------------------------- | ------ | -------------------- |
| ChatGPT plugin tool call          | A      | `334b3bb9708b`       |
| Claude custom connector tool call | B      | `9c675f46b8ec`       |

Both hosts independently completed public DCR, authorization-code plus `S256`
PKCE, Firebase sign-in, explicit `openid coach:review` consent, token exchange,
authenticated MCP initialization, tool discovery, and one `auth_probe` call
against the same issuer and protected resource. The host connections remained
simultaneously usable with different Firebase Players. ChatGPT returned
`{"playerFingerprint":"334b3bb9708b"}`; Claude returned
`Authenticated Player fingerprint: 9c675f46b8ec`.

Direct live smoke runs for both Players additionally proved:

- protected-resource and authorization-server discovery, JWKS discovery, the
  unauthenticated MCP `401` challenge, public DCR, and rejection of an
  untrusted redirect;
- exact issuer, exact `/mcp` audience, `at+jwt`, RS256, `kid`,
  `coach:review`, and the same pseudonymized subject at the MCP and Review Engine gates;
- rejection of a wrong PKCE verifier and one-time authorization-code reuse;
- rotating refresh tokens, refresh-family invalidation on replay, RFC 7009
  refresh-token revocation, and rejection of later refresh; and
- continued acceptance of an already-issued access token only for its bounded
  residual lifetime, measured below the configured ten-minute maximum.

ChatGPT exposed two additional interoperability requirements that were not
clear from configuration alone:

- `coach:review` must be allowed by the authorization provider's own scope
  registry as well as advertised in metadata;
- the OIDC UserInfo endpoint (`/me`) must accept the same Coach bearer token
  and return its `sub`, or ChatGPT completes the token exchange but leaves the
  plugin disconnected.

Railway health checks also required accepting Railway's health-check host, and
the staging authorization UI needed an explicit **Use a different Player**
action. Without that action, a shared browser OAuth session can silently reuse
the previous test Player even though the host grants themselves remain
separate.

This evidence closes the authorization-layer question: one Firebase-backed
Railway façade works unchanged for both hosts, preserves Firebase `uid` as the
Player subject, and does not share a current-Player value across clients or
hosts. The proof is deliberately bounded to `auth_probe`. Host-managed refresh
timing, live signing-key rollover, and ownership of real game/review objects
remain part of the later full Coach App staging journey; the deterministic
protocol harness, rather than the host UI, proves refresh and revocation for
this prototype.

## Production-readiness exit criteria

The façade is proven only when:

- both hosts independently discover, register, authorize, refresh, and revoke against the same nonproduction resource;
- neither host presents a Firebase or social-provider token to `/mcp`;
- access tokens are Coach-issued, exact-audience, scoped, short-lived, and mapped to Firebase `uid`;
- rotating refresh tokens, replay response, DCR persistence, Firestore expiry enforcement, and JWKS rollover pass direct tests;
- two-player ownership and concurrency checks show no cross-player or cross-host state leakage;
- each evidence claim is labeled as specification-backed, directly observed, or still unconfirmed; and
- all retained artifacts are redacted and reproducible without credential disclosure.
