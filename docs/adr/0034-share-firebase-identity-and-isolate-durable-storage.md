# Share Firebase identity and isolate durable storage

## Status

Accepted.

This decision supersedes any earlier requirement that the Coach Engine use the
`(default)` Firestore database. It also supersedes ADR 0028's provisional
staging-only Firebase topology and its deferral of the production topology.
ADR 0028's separate Coach App connection, OAuth issuer, client, token,
credential, cookie, and origin decisions remain in force.

This decision amends ADR 0018's evaluation retention shape and ADR 0033's Game
Findings shape. It does not change ADR 0031 or the public Game Import and Review
Session command contracts.

## Context

Staging and production currently name the same Firebase project and therefore
share Firebase Authentication, but their Coach Engine runtimes both write to
`(default)`. The Central Host also uses one hard-coded `coach-oauth` database.
Product data and OAuth state can cross environment boundaries even though the
OAuth issuers and credentials are separate.

The durable product model also repeats data that the Coach Engine can derive.
A Game Review may exist in both shared findings and an import. A Review Moment
stores facts that restoration re-derives from the import, and persisted
positions repeat board occupancy derived from FEN. Queryless documents use
indexed Firestore fields even though the Coach Engine has no query operation.

No stored product data has been released. Compatibility readers and data
migration would add permanent code for records no Player depends on.

## Decision

Use the existing `chenchess` Firebase project and its one Authentication user
directory for staging and production. Register a separate Firebase Web App for
each environment. The verified Firebase `uid` is the canonical Player ID in
both environments.

Each Firebase Web App uses its Central Host origin as `authDomain`:
`staging.example` for staging and `example.test` for production. The
Central Host proxies only Firebase's reserved `/__/auth/*` helper namespace to
the project's managed `https://chenchess.firebaseapp.com` origin. The Google
OAuth client registers both environment-specific `/__/auth/handler` redirect
URIs. Firebase Hosting remains unused, and no other `/__/` route is exposed.
This changes the browser-visible authentication origin without splitting the
shared Firebase Authentication directory.

Use five named Firestore databases in `eur3`:

| Database                 | Contents                                   | Writer                                             |
| ------------------------ | ------------------------------------------ | -------------------------------------------------- |
| `coach-app-staging`      | Staging product data                       | Staging Coach Engine                               |
| `coach-app-production`   | Production product data and quality outbox | Production Coach Engine                            |
| `coach-oauth-staging`    | Staging OAuth and OIDC protocol state      | Staging Central Host                               |
| `coach-oauth-production` | Production OAuth and OIDC protocol state   | Production Central Host                            |
| `coach-quality`          | Production quality captures                | Production exporter and offline evaluation tooling |

No runtime may use `(default)`. Each hosted service requires a validated
`DEPLOYMENT_ENVIRONMENT` and an explicit database ID from that environment's
allowlist. Missing, malformed, default, and cross-environment database IDs stop
startup.

Each service and environment has a distinct service account restricted to its
database with a database-scoped IAM condition. The production Coach Engine
holds a separate quality-export credential. Staging has no quality credential.
All five databases deny browser and other client SDK access through Security
Rules because server SDK authorization uses IAM. Enable delete protection for
`coach-app-production` and `coach-quality`.

Keep `ImportGame`, `StartReviewSession`, `GameImportId`, `GameImported`, and all
Review Moment commands. Storage sits behind one Coach Engine-owned durability
module. Its contract covers optional analysis resolution, reusable
self-contained Game Imports, retry-safe Review Session incarnation creation
and loading, atomic completed
command commits, and Player-data deletion. It does not expose Firestore
collections, cache retention, owner pointers, evidence packing, or checkpoint
layout.

Render every dynamic Firestore document path segment as a SHA-256 digest.
Player IDs, Game Import IDs, session identities, moment identities, OAuth
adapter IDs, and capture identities never appear raw in a Firestore path.

Store queryless payloads as canonical JSON behind one versioned decoder. Keep a
typed top-level field only when a read needs it before payload decoding, a TTL
policy needs it, or a real query uses it. Unknown schema versions and malformed
records fail closed. There are no legacy decoders.

Replace `gameFindings` with an environment-local `gameAnalysis` cache. Its
canonical digest includes the schema version, generation, canonical Game
digest, Review Side, and resolved Elo. The record contains only an
identity-free Game shape, the Game Review, selected Automatic Critical Moments,
the evaluation timeline, and provider provenance. It contains no Player ID,
session reference, Player name, event, site, source URL, or Player-authored
content.

The analysis cache is optional. A hit seeds a complete Player-owned Game Import
record. The import never depends on the cache after creation. Missing,
malformed, expired, or superseded analysis and all cache read or write failures
become cache misses. Each hit may advance `purgeAt`, but never past the
immutable `hardExpiresAt`.

