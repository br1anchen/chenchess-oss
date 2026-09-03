# ChenChess

ChenChess helps players import chess games and receive tailored coaching feedback.

**Language** and **Relationships** are the live model. Superseded ADRs are
history: they record what was decided then. ADR 0026 retired the typed intent
lifecycle. ADR 0042 retired Review Session Checkpoint. ADR 0023 replaced
Convex/Better Auth with Firebase and Coach OAuth. ADR 0043 supports only MCP
`2026-07-28`. ADR 0052 promotes production by fast-forwarding protected
`prod` to a `main` SHA; it replaces ADR 0025's GitHub Release path.
ADR 0053 hosts the web Review Session on the pinned Language Layer as
one **HostTurn**.

## Do not restore

These names are not current product terms. Keep them out of issues, types,
plans, and new docs. History lives in the ADRs that retired them.

- **Review Session Checkpoint**, **Review Session ID**, resume path, `/app/review-sessions/{sessionId}`
- **Entry Moment** (session start prepares the Automatic set; surfaces pick what to display)
- **Convex**, **Better Auth** (auth is Firebase plus Coach OAuth)
- **Saved Game** (durable Game state is the **Game Import Record**)
- **Intent Assessment**, **Intent Clarification**, **Intent Assessment Abstention**
- **Coach Intent Abstention**, **Coach Intent Unavailability**
- **Intent Selection Policy**, **Intent Selection Trace**, **Intent Projection**
- **Intent Calibration Set**, **Intent Hypothesis Precision**, **Intent Hypothesis Coverage**
- **Review Moment Intent State**, **Favorable Continuation**
- Sessionful MCP `2025-11-25`

## Language

**ChenChess**:
The product that turns imported chess games into tailored coaching feedback. Coach is the role, not the product name.
_Avoid_: Chen Chess Coach as the product name, Personal Chess Coach, Chess app, analysis app

**Coach Engine**:
The private Rust application service that authenticates Players, owns durable review storage and the transient coaching state above it, enforces domain authorization and compute admission, and routes every coaching operation through the Game Review Engine and its infrastructure adapters. It never acts as a Language Layer.
_Avoid_: Game Review Engine, Coach MCP Server, chess engine

**Coach Engine SDK**:
The shared TypeScript interface to Coach Engine commands and results. It supplies generated contracts, validation, a typed client, account and retention operations, auth-neutral credential injection, and common outcome handling without implementing Firebase Authentication, Coach OAuth, cookies, consent, or token storage.
_Avoid_: Coach Engine, UI library, Firebase adapter, OAuth server

**Game Review Engine**:
The deterministic authority that turns a Game into validated review facts and admits grounded coaching outputs across ChenChess delivery surfaces. Review Engine is the accepted short form.
_Avoid_: Rust, Rust backend, chess engine

**Coach Skill**:
A local coding-agent workflow that accepts a Game and coaching preferences, obtains chess facts from ChenChess, and produces a Game Review through the active agent.
_Avoid_: Web-app LLM provider, OpenRouter replacement server, chess engine

**Coach App**:
A consumer-chat delivery surface installed in ChatGPT or Claude that uses the host model as its Language Layer and pairs the native conversation with an inline chess workspace.
_Avoid_: Coach Skill, web application, LLM provider, standalone MCP server

**Coaching Board**:
The ChenChess web board a Player and a host agent share, where the host model is the Language Layer and reaches the board through tools the page registers with the browser. It carries no installation — no connector grant, no **Beta Coach App Connection**, no inline artifact. `/app/board` and the two board addresses are visitable without Firebase sign-in. Tools still do not register until **Beta Access** authorizes a **Player**.
_Avoid_: Coach App, WebMCP app, installed connection, Review Session door, in-place ConversationPanel toggle

