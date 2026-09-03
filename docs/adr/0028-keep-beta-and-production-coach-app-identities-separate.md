---
status: accepted-in-part
---

# Keep beta and production Coach App identities separate

ADR 0034 supersedes the provisional staging-only Firebase database topology
and resolves the production topology. The separate Coach App connection,
OAuth issuer, client, token, credential, cookie, and origin decisions below
remain accepted.
ADR 0055 supersedes the `/join` identity route below: Coach OAuth now begins at
`/login`, visits `/join` only when Beta Access is missing, and returns to the
server-owned interaction after both identity and admission are confirmed.

The staging Central Host uses `staging.example`, while the production Central Host uses `example.test`. A Beta Coach App Connection is a controlled, disposable tester connection and is never published as the permanent OpenAI plugin or Claude directory connector; production publication uses the apex-domain MCP origin and requires beta testers to install again and reauthorize. This preserves staging as a truthful end-to-end proving environment without binding a public marketplace identity, OAuth issuer, or protected-resource audience to a disposable origin.

The initial beta reuses the existing Firebase project rather than requiring a separate beta project. This intentionally shares the Firebase Authentication user pool and configured sign-in providers for now; it does not grant Beta Access, promote beta application data, or make a Beta Coach App Connection valid in production. Within that project, staging uses dedicated named Firestore databases for Coach Engine application data and Coach OAuth protocol state. Production never reads those databases, and closing the beta may delete them without deleting the shared Firebase identities.

The staging host also owns beta-only OAuth issuer metadata, client registrations, signing keys, cookie secrets, and encryption secrets. None are copied or promoted to production, so production installation and authorization cannot accept a beta client, token, grant, browser session, or cryptographic credential. The Firebase project topology for production remains a later release decision.

Within the shared Firebase Authentication user pool, Email/Password and Google credentials that present the same email may resolve to one Player ID only through Firebase's authenticated account-linking flow. ChenChess first requires the Player to authenticate with the existing provider and then explicitly links the pending credential. Email equality alone never merges identities or authorizes a link.

Coach OAuth does not implement a second identity UI. Its server-owned login
interaction sends the Player through the same `/join` Email/Password, Google,
verification, reset, and provider-linking journey as the web product. The join
URL carries only the opaque `oidc-provider` interaction UID, and the client
accepts that UID only as a bounded same-origin `/interaction/<uid>` return; it
never accepts an arbitrary return URL. The interaction cookie and durable OAuth
adapter retain client, PKCE, state, consent, and callback state while the
identity journey runs or the Central Host restarts.

On return, the browser supplies a fresh Firebase ID token to the original
interaction. Coach Engine accepts the OAuth identity bridge only when the token
has a verified email and reports `password` or `google.com` as the sign-in
provider. Enabling another provider in Firebase therefore cannot expose it as a
Supported Sign-In Method or bypass the Coach OAuth policy.

The beta now ships on both `staging.example` and `example.test`. The hosts remain separate issuers with separate keys, cookies, and Firestore; apex traffic is never redirected into beta, and cookies stay host-only without a parent-domain attribute. Staging pages stay `noindex`; production drops it. Testers who used the staging beta install and authorize afresh on production.