Store every Player-owned product record under one hashed `users/{player}`
subtree. One stable Game Import per durability generation, Game, Review Side,
and resolved Elo is self-contained and holds the normalized Game,
Player-specific display metadata, frozen Game Review, selected Automatic
Critical Moments, evaluation timeline, and provider provenance. Repeating the
import returns the same Player-owned ID and record. A Review Session
incarnation derives from that import plus the semantic start operation ID.
Retrying the same operation resumes the same aggregate; a new operation
creates another independently resumable aggregate over the import.

Persist only completed Review Session state. Restore positions from FEN, ply,
and root-relative UCI paths. Do not persist board occupancy, facts derivable
from the import, active operations, delivery envelopes, cancellation outcomes,
retry history, presentation state, or duplicated parent identifiers. Evidence
may be inlined into its Review Moment only after a measured 400-ply,
maximum-moment record stays below 700 KiB with headroom and creation stays
within Firestore and repository mutation limits. Persist provider evidence only
for candidate plies. A Player-Selected Moment at another ply requests fresh
provider evidence. The Phase 2 upper-bound fixture measured a 533,679-byte
moment payload, leaving 183,121 bytes below the 700 KiB guard, and nine
creation writes; evidence inlining is therefore approved. The executable
fixture remains the guard for the production serializer.

A Review Session is unavailable after 72 hours without a successful
Player-initiated command or 14 days from creation. Business reads enforce both
limits. Root and child records carry bounded TTL cleanup times.

Replace per-model OAuth collections with `oauthRecords` and
`oauthRevocations`. The adapter payload is opaque. Model, adapter ID, expiry,
consumption, and the subject, grant, user-code, and device projections used by
real queries remain typed. Subject IDs are canonical Firebase UIDs only when a
record represents an authenticated Player. Staging and production issuers,
audiences, clients, grants, revocations, signing keys, and credentials remain
isolated.

Quality capture is production-only. The production Player account record owns
the "Help improve coaching" preference and acknowledged disclosure version.
Absent state means the disclosure has not been acknowledged and no capture may
occur. Turning capture off does not make product commands unavailable.

A qualifying business result and its Player-owned quality outbox record commit
in one `coach-app-production` transaction. An idempotent exporter writes an
identity-free capture to `coach-quality`, accepts an existing identical content
digest, and fails closed on a digest conflict. Export failure never fails the
business command. Pending and unadmitted captures are deleted on opt-out.
Admitted captures receive withdrawal tombstones.

Quality captures are immutable `gameAnalysis` or `coachingResponse` records.
They may contain canonical chess inputs, structured outputs, and versioned
reproducibility provenance. They contain no Player ID, session ID, names, URLs,
raw PGN, Player-authored free text, full transcripts, request IDs, timings, or
provider traces. Unadmitted captures expire after 12 months. Evaluation cases,
candidates, runs, judgments, and competitor ingestion remain deferred until an
implemented evaluation workflow consumes them.

Operational logs contain environment, trace ID, command, status, timing,
sizes, cache outcome, and component or provider versions. They contain no
chess content, Player ID, profile data, prompts, comments, or quality payloads.
Evaluation prompts, provider request and response traces, and evaluator
reasoning traces are transient and never durable evaluation data.

Production account deletion is an idempotent Coach Engine-owned saga. It
blocks new commands, writes short-lived deletion markers in both product
databases, withdraws production captures, recursively deletes both Player
subtrees, asks the Node OAuth adapter to revoke the Player's grants in both
OAuth databases, then revokes refresh tokens and deletes the shared Firebase
identity. Firebase identity deletion occurs last. Staging does not expose
self-service account deletion. Node writes a bounded subject revocation before
querying OAuth records, preventing an already-running consent from recreating a
grant after the deletion pass.

The separately reviewed cleanup on 2026-08-03 purged pre-release game and
review records and retired `(default)`. That reset initially removed the need
for legacy readers. On 2026-08-04, however, schema-v1 Game Imports created
after the reset survived the addition of required Learning Path identity.
Game Import schema v2 therefore introduces one narrow compatibility decoder:
it accepts v1, reconstructs `learningPathRef` only from already durable
semantic inputs, and requires the complete result to decode as v2. A
fail-closed, update-time-preconditioned operator migration rewrites every v1
Game Import. Unknown versions and other malformed records remain invalid. The
protected legacy `coach-oauth` database remains outside runtime allowlists
until its own OAuth cleanup is reviewed.

## Consequences

One Firebase account and Player ID work in both environments, including shared
email, password, provider linking, and eventual account deletion. Product data,
OAuth authority, and quality credentials remain environment-specific.

Database configuration becomes a startup invariant. Firestore declarations,
TTL policies, index definitions, Railway variables, service accounts, and IAM
conditions must change together.

The durable product model has fewer records and fewer duplicated fields.
Imports and sessions remain readable if the optional analysis cache is deleted
or corrupted.

The quality database cannot be written by staging and cannot identify the
Player whose product command produced a capture. The production outbox keeps
the revocable Player association on the product side of the database boundary.

The pre-release product reset allowed the original durability schema counters
to start at version 1. Game Import documents advance to schema v2 after the
post-reset Learning Path contract change. The temporary v1 reader is scoped to
that one migration and does not authorize a general legacy-decoding policy.