**Coaching Board Snapshot**:
The whole account of what one game or opening **Coaching Board** shows, returned complete on every board-tool read: its **Review Moment** or **Opening Line** origin, the retained **Alternative Move Exploration** with each branch's move and evaluation, which branch is active, the path to the current Position, where the viewed ply sits on the Game's own line (the move that reached it, the move played next, the Review's evaluation), the opening study session when the line has one, and a monotonic revision. It is self-sufficient by construction, so no caller carries a cursor and no read returns a fragment. Lobby import and find do not return one.
_Avoid_: Interaction journal, FEN alone, board delta, exploration event log, since-cursor read

**Beta Coach App Connection**:
A controlled, disposable connection between a Coach App host and a non-production Central Host. It is never promoted in place; using production requires a fresh installation and authorization.
_Avoid_: Production installation, marketplace listing, permanent connection

**Beta Invitation**:
A one-time authorization for beta registration that is issued to one Invitation Email, remains valid until redeemed or revoked, and is permanently claimed by one Player ID when redeemed. Redemption requires both its code and an exact match to the Player's verified account email. Its stored code authenticator is a versioned keyed HMAC over the invitation ID, normalized Invitation Email, and random code; a separately encrypted copy exists only so a failed Invitation Delivery can retry the same code.
_Avoid_: Bearer invitation, phone invitation, reusable access code

**Beta Access Request**:
An authenticated request from a verified Email/Password or Google identity asking an Administrator to consider that identity's normalized verified email for Beta Access. The request endpoint derives the email from Firebase token claims rather than caller-submitted text. At most one request exists per normalized email: repeated submissions are idempotent and receive the same generic response as a new submission. Its email may be used only to administer beta access and essential beta notices; it is neither a marketing subscription, a Beta Invitation, nor a promise of access.
_Avoid_: Waitlist account, mailing-list subscription, Beta Access, approved invitation

**Beta Access**:
The authorization for an authenticated Player to use a non-production Central Host. It is granted to one Player ID by redeeming a Beta Invitation and is distinct from creating or authenticating a Firebase identity.
_Avoid_: Firebase account, production access, Coach OAuth grant

**Invitation Email**:
The normalized email address to which a Beta Invitation is issued and delivered. A Player may redeem the invitation only when Firebase verifies the same normalized account email.
_Avoid_: Contact-only email, phone number, Player ID

**Invitation Delivery**:
The attempt to send one Beta Invitation to its Invitation Email through Resend. It may be pending, sent, or failed; retrying delivery never creates another invitation. Its message contains both a copyable code and a one-click `/join` link whose secret is carried in the URL fragment rather than an HTTP query. The sender and reply-to addresses, and the mail routing behind them, are the operator's own configuration.
_Avoid_: Beta Invitation, Beta Access, approval status

**Administrator**:
A person authorized to operate the Beta Back Office and manage Beta Invitations and Beta Access. The administrative role is distinct from the Player role even when the same person holds both. It is represented by the Firebase custom claim `chenchessAdmin` and may be granted or revoked only by an out-of-band operator CLI that requires an explicit Firebase UID and matching verified email.
_Avoid_: Privileged Player, invited Player, email allowlist member

**Beta Back Office**:
The restricted Central Host surface where an Administrator lists and filters Beta Access Requests, grants a request by creating and sending its Beta Invitation, retries a failed Invitation Delivery with the same code, revokes an unredeemed invitation or active Beta Access, views delivery, redemption, and access status, performs a Digest Email Replay, and starts a Manual Digest Run for a Player with active Beta Access. The digest projection contains only its coverage date, publication time, game count, learning-path count, and email readiness. Revoking Beta Access preserves the redeemed invitation history and cannot be reversed by replaying its code. The surface cannot edit an Invitation Email, delete a Firebase identity, expose digest contents or game identities, restore revoked Beta Access, or grant administrative authority.
_Avoid_: Player account settings, Firebase console, coaching dashboard

**Digest Email Replay**:
An Administrator-only request to send the latest already-published Coaching Digest email again. It creates a new email-delivery record without changing the Coaching Digest, digest run, archive, or schedule. The request stays within its deployment environment, checks account-deletion and Beta Access state before reading coaching metadata, and uses the Player's current verified-email preferences and suppression state.
_Avoid_: Digest regeneration, scheduled retry, cross-environment send

**Manual Digest Run**:
An Administrator-only request to start the Player's already-due Daily Coaching window when no Coaching Digest has been published yet. It uses the normal previous-local-calendar-day selection, publication, and digest-email delivery pipeline. It cannot select a date, rerun a terminal window, bypass email readiness or the Run kill switch, or alter a published Coaching Digest.
_Avoid_: Digest Email Replay, arbitrary backfill, forced email

**Landing Page**:
The public, unauthenticated introduction to ChenChess at a Central Host's root. It explains the product and sends product and beta actions through the Sign-In Page without exposing coaching or account state.
_Avoid_: Web application, sign-in page, Beta Back Office

**Sign-In Page**:
The Central Host surface at `/login` that owns Firebase sign-in, signup, email verification, password reset, and authenticated provider linking. After identity verification it sends a Player with Beta Access to an allowlisted product destination and a Player without access to the Beta Admission Page.
_Avoid_: Beta Admission Page, Landing Page, Player dashboard

**Beta Admission Page**:
The Central Host surface at `/join` where a verified Player without Beta Access requests or redeems a Beta Invitation. It never owns sign-in or provider-linking UI; a signed-out or unverified visitor returns to the Sign-In Page.
_Avoid_: Sign-In Page, Landing Page, Player dashboard

**Player Dashboard**:
The Beta-authorized Central Host home at `/dashboard`. It links the Web coaching product and presents the private ChatGPT and Claude Beta Coach App Connection setup.
_Avoid_: Beta Back Office, Beta Admission Page, mutable coaching workspace

**Preview Catalog**:
The public, unauthenticated, and non-indexed collection of fixture-only UI studies served by a Central Host. It never initializes Player authentication, accesses product backends, or presents real Player data.
_Avoid_: Storybook deployment, staging console, authenticated product, production demo

**Coach App Artifact Set**:
The versioned, self-contained HTML resources and manifest built by the non-deployable Coach App workspace for both MCP resource reads and exact-artifact Preview Catalog rendering. Each manifest entry binds one `ui://` resource URI to its file, MIME type, digest, and preview fixture.
_Avoid_: Coach App preview application, source-level web import, preview asset route

**Service Operator**:
The accountable person identified by the public legal pages as operating this instance and controlling its personal data. Whoever runs a deployment is its Administrator, and names themselves on those pages.
_Avoid_: Administrator, Firebase project owner, Resend sender

**Supported Sign-In Method**:
A Firebase Authentication method deliberately offered by ChenChess across the web application and Coach OAuth. Beta supports Email/Password and Google; enabling another Firebase provider alone does not make it supported. When two supported providers present the same email, ChenChess requires authentication with the existing provider before Firebase links the second credential to the same Player ID.
_Avoid_: Every enabled Firebase provider, host authentication, Auth Token profile

**Coach MCP Server**:
The remote MCP server that supplies deterministic coaching operations and interactive chess UI resources to a Coach App without generating coaching prose.
_Avoid_: Coach App, Coach Skill, LLM Explainer, OpenRouter replacement server

**Review Facts Tool**:
A local agent-callable tool that runs the deterministic chess pipeline through Rule Extraction and returns structured coaching facts without writing the Game Review.
_Avoid_: Coach Skill, LLM Explainer, prose generator

**Draft Game Review**:
A structured Game Review written by the active coding agent from Rule Extraction facts but not yet approved for presentation to the Player.
_Avoid_: Final response, unstructured coaching prose

**Review Validator**:
A deterministic atomic admission check that a Draft Game Review contains the required content, matches every Critical Moment in its Rule Extraction facts in Game order, preserves each completed typed intent sentence exactly once, and includes the validated kind-aware literals required by the Grounding Gate. No part of the Draft Game Review is presented until every moment passes bounded repair or Safe Review Moment Rendering.
_Avoid_: Chess fact generator, semantic judge, LLM reviewer

**Grounding Ledger**:
An internal, kind-aware record of the factual claims one Draft Game Review or Review Moment Comment asserts, derived from the **Slot Markers** its prose used and checked against the claims the active Review Moment Comment Facts variant supports. Every required claim must appear and no claim outside that variant may; two admissible comments about one moment can therefore differ in their optional claims. Intent claims remain separate. It is not part of the Player-facing Game Review.
_Avoid_: Player citation, reference answer, chain of thought, restatement of the facts

**Slot Marker**:
A typed placeholder — `{betterMove}`, `{bestEval}`, `{playedPopularity}` — that a Language Layer writes in place of a fact it may not state itself, substituted with its canonical rendering by the runtime after the **Grounding Gate** passes. Prose is otherwise free. Markers are how the guarantee moves from _the required fact is present_ to _no wrong fact is expressible_: no evaluation, percentage, or probability can appear in model output at all, so every figure a Player reads was rendered by Chen Chess Coach. The marker vocabulary and its renderings are part of the prompt digest, so changing a rendering mints a new **Explainer Candidate**.
_Avoid_: Template slot the model fills, mail merge, post-hoc string replacement of model-written figures

**Grounding Gate**:
A deterministic check, in order: parse **Slot Markers** and reject an unknown or repeated one; require every claim the facts variant demands; reject any evaluation, percentage, or probability the model wrote itself; check every chess literal against the **Chess Literal Projection**; substitute; then apply the post-substitution checks — one paragraph, no internal vocabulary, exact Learning Track and Learning Resource literals and URLs, and Review Moment intent prose consistent with its typed state and uncertainty marker. A surviving brace after substitution means substitution failed and the comment must not ship. The web **Review Moment Comment** and **HostTurn** pass it; a Coach App **Coach Turn** assessment passes it. It does not decide whether chess evidence semantically supports prose, and it cannot catch an invented _claim_ around correctly rendered facts.
_Avoid_: LLM Judge, chess expert, semantic validator, positional skeleton, required-phrase check

**Chess Literal Projection**:
The deliberate list of moves and squares a Language Layer may name for one Review Moment, **HostTurn**, or **Coach Turn**: SAN for the played, better, mechanism, and engine-line moves, the squares those moves land on, and the squares the moment's effects and the position itself carry. It replaces an allowlist derived incidentally by walking serialized facts, which yielded `Nxd4` but never `d4` and never any SAN for a principal variation stored as UCI. It is a prompt input, so its shape joins the prompt digest; it carries chess facts only and no Player data.
_Avoid_: Serialized-facts token dump, free-text chess vocabulary, UCI

**Local Coach Execution**:
A Coach Skill mode in which PGN processing, Engine Analysis, Human Move Model inference, and Rule Extraction run on the Player's machine without a separate LLM provider key. The active coding agent may still send the resulting facts to its configured hosted model.
_Avoid_: Fully offline review, local-model inference

**Local Pipeline Runtime**:
The installed Stockfish and Maia components owned by ChenChess for Local Coach Execution.
_Avoid_: Player-managed engine setup, external LLM runtime

**Player**:
The person using ChenChess to review their own chess games.
_Avoid_: User, account, customer

**Amateur Player**:
A Player who needs coaching feedback framed around learnable patterns rather than expert engine notation.
_Avoid_: Beginner only, casual user

**Player ID**:
The canonical identifier for a Player, taken from the Auth Token `sub` claim.
_Avoid_: Email, display name, host account ID

**Game**:
A chess game imported from PGN-compatible sources such as Chess.com or Lichess.
_Avoid_: Match, PNG, replay

**Lichess Game URL Import**:
A Game import initiated from one public Lichess game URL and resolving exactly one completed standard-chess Game for review. A side-qualified URL preselects its Review Side without proving Player identity.
_Avoid_: Lichess account import, account synchronization, game-library sync

**Chess.com Game URL Import**:
A Game import initiated from one public Chess.com shared Game URL in the exact `https://www.chess.com/game/computer/<numeric-id>`, `https://www.chess.com/game/daily/<numeric-id>`, or `https://www.chess.com/game/live/<numeric-id>` form and resolving exactly one completed standard-chess Game for review. Computer URLs must resolve to exactly one computer Player; live and daily URLs must resolve to two non-computer Players, and a daily URL must resolve to a Game carrying a days-per-turn clock. The URL never selects a Review Side.
_Avoid_: Chess.com account import, bot catalog import

**Daily Coaching**:
A Player-level enabled or disabled coaching lifecycle that uses every Playing Profile Connection to produce Coaching Digests. It is enabled automatically only when the connection count changes from zero to one, cannot remain enabled without a connection, has no provider-specific enabled state, and acknowledged disablement preserves connections and archived digests while fencing every unpublished digest.
_Avoid_: Provider synchronization, per-profile coaching, scheduled notification

**Playing Profile Connection**:
A durable Player-owned association with one Playing Profile Identity and its canonical public profile URL. A Player may have at most one per provider; resubmitting the same identity is idempotent, while a different identity at that provider is a replacement.
_Avoid_: Profile Game Feed, account link, verified profile, provider authorization

**Playing Profile Identity**:
The stable identity extracted from a public playing profile URL as its provider plus case-insensitive username. A trailing slash and Lichess `/all` are aliases; the canonical URL contains neither.
_Avoid_: Submitted URL string, provider credential, verified ownership

**Playing Profile Replacement**:
Changing one provider's Playing Profile Connection to a different Playing Profile Identity. It immediately excludes the old identity from future selection, preserves existing Coaching Digests, and starts a new Initial Backfill for the replacement.
_Avoid_: URL alias, reconnection, editing a username

**Playing Profile Reconnection**:
Adding a Playing Profile Identity after its prior connection was removed. It creates a new connection and Initial Backfill, while Games already represented in retained Coaching Digests remain ineligible for duplicate coaching.
_Avoid_: Resuming a removed connection, re-enabling Daily Coaching, URL alias

**Connecting**:
The initial Player-visible state of a Playing Profile Connection while asynchronous provider validation is unresolved.
_Avoid_: Preparing First Digest, synchronously validating, provider authorization

**Connected**:
The state of a Playing Profile Connection after its Profile Game Feed has successfully confirmed the public profile. It is independent of Daily Coaching review and digest progress.
_Avoid_: Daily Coaching enabled, backfill complete, verified ownership

**Initial Backfill**:
The durable one-time Daily Coaching preparation that considers exactly the latest five eligible Games from a newly connected or replacement Playing Profile Identity. An interrupted preparation resumes from its recorded progress rather than restarting.
_Avoid_: Profile validation, daily run, profile synchronization

**Preparing First Digest**:
The Player-visible Daily Coaching state while an Initial Backfill is unresolved, after its Playing Profile Connection is already Connected. It promises durable background work without exposing per-Game progress, reviewed counts, or skipped counts.
_Avoid_: Connecting, provider validation, scheduled daily run

**Daily Coaching Timezone**:
The effective IANA timezone that defines a Player's Daily Coaching calendar-day boundary, labelled as either Detected or Default. The latest valid authenticated client observation applies to the next unopened daily window; otherwise the explicitly configured backend timezone remains effective.
_Avoid_: Schedule control, browser locale, UTC offset

**Daily Game Selection**:
The deterministic selection of at most ten eligible Games from one Daily Coaching calendar-day window. It ranks classical, correspondence, rapid, blitz, bullet, then ultrabullet, followed by longer expected clocks, more played plies, and newer completion, without preferring a provider or Game outcome.
_Avoid_: Result quota, provider balance, carry-over queue

**Profile Game Feed**:
A bounded, newest-first resolution of completed standard-chess Games from one exact public Chess.com member URL or Lichess profile URL, including its `/all` game-history form. It infers the Review Side by matching the profile handle to each Game, then emits independent ordinary Game Import requests; it stores no provider credentials, follows no provider-returned URL, and creates no account link, background synchronization, or game library.
_Avoid_: Account import, profile import, account synchronization, Game batch

**Game Import ID**:
An opaque, durable Player-scoped handle returned by Game Import. For one Player, durability generation, canonical Game, Review Side, and resolved Elo, repeated imports return the same ID and frozen Game Review. It is the canonical cross-surface address of that frozen review until the Player deletes their account or the product explicitly deletes the import under its data-retention policy. Follow-up operations use it to address the server-owned imported Game, Review Side, and import provenance without carrying or signing that state through the client.
_Avoid_: Game Import Snapshot, Quality Capture, Saved Game, client-carried Game payload

**Imported Game**:
The immutable normalized import data embedded directly in a Game Import Record: canonical Game, Review Side, resolved Elo Profile, and import provenance. It has no generic `content` wrapper and is returned only by operations that need full interactive Game state. Read-only frozen-review retrieval returns the narrower Game Review.
_Avoid_: Game Import Snapshot, snapshot wrapper, Game Review, raw PGN

**Game Import Record**:
A durable, Player-owned Coach Engine data object created by the first successful Game Import for one Game Import identity and reused by later matching imports. Its direct fields are identity, owner, creation time, Imported Game, frozen Game Review, Player-selected moments, and optional engine provenance. It has no time-based expiry, is self-contained after creation, and never depends on the optional Game Analysis cache. It contains no original pasted PGN text and is deleted with the Player-owned product subtree during account deletion or another explicit product-data deletion workflow.
_Avoid_: Saved Game, session checkpoint, raw PGN archive

**Game Analysis**:
An optional, environment-local, identity-free durable cache entry for one schema version, analysis generation, canonical Game digest, Review Side, and resolved Elo. It may seed a self-contained Game Import Record but never backs one. It contains no Player ID, conversation reference, Player name, event, site, source URL, or Player-authored content. Cache failure is always a recompute.
_Avoid_: Game Findings, shared Game Import, Saved Game, durable dependency

**Game Review**:
The generated coaching output for one imported Game.
_Avoid_: Analysis, report, generic review

**Coaching Digest**:
A durable Player-visible coaching artifact that preserves every grounded learning finding from one bounded Daily Coaching run and highlights no more than two cross-Game learning priorities.
_Avoid_: Email, notification, Game Review, ungrounded summary

**Quality Capture**:
An immutable, identity-free evaluation record in the separately credentialed `coach-quality` database, written from either staging or production. It contains reproducible Game Analysis, a generated Coaching Response with structured chess inputs, or a hosted Language Layer generation of fingerprint plus call-shape facts. It excludes Player ID, Review Session ID, Game Import ID, names, URLs, raw PGN, Player-authored free text, full transcripts, request IDs, wall-clock timings, latency, and raw provider payloads.
_Avoid_: Review Snapshot, Intent Response Record, product state, anonymous retention system

**Evaluation Fingerprint**:
The content-addressed identity of the configuration that produced an evaluation record: a digest over a canonical, ordered set of declared axes, resolvable to one immutable axis record in `coach-quality`. Quality Captures, Review Feedback Reports, and operational metric rows carry the digest so they can be joined and cohorted.
_Avoid_: Model name, deployment ID, Explainer Candidate, request ID, Game identity

**Evaluation Contract Version**:
The integer naming which set of Evaluation Fingerprint axes is in force. It is itself an axis, so adding, removing, renaming, or recanonicalizing an axis produces new digests while historical fingerprints keep theirs.
_Avoid_: Schema version, API version, pipeline revision, code revision

**Capture Origin**:
The declared source of the traffic behind a Quality Capture, either genuine `beta-player` coaching or a `synthetic` evaluation run. It is an Evaluation Fingerprint axis, so synthetic sweeps can never be pooled with real coaching.
_Avoid_: Environment, Capture Trigger, test flag, Delivery Surface

**Capture Trigger**:
Why one Quality Capture exists: the Quality Capture Preference permitting it at authoring time, or a Player's feedback submission inducing it. It is a per-record field rather than an Evaluation Fingerprint axis, and feedback-induced captures are selection-biased toward complaints.
_Avoid_: Capture Origin, consent state, feedback reason code

**Capture Outcome**:
What became of the generation a Quality Capture records: published, or rejected as grounding-rejected, schema-invalid, pin-mismatch, timed out, or budget-refused. Quality rates are only meaningful when read per outcome.
_Avoid_: Error code, Grounding Gate verdict alone, HTTP status

**Language Layer Attestation**:
Whether the model behind an authored output is one Chen Chess Coach pinned and can verify (`attested`) or a delivery surface's own host model that cannot be pinned or replayed (`unattested`). It is an Evaluation Fingerprint axis; unattested records serve only as a labelled baseline cohort and can never be an Explainer Candidate.
_Avoid_: Trusted output, verified model, Pin Verification, provider route

**Quality Capture Retention Window**:
The 12-month period from creation during which an unadmitted Quality Capture may be retained. Expiry is fixed at creation and is not extended by later Player activity.
_Avoid_: Account inactivity window, Saved Game retention, indefinite evaluation storage

**Quality Capture Preference**:
The Player account setting labelled "Help improve coaching" that controls Quality Capture in both staging and production. No capture occurs until the disclosure version has been acknowledged. The setting remains available in account settings and the Coach Engine enforces it before writing the Quality Outbox. It does not govern a feedback-induced capture, which carries its own submit-time disclosure.
_Avoid_: Cookie consent, local UI flag, Saved Game setting

**Language Layer Operational Record**:
The Player-associated product-database record of one hosted Language Layer call, holding request ID, latency, cost, token usage, budget decision, error class, the Evaluation Fingerprint, and the honoured provider cooldown when a 429 opened one or admission was denied for that cooldown. It is operational data under ordinary retention, is never exported to `coach-quality`, and is authoritative for money.
_Avoid_: Quality Capture, provider trace, audit log, evaluation data

**Quality Outbox**:
The Player-owned product-database record written atomically with a qualifying business result in staging or production and exported idempotently to `coach-quality`. It keeps the revocable Player association inside the product database. Quality Capture export failure never fails the business command.
_Avoid_: Artifact Owner Index, quality database owner mapping, cross-database transaction

**Review Session**:
The transient coaching interaction one Player holds over one **Game Import**: the conversation in which they examine Review Moment Comments, ask free-text questions, and explore Alternative Moves. It is process-local working state — engine leases, prefetched analysis, and at most one in-flight **HostTurn** or **Coach Turn** — keyed by Player and Game Import and by nothing else. It has no identifier of its own, nothing durable behind it, and therefore nothing to resume or expire out from under a Player: losing one costs only warm memory, because the review is addressable, its analysis is cached, and its comments are in the **Review Annotation Store**. It is never a handle a caller carries, an address a surface links to, or a lifetime any Player-visible guarantee depends on.
_Avoid_: Review Session ID, resumable session, Game Review, analysis session, durable session

**Review Session Residency**:
How long an idle **Review Session** keeps its process-local memory: 72 hours from its last command and 336 hours absolute. It exists so a forgotten actor eventually releases its engine leases, not so any Player-visible state expires. Reaching it costs the next command a rebuild from durable state the Player already owns, and never costs the Player a review, a comment, or an address.
_Avoid_: Review Session Checkpoint, session expiry, retention window, Player-visible timeout

**Review Analysis Cache**:
The shared, identity-free home of prepared Review Moment analysis, addressed by the review key the Game Import ID already carries. One entry holds a Review Moment's selection, whether preparation completed, its local decision reference, and its allowlisted provider evidence — all derived from the Game and the engine, so nothing in it identifies a Player or a conversation, and nothing in it records what any Player did. Committed **Alternative Moves** are excluded for exactly that reason. Two Players who name the same Game, side, and Elo address one entry, and the Player who returns a week later reads the analysis their earlier visit wrote. The review key hashes the durability schema version and the analysis generation, so bumping either misses rather than serving stale analysis. It is a cache and never a dependency: a missing entry means the Review Moment is prepared again. A **Review Session** seeds its analysis under a create precondition, so it never overwrites an entry that already exists: first writer wins, and only preparing a Review Moment upgrades one. Its retention is its own eviction policy, decided by the cache and by no conversation.
_Avoid_: Review Session Checkpoint, Game Analysis, review history, session cache

**Review Evidence Packet**:
The append-only collection of normalized Position, Engine Analysis, and Human Move Model evidence accumulated while one Player studies one Game Import. Player-facing comments cite its stable evidence identifiers; raw provider payloads and Intent Selection Traces are excluded. It lives with the **Review Session** that accumulated it and is not durable.
_Avoid_: Intent Evidence Packet, Intent Selection Trace, parallel evidence packet, raw provider response, durable evidence store

**Position Snapshot**:
An immutable, content-identified representation of one standard-chess Position, including its board occupancy, rule state, and history-derived draw state. Its ply, preceding move, and Game or exploration origin are separate context because the same Position may occur more than once.
_Avoid_: FEN alone, prose board reconstruction, Critical Moment, branch node

**Review Moment**:
A Game position or move opened for on-demand coaching in a Review Session. It may be a Critical Moment or a neutral Player-Selected Moment; how it was selected does not determine its coaching kind.
_Avoid_: Critical Moment, review entry, selection provenance

**Critical Moment**:
A Review Moment supported by deterministic coaching facts and classified as exactly one of Positive Highlight or Improvement Opportunity. Its kind is independent of its tactical or positional category and of Player-selection provenance.
_Avoid_: Interesting move, key point

**Critical Moment Classification**:
The deterministic, Game Review-scoped classification of one Review Moment as a Positive Highlight, Improvement Opportunity, or neutral. Neutral is an internal no-Critical-Moment result for a Player-Selected Moment, not a third coaching kind or grade, and is never selected automatically. Classification is bound to the review's resolved Elo Profile, evidence, and selector behavior rather than treated as timeless truth about a Game move. A Positive Highlight and Improvement Opportunity result for the same reviewed move is contradictory evidence and fails classification rather than being resolved by precedence. Canonical facts carry the typed classification evidence rather than a parallel free-form selection reason.
_Avoid_: Selector ranking, move annotation, coaching prose

**Critical Moment Selection Provenance**:
The required, immutable record of how a Critical Moment entered the review set: Automatic with Selector Policy and trace provenance, or Player-Selected through direct nomination. Opening or navigating to an Automatic Critical Moment does not change its provenance.
_Avoid_: Critical Moment kind, navigation state, review priority

**Selector Trace**:
The review-level deterministic evidence for Automatic Critical Moment selection. It records the candidate set, episode collapse, target, reservation, utility, diversity, and final Game-order result. Each Automatic Critical Moment references it through Selection Provenance. It is preserved in Review Feedback Reports and evaluation artifacts, but is not Player-facing delivery-surface content.
_Avoid_: Intent Selection Trace, Player-facing ranking explanation, per-moment selection reason

**Mechanically Forced Move**:
A move played from a Position with exactly one legal move. It cannot qualify as a Positive Highlight merely because it completes an achievement or matches objective best play.
_Avoid_: Only good move, forced line, Coaching Episode

**Coaching Episode**:
A group of related decisions and forced continuations that explain one achievement or correction. Episode collapse retains the earliest meaningful decision, or a final verified payoff only when that move independently passes a Critical Moment classifier; an episode with no independently valid representative is suppressed.
_Avoid_: Mechanically Forced Move, arbitrary ply window, every move in a forcing line

**Positive Highlight**:
A Critical Moment selected because the Player found a notably strong, instructive, or praiseworthy move. It carries exactly one Positive Highlight Grade; qualification does not depend on whether current teaching material covers its achievement.
_Avoid_: Separate review section, generic compliment, best move only

**Positive Highlight Grade**:
The required Good (`!`) or Great (`!!`) classification carried only by a Positive Highlight and deterministically derived from its Positive Highlight Qualification. Great requires both objective excellence and strong Elo-relative achievement; every other qualifying combination is Good. A grade that disagrees with its qualification is invalid. It is not an Improvement Opportunity severity or a generic move annotation.
_Avoid_: Critical Moment grade, move quality, Improvement Opportunity grade

**Positive Highlight Qualification**:
The deterministic, typed record of every satisfied objective-excellence and Elo-relative-achievement reason supporting one Positive Highlight and its grade, plus non-empty references to the existing Played-Move Effect, Tactical Mechanism payoff, or terminal checkmate facts that establish its concrete achievement. Raw Stockfish and Maia values remain evidence rather than prose qualification reasons; achievement references cannot dangle.
_Avoid_: Selection reason string, praise rationale, grade

**Improvement Opportunity**:
A Critical Moment selected because the Player's move offers a concrete, instructive opportunity to improve future play. It is intentionally ungraded; objective comparison, Residual Outcome, and selector evidence express its importance without a second severity taxonomy.
_Avoid_: Mistake type, negative highlight, blunder only

**Improvement Correction**:
The required deterministic comparison supporting an Improvement Opportunity: a legal better move distinct from the played move and evidence of the improved analyzed outcome or avoided terminal outcome. A validated first refutation or Tactical Mechanism is included only when its corresponding line or payoff exists.
_Avoid_: Invented refutation, mandatory tactic, generic better-move advice

**Review Moment Comment**:
The Player-facing coaching explanation for one Review Moment. Classification-specific facts and at most one explicitly uncertain **Coach Intent Hypothesis** form one coherent paragraph; the Language Layer does not return separately assembled sentence slots.
_Avoid_: Critical Moment Comment for a neutral moment, engine comment, intent assumption

**Review Moment Comment Facts**:
The canonical tagged facts supplied to Review Moment Comment authoring, including its Review Moment Learning Material. A Critical variant contains exactly one Positive Highlight or Improvement Opportunity fact bundle; a Neutral variant contains only its typed neutral reasons and verified observations, while shared intent, grounding, failure, and rendering behavior lives outside the classification-specific variants.
_Avoid_: Flat optional-field bag, prose instructions as validation, third Critical Moment kind

**Review Moment Authoring Context**:
The immutable, moment-scoped input at a Language Layer seam: Review Moment Comment Facts, optional **Intent Enrichment** plus classification-aware instructions, the required Grounding Ledger, and the Player's per-write Idempotency Key. Transport shape is surface-specific; it is not a universal product response wrapper.
_Avoid_: Review Moment Comment, Draft Game Review, Coach Turn Context, Review Moment Intent State

**Critical Moment Comment**:
The Review Moment Comment for a Critical Moment, classified as either a Positive Highlight or Improvement Opportunity.
_Avoid_: Neutral Review Moment Comment, Critical Moment analysis

**Neutral Review Moment Comment**:
The brief Review Moment Comment for a neutral Player-Selected Moment. It explains only validated soundness, routine-play, or low-consequence facts and intent without manufacturing praise, correction, a reusable lesson, or Critical Moment status.
_Avoid_: Critical Moment Comment, generic move commentary, unclassified comment

**Neutral Review Reason**:
A closed reason explaining why a valid Player-Selected Moment is not a Critical Moment: mechanically forced, sound without a concrete achievement, below the Improvement Opportunity threshold, or a non-instructional terminal outcome. Every applicable reason is retained canonically, while Player-facing prose uses only the most informative concise explanation.
_Avoid_: Classification error, free-form selection reason, third Critical Moment kind

**Intent Enrichment**:
Ephemeral, request-scoped evidence built lazily during the first authoring attempt of an unpublished Review-Side Positive Highlight or Improvement Opportunity: one four-ply **Projected Plan** SAN line and one independent **Objective Counterplay** SAN line, plus static classification-aware instructions. It is not durable. A published comment is the only canonical output that may contain a **Coach Intent Hypothesis**. If enrichment is unavailable, authoring continues from the played move and grounded facts. Neutral and outside-Review-Side moments receive none.
_Avoid_: Review Moment Intent State, Intent Selection Trace, Intent Projection, persisted intent record

**Review Moment Comment Publication**:
The central server-side admission boundary for a host-authored Review Moment Comment. A Coach App submits its draft text and Grounding Ledger with the Game Import, Review Moment, and its own Idempotency Key; the server resolves the authoritative facts and intent, applies the Grounding Gate, rejects invalid drafts, and internally produces the canonical comment plus authoring provenance. An admitted comment is written to the **Review Annotation Store** before the Review Session records it, so a later persistence failure costs the conversation its in-memory record rather than the Player their comment. Replaying one Idempotency Key returns that write's original canonical comment, in this conversation or in any other; a key the Review Moment has not seen is a new logical write. Which comment is active is the **Review Annotation Store**'s answer alone — never the order a conversation happened to record its own writes in — so two conversations on one review never disagree about what the Player sees. Nothing goes stale, because the reviewed Review Moment is immutable. The product response returns only the canonical comment. Production may include the admitted comment, grounding facts, and allowlisted provenance in an identity-free Quality Capture when the Player's preference permits it. Publication validation is an operation within the transient Review Session; durability is not.
_Avoid_: Transport-session validation, client-only grounding, session-scoped comment

**Idempotency Key**:
The caller-generated identifier naming one logical write against a Review Moment, deduplicated on Player, Game Import, Review Moment, and key. Replaying it returns that write's original result instead of an error, so a double-tap or a retried request costs one comment; a key the Review Moment has not seen is a separate logical write. The Coach Engine never issues one and never rejects one as stale — the reviewed Review Moment is immutable, so there is nothing to be stale against.
_Avoid_: Publication fence, server-issued nonce, session revision, state token

**Review Annotation Store**:
The durable, append-only home of published Review Moment Comments, keyed by Game Import, Review Moment, and Player. A canonical comment has already passed the Grounding Gate, so it is a property of the review rather than of the chat that wrote it: it outlives the conversation that wrote it, is the active comment when the same Player reviews the same Game in another conversation or on the web, and is erased only with the Player subtree during account deletion or another explicit product-data deletion workflow. It records no conversation, no session, no revision, and no purge time, and no eviction reaches it. Nothing is replaced or removed: a replayed Idempotency Key reads back its own annotation, a distinct key appends beside it, and the newest by publication time is active. It is the sole authority on which comment a Review Moment shows; a Review Session's own record of its writes answers replay and Grounding Gate retry only. A Review Session reads a whole review's annotations once when it opens and answers every Review Moment from that snapshot; a comment published elsewhere meanwhile reaches the Player through the host model, not through mid-conversation reconciliation.
_Avoid_: Comment history, per-conversation comment store, mutable annotation, shared draft store

**Review Share Grant**:
A Player's deliberate, expiring, withdrawable grant of read-only access to one of their Game Reviews, opened at one address inside it. Sharing is its own action, taken in person: minting, listing, and withdrawing require the Firebase web identity and refuse a Coach OAuth bearer, so a host model cannot mint a capability on the Player's behalf. Opening a review mints nothing, and every web address below a Game Review remains an identifier that resolves only behind sign-in. A grant is minted only over a Game Import whose owner segment matches the caller and only over a Critical Moment that Game Review actually contains, so a link never looks live and fails on the recipient's screen. It names a Review Moment or one of its canonical continuations and hands the Player one token they can pass on. It is stored inside the Player subtree under the digest of its own token, so durability holds no bearer material, the public share id cannot reconstruct the link, and account deletion withdraws every outstanding grant structurally; a deletion already under way stops resolution before the subtree is removed. A Player reads their own outstanding grants back by identity, which is what makes withdrawal outlive the page that minted a link — the token is never among the answers. It expires 24 hours after it is minted, and both expiry and withdrawal are decided by the Coach Engine on every resolve and every read rather than once when the page loads. Resolution is metered per grant, because it is the only unauthenticated path that spends engine work. A recipient names a resource and never a command; the read runs as the Player who shared it, over the shared Game Import only, and writes nothing — no Learning Path vote, no comment, no Coach Turn. Read scope is the whole review snapshot, because a Review Moment renders from it.
_Avoid_: Public review link, bearer review address, permanent share, per-conversation share

**Coach Turn Scope**:
The at-most-one active **Coach Turn** one Player holds on one Game Import. It is keyed by Player and Game Import and by nothing else, so it spans every Review Moment and every Review Session over that imported Game: a second turn on the same Game is refused while the first is in flight, and turns on two different Games run independently. It covers the in-flight window only: a turn that has returned a prepared **Alternative Move Assessment** has released the scope, and publishing that preparation afterwards is not gated by it. Steering releases and retakes the scope, so two conversations reviewing one Game can refuse each other's steering replacement — an accepted testing-only edge case, not a guarantee.
_Avoid_: Session-wide turn lock, per-conversation Coach Turn, coach mutex

**Safe Review Moment Rendering**:
The deterministic fallback that renders the complete validated Review Moment Comment Facts variant through a fixed kind-specific template after one identical-contract LLM regeneration also fails the Grounding Gate. It never degrades valid facts to a generic played-move comment, and invalid classification facts fail instead of rendering.
_Avoid_: Fallback Game Review, second hypothesis, unvalidated prose, Review Moment Intent State

**Teaching Theme**:
A closed, versioned identifier for a learnable pattern established by Rule Extraction evidence at a Critical Moment.
_Avoid_: Critical Moment category, LLM topic, lesson tag

**Played-Move Effect**:
A board-derived, observable consequence of the played move at a Critical Moment, drawn from a closed allowlist (captured piece, advanced passed pawn, attacked piece, allowed queen exchange). Describes what the move did, never why the Player chose it.
_Avoid_: Move intent, move purpose, motif guess

**Played-Move Outcome Evidence**:
The required account of the position after a reviewed move, in exactly one of two forms. An analyzed outcome supplies a post-move Engine Evaluation and Residual Outcome; a terminal outcome supplies a verified board-terminal result and supplies neither. Resignation and timeout are Game terminations, not board-terminal outcomes.
_Avoid_: Optional post-move evaluation, terminal flag, Game result

**Residual Outcome**:
The deterministic classification of the mover's standing before and after the played move, with one derived label such as missed forced mate, advantage kept, or advantage lost. The only license for prose claims like "still winning".
_Avoid_: Centipawn-threshold prose, severity adjective policy

**Tactical Mechanism**:
The machine-truncated prefix of the returned best line ending at a deterministic payoff (mate, promotion, material win, queen exchange), with the mover's first forcing move marked. The shortest sufficient evidence for explaining why the better move works.
_Avoid_: Full principal variation, agent-shortened line, invented tactic

**Opening Principle**:
A closed, versioned identifier for a general opening guideline established by Rule Extraction evidence at a Critical Moment.
_Avoid_: Opening Identification, opening-name inference, theory departure

**Move Intent**:
Player-provided conversational context about a move from a specific pre-move Position. It is not durable product state, not a retained record, and not a confirmation lifecycle.
_Avoid_: User plan, original plan, Intent Assessment, durable intent record

**Coach Intent Hypothesis**:
One explicitly uncertain sentence inside a Review-Side Critical Moment Comment, grounded in **Intent Enrichment** when that evidence exists. It never establishes classification, praise, correction, grade, or objective chess facts. The product has no confirm, correct, skip, or assessment transition.
_Avoid_: Original plan, predicted intent, inferred fact, Coach Intent Abstention

**Projected Plan**:
The selected four-ply SAN continuation inside **Intent Enrichment**. The played move is held fixed; the Language Layer receives only this SAN line.
_Avoid_: Favorable Continuation, Intent Projection, principal variation

**Objective Counterplay**:
The independent Stockfish strongest-reply SAN line inside **Intent Enrichment**. It is not the Projected Plan and is not a proof of the Player's purpose.
_Avoid_: Objective Refutation, expected reply, human-likely continuation, proof of intent

**Alternative Move Exploration**:
An in-memory branching analysis rooted at a reviewed Position. The Player may play legal moves for either side, while Engine Analysis evaluates each move and offers the strongest reply as a default rather than a forced continuation.
_Avoid_: Forced engine line, persisted analysis tree, free-form Position Exploration

**Alternative Move**:
A legal move the Player tries for either side from a Position in an Alternative Move Exploration.
_Avoid_: Best move, correction, variation

**Alternative Move Evaluation**:
A synchronous Engine Analysis comparison of one Alternative Move with objective best play from the same Position. It reports nonnegative centipawn loss from the mover's perspective or a structured mate-outcome change.
_Avoid_: Alternative Move Assessment, coach response, background analysis

**Move Sequence**:
A canonical continuation ChenChess established at a Review Moment: either the Engine's best line from the reviewed Position or the refutation of the move the Player played. It is addressed by its kind, and that address answers the same moves indefinitely.
_Avoid_: Player Line, principal variation, explored branch, arbitrary line

**Player Line**:
An ordered sequence of Alternative Moves the Player proposes from one Review Moment, evaluated ply by ply within that moment's Alternative Move Exploration. It is the Player's own candidate continuation, never a continuation ChenChess authored, and carries no authority of its own until each of its moves has an Alternative Move Evaluation.
_Avoid_: Move Sequence, canonical line, variation, engine line

**Alternative Move Assessment**:
A complete grounded coaching response produced only when the Player explicitly asks the coach to interpret an Alternative Move. It may assess intent and Elo-aware practical fit in addition to objective quality.
_Avoid_: Alternative Move Evaluation, automatic move annotation, background coaching

**Coach Turn**:
One synchronized processing of a free-form Player message into one complete coach response, bound to an immutable snapshot of its selected Alternative Move, branch context, and Move Intent. Coach App and local surfaces still start a Coach Turn. The web Review Session composer uses **HostTurn** instead.
_Avoid_: Coach Request, coach command, background coaching job, web HostTurn

**HostTurn**:
One Player message on the web Review Session, authored by the pinned Language Layer under the web system prompt, routed over the in-process capability channel, and admitted, fingerprinted, grounded, and recorded as one generation. The outcome is a grounded answer, a `HostTurnRefused` reason, or a typed unavailable thread item. The web composer has no modes. Prior capability results never re-enter; the last four turns' prose and the on-screen branch do.
_Avoid_: Coach Turn, web Coach Turn, discuss/coach composer, native tool calling

**Coach Turn Context**:
The immutable, surface-carried view of the selected Position and move, complete current Coach Intent Hypothesis, authoritative Move Intent state, relevant branch context, and evidence references supplied to a Language Layer or validator for one Coach Turn. It excludes the full chat transcript and the evidence payload itself.
_Avoid_: Review Evidence Packet, server session, chat history, LLM memory

**Opening Identification**:
The opening and variation canonically recognized from Service-Attributed Opening Metadata when present, otherwise from the Game's Positions by one versioned Opening Catalog; it carries typed source provenance and is absent when neither source yields a complete identification. It never identifies an error or prescribes a repertoire.
_Avoid_: Opening Departure, mistake cause, repertoire advice

**Opening Catalog**:
A pinned, versioned collection of named opening Positions and continuations used to produce Opening Identification when Service-Attributed Opening Metadata is absent and to establish canonical opening-resource identities. It establishes descriptive opening knowledge, never objective move quality.
_Avoid_: Opening Database Context, Engine Analysis, live opening explorer

**Opening Line**:
One named entry of the **Opening Catalog**: its ECO code, its name, and the move path from the initial Position that reaches it. Only the move path identifies it — ECO code, name, and the two together all name more than one line, because different move orders reach the same named position.
_Avoid_: ECO code as an address, opening name alone, Opening Identification, repertoire, opening variation

**Opening Analysis Cache**:
The shared, identity-free home of Engine Analysis for Positions studied from an **Opening Line** rather than from a Game. It is addressed by normalized Position with no owner and no session segment, so transpositions and repeat study answer from one entry and nothing in it identifies a Player or what any Player explored.
_Avoid_: Review Analysis Cache, Game Analysis, opening book, per-Player opening history, Opening Catalog

**Last-Named Position**:
The latest historical Game Position whose normalized EPD exactly matches a Position in one versioned Opening Catalog. It is identified by scanning the Game backward and is independent of the move sequence used to reach that Position.
_Avoid_: Opening Departure, last book move, representative catalog line

**Position Phase**:
A versioned, deterministic classification of one pre-move Position as Opening, Middlegame, or Endgame. V1 shares one move-number-and-material predicate across review selection and learning evidence; Opening Identification cannot override the classification.
_Avoid_: Opening Identification, narrative game stage, catalog membership

**Service-Attributed Opening Metadata**:
A complete, non-empty opening-name-and-ECO pair supplied by a direct Lichess or Chess.com import, or carried by a PGN whose `Site` or `Link` provenance header contains one unambiguous, syntactically valid Game URL for either service. V1 trusts the pair without an additional fetch; an incomplete pair, conflicting supported-service URLs, URLs outside those headers, and opening headers from other PGNs are ignored.
_Avoid_: Verified service metadata, arbitrary PGN opening headers, Opening Identification

**Opening Resource Mapping**:
A bundled, release-verified association from either an exact Opening Catalog Position or an exact service, ECO, and opening-name tuple to canonical learning resources. It permits deterministic resource selection without fuzzy matching, URL derivation, runtime discovery, or browsing.
_Avoid_: Opening Identification, generated resource URL, live resource lookup

**Opening Departure**:
The point where a Game leaves established opening theory. V1 does not claim an Opening Departure because the Opening Catalog is not a complete continuation graph; an uncommon or uncatalogued move is not thereby an error.
_Avoid_: Opening error, blunder, novelty

**Opening Learning Opportunity**:
An Improvement Opportunity whose pre-move Position has Opening Position Phase and whose Opening Identification has at least one Opening Resource Mapping. Its supporting Game ply establishes that the opportunity occurred during the identified opening, not that unfamiliarity with the opening caused the mistake; an Opening Principle may add a specific concept but is not required.
_Avoid_: Out-of-book move, generic opening advice, Opening Identification

**Chess Knowledge Graph**:
The reusable, versioned graph of chess concepts, recognition rules, goal templates, procedures, relationships, difficulty, and Learning Resource mappings used by Decision Explanations. It contains no position-specific candidate or Player reasoning state.
_Avoid_: Decision Explanation, curriculum DAG, Player knowledge model

**Decision Candidate**:
A legal root move and retained variation supplied by Engine Analysis or the Player's move for one Review Moment. A candidate may be both Player-played and engine-ranked.
_Avoid_: Generated tactic, proof path, search-tree node

**Atomic Chess Fact**:
An independently recomputable, concept-neutral observation about one Position Snapshot, move, or transition. It may summarize a complete deterministic set such as attacks or legal recaptures, but never names a teaching concept or infers Player intent.
_Avoid_: Learning Track Evidence, motif label, raw engine score

**Position Goal**:
A concrete, position-specific desired chess-state change that reusable knowledge may suggest for a Decision Candidate. It names exact pieces, squares, or terminal or material targets and contains neither a concept label nor an engine score.
_Avoid_: Learning Track Key, generic plan, evaluation target

**Semantic Outcome**:
A typed, concept-neutral chess-state change produced by a retained variation, such as material gain, mobility restriction, king-pressure change, pawn progress, or a terminal result. Engine Evaluation assesses a candidate separately and is not itself a Semantic Outcome.
_Avoid_: Engine Evaluation, concept label, prose consequence

**Decision Explanation**:
The deterministic, moment-local proof aggregate that relates Decision Candidates to Atomic Chess Facts, activated Chess Knowledge, optional Position Goals, Semantic Outcomes, and optional engine-backed preference. It never claims to reproduce Engine Analysis internals or the Player's mental process.
_Avoid_: Review Moment Comment, engine search tree, Learning Track Evidence

**Explanation Path**:
One selected, candidate-owned path inside a Decision Explanation, attributed as Missed Best, Conceded Refutation, or Reinforcement. It contains required Concept Validation Proof and may contain Candidate Generation Proof.
_Avoid_: Learning Track, principal variation, Player reasoning trace

**Candidate Generation Proof**:
Optional evidence that pre-move Atomic Chess Facts activate reusable knowledge and a concrete Position Goal that suggests a Decision Candidate. Its absence must remain explicit and cannot be treated as a diagnosed discovery failure.
_Avoid_: Concept Validation Proof, engine candidate source, inferred Player intent

**Concept Validation Proof**:
Required evidence that legal candidate-variation facts satisfy a versioned recognition rule and produce at least one Semantic Outcome for exactly one candidate. A concept label alone is never proof.
_Avoid_: Candidate Generation Proof, detector result, generic citation

**Proof Capability**:
The strongest comparison claim supported by a Decision Explanation: `ValidationOnly`, `EnginePreference`, or `SemanticPreference`. Capability is derived from validated proof coverage and is independent of Automatic or Player-Selected provenance.
_Avoid_: Review Moment provenance, confidence score, detector rank

**Learning Plan**:
The Game Review's frozen, deterministically ordered union of Learning Tracks projected at its Automatic Critical Moments, with no Game-level track cap. Player-Selected Moment learning material never joins or mutates it.
_Avoid_: Training plan, review schedule, Player-Selected Moment material

**Learning Track**:
A Player-facing bundle with exactly one typed Motif, Endgame, Curriculum, or Opening target and one or more verified Lichess learning resources. Chess-concept tracks project selected Explanation Paths; Opening tracks project exact Opening Resource Mappings.
_Avoid_: Course, schedule, rank, generic topic

**Learning Track Key**:
The stable ChenChess semantic identity of a Learning Track: its motif, endgame, or curriculum concept, or exact Opening Resource Mapping. It governs support aggregation, plan-track references, and deterministic tie-breaking without encoding a Lichess identifier, rank, or selection provenance.
_Avoid_: Generated track ID, title string, rank ordinal

**Learning Track Support**:
A unique Critical Moment and Game ply whose selected Explanation Path or exact Opening Resource Mapping grounds a Learning Track, tagged as improvement or reinforcement. A track's effective purpose is improvement when any support is improvement and reinforcement otherwise.
_Avoid_: Track-level purpose field, duplicate proof, generic citation

**Learning Resource**:
A complete verified Lichess learning link embedded in a Learning Track, with a stable ChenChess identity, `learn` or `drill` instructional role, concrete resource kind, title, and canonical URL. Delivery surfaces consume it directly and never resolve a catalog reference or derive its URL.
_Avoid_: Generic URL, live catalog reference, LLM-generated link

**Learning Resource Catalog**:
The bundled, versioned mapping from eligible learning targets to fully materialized Learning Resources. The Game Review Engine owns its identities and canonical URLs and verifies them at release; runtime clients never fetch or join it.
_Avoid_: Live resource discovery, client-side catalog, scraped link index

**Review Moment Learning Material**:
The fully materialized zero-to-two Learning Tracks available at one Review Moment. An Automatic moment receives an exact subset of the frozen Learning Plan, while a Player-Selected Moment receives an on-demand, session-local projection; the moment's provenance distinguishes them and neither changes the Learning Plan.
_Avoid_: Track-key-only projection, Moment Learning Plan, generic advice, plan mutation

**Opening Database Context**:
Descriptive evidence about move frequency and outcomes in relevant Lichess games at a Position. It does not determine the objective quality of a move.
_Avoid_: Opening Catalog, Engine Analysis, move verdict, opening diagnosis

**Player-Selected Moment**:
A Review Moment directly nominated by the Player for on-demand review, even if the pipeline did not select it automatically. It is selection provenance, not ordinary navigation or a coaching kind, and is a Critical Moment only when deterministic evidence supports exactly one Critical Moment kind.
_Avoid_: Manual analysis request, feedback click

**Review Moment Reference**:
How a caller names the Review Moment it wants opened from an addressed Game Review: an exact Critical Moment, any legal ply, the Critical Moment after a named one, or the next Improvement Opportunity after it. Only the ordered review can answer the forward references; the optional classification constraint filters the forward scan without changing its full-order anchor. A reference is the question rather than the answer, and resolving it establishes the moment's **Critical Moment Selection Provenance** rather than asserting it. Resolving one needs the Game Import and nothing else: no Review Session, no revision, and no state. A reference naming a Review Moment outside that Game Import is rejected rather than resolved to a neighbour.
_Avoid_: Critical Moment ID, Critical Moment Selection Provenance, session handle, navigation cursor

**Review Feedback Report**:
An immutable, one-ply record of a Player's selection or explanation feedback, annotating the Quality Capture that holds the output and evidence it refers to. It carries that capture reference and the Evaluation Fingerprint rather than its own copy of the payload, and contains no Player identity.
_Avoid_: Feedback issue, mutable report, telemetry event, second evidence copy

**Report Digest**:
A hash of one canonical Review Feedback Report used by the producing client to warn about repeated local preparation. It is not a report identity, cross-client deduplication key, or dataset case identifier.
_Avoid_: Report ID, global feedback ID, admission key

**Provider Evidence**:
Provider-neutral Engine Analysis and Human Move Model observations consumed by the Rule Extractor. It excludes raw process logs and transport payloads.
_Avoid_: Raw Stockfish output, raw Maia response, selector score

**Review Side**:
The side whose moves are eligible for pipeline-selected Critical Moments in a Game Review. It is White, Black, or both when the Player explicitly requests a two-sided review.
_Avoid_: Player color, assumed side, PGN color

**Engine Analysis**:
Objective chess evaluation of a position or move, produced by a strength-seeking chess engine.
_Avoid_: Human analysis, coach explanation

**Human Move Model**:
A skill-aware model that predicts moves a human Player at a given Elo Profile is likely to consider or play.
_Avoid_: Human model, weaker engine, Maia analysis

**Rule Extractor**:
A deterministic layer that converts Engine Analysis and Human Move Model outputs into named coaching facts.
_Avoid_: Heuristic blob, prompt logic

**Critical Moment Selector**:
A deterministic ranking layer that applies hard eligibility gates from a Selector Policy, transparent versioned Selector Weights, and diversity rules to Rule Extractor candidates before choosing Critical Moments.
_Avoid_: Rule Extractor, learned ranking model, direct feedback tuner

**Selector Policy**:
The versioned, non-learned eligibility and diversity limits enforced by the Critical Moment Selector.
_Avoid_: Selector Weights, runtime tuning

**Selector Weights**:
The versioned coefficients used by the Critical Moment Selector to score eligible candidates.
_Avoid_: Selector Policy, unversioned tuning

**Selector Weight Candidate**:
A versioned candidate set of Selector Weights tested against admitted feedback before deliberate promotion.
_Avoid_: Production weights, automatic feedback patch

**Selector Evaluation Dataset**:
A versioned collection used to compare Selector Weight Candidates. It keeps admitted selection feedback and a Curated Selector Benchmark as separate evidence rather than pooling them into one score.
_Avoid_: Combined selector score, feedback queue, production weights

**Curated Selector Benchmark**:
The representative, hand-labelled part of the Selector Evaluation Dataset. Each case retains a whole Game's eligible candidate pool and classifies every candidate ply as should-select, should-not-select, or uncertain.
_Avoid_: Admitted feedback, reference weights, unlabeled candidate pool

**Selector Promotion Partition**:
The held-out, whole-Game portion of one Selector Evaluation Dataset evidence stratum that is evaluated only after a Selector Weight Candidate is frozen.
_Avoid_: Search cases, reusable holdout, isolated positions

**Selector Experiment Run**:
An internal evaluation result that directly records its dataset revision, partitions, annotations, production baseline, candidate weights, bounded search fields and ranges, tie-breakers, Selector Policy, code revision, traces, metrics, uncertainty, and case-level differences.
_Avoid_: Candidate search, mutable dashboard, production change

**Selector Promotion**:
A reviewed production change that adopts one proven Selector Weight Candidate without changing the Selector Policy.
_Avoid_: Search winner, automatic tuning, policy change

**Pipeline Evaluation**:
A repeatable comparison of Rule Extraction facts for fixed Games across code or Local Pipeline Runtime versions. It can use recorded provider evidence for fast rule checks or refresh that evidence through the live pipeline.
_Avoid_: LLM evaluation, prose review, smoke test

**Dataset Admission**:
The addition of an immutable feedback fixture to a repository-owned evaluation dataset through a merged change. Submitting, reviewing, or labelling a Review Feedback Report does not admit it.
_Avoid_: Report approval, issue labelling, automatic learning

**Dataset Tombstone**:
A non-content marker replacing a withdrawn, previously admitted evaluation case. It excludes the case from future dataset revisions and experiment runs without retaining the removed Player-derived payload, and identifies historical experiment runs affected by the withdrawal.
_Avoid_: Soft-deleted case, retained private fixture, negative label

**Frozen Reproduction**:
A deterministic replay of a Review Feedback Report's captured inputs through its versioned policy without calling live providers. It is the required reproduction gate before Dataset Admission.
_Avoid_: Live reproduction, provider refresh, report approval

**Explanation Evaluation Dataset**:
A repository-owned collection of admitted explanation cases used to compare LLM Explainer candidates. Each case retains the full Game Review context and records its Admission Source.
_Avoid_: Prompt test set, feedback queue, prose corpus

**Admission Source**:
How one Explanation Evaluation Dataset case entered the dataset: `feedback-anchored`, from a Review Feedback Report and the Quality Capture it references, targeting the Player-reported ply; or `sampled`, from a capture with no feedback, recording the sampling basis instead. A dataset of only the former is a complaint corpus.
_Avoid_: Dataset Admission, Capture Trigger, case label, sampling weight

**LLM Judge**:
A versioned evaluator that scores LLM Explainer outputs against admitted case evidence. It cannot establish chess truth or promote an experiment candidate.
_Avoid_: Chess engine, Review Validator, promotion authority

**Judge Contract**:
An immutable LLM Judge configuration identified by its prompt, rubric, model revision, generation settings, structured-verdict schema, and code revision.
_Avoid_: Model name, latest judge, experiment result

**Judge Calibration Set**:
A held-out, repository-owned collection of human-adjudicated pairwise cases used to test one Judge Contract before it evaluates Explainer Candidates.
_Avoid_: Judge prompt examples, Explanation Evaluation Dataset, experiment results

**Human Audit**:
An independent, blinded human review of a frozen Explanation Experiment Run required before that run can support deliberate promotion.
_Avoid_: Dataset Admission, Judge calibration, automatic approval

**Language Layer**:
The LLM-controlled part of a delivery surface that interprets bounded chess evidence into hypotheses and explanations, phrases findings, and may orchestrate deterministic coaching operations. It never invents or alters positions, legal moves, evaluations, probabilities, or engine lines.
_Avoid_: Source of objective chess truth, chess engine, Rule Extractor

**LLM Explainer**:
The web application's Language Layer that turns extracted coaching facts into a Game Review.
_Avoid_: LLM analyzer, chess engine, reasoner

**Explainer Candidate**:
An immutable LLM Explainer generation contract evaluated as one unit. Its identity covers the prompt, model revision, generation settings, response schema, Grounding Ledger schema, and code revision.
_Avoid_: Prompt version, model alias, experiment output

**Explanation Experiment Run**:
An immutable comparison of Explainer Candidates over a pinned set of Explanation Evaluation Dataset cases using one versioned LLM Judge contract and randomness policy.
_Avoid_: Mutable dashboard, prompt draft, production rollout

**Explainer Promotion**:
A reviewed production change that adopts one proven Explainer Candidate without changing deterministic chess facts or Critical Moment selection.
_Avoid_: Experiment win, automatic prompt update, Dataset Admission

**Explanation Style**:
The wording strategy used by the LLM Explainer to make a Game Review understandable to its intended reader. It is one of simple, standard, or advanced. In v1 it is a prompt-template constant covered by the compiled prompt digest, identical for every Player; it becomes Player-supplied only when Coaching Preferences ship.
_Avoid_: Age profile, skill model, per-review picker

**Coaching Profile**:
The Player's durable, Coach Engine-owned personalization state. In v1 it consists of a Coaching Signal Profile alone, because Coaching Preferences are deferred past v1. It exists once per Player, is read when a Review Session starts, and is deleted with the Player account without an independent retention window. No record exists until a real write; its absence means typed defaults.
_Avoid_: Player memory, chat history, Elo Profile, conversation state

**Coaching Preferences**:
The Player-authored half of a Coaching Profile: Explanation Style, tone, current focus, and goal, each a closed enum with a code-defined default. It carries a version so a stale write is rejected rather than silently reverting a newer edit. It admits no free text. **Deferred past v1** together with the surface that would author it: v1 stores no Coaching Preferences record and projects no preference tokens.
_Avoid_: Free-text goal, prompt override, system prompt, account metadata

**Coaching Signal Profile**:
The deterministically derived half of a Coaching Profile, and in v1 the whole of it: a capped set of Learning Track Keys with their improvement or reinforcement support, ranked by how often a Player's reviewed games surfaced them. Learning-path votes are excluded — a vote judges a learning resource, not a Player's weakness. It is updated transactionally with the command that produces the signal, and an update failure never fails that command. The Player may inspect it in plain language and clear it as a whole, but cannot edit it item by item.
_Avoid_: Inferred demographics, model-authored memory, per-track suppression list, Learning Plan, vote counts

**Coaching Profile Projection**:
The bounded, identity-free derivation of a Coaching Profile that may reach a Language Layer: ordered top-K Learning Track Keys, and nothing else in v1. It carries no counts, timestamps, versions, or Player identity, and its schema digest is part of the Explainer Candidate identity. Its shape is invariant — a Player with no exposures yet projects an empty list rather than omitting the block.
_Avoid_: Stored profile, raw signal counts, Player context blob, transcript, preference tokens

**Personalization Preference**:
The Player account setting that governs whether a Coaching Profile steers coaching. When it is off, no Coaching Profile Projection is built and no Coaching Signal Profile write occurs. It is a separate setting from the Quality Capture Preference and neither implies the other.
_Avoid_: Quality Capture Preference, "Help improve coaching", cookie consent, model opt-out

**Language Layer Task**:
One of the closed set of authoring jobs a hosted Language Layer performs for a delivery surface. The web surface has exactly two: Review Moment Comment authoring and **HostTurn**. Coach App and local surfaces still author Alternative Move Assessments as a **Coach Turn**. A new task is a code change, never a configuration value.
_Avoid_: Prompt, chat endpoint, freeform request, agent tool

**Language Layer Task Contract**:
The provider-neutral governance envelope shared by every Language Layer Task: its task identity, prompt, response-schema, evidence-schema, and Coaching Profile Projection digests, its pinned generation contract and Structured Output Mode, its Delivery Surface, and its deadline, retry, cancellation, budget, and provenance rules. Task semantics stay in the individual authoring seams; the envelope carries only what makes a hosted call reproducible and governable.
_Avoid_: Universal request wrapper, shared prompt, generic LLM client config, Coach Turn Context

**Structured Output Mode**:
The fixed way a pinned generation contract obtains schema-conforming output, either native provider schema enforcement or prompted JSON. It is chosen when the model is pinned and is part of the contract identity. Local schema validation always runs in both modes, and a mode is never renegotiated mid-request.
_Avoid_: Response format retry, adaptive downgrade, per-request option

**Fact Shape**:
The authoring problem one Review Moment presents to the Language Layer: the marker slots its facts offer, with the rendering branch each slot took. Two Review Moments share a Fact Shape when the Language Layer is handed the same problem, whatever chess produced them. Derived from the comment facts policy, never enumerated by hand.
_Avoid_: Fact bundle, moment kind, Critical Moment Classification, prompt shape

**Exemplar**:
The one Review Moment a Fact Shape resolves to for measurement. Addressed by its Fact Shape and resolved against the pinned evaluation corpus, never named by ply.
_Avoid_: Frozen case, task, grounding entry, test case

**Fact Shape Census**:
The count of Fact Shapes the GothamChess ladder exhibits, union the Player-selected family it structurally cannot exhibit, and which of them the pinned evaluation corpus supplies an Exemplar for. It is the coverage authority; no Fact Shape count is pinned anywhere else.
_Avoid_: Frozen set, task set, coverage baseline

**Pin Verification**:
The after-the-fact check of an actual model response against the pinned generation contract using the provider's own reported model and route from `GET /generation`. A mismatch is recorded on the Quality Capture and Language Layer Operational Record and alerts as an operational fault. It does not discard paid output or change what the Player sees.
_Avoid_: Trusting the request, provider fallback, silent substitution, publication gate

**Out-of-Scope Coach Turn**:
A Coach Turn whose Player message the Language Layer classifies as outside the reviewed position and chess coaching. The Language Layer returns only a closed reason, the delivery surface renders deterministic text, and no authored prose reaches the Player. It is distinct from an unavailable Language Layer.
_Avoid_: Refusal message, moderation result, provider error, safe rendering

**Maia**:
The preferred Human Move Model family for predicting human-like chess moves at a target skill level.
_Avoid_: Engine Analysis provider

**Model Adapter**:
A stable boundary around an external chess engine or model used by the Game Review pipeline.
_Avoid_: Direct integration, vendor-specific call

**Central Host**:
A managed deployment of ChenChess for Players who do not self-host. It is the single public origin for its Landing Page, authenticated web product, and Coach App connection.
_Avoid_: Cloud-only app, Railway app, web portal

**Elo Profile**:
The Player's playing strength used to adapt a Game Review. It comes from imported metadata for the selected Review Side when available, otherwise the Player provides it for the review.
_Avoid_: Level, skill setting, rating input

**Auth Token**:
A signed JWT presented to the Coach Engine to prove a Player's authenticated identity. Central Host token profiles preserve the same Firebase identity in `sub`, whether the Player arrives through the web application or a Coach App.
_Avoid_: Player Session, frontend session cookie, social-provider token

## Relationships

- A **Player** imports one **Game** at a time.
- The **Service Operator** is identified on the privacy, terms, and support pages and is distinct from an authenticated **Administrator** role.
- The V1 **Landing Page** and authenticated product use no marketing analytics, advertising pixels, session replay, or nonessential tracking cookies; only essential auth/session state and minimized operational or security logs are permitted.
- An **Administrator** may grant a pending **Beta Access Request** by issuing one **Beta Invitation** to its **Invitation Email**.
- The authenticated **Beta Access Request** endpoint derives its normalized email from verified Firebase claims, rate-limits by that email and source IP, and returns one generic success response for new, duplicate, and previously handled addresses.
- The **Beta Back Office** manages the beta admission lifecycle, including live **Beta Access** revocation. Its only coaching-data exceptions are the narrow metadata and delivery action required for a **Digest Email Replay** and the already-due lifecycle promotion required for a **Manual Digest Run**. Firebase identities, digest contents, game identities, and administrative authority remain outside its control.
- The out-of-band Administrator CLI preserves unrelated Firebase custom claims, grants or revokes only `chenchessAdmin`, rejects an unverified or mismatched UID/email pair, and requires the affected account to obtain a fresh Firebase ID token.
- Granting a **Beta Access Request** starts an automatic **Invitation Delivery**; retrying a failed delivery keeps the same **Beta Invitation**.
- **Resend** sends each **Invitation Delivery**; inbound mail forwarding is the operator's own arrangement, and neither is a source of authorization.
- Beta **Invitation Delivery** accepts Resend's standard 30-day sent-email retention, disables open and link tracking, and never places an invitation code in provider tags, application telemetry, or logs.
- Beta access records are erased on revocation where applicable, a verified support deletion request, or beta closure; keyed rate-limit identifiers expire after 24 hours, and application security or operational logs expire within 30 days without containing emails, codes, tokens, form bodies, or OAuth secrets.
- A **Beta Invitation** grants **Beta Access** only after its code and **Invitation Email** match an authenticated **Player**'s verified account email and it is bound to that Player's **Player ID**.
- Revoking **Beta Access** removes the Player's shared staging authorization without changing the redeemed **Beta Invitation**; replaying that invitation cannot restore access.
- A **Beta Invitation** never stores or logs its code in plaintext: redemption compares a versioned keyed HMAC in constant time, while Invitation Delivery alone may decrypt the separately protected retry copy.
- Staging and production use separate Firebase Web Apps in the same Firebase project and Authentication user pool. The verified Firebase `uid` is the same **Player ID** in both environments. Each environment keeps Coach Engine application data and Coach OAuth protocol state in dedicated named Firestore databases that the other environment never reads. A Firebase identity alone never grants **Beta Access** or promotes beta application data to production.
- Equal email strings never merge Firebase identities or link **Supported Sign-In Methods** by themselves; the Player must authenticate with the existing provider before linking another credential.
- A **Beta Coach App Connection** depends only on beta OAuth issuer metadata, client registrations, signing keys, cookie secrets, and encryption secrets; production accepts none of them.
- A staging Central Host is confined to its own host, uses host-only cookies, marks its pages `noindex`, leaves the production apex unchanged, and never redirects apex traffic into staging.
- A **Lichess Game URL Import** resolves one public Lichess game URL into one **Game** and does not browse or synchronize a Lichess account.
- A **Chess.com Game URL Import** resolves one supported public shared computer or live PvP Game URL into one **Game** and does not browse or synchronize a Chess.com account.
- A **Profile Game Feed** resolves at most ten newest eligible Games and never itself runs or persists a Game Review. A digest worker submits every resolved Game through the ordinary one-Game **Game Import** boundary, where each success receives its own **Game Import ID** and **Game Review**.
- A Lichess **Profile Game Feed** uses the official newest-first user-games export. A Chess.com **Profile Game Feed** uses only the official monthly PubAPI archives, traverses them serially newest first, and admits only supported live standard-chess Game URLs.
- Concurrent dashboard and MCP mutations of **Daily Coaching** use semantic preconditions rather than exposing aggregate revisions. Changes to different providers may both succeed; stale replacement or removal of one **Playing Profile Identity** conflicts with the current state, and exact retries are idempotent.
- Re-enabling **Daily Coaching** resumes an incomplete **Initial Backfill**, never repeats a completed one, and never catches up ordinary calendar days missed while disabled. The next scheduled run considers only its ordinary previous-calendar-day window.
- Removing the last **Playing Profile Connection** automatically disables **Daily Coaching** and fences unpublished work; removing one of several connections leaves coaching enabled for the rest.
- While at least one **Playing Profile Connection** remains, adding or replacing another connection preserves an explicit disabled **Daily Coaching** state rather than enabling it automatically.
- Removing a **Playing Profile Connection** erases its URL and operational cursor. A later **Playing Profile Reconnection** starts a new **Initial Backfill**, but retained Coaching Digests keep their Games ineligible for duplicate coaching.
- Removing a **Playing Profile Connection** also clears its validation state and pending contribution to unpublished work, but does not delete existing Coaching Digests; those follow their own retention and deletion policy.
- **Initial Backfill** publishes a Coaching Digest only when at least one eligible Game yields coaching. When no eligible Games exist, **Daily Coaching** remains enabled with a Player-visible `no eligible games yet` state and no empty digest.
- A **Coach Skill** may start a **Lichess Game URL Import** through its **Review Facts Tool**. The **Coach Engine** applies the same fixed-origin import, eligibility, provenance, and recovery contract used by the authenticated web path.
- A **Coach Skill** may start a **Chess.com Game URL Import** through its **Review Facts Tool** under the same completed-standard-Game, provenance, and recovery contract.
- A **Coach App** calls a shared **Coach MCP Server** from ChatGPT or Claude. The host model remains the **Language Layer**: the Coach MCP Server has no hosted model provider, and conversational coaching stays in the surrounding chat rather than a server-authored sampling round trip.
- The **Coach MCP Server** registers 18 tools. The eight-tool Language Layer surface is `get_coaching_digest`, `search_reviewed_games`, `connect_playing_profile`, `review_game`, `list_critical_moments`, `open_review_moment`, `evaluate_player_line`, and `render_move_sequence`. `get_coaching_digest` reads the latest or one exact permanent **Coaching Digest**, accepts an optional inclusive caller freshness boundary, and returns typed not-ready reasons without maintaining a server watermark or returning stale content for `noNewDigest`; its compact projection contains at most two priorities with exact resources and supporting Game Import IDs, plus one typed line per included Game. `search_reviewed_games` finds one of the Player's older reviewed Games across Coaching Digests and manual imports without guessing which digest holds it: every supplied filter is AND-ed, results are newest-first and capped at 20, coverage describes reviewed Games only, and a truncated result carries the exact boundary the next search narrows to. The five Game Review tools are addressed by Game Import ID, so the same call answers the same way on first paint and in a conversation reopened a year later; there is no frozen-versus-interactive choice to make and no handle that can age out between two calls. `review_game` reviews a Game the Player supplies, reusing the durable Game Import, which is also how a review whose Game Import ID is lost from a historical card is recovered. `list_critical_moments` re-presents that review's whole ply-ordered Critical Moment set and tagged **Opening Identification** for a Game Import ID, reading the same immutable snapshot the selector renders, so showing the moments again costs no import and no analysis. `open_review_moment` opens one Review Moment of a Game Import by **Review Moment Reference**. `evaluate_player_line` evaluates up to twelve plies from an exact Critical Moment or legal Game ply, accepts SAN or UCI, and requires either `supplied` alternating moves or Player-only moves interleaved with `engineBest` replies. Every result entry carries both notations, the mover, Player-or-Engine source, verbatim **Alternative Move Evaluation**, and offered strongest reply. Illegal moves, moment exhaustion, per-move deadlines, and a rate-cap pause retain every completed ply and name the stopped index; every evaluated result also carries the Engine-owned remaining allowance as private Language Layer pacing state. Stable per-prefix identities make replay and extension reuse resident branch nodes, while a fresh identity retries only a previously interrupted ply. The Coach MCP Server serializes this work by Player ID, charges only unique Engine progress that reached analysis, and stops before another ply once that Player's fixed-window cap is reached, so concurrent lines cannot bypass admission and deduplicated prefixes cost nothing. The tool mounts no app, exposes no branch handle, and mints all computation state server-side. Every nonempty evaluated prefix carries one exact `render_move_sequence` option containing the canonical UCI sequence of all returned plies, including interleaved Engine replies. `render_move_sequence` shares one board-and-notation renderer across canonical continuations and evaluated Player Lines without merging their meanings. A canonical line is named by Game Import ID, Review Moment ID, and kind. An evaluated Player Line adds its exact one-to-twelve-ply UCI path and is replayed only from the durable Review Moment Position, with no evaluation or transient branch state in the display snapshot. The emergency legacy-low-level model list remains frozen at its pre-Daily Coaching shape and keeps `get_coaching_digest` and `search_reviewed_games` app-only while active. App-only and compatibility tools are `open_review_moment_in_place`, `publish_review_moment_comment`, `inspect_position`, `explore_alternative_move`, `record_learning_path_exposure`, `update_learning_path_vote`, `cancel_operation`, `report_app_performance`, `read_game_review_snapshot`, and `read_move_sequence_snapshot`. `connect_playing_profile` connects the one public profile the Player explicitly names: it performs the Coach Engine's live provider existence check, returns canonical identity, is idempotent for the same identity, and rejects an occupied provider; MCP exposes no Daily Coaching state/list tool and no enable, disable, email, replace, remove, or timezone mutation. `publish_review_moment_comment` remains the explicit Grounding Ledger admission boundary. `explore_alternative_move` returns a safe objective evaluation and Position projection; when the Player asks about it, the workspace pins that exact target and sends the Player's ordinary message to chat without another tool call. `report_app_performance` remains registered because its enabled-staging batch carries allowlisted timings together with the exact Learning Path handles the card rendered, while production selects a telemetry-free artifact and sends no performance beacon. `record_learning_path_exposure` remains for the exposure a vote records before saving.
- A side-qualified **Lichess Game URL Import** preselects the **Review Side** as White or Black; the Player may change it to White, Black, or both before review.
- A bare **Lichess Game URL Import** requires the Player to choose White or Black before review.
- A **Chess.com Game URL Import** requires the Player to choose White or Black before review.
- A **Coach Skill** calls a **Review Facts Tool**, the active coding agent turns the returned facts into a **Draft Game Review**, and a **Review Validator** checks it before presentation as a **Game Review**.
- A production **LLM Explainer** produces a **Draft Game Review** with an internal **Grounding Ledger**. The **Grounding Gate** rejects missing or unknown evidence references and mismatched literal values before the Player sees the **Game Review**.
- **Local Coach Execution** keeps chess processing local but does not imply that the active coding agent runs a local language model.
- The **Coach Skill** provisions and starts its **Local Pipeline Runtime** rather than requiring the Player to configure Stockfish or Maia separately.
- An **Amateur Player** may be a child and needs age-appropriate explanations.
- An **Auth Token** identifies exactly one **Player** by **Player ID**.
- The public Node adapter may persist only OAuth protocol records in `coach-oauth-staging` or `coach-oauth-production`. The **Coach Engine** owns product data in `coach-app-staging` or `coach-app-production`; staging and production exporters write identity-free **Quality Captures** to one `coach-quality` collection through distinct write-only, database-scoped service accounts. `(default)` is prohibited. Distinct service and environment accounts receive database-scoped IAM access, and browser clients receive direct access to none of the databases.
- Every dynamic Firestore document path segment is a SHA-256 digest. Player IDs, Game Import IDs, review keys, moment identities, OAuth adapter IDs, and capture identities never appear raw in a document path. No session identity appears at all, because none is issued.
- The web application binds its Firebase ID-token supplier to the **Coach Engine SDK**; the public Node adapter binds the Coach OAuth bearer token; a Coach App delegates authorization to its host and never stores or refreshes either credential itself.
- A **Game Review** request to the **Coach Engine** must include an **Auth Token** as bearer authentication.
- A **Game** produces one **Game Review**.
- The first successful Game Import for one Player, durability generation, canonical Game, Review Side, and resolved Elo atomically persists one self-contained **Game Import Record** without a time-based expiry. Matching imports return its existing **Game Import ID** and exact frozen **Game Review**. An optional **Game Analysis** hit may seed the first record, but the record never depends on that cache afterward. The durable review is retrieved passively at its Game Import ID by the addressed reads and the snapshot resource. Starting a **Review Session** accepts the handle, verifies the same Player, resolves the server-owned imported Game, Review Side, and Automatic Critical Moments, and returns only interactive product state. Clients do not carry or integrity-wrap imported Game state.
- For MVP, `review_game` is the only import operation and accepts a supported Chess.com Game URL, a Lichess URL, or raw PGN. There is no separate app-only import path, so the product promises no model-private import channel and never redirects the Player between methods. The Coach Engine still avoids echoing raw PGN in results and excludes the original text from logs and durable records after normalization.
- Staging and production may each create an identity-free **Quality Capture** for a qualifying Game Analysis, Coaching Response, or hosted Language Layer generation only after the Player acknowledges the disclosure and the **Quality Capture Preference** permits it, or when a Player's feedback submission induces one under its own submit-time disclosure. Both environments write the same `coach-quality` collection through distinct write-only, database-scoped service accounts, and neither can read or delete the other's records.
- The qualifying business result and its **Quality Outbox** record commit in one product-database transaction. Export to `coach-quality` is idempotent by capture ID and content digest. An export failure never fails the business command, while a digest mismatch fails closed.
- A **Quality Capture** may retain the canonical Game, Review Side, resolved Elo, structured chess facts or generated response, versioned reproducibility provenance, its **Evaluation Fingerprint**, **Capture Trigger**, **Capture Outcome**, observed provider route and **Pin Verification** verdict, and call-shape facts: token counts, cost, finish reason, attempt and retry count, deadline-hit flag, and a day-precision creation date. It excludes Player ID, Review Session ID, Game Import ID, names, URLs, raw PGN, Player-authored free text, full transcripts, request IDs, wall-clock timestamps, latency, and raw provider payloads, because a precise time joins a capture back to one Player's request while a token count does not.
- The product-database **Quality Outbox** is the only Player-to-capture association. Evaluation tooling cannot query or export it, and `coach-quality` stores no Player association.
- Quality Captures are not directly browsable through product APIs. An unadmitted capture expires at the end of its **Quality Capture Retention Window**. Dataset Admission and Dataset Withdrawal remain deliberate workflows.
- Evaluation prompts, provider request and response traces, and evaluator reasoning traces are transient. They are never durable evaluation data. A hosted request is regenerated from its prompt digest and captured inputs rather than stored, and only a bounded, free-text-stripped excerpt of an output-shaped failure persists on its **Quality Capture**.
- Every **Quality Capture**, **Review Feedback Report**, and **Language Layer Operational Record** carries an **Evaluation Fingerprint** digest. Its axes are declared configuration known before the call; the served provider, **Pin Verification** verdict, **Capture Trigger**, and **Capture Outcome** are per-record facts recorded beside the digest, never inside it.
- A fingerprint's axis record is immutable and never recomputed. Pooling cohorts across **Evaluation Contract Versions** is an explicit, recorded choice in an **Explanation Experiment Run**.
- Production capture opt-out stops new capture immediately, deletes pending and unadmitted captures, and queues withdrawal of admitted captures.
- The non-production MVP includes web sign-out and per-host Coach OAuth grant revocation and relinking. Self-service Firebase account deletion is production-only.
- Production self-service account deletion is an idempotent **Coach Engine**-owned saga: block new Player commands, write deletion markers in both product databases, withdraw production Quality Captures, recursively delete both Player subtrees, require the Node OAuth adapter to revoke every grant for that Firebase Player ID in both OAuth databases, then revoke refresh tokens and delete the shared Firebase identity. Retries resume the recorded deletion state.
- The **Quality Capture Preference** explains the improvement purpose, 12-month **Quality Capture Retention Window**, and withdrawal behavior before the first capture. Turning it off later invokes the same withdrawal behavior.
- Accepted selection feedback may produce a **Selector Weight Candidate**; it never edits production selection weights directly.
- The **Critical Moment Selector** keeps legality, Review Side, evidence validity, forced-mate handling, and maximum moment count outside tunable weights.
- A **Selector Weight Candidate** must beat fixed regression cases and balanced evaluation slices before deliberate promotion.
- Accepted explanation feedback enters the **Explanation Evaluation Dataset** for versioned prompt and model experiments.
- An **LLM Judge** evaluates versioned explanation experiments after deterministic grounding validation; a human still approves promotion.
- The first feedback-loop implementation does not automatically fine-tune a production model or rewrite a production prompt.
- One frozen **Game Review** grounds at most one **Review Session** at a time, because a session is keyed by Player and Game Import and by nothing else. What two conversations studying one import share beyond that session is durable and addressed by the review rather than by the chat: canonical Review Moment Comments in the **Review Annotation Store**, prepared analysis in the **Review Analysis Cache**, and the frozen review itself. Two conversations over one import therefore never disagree about what the Player has published; the store's answer is authoritative and a session's own record of its writes is not.
- Starting a **Review Session** prepares objective facts and Position Snapshots for every Automatic Critical Moment. It does not run **Intent Enrichment**. Preparation uses runtime-bounded concurrency, waits for every moment to reach a valid ready state, and returns the complete set in ascending Game ply. No moment is selected as active.
- A **Review Session** is admitted only after every Automatic Critical Moment is prepared. Cancellation or a hard invariant failure such as missing facts, invalid classification, or an illegal position admits no session and exposes no partial result. Missing **Intent Enrichment** never fails the session or the Game Review.
- A **Review Session** is get-or-create on one Player and one Game Import ID. Concurrent or repeated starts join the same in-progress preparation or receive the ready session already resident, so a second conversation on one import shares that session rather than minting an incarnation beside it. Cancellation or hard failure leaves no session and permits an ordinary retry, because there is no acknowledged handle to reconcile.
- Starting a **Review Session** hydrates prepared moments from the **Review Analysis Cache** before preparing anything, so returning to a Game the Player has already studied costs the engine nothing it has already spent. Nothing about the session is written down: the durable **Game Import Record** is the only thing the start reads that outlives it.
- On a fresh interactive review, the web application or Coach App may immediately display the earliest Automatic Critical Moment from the returned prepared set without another server operation. This is delivery-surface navigation policy, not a server invariant; an explicit target may display first instead, and a review with no Automatic Critical Moments displays none.
- Reopening an initialized **Review Moment** in the same Review Session resumes its prepared facts, published Idempotency Keys, and any canonical comment. Published comments restore without reconstructing **Intent Enrichment**. **Alternative Move Exploration** is in-memory and does not survive the process that holds it. Navigation never regenerates coaching.
- The currently displayed **Review Moment** is delivery-surface navigation state, not server state. Server operations address the target Review Moment explicitly and do not mutate a shared active-moment pointer.
- One Review Moment opening operation accepts a legal Game ply within the shared Review Session. An Automatic Critical Moment is already prepared and is simply resumed; a previously unseen ply creates and prepares a Player-Selected Moment. Both use the same classification, comment, validation, and publication lifecycle while their **Critical Moment Selection Provenance** remains distinct.
- Within one **Review Session**, one Game ply resolves to at most one **Review Moment**. Directly selecting a ply already represented by an Automatic Critical Moment reopens that moment and preserves its Automatic provenance.
- One Review Moment opening operation also exists without a Review Session, addressed by **Game Import ID** and a **Review Moment Reference**. It resolves the reference against the stored Game Import — an exact Critical Moment, any legal ply, an index step through the ply-ordered Automatic Critical Moments, or a forward step restricted to Improvement Opportunities — and answers with that moment's grounded detail. A filtered step locates its anchor in the full moment order before scanning for a matching classification, so any Automatic or Player-Selected Moment can be the starting point. It is a read: it creates no Review Session, writes nothing, and answers the same address the same way indefinitely. A reference naming a Review Moment outside that Game Import is rejected as an unknown moment rather than resolved into a neighbouring one, and a ply already represented by an Automatic Critical Moment resolves to that moment with its Automatic provenance and its pipeline proof rather than to a thinner Player-Selected one.
- An opened **Player-Selected Moment** joins the transient Review Session's Review Moment navigation at its Game ply with its provenance and classification. It does not modify the frozen automatic Critical Moment set or Selector Trace.
- A **Review Session** is transient operational state, not a permanent or browsable review library, and it is not written down anywhere. There is no session identifier, no session record, no revision to validate, and therefore no stale or conflict outcome a caller can receive from one: a command either runs against the session resident for that Player and Game Import or rebuilds it first. Losing the process loses only warm memory. No caller retains a session handle, because none is issued.
- Nothing a Player can see depends on a **Review Session** surviving. A session that is evicted or lost is rebuilt on the next command from state the Player already owns: the durable **Game Import Record** supplies the frozen review, the **Review Analysis Cache** supplies prepared moments, and the **Review Annotation Store** supplies published comments. What is genuinely lost is what was never promised — in-flight operations, delivery envelopes, cancellation outcomes, retry histories, presentation events, and **Alternative Move Exploration**. Replaying a write's **Idempotency Key** still returns its original result, because the annotation store answers it rather than the session.
- There is no resume operation, at any layer. Nothing a Player asks for needs one: `list_critical_moments` re-presents the chronological Critical Moment selector from the durable Game Import, `open_review_moment` opens any moment of it, and `review_game` recovers the review itself when only the original Game identity is known. Each answers from the immutable snapshot at the Game Import ID rather than from anything a conversation left behind.
- Coach App commands carry only the minimal typed handles required by their domain operation, such as Game Import ID, target Review Moment or Game ply, operation ID, and the caller's Idempotency Key. There is no universal client-carried state token and no host widget-state store: a widget renders from one addressed immutable resource it reads on mount, so its whole rehydration is `render(snapshot, selection)` and nothing about what is on screen is cached on the host.
- The canonical authenticated web address for a frozen review is `/app/game-reviews/{gameImportId}`. It loads the durable Player-owned Game Import. Every other web address hangs off it and is written with the same handles the resource URIs use: `/app/game-reviews/{gameImportId}/moments/{reviewMomentId}` and `/app/game-reviews/{gameImportId}/moments/{reviewMomentId}/sequences/{kind}`. Those two render the widget's own component tree, differing from the widget mount only in fetching the review from the Coach Engine rather than as an MCP resource. A `?ply=` parameter names which ply the board stands on inside the addressed resource. It is a parameter rather than a path segment because it selects no different resource: the path handles are what the Coach Engine answers, and where the board is standing within one is presentation. Carrying it in the address rather than remembering it per Player is what keeps one address meaning one thing — a reload and a copied link both show the position the sender was looking at, instead of the same URL showing two people different boards. There is no `/app/review-sessions/{sessionId}` route, because there is no Review Session to continue, and the web application remembers no navigation state between visits: a workspace opens the Game Import its address names, and the bare `/app/` address is retired to the dashboard. It does cache bytes. The frozen Game Review snapshot at an address is held in IndexedDB so a revisit repaints without refetching roughly a megabyte, and that cache is a transport cache and nothing more: it changes where the bytes come from, never what an address means, never what is on screen when one opens, and never what a reload restores. Every entry is namespaced by Firebase `uid` and the whole store is erased on sign-out, because a browser is a shared device. An entry is used only when the Coach Engine confirms the `ReviewContentDigest` it was stored with — which folds the durability schema version, the analysis generation, and the comment template digests — and only when the client's own projection version still matches, so a change on either side is a miss rather than a stale hit. A Player's Alternative Move Exploration is not cached and still dies with the process. A web address is an identifier and never a bearer capability: malformed locators fail closed, every route resolves behind Firebase authentication and Beta Access, and the Game Import's owner subtree remains the authorization boundary. Static delivery returns the SPA shell with no serialized review data, `no-referrer`, and `noindex`.
- Exactly one web address carries its own authorization: `/app/shared/{shareToken}` resolves a **Review Share Grant** before the sign-in gate and is the only route that does. It is a separate path rather than a parameter on an existing one so that a reader of a URL, and the router, can tell a capability from an identifier. A token that could not have been minted is refused in the browser without asking the Coach Engine; a link that expired, was withdrawn, or never existed renders a visible typed explanation rather than a disabled surface, because a visitor has no session to retry from and no account to check. The shared mount is the same component tree the owner sees, differing only in its bridge: it reads through the grant and refuses every action.
- Coach App results carry the Game Import ID and exact authenticated durable Game Review URL in host-visible result state so a historical `critical-moments-selector` can re-present itself, or open the same frozen review in the web application, without depending on anything a conversation holds. When a host accepts only textual `ui/update-model-context`, the app adds a bounded machine-readable continuation envelope containing only exact addressed moment-open and canonical sequence render arguments plus authenticated URLs; the model must never quote raw opaque handles to the Player. The Node adapter stores no host-conversation mapping, and no surface lists or discovers a Player's imports.
- Review authorization binds to the Firebase Player rather than to the delivery surface that imported the Game. The same authenticated Player may open the same review by its **Game Import ID** through web, ChatGPT, or Claude, and reads the same durable comments in each. Explicit deep-link handoff is supported, but surfaces do not otherwise discover, automatically select, or synchronize host conversation around a review.
- A canonical graphical Move Sequence is addressed by Game Import ID, Review Moment ID, and stable sequence kind, and is read at `chenchess://game-review/{gameImportId}/moment/{reviewMomentId}/sequence/{kind}`. A Review Moment offers at most one line of each kind, so nothing is minted and nothing expires: the same address answers with the same moves whether or not any Review Session is alive. An evaluated graphical Player Line is addressed by Game Import ID, Review Moment ID, and the exact bounded canonical UCI path returned by `evaluate_player_line`, and is read at `chenchess://game-review/{gameImportId}/moment/{reviewMomentId}/sequence/playerLine/{moves}`. Its notation and boards are recomputed solely from the durable Review Moment Position; the resource contains no evaluations and does not depend on the transient evaluation branch. The `render_move_sequence` tool takes either complete address in its arguments, which is the whole of what the widget needs: it mounts, reads the resource, and plays the line without a tool call or `_meta`. Player Line evaluation itself mounts no app; rendering is an explicit follow-up, normally for a useful multi-ply view and for one ply only when the Player asked to see that move on a board. The shared renderer never turns a Player Line into a canonical Move Sequence or recommendation. The Critical Moment selector navigates moments only and is never a substitute for line playback.
- Exactly two stores stand behind a **Review Session**, and neither belongs to it. The identity-free **Review Analysis Cache** holds prepared Review Moment analysis addressed by review key, carrying each entry's purge time, whether server-owned preparation completed, and allowlisted provider evidence; it is shared across Players and evicted on its own retention. The Player-owned **Review Annotation Store** holds published Review Moment Comments keyed by Game Import, Review Moment, and Player, and is erased only with the Player subtree. Both are excluded from evaluation input and neither changes the separate retention policy for **Quality Captures**. Facts and Position Snapshots are derived from the self-contained **Game Import Record** rather than stored a second time.
- Completed **Alternative Moves** are deliberately in neither store. They record what one Player explored, and a cache entry is read by every review of that Game, so persisting them would leak one Player's exploration into another's analysis. **Alternative Move Exploration** is in-memory and dies with the process that holds it.
- Each durable record declares its own schema version. Pre-release data is disposable and has only the current decoder; unknown versions and malformed records fail closed without partial reads. Local and self-hosted deployments persist nothing beyond what an explicit export writes.
- **Review Session Residency** is a memory bound and nothing else. A successful Player-initiated command refreshes the 72-hour idle window; passive reads, polling, and retries do not, and the 336-hour absolute ceiling is never refreshed. Reaching either releases the actor's engine leases and costs the next command a rebuild — never a review, a comment, or an address.
- An evicted **Review Session** is not resurrected because there is nothing to resurrect: the frozen **Game Review** and the Player's published Review Moment Comments remain retrievable by Game Import ID, and the next command builds fresh transient state over that durable import without reimporting or rerunning the Game Review. If both every opaque address and the exact original Game identity are lost, the product does not guess or enumerate another Player record.
- The web application and **Coach Skill** render boards from the same canonical **Position Snapshot**. A Language Layer never reconstructs board state from prose.
- A **Position Snapshot** has content identity independent of the Game ply or Alternative Move branch that reached it.
- A **Game Review** is tailored by one per-review **Elo Profile**.
- **Intent Enrichment** uses the same **Elo Profile** for both sides when it builds a **Projected Plan**.
- A **Game Review** is generated from **Engine Analysis**, **Human Move Model** output, **Rule Extractor** facts, **Critical Moment Selector** choices, and an **LLM Explainer**.
- The web application's **LLM Explainer** and the **Coach Skill**'s active coding agent are **Language Layers**. They may interpret grounded evidence, phrase findings, and orchestrate coaching operations, while objective position data, move sequences, probabilities, evaluations, and engine lines come only from deterministic pipeline or tool outputs.
- A **Game Review** contains pipeline-selected **Critical Moments**.
- The **Rule Extractor** produces candidate facts independently of **Critical Moment Selector** ranking.
- The **Selector Policy** fixes non-learned eligibility and diversity limits; changing a published policy creates a new version.
- The **Selector Weights** rank eligible candidates; changing published weights creates a new version.
- A **Selector Weight Candidate** cannot bypass the **Selector Policy** and becomes production **Selector Weights** only after deliberate promotion.
- A **Selector Experiment Run** records its bounded search space and tie-breakers directly. Search never changes the Selector Policy or promotes a candidate.
- A **Selector Weight Candidate** must pass the admitted-feedback and **Curated Selector Benchmark** gates independently. An unreported ply in admitted feedback is unknown, not a should-not-select example.
- A **Curated Selector Benchmark** preserves each Game's natural label mix and uses case-level coverage across Elo Profile, Review Side, Game phase, category, forced-mate order, adjacency, diversity, and candidate-pool density.
- A promotion-grade **Curated Selector Benchmark** label comes from two blinded human annotation passes that hide selector outputs, scores, and weights. Agreement fixes the label; unresolved disagreement becomes uncertain.
- Related cases from one Game stay in the same development or **Selector Promotion Partition**. Once a promotion partition evaluates a frozen candidate, a revised candidate requires fresh held-out coverage under a new dataset revision.
- Selector evaluation gates and ranking diagnostics use each Game's realized adaptive target, pinned with its eligible candidate pool and **Selector Trace**. The hard maximum of ten Critical Moments is a safety invariant, not a coverage target or evaluation cutoff.
- A **Selector Promotion** is blocked by any **Selector Policy**, forced-mate ordering, episode-collapse, Positive Highlight reservation, determinism, or reproducibility failure. Supported evaluation slices may also block promotion under predeclared non-regression rules; underpowered slices remain visible but cannot prove or veto improvement.
- A **Selector Experiment Run** directly pins its dataset revision, partitions, annotations, baseline and candidate weights, Selector Policy, bounded search configuration, code revision, traces, metrics, uncertainty, and case-level selection differences.
- **Selector Promotion** merges a separate human-approved change that links one immutable Selector Experiment Run, adds one immutable Selector Weights version, selects it as the production default, and names the previous version as the rollback target. Automation verifies but never approves promotion.
- A delivery surface may display the earliest pipeline-selected **Critical Moment** from the session-start result as its initial navigation default. This does not create a distinguished Critical Moment, change selection priority or provenance, require another server operation, or prevent any other prepared Review Moment from being displayed first.
- A Game Review with no Automatic Critical Moments still grounds a valid **Review Session**. No Review Moment or **Intent Enrichment** starts automatically; the surface presents the Game summary and timeline and lets the Player open any legal Game ply, while the Coach Skill validates a complete review with zero moment comments rather than manufacturing one.
- An opened **Review Moment** immediately presents its **Review Moment Comment**; objective facts, coaching, and any **Coach Intent Hypothesis** are not hidden behind a reveal gate.
- A **Review Moment Comment** opens according to its classification: Positive Highlight grade, played move, and achievement; Improvement Opportunity evaluation and concrete consequence; or a concise neutral verdict. An analyzed comment still preserves its required evaluation literals, while a terminal comment uses its verified board-terminal outcome. At most one explicitly uncertain **Coach Intent Hypothesis** may follow the factual opening.
- Every **Review Moment Comment** has exactly one valid **Review Moment Comment Facts** variant. Fields belonging to another variant are invalid rather than ignored.
- **Intent Enrichment** is built lazily on the first authoring attempt of an unpublished Review-Side Positive Highlight or Improvement Opportunity. The Language Layer receives the selected **Projected Plan** SAN, **Objective Counterplay** SAN, and classification-aware instructions. It cannot select, replace, or add a hypothesis from candidates, scores, or traces.
- Opening a Review Moment returns its canonical Review Moment Comment after the web Grounding Gate, Coach Skill Review Validator, or Coach App Review Moment Comment Publication admits it. Publication provenance remains internal unless a durable evaluation artifact is retained.
- The shared session-start operation returns every fully prepared Automatic Critical Moment to its delivery adapter. The web application completes Review Moment authoring internally; the Coach App supplies the prepared facts and optional **Intent Enrichment** to its host model; and the Coach Skill uses the ordered prepared set for batch drafting and Review Validator admission. Canonical prose remains surface-authored even though objective preparation is unified.
- The **Coach Engine** keeps transient **Review Session** domain state keyed by Player and Game Import, including independent per-Review-Moment state plus a session-wide Coach Turn generation; this is distinct from an MCP transport session and from live-operation capacity tracking, and it creates no durable review history. That bookkeeping — the generation and the turn's own state — stays in memory; what the admission rule enforces is keyed by the same Player and Game Import, so it refuses a second in-flight turn across every conversation over one imported Game rather than only within one of them.
- Comment publication and Alternative Move Exploration each carry a caller-generated **Idempotency Key** naming one logical write, deduplicated against the target Review Moment. Replaying a key returns that write's original result rather than a conflict; the Coach Turn generation and the Player-and-Game-Import turn scope together enforce the one-active-turn rule.
- A host-authored canonical **Review Moment Comment** in the **Coach App** must pass **Review Moment Comment Publication**. The request identifies the Game Import and Review Moment and submits draft text, Grounding Ledger, and Idempotency Key; the server resolves the authoritative facts and intent from its transient session and facts registry rather than trusting a client-carried copy.
- `publish_review_moment_comment` validates and publishes one draft atomically. There is no separate validation-only tool that could approve one payload and allow another to be displayed. The unused Coach Engine operations for plan evaluation and Coach Turns retain their grounded preparation and admission boundary so a future producer cannot bypass it; the Coach MCP Server exposes neither operation.
- **Review Moment Comment Publication** never fails because a conversation aged: the Game Import is durable, so the authoritative facts are rebuilt and the publication proceeds. It fails closed when the server cannot resolve those facts for the named Review Moment at all. Client-supplied facts never restore publication authority unless the server first reconstructs and verifies the authoritative review context.
- A **Review Moment Comment** written in host conversation without **Review Moment Comment Publication** is noncanonical conversational prose and is never rendered by the inline chess workspace as the official comment. The web path applies the same Grounding Gate internally, while the **Coach Skill** applies it through the Review Validator.
- A **Player-Selected Moment** outside the **Review Side** offers no **Coach Intent Hypothesis**, identifies the mover by color, and never attributes achievement, blame, or private purpose to the Player. When Review Side is both, comments also use mover-color wording rather than treating both sides as the Player.
- The **Grounding Ledger** uses kind-aware factual claims: played move and outcome for every variant; grade, achievement, difficulty, and optional supported takeaway for a Positive Highlight; consequence, better move, conditional refutation or mechanism, and decision cue for an Improvement Opportunity; Neutral Review Reasons plus verified observations for neutral; and, for every variant, the optional **played-move popularity** claim that `{playedPopularity}` asserts. Intent claims remain separate; free-form selection reason and generic causal-explanation claims are absent.
- The **Grounding Gate** rejects a **Review Moment Comment** that omits required hypothesis uncertainty, asserts a hypothesis as authoritative **Move Intent**, adds a second intent, lets intent establish the moment's classification, omits a required factual claim or asserts one the facts do not support, uses an unknown or repeated **Slot Marker**, writes any evaluation, percentage, or probability of its own, changes Learning Track or Learning Resource literals or URLs, introduces a chess literal outside the **Chess Literal Projection**, exposes internal references, headings, or the name of the Human Move Model, or spans multiple paragraphs. The Player-facing phrasing for the Human Move Model is "players at your rating"; `maia`, `human model`, `move model`, and `human-likely` are rejections rather than style misses. It also rejects any **internal identifier** — a machine spelling the facts carry as reasoning input, recognized by shape rather than by an enumerated list: a lowercase letter immediately followed by an uppercase one, outside a URL. The facts and the Player-facing note are two vocabularies, and no enum spelling crosses from one to the other; a fact the Player is meant to hear earns a **Slot Marker** instead. Rejected prose never reaches the Player. Semantic explanation quality remains separate evaluation work, and an invented claim around correctly rendered facts is exactly what it cannot catch.
- After a Grounding Gate failure, the pipeline retries the same **Explainer Candidate** once with identical facts and the same **Intent Enrichment** (or its absence). A second failure uses **Safe Review Moment Rendering**; neither failure removes required content, substitutes a weaker played-move-only comment, or switches to an unpinned model.
- Session start prepares every Automatic Critical Moment once with runtime-bounded concurrency and restores strict Game order before returning. The web application and Coach App may defer prose authoring until presentation, while the Coach Skill drafts and validates the complete ordered set without issuing per-moment preparation calls.
- A **Review Moment Comment** treats its **Coach Intent Hypothesis** as an uncertain conversation opener, never as authoritative **Move Intent**. The product has no confirm, correct, skip, clarification, or **Intent Assessment** controls.
- A **Review Moment Comment** communicates hypothesis uncertainty in words and never displays a numeric or categorical confidence level.
- A **Review Moment Comment** offers at most one **Coach Intent Hypothesis**. When **Intent Enrichment** is present, that sentence uses only the selected **Projected Plan** SAN. When enrichment is absent, the Language Layer may infer one reasonable, explicitly uncertain possibility from the played move and grounded facts.
- A **Positive Highlight** remains qualified when its achievement lacks a supported **Teaching Theme**, **Opening Principle**, or eligible **Learning Track**. Its **Critical Moment Comment** explains the grounded achievement and difficulty, includes a reusable takeaway only when validated teaching support exists, and otherwise omits the takeaway rather than inventing one.
- A **Positive Highlight** derives difficulty wording from its **Positive Highlight Qualification** rather than carrying a duplicate difficulty field. Objective reasons support precision or conversion claims but not human rarity; Elo-relative reasons support notable or strong achievement at the resolved **Elo Profile**. Raw rank and probability remain evidence and need not appear in the comment.
- Every **Improvement Opportunity** has one **Improvement Correction**. An analyzed correction may include a validated first refutation and Tactical Mechanism; a terminal correction explains the verified terminal result and missed alternative without a refutation or post-move evaluation. A reviewed move without a grounded better move cannot be classified as an Improvement Opportunity.
- Every Improvement Opportunity **Critical Moment Comment** ends with a concrete reusable decision cue derived from its **Improvement Correction**. A separate **Teaching Theme**, **Opening Principle**, or **Learning Track** is optional enrichment rather than another qualification gate.
- **Intent Enrichment** is not a policy version, confidence threshold, or accuracy claim. Candidates, probabilities, leaf scores, and traces stay inside the authoring attempt and never reach the Language Layer or a Quality Capture as intent-accuracy labels.
- A **Review Evidence Packet** supplies shared Position and Engine Analysis evidence for comments and **Alternative Move Assessments**. It does not store Intent Selection Traces or Player-authored **Move Intent**.
- An **Alternative Move Exploration** contains independently validated **Alternative Moves** and may branch from any Position already reached in the exploration.
- An **Alternative Move Exploration** does not inherit **Move Intent** or intent-fit fields. It applies no intent to an opponent move unless the Player explicitly supplies a hypothetical opponent intent.
- Every **Alternative Move** receives a synchronous **Alternative Move Evaluation** before the Player continues the line.
- A **Coaching Board** lives on its own path. **Review Session** cannot enter it. `ReviewSessionWorkspace` is not a v1 call site. v1 does not mount **ConversationPanel**.
- A **Coaching Board** registers its tools from one hook, called on every Coaching Board surface — lobby, game board, and opening board — only after **Beta Access** authorizes a **Player**, and retracts them when that authorization ends. It never registers at module load, and never while identity is loading, signed out, unverified, or unauthorized. Sign-in and beta-admission pages register session-status only, never board tools. Anonymous page visits are allowed; tools still require **Beta Access**.
- Anonymous staging of the lobby import form is capped at ten attempts per rolling hour per client. A Sign-in refusal for the durable Game import does not spend that allowance. The unused opening-analysis allowance is retired until an anonymous analysis route exists.
- A **Coaching Board** over a **Game Import** is a **Review Session** whose Language Layer is the host agent rather than the pinned web Language Layer. Its exploration is that session's **Alternative Move Exploration**, and the distinction between a **Move Sequence** and a **Player Line** is unchanged by who is asking.
- A **Coaching Board Snapshot** is a board-tool result: it belongs to a game or opening origin. Lobby import and find return `kind: "lobby"` plus constraints, not a snapshot; the lobby has no **Review Moment** or **Opening Line** origin.
- A **Coaching Board** shows only Positions ChenChess grounds: a ply of its **Game Import**, a node of its **Alternative Move Exploration**, or an **Opening Line** and lines evaluated from one. An agent may point at a line ChenChess established or one already evaluated; it never puts an unevaluated Position on the board.
- **Opening Line** evaluation holds no session state, is bounded to the same twelve plies as a **Player Line**, and is rate-limited per **Player** because no **Review Session** allowance scopes it. Its results land in the **Opening Analysis Cache**.
- Opening **offer** — empty-state hints — names only openings the **Player** has played. A **Player** with no imported **Game** is offered no openings at all. That is “names no opening it cannot attribute”: it governs offer, not find or open.
- Typed **find** of an **Opening Line** returns **Opening Catalog** rows that already match the query. Played matches rank first; unplayed matches are allowed. A played opening never surfaces for a query it does not match. Analyzing a found line is not Player-scoped: nothing about who searched, or what they have played, reaches the **Opening Analysis Cache**.
- Opening a path-identified **Opening Line** is navigation. Find is ranked; open does not re-rank and does not refuse an unplayed catalog path.
- A **Player**'s played openings are known only as an ECO code and name, so one played opening ranks every **Opening Line** sharing that pair, and resolves to the shortest move path among them. It is a ranking signal for offer and find, never an identification.
- A **Coaching Board** carries its grounding policy in tool descriptions and tool results, because the surface has no instructions channel. The **Coach App** receives the same policy once, at initialization.
- A **Player** has at most one active **Coach Turn** per **Game Import**, across every Review Moment and every Review Session over that imported Game. Navigation alone does not cancel it; a new Player message steers an active turn by cancelling it completely before its replacement starts, while Stockfish-only **Alternative Move Exploration** remains independent. Two conversations reviewing one Game therefore contend for the same turn; the second is refused while the first is in flight.
- A steering message preserves its active **Coach Turn** target by default. An explicitly addressed Review Moment or attached Alternative Move retargets the replacement turn.
- Every admitted **Coach Turn** Language Layer invocation and agent-authored coaching validation receives a complete **Coach Turn Context** by value. Low-level deterministic evidence operations receive only the typed facts and references they require. The Coach App's ordinary-chat Alternative Move handoff is deliberately different: it pins only the exact safe objective evaluation and strongest reply. Elo-aware findability and authored-assessment quality capture are accepted losses while the Coach Turn tools remain withdrawn.
- A **Coach Turn** blocks further coach interaction until it returns one complete **Alternative Move Assessment**, an unavailable outcome, or cancellation. V1 does not invoke the **Human Move Model** for every explored move.
- An **Alternative Move Assessment** evaluates only the Alternative Move targeted by its **Coach Turn**; ancestor moves provide branch context but are not reassessed.
- Repeating a **Coach Turn** for one **Alternative Move** appends a new immutable **Alternative Move Assessment**. The newest assessment is active while earlier assessments remain in the transient Review Session history.
- An **Alternative Move Assessment** compares its selected move with the strongest move found by **Engine Analysis** from the same Position. The move played in the imported **Game** is an additional comparison only at the exploration root.
- An **Alternative Move Assessment** reports Elo-aware practical fit through both same-Elo move findability and resilience against human-likely peer replies. Neither replaces the separate **Engine Analysis** verdict.
- An **Alternative Move Assessment** offers the **Objective Refutation** as its default continuation and keeps a **Human Move Model** cohort as separate evidence about likely peer replies. The Player may instead choose any legal reply.
- An **Opening Identification** provides context for a **Game Review** but does not by itself explain a mistake.
- A v1 **Lichess Game URL Import** may populate **Opening Identification** from Lichess export metadata. Its opening ply remains internal provenance and does not establish where the **Game** left theory.
- A **Lichess Game URL Import** does not produce **Opening Database Context**.
- **Opening Database Context** may describe common play but cannot override **Engine Analysis** or make rarity a mistake.
- The **Rule Extractor** may attach **Teaching Themes** and **Opening Principles** to a **Critical Moment** only when its position-specific evidence establishes them. Neither is inferred from **Opening Identification**.
- The **Game Review Engine** constructs the frozen **Learning Plan** from Automatic Critical Moments whose selected **Explanation Paths** or exact **Opening Resource Mappings** project verified **Learning Resources** under one immutable selection-policy version and **Learning Resource Catalog** version. A **Language Layer** may explain selected material but cannot select, rank, replace, add, browse for, or author its tracks, resources, or URLs.
- A **Learning Track** aggregates every qualifying Automatic Critical Moment with the same **Learning Track Key**. Its nonempty, Game-ordered **Learning Track Support** preserves each exact ply and selected **Explanation Path** or exact **Opening Resource Mapping**; a track is improvement when any support is improvement and reinforcement otherwise.
- A chess-concept candidate is eligible only when one selected, candidate-owned **Explanation Path** contains valid **Concept Validation Proof**, a nonempty **Semantic Outcome**, and a resolvable **Learning Resource** mapping. Opening eligibility remains independently grounded by exact **Opening Resource Mapping**. Path-local failure or an unavailable mapping abstains with diagnostics; dangling support, duplicate keys, mixed versions, malformed proof, or an invalid selected plan fails Game Review construction rather than becoming an empty plan.
- Each Automatic Critical Moment projects at most two Learning Tracks from its selected **Explanation Paths** and independent opening mapping. The **Learning Plan** is the canonically ordered union of those moment-selected tracks, aggregates support with the same key in Game order, and has no separate Game-level two-track cap. A rejected proof match or unselected path never enters the plan.
- Every Review Moment receives fully materialized **Review Moment Learning Material**. An Automatic moment receives exactly the subset of frozen Learning Tracks it supports; a Player-Selected Critical Moment receives an on-demand, session-local zero-to-two-track projection from frozen SinglePV evidence without a new engine request; a neutral Player-Selected Moment receives none.
- **Review Moment Comment Facts** expose only the active moment's selected Learning Tracks to a **Language Layer**, never the rejected candidate pool, wider catalog, or selection trace. Any authored learning claim must pass the **Grounding Gate**, while delivery surfaces render canonical Learning Resource URLs directly from typed data.
- A full **Game Review** selects automatic **Critical Moments** only from its **Review Side**.
- A **Player** can nominate a **Player-Selected Moment** for on-demand review.
- A **Player-Selected Moment** may belong to either side regardless of the full Game Review's **Review Side**.
- Every **Player-Selected Moment** receives a Game Review-scoped **Critical Moment Classification** as a **Positive Highlight**, **Improvement Opportunity**, or neutral.
- Every **Player-Selected Moment** receives a **Review Moment Comment** through the same comment-authoring path; its classification determines whether that comment is a **Critical Moment Comment** or **Neutral Review Moment Comment**.
- A neutral classification is never admitted by the **Critical Moment Selector** and never appears among a **Game Review**'s automatic Critical Moments.
- A neutral classification carries one or more **Neutral Review Reasons**. Missing, contradictory, or invalid evidence fails classification rather than producing a neutral reason; comments do not expose thresholds, probabilities, or failed-gate lists.
- Starting a **Review Session** is navigation-neutral even though it prepares all Automatic Critical Moments. A delivery surface may display any prepared Automatic Critical Moment or explicitly open a legal Player-selected Game ply afterward.
- **Player-Selected Moments** outlive the conversation that opened them, and are durable in two places for two reasons. Every eligible ply is classified once at import and stored on the Player-owned **Game Import Record**, which has no expiry — that is what makes any ply of a review openable years later. Opening one materializes its prepared analysis into the identity-free **Review Analysis Cache** at the review key, so reopening the same ply — in this chat, another chat, or on the web — resolves the same moment without preparing it again. Neither is review history: the record stores what the pipeline classified and the cache stores what the engine computed, and the cache entry never records which Player asked. Both stay outside evaluation input.
- A **Review Feedback Report** targets exactly one ply and preserves full **Provider Evidence** for that ply and the originally selected Critical Moments.
- A producing client may retain a **Report Digest** and preparation time to warn about repeated local preparation, but the Player can override the warning.
- A **Central Host** runs the same product as a self-hosted deployment, but is operated as a managed service.
- The pipeline is designed by **Elo Profile**; child-friendly language belongs to **Explanation Style**, not to a separate skill model.
- **Explanation Style** is one of simple, standard, or advanced and is not chosen afresh per **Game Review**. In v1 it is fixed in the compiled prompt for every Player; it comes from the Player's **Coaching Preferences** only once those ship.
- A **Coaching Profile** is exactly one per **Player**. Its design splits into a **Coaching Preferences** half the Player writes and a **Coaching Signal Profile** half only deterministic signals write, written independently and never contending — but v1 ships the signal half alone, so the Player-authored half has no record and no writer.
- The **Personalization Preference** is on by default, disclosed rather than gated, and lives beside the **Quality Capture Preference** in account settings without being it. Turning it off halts both the **Coaching Profile Projection** and every **Coaching Signal Profile** write.
- An **Elo Profile** is resolved per **Game Review** from imported metadata for the selected **Review Side** and is never stored on a **Coaching Profile**.
- A **Coaching Signal Profile** reads **Learning Track Keys** produced by the **Learning Plan** but never creates, orders, or mutates **Learning Tracks**.
- A **Language Layer** receives only a **Coaching Profile Projection**, never the stored **Coaching Profile**. It has no write path to a **Coaching Profile** and may not propose changes to one.
- **Coaching Profile Projection** values are recorded with a captured **Coaching Response** so the generation remains reproducible; they contain no Player identity.
- Clearing a **Coaching Signal Profile** does not prevent it from re-accumulating; only the **Personalization Preference** stops accumulation and projection.
- A **Coaching Profile Projection** may change register, length, and emphasis order in authored prose. It never changes a **Critical Moment Classification**, an **Improvement Correction**, a required evidence citation set, the **Grounding Ledger** fact set, or an **Out-of-Scope Coach Turn** decision.
- The web surface has exactly two **Language Layer Tasks**: **Review Moment Comment** authoring and **HostTurn**. **Intent Enrichment** (when present) enters the first as input. A **Learning Plan** selection is never a Language Layer Task. Coach App and local surfaces still author **Alternative Move Assessments** as a **Coach Turn**.
- Every hosted Language Layer call carries one **Language Layer Task Contract**. Both web tasks share that envelope and keep their own typed input, response schema, **Grounding Gate** rule, and fallback.
- A hosted Language Layer binds only in the web runtime composition. The **Coach MCP Server**, **Coach Skill**, and **Coach App** compositions keep no hosted model provider, and a **Language Layer Task Contract** records its **Delivery Surface**.
- A **Review Moment Comment** is authored when its Review Moment is first opened, published once, and then frozen. A later **Coaching Preferences** change affects later comments only and never re-authors a published one.
- A failed authoring attempt retries exactly once with byte-identical input inside one task deadline. A **Review Moment Comment** then degrades to deterministic safe rendering; a **HostTurn** or **Coach Turn** degrades to a typed unavailable outcome rather than synthesized prose.
- A **HostTurn** carries the last four turns as prose. Prior capability results never re-enter; the on-screen branch does. A **Coach Turn** may carry the immediately prior Player message and published **Alternative Move Assessment** only when it steers that same **Alternative Move**.
- **Pin Verification** records whether the observed model or provider route contradicts the pinned generation contract. The attempt stays billed and published when the completion and Grounding Gate succeed. The observed identity is recorded alongside the pinned one.
- A pinned provider route is admissible only when its counterparty's abuse-monitoring retention resolves to a stated maximum duration: an operative contract clause must govern retention and resolve, directly or through a document it references, to a published ceiling for that exact route. An undisclosed window is an absent commitment rather than an unknown quantity and disqualifies the counterparty, regardless of how strong its training terms are. The rule is no **Evaluation Fingerprint** axis. It is not encoded in the v1 pin; ToS / #340 revisit is post-v1 on #438.
- Where a counterparty protection cannot be verified from primary sources, the reading assumes the least favorable admissible branch. A favorable reading of an unverifiable is never treated as a fact, and only evidence improves the reading. v1 does not record that reading on the pin; ToS / #340 revisit is post-v1 on #438.
- Vendor-documentation guards are gone. The pin record does not carry a counterparty attestation, a vendor page URL, a read date, or a page digest. No code path reads, fetches, or digests a vendor documentation page, and no test or release gate asserts those values. The runtime asserts the account posture and the pinned endpoint's ZDR listing — facts an API answers about the route actually served. Digesting a vendor's documentation page detected vendor edits rather than term changes, and terms held in a governing agreement can move without that page moving at all.
- Chen Chess Coach makes Players no unqualified zero-retention claim about the hosted **Language Layer**. The permitted claim is that no inputs or outputs train a model and that none are retained beyond bounded provider abuse monitoring, tightening to genuine zero retention only where the pinned route's counterparty terms establish it. The Player-facing wording names chess facts, the current message, and ChenChess-generated coaching context; it names neither a provider nor a cloud and does not state a numeric retention window. Because the assurance is transitive and unobservable at serve time, the wording carries it: the claim is worded to hold across the recorded terms rather than to quote them, and it is stated in Player-facing words rather than pin vocabulary. It lives on the public privacy page and appears in account settings beside the **Quality Capture Preference**. Account Settings owns that toggle; first-run is disclosure + acknowledge, without a second first-run sheet.
- An **Alternative Move Assessment** cites exactly its required evidence references, and each dimension's prose passes the **Grounding Gate** with its own **Slot Marker** vocabulary and **Chess Literal Projection**, both derived from the evidence packet: a dimension may only name what it cites, so findability cannot state the resulting evaluation. Authored output containing a URL is rejected whole. A rejection takes the whole Coach Turn rather than retrying one dimension — a Coach Turn degrades to unavailable where a comment degrades to Safe Review Moment Rendering, and a turn assembled from two generations would have no single identity to fingerprint.
- The MVP **Game Review** must use the full pipeline, not fallback-only architecture.
- **Maia** is a **Human Move Model**, not the source of **Engine Analysis**.
- **Engine Analysis** and **Human Move Model** providers are accessed through **Model Adapters**.
- A **Pipeline Evaluation** measures deterministic chess facts independently from the active coding agent's prose.
- **Dataset Admission** occurs only when a feedback fixture change merges into a repository-owned evaluation dataset.
- Withdrawing an artifact that has undergone **Dataset Admission** removes its Player-derived payload and leaves a **Dataset Tombstone**; future dataset revisions and experiment runs must exclude it.
- A completed experiment run that used a subsequently tombstoned case deletes that case's inputs and outputs, retains only aggregate metrics and non-content audit metadata, and is marked affected. An affected run remains historical evidence but cannot support a future promotion.
- **Frozen Reproduction** must match before an agent prepares a feedback fixture; a live provider comparison is optional evidence.
- Explanation feedback admitted through **Dataset Admission** becomes an **Explanation Evaluation Dataset** case. An experiment replays the full **LLM Explainer** call but judges quality at the **Review Feedback Report** target ply.
- Each **Explanation Evaluation Dataset** case derives its input from an immutable **Quality Capture** and its **Frozen Reproduction**, reached either through a **Review Feedback Report** or by sampling, and records that **Admission Source**. It does not introduce a hand-authored reference Game Review.
- A capture whose **Language Layer Attestation** is `unattested` cannot be **Frozen Reproduced** and is therefore ineligible for **Dataset Admission** and for standing as an **Explainer Candidate**. It may only serve as a labelled baseline cohort.
- An **LLM Judge** treats Rule Extraction facts and **Provider Evidence** as authoritative for chess claims and **Move Intent** as authoritative only for the Player's stated purpose. Feedback prose identifies what to inspect but does not establish chess truth, and the reported explanation is a baseline rather than a reference answer.
- An **LLM Judge** compares one gate-passing production baseline with one gate-passing challenger under opaque, randomly ordered candidate labels. It sees the reported failure and case evidence but not candidate identity, model, prompt, provider, revision, or production status; the runner deblinds only after judgment.
- An **LLM Judge** returns structured semantic-grounding, feedback-resolution, Player-alignment, and coaching-usefulness judgments plus an overall preference, confidence, rationale, and evidence references. A semantic-grounding failure makes an Explainer Candidate ineligible to win regardless of its other qualities.
- An **LLM Judge** returns `tie` only when case evidence supports equivalent quality and `insufficient-evidence` when the evidence cannot support a preference. Insufficient judgments are not retried or counted as decisive preferences; technical failures may receive a bounded identical-input retry and remain experiment errors if they persist.
- A **Judge Contract** also records content hashes and request provenance. Any change creates a new Judge Contract, and results from different Judge Contracts are not pooled without fresh calibration.
- A **Judge Calibration Set** includes human verdicts for clear preferences, ties, insufficient evidence, grounding failures, Move Intent misunderstandings, Elo Profile and Explanation Style mismatches, and reversed candidate order. Prompt examples are kept separate, and every new **Judge Contract** must pass calibration before use.
- A **Judge Contract** should use a model family independent from both compared Explainer Candidates when practical, but promotion does not require model-family independence. Full provenance, calibration, blinding, and Human Audit remain mandatory.
- Any prompt, model revision, generation-setting, response-schema, Grounding Ledger-schema, or code-revision change creates a new **Explainer Candidate**. Experiments disable fallback generation.
- An **Explanation Experiment Run** pins its dataset revision and cases, Explainer Candidates, LLM Judge contract, trial count, and randomness policy. It preserves generated outputs, Grounding Gate results, Judge verdicts, and metrics; a rerun creates a new record.
- An **Explanation Experiment Run** reports insufficient judgments as coverage and requires enough decisive cases for promotion rather than adding them to the preference-rate denominator.
- An **Explanation Experiment Run** uses the case as its primary metric unit and reports gate pass rates, audited wins, losses, ties, insufficient outcomes, decisive coverage, paired preference uncertainty, trial and order stability, Judge-to-human agreement, and feedback-reason, Elo Profile, and Explanation Style slices. Trial-level metrics are diagnostic and never give one case extra weight.
- A promotion-bound **Explanation Experiment Run** requires a **Human Audit** of every case and trial under the same rubric before candidate identity or LLM Judge verdicts are revealed. Human verdicts are authoritative and preserve disagreements and gate failures; they never rewrite the run or dataset. Unaudited exploratory runs cannot support promotion.
- A human-confirmed semantic-grounding regression blocks promotion regardless of an Explainer Candidate's aggregate preference.
- **Explainer Promotion** requires a separate human-approved change that binds the exact Explainer Candidate to its full eligible dataset revision, Judge calibration, immutable Explanation Experiment Run, Grounding Gate results, Human Audit, case-level metrics, uncertainty, and rollback target. Automation prepares and verifies the evidence but never promotes a candidate.
- **Explainer Promotion** changes only the LLM Explainer generation contract. It never changes Rule Extraction facts, Selector Policy, or Selector Weights.

## Example dialogue

> **Dev:** "When a Player imports a Game, what is durable?"
> **Domain expert:** "The **Game Import Record**: identity, Imported Game, frozen Game Review, and optional engine provenance. There is no Saved Game and no Review Session Checkpoint."
> **Dev:** "Does the Review Engine sign the Player in?"
> **Domain expert:** "No. Firebase Authentication signs the Player in, Coach OAuth issues host-specific access when required, and the Coach Engine validates the resulting Auth Token."

## Flagged ambiguities

- "PNG format" was used to describe chess imports — resolved: **Game** imports use PGN, not PNG.
- "Analysis" was used for the generated output — resolved: the player-facing output is a **Game Review**.
- "Player Session" was used for Coach Engine-issued HTTP-only cookies — resolved: Central Host surfaces send an **Auth Token** whose `sub` identifies the same Firebase Player.
