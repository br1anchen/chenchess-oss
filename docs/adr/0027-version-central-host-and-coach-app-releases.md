# ADR 0027: Version Central Host and Coach App releases as one tuple

## Status

Declined.

ADR 0052 replaces SemVer / GitHub Release promotion with the git SHA of a
protected `prod` bookmark. Host-specific Coach App publication (ChatGPT
plugin review versus Claude's live connector contract) remains a later
concern and is not a git identity. The text below is the declined proposal.

## Context

ADR 0025 gives every production deployment one
`central-host-v<major>.<minor>.<patch>` identity. It does not distinguish that
deployment from the public Coach App contract consumed through ChatGPT and
Claude.

The repository currently repeats `0.1.0` in the MCP server implementation, the
embedded Coach App runtime, Rust and JavaScript package manifests. Those values
do not have one shared release meaning. MCP protocol revisions and versioned
UI resource URIs add two more independent compatibility domains.

The public hosts also have different change models:

- OpenAI's publication unit is now a plugin, not a legacy
  `ai-plugin.json` integration. OpenAI stores a reviewed snapshot of MCP tool,
  instruction, security, and linked UI metadata. Changing that contract
  requires a new scan, review, approval, and publication. A compatible
  server-only fix or same-URI UI content update may deploy without review.
- Anthropic publishes the remote MCP service as a directory connector and
  consumes its live contract. Tool changes appear on the next connection and
  need no connector resubmission. Listing changes are a separate reviewed
  operation.
- Neither host defines a product SemVer for a remote MCP service or MCP App.
  Host-reported `clientInfo.version` and `serverInfo.version` are observability
  metadata, not safe rollout, authorization, or compatibility gates.

The first-party constraints and citations are recorded in
[ChatGPT plugin and Claude connector release guidelines](../research/chatgpt-plugin-and-claude-connector-release-guidelines.md).

## Decision

### Release identity

Every production release is an immutable **Release Tuple**:

| Field           | Form                                    | Meaning                                                                                                                                |
| --------------- | --------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Service version | `central-host-v<major>.<minor>.<patch>` | The exact deployed Central Host source revision and its selected Railway release units.                                                |
| Client version  | `coach-app-v<major>.<minor>.<patch>`    | The public Coach App contract: reviewed MCP metadata and instructions plus the embedded Review Session and Move Sequence UI artifacts. |
| Source revision | 40-character Git commit                 | The source from which both parts were built.                                                                                           |
| Release note    | `docs/releases/<service-version>.md`    | The human-authored change and reviewer record for this tuple.                                                                          |

The Central Host version remains the primary Git tag and GitHub Release. There
is no second Coach App Git tag in the MVP because the client is served by that
Central Host revision rather than downloaded independently.

Every production source change increments the service version. The client
version increments only when the public Coach App contract or one of its UI
artifacts changes. Several consecutive service releases may therefore name the
same client version, but a client version may never be reused with a different
metadata fingerprint or UI artifact digest.

The initial public tuple is:

```text
service = central-host-v0.1.0
client  = coach-app-v0.1.0
```

The unprefixed client value is embedded in the MCP server and Coach App
implementation metadata for diagnosis. If a `.codex-plugin/plugin.json` is
packaged, its `version` must equal that same unprefixed client value. These
copies help identify evidence; clients and servers must not branch behavior or
authorization on them.

Private Cargo and workspace-package versions are build metadata. They are not
release identities and need not advance with the Release Tuple.

### Keep compatibility versions separate

The Release Tuple does not replace any wire or data-contract version:

- MCP protocol revisions remain date versions such as `2026-07-28`; ADR 0043
  retires negotiation with `2025-11-25`.
- MCP Apps extension capability negotiation remains independent.
- UI resource identities are content-addressed from the exact single-file HTML
  bytes. Any artifact change produces a new URI; these hashes are not release
  numbers.
- Published version-named resource URIs remain registered as compatibility
  aliases during the content-addressing migration; only new tool metadata
  advertises content-addressed URIs.
- Review Session checkpoint, generated DTO, selector, prompt, catalog, and
  other schema or policy versions keep their existing owners.
- ChatGPT and Claude implementation names and versions may be recorded in
  certification when visible, but never select code paths or gate a release.

### Version-bump and channel policy

SemVer communicates product intent. Patch releases preserve the published
contract, minor releases add backward-compatible capability, and major
releases may break it. While the product remains below `1.0.0`, any intentional
incompatibility must at least advance the minor component and be labelled
`breaking` in the release note.

The release preparer classifies the change from canonical fingerprints and
artifact digests rather than commit-message wording:

| Change class                                                                                                                    | Service version                      | Client version                      | OpenAI action                                           | Anthropic action                    |
| ------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------ | ----------------------------------- | ------------------------------------------------------- | ----------------------------------- |
| Server implementation, business result, result `_meta`, dependency, or infrastructure change that preserves the public contract | Bump                                 | Unchanged                           | None                                                    | Live on reconnect                   |
| Backward-compatible UI content with a new content-addressed resource URI                                                        | Bump                                 | Bump patch                          | Scan, review, approve, publish                          | Live on reconnect                   |
| Additive tool, schema, annotation, security, instruction, resource reference, or CSP change                                     | Bump                                 | Bump minor                          | Scan, review, approve, publish                          | Live on reconnect; no resubmission  |
| Breaking tool/schema removal, incompatible same-name behavior, MCP origin change, or removal of a published UI resource         | Disallowed as a one-step MVP release | Bump major, or minor before `1.0.0` | New reviewed version or plugin, depending on the change | Requires an explicit migration plan |
| Listing-only copy or asset change                                                                                               | No runtime release                   | No runtime bump                     | Reviewed portal listing update                          | Reviewed listing update             |

An urgent legal, privacy, support, or branding correction may update a channel
listing without claiming a runtime release. Ordinary listing changes travel
with the next Release Tuple so the public descriptions, tests, screenshots,
and product behavior are reviewed together.

The canonical OpenAI metadata fingerprint covers the fields its scanner stores:

- MCP server instructions;
- tool names, titles, descriptions, input and output schemas;
- tool security schemes, annotations, visibility, and `_meta`;
- linked UI resource URIs and metadata, including CSP.

The client artifact fingerprint records SHA-256 digests for the Review Session
and Move Sequence single-file HTML resources. A changed fingerprint with an
unchanged client version fails release preparation.

### Immutable release manifest

`release:central-host` is extended rather than replaced. It accepts the client
version and release-note path, verifies both against the exact staging
certification, and emits manifest schema version 3:

```json
{
  "schemaVersion": 3,
  "service": {
    "version": "central-host-v0.1.0",
    "previousVersion": null,
    "commitSha": "0123456789abcdef0123456789abcdef01234567",
    "releaseUnits": [
      "railway-central-host",
      "railway-coach-engine",
      "railway-maia"
    ]
  },
  "client": {
    "version": "coach-app-v0.1.0",
    "previousVersion": null,
    "metadataFingerprint": "sha256:<digest>",
    "resources": [
      {
        "uri": "ui://chenchess/review-session/sha256-<digest>.html",
        "sha256": "<digest>"
      },
      {
        "uri": "ui://chenchess/move-sequence/sha256-<digest>.html",
        "sha256": "<digest>"
      }
    ]
  },
  "channels": {
    "openai": { "action": "initial-submission" },
    "anthropic": { "action": "initial-submission" }
  },
  "releaseNote": {
    "path": "docs/releases/central-host-v0.1.0.md",
    "sha256": "<digest>"
  },
  "rollback": {
    "serviceVersion": null,
    "clientVersion": null
  }
}
```

The real manifest retains ADR 0025's source range, certification summary, and
selected gate plan. Channel actions describe the immutable plan; asynchronous
portal submission IDs and states remain in the GitHub Release evidence and do
not mutate the manifest.

The release note is checked into the candidate commit and follows
the release-note template (`docs/releases/template.md` — removed; that file
was deleted after ADR 0052 retired SemVer GitHub Releases). It contains:

- Player-facing changes;
- the service and client versions;
- compatibility, migration, cache, and known-issue statements;
- the exact public-contract and data-handling delta;
- validation and rollback evidence;
- OpenAI reviewer notes stating what the plugin does, initial versus update
  status, changes since the prior submitted version, and credential or fixture
  setup context; and
- Claude submission deltas for tools, resources, annotations, links, listing
  assets, screenshots, tested surfaces, and launch readiness.

Credentials, OAuth state, Player data, secret values, and private configuration
never enter the release note or manifest.

### MVP release pipeline

1. **Author the candidate.** Choose the service and client bumps, update the
   single client-version source, author the release note, and make compatible
   implementation changes. `checked-push` continues to select and run only the
   affected staging gates.
2. **Deploy and certify staging.** Railway autodeploys the exact commit. Review
   Session certification schema version 2 additionally records the proposed
   service and client versions, OpenAI metadata fingerprint, both UI resource
   digests, and the real ChatGPT and Claude journeys. Host implementation
   versions are optional observations.
3. **Prepare the immutable release.** Run:

   ```sh
   bun run release:central-host -- \
     --version central-host-v0.1.0 \
     --client-version coach-app-v0.1.0 \
     --from <previous-production-tag-or-sha> \
     --to <candidate-revision> \
     --certification <staging-certification.json> \
     --release-note docs/releases/central-host-v0.1.0.md
   ```

   The command verifies monotonic versions, source copies, note headings,
   fingerprints, digests, change classification, exact-revision
   certification, and selected gates before emitting the schema-3 manifest.

4. **Create the GitHub Release.** Create the service-version tag at the exact
   commit. Attach the generated manifest and the checked-in note. Keep the
   release as a draft until the exact commit is deployed and production smoke
   checks pass.
5. **Promote production.** Deploy only the manifest's selected Railway units by
   exact commit SHA, as required by ADR 0025. Verify health, OAuth, the private
   service routes, version evidence, metadata fingerprint, resource digests,
   and one low-risk real-host journey. Then publish the GitHub Release.
6. **Advance distribution channels.**
   - For an initial or metadata-changing OpenAI release, scan the production
     endpoint, verify the portal snapshot fingerprint, provide at least five
     positive and three negative reviewer cases, copy the reviewer section of
     the canonical note, submit, and manually publish only after approval.
   - For an initial Claude listing, exercise every tool through MCP Inspector
     and a Claude custom connector, supply the required test account,
     documentation, declarations, and three to five qualifying MCP App
     screenshots, then submit through the directory portal.
   - For a release whose OpenAI action is `none`, take no portal action. Claude
     consumes the compatible live change on reconnect. A Claude listing change
     is submitted separately from a live tool change.
7. **Close the launch record.** Record production deployment IDs, portal
   submission IDs, approvals, publication states, directory URLs, and the
   effective rollback floor in the GitHub Release evidence. The first MVP
   launch is complete only when both public channels are published; an
   asynchronous directory review does not make a healthy service deployment
   fail retroactively.

Portal interactions remain manual in the MVP. No workflow stores reviewer
credentials or attempts to automate approval.

### Compatibility and rollback

OpenAI and Claude cannot be switched atomically. A metadata-changing release
must therefore be additive:

- keep published tool names, accepted inputs, and UI resource URIs working;
- deploy the compatible server before scanning the new OpenAI snapshot;
- expect Claude to see the additive live contract before OpenAI publication;
  and
- retain old handlers and resources after the new OpenAI version is published.

After the initial MVP, a metadata-changing client release uses an
**expand-then-publish** pair. The expand service release accepts the future
tool calls and serves the future UI resources without removing the published
contract. A later Release Tuple advertises and publishes that client contract.
The expand release is then a valid service rollback target even after OpenAI
publishes the new snapshot.

Same-URI UI updates must remain compatible with the immediately previous
client for at least OpenAI's documented one-hour cache window. Certification
proves the previous and current client artifacts against the candidate service.

Before OpenAI publishes a new metadata snapshot, rollback may restore the prior
Release Tuple. After publication, rollback may not cross below the newest
service revision that accepts the published snapshot. The GitHub Release
records that effective rollback floor. A severe initial-launch defect has no
older public contract to restore; disable or delist the affected channel and
ship a forward fix rather than presenting an incompatible service as a
rollback.

The MCP origin and Claude directory slug are stable identities. Changing the
origin is a migration and new OpenAI plugin review; existing Claude connections
do not automatically move to a new endpoint.

## MVP exclusions

- No independent versions for the web adapter, Coach Engine, or Maia services.
- No host-specific Coach App bundles or tool surfaces.
- No rollout decisions based on ChatGPT or Claude client version strings.
- No breaking published-contract contraction.
- No automated marketplace submission, approval, publication, or delisting.
- No generated release prose; the operator owns the Player and reviewer
  summary while tooling validates its structure and bindings.
- No replacement of ADR 0025's scoped gates, exact-commit deployment, staging
  evidence, or explicit production promotion.

## Consequences

Operational releases and public-client releases can advance independently
without conflating package, protocol, schema, or host versions. Every
Player-visible artifact remains attributable to one commit and one Central
Host deployment.

The OpenAI/Claude asymmetry becomes an explicit release decision. Compatible
server fixes remain inexpensive; metadata changes pay for OpenAI review and a
cross-host compatibility window.

The first implementation adds a canonical client-version source, deployed
metadata and artifact fingerprinting, certification schema version 2, manifest
schema version 3, release-note validation, and tests for every change class.
It does not need marketplace API credentials.
