# ChenChess beta release readiness

Assessment date: 2026-08-02

This assessment combines:

- the
  [shared product research chat](https://chatgpt.com/s/t_6a6f52a7313081919ddbfe27e7afab88);
- the sourced
  [community-needs research](./chenchess-beta-community-needs.md);
- the current repository implementation and accepted architecture decisions;
  and
- the issue pipeline in the private development repository.

It evaluates release readiness. It does not implement the missing work or
change the issue tracker.

## Decision

**ChenChess is not yet ready for a chess-community beta.** It is close
to an operationally deployable invite-only staging environment, but the web
product does not yet complete its defining coaching job.

The first community release should be narrower than "AI coach for every kind of
chess participant":

> An adult Player imports one completed game, discusses a few critical
> decisions, and leaves with one grounded lesson and one practical next step.

The approximately 800-1200 Elo casual self-learner should be the primary beta
user. Advanced players and adult/junior coaches should initially be
correctness, pedagogy, and oversight reviewers. Commentators should initially
review post-game clarity and visual communication. Direct child accounts and a
live-broadcast product should not be part of this beta.

The current code has a strong chess-truth spine, but the differentiating
learning loop is incomplete:

1. the web composer accepts an ordinary plan or follow-up but produces no coach
   response;
2. the deployed web runtime has no hosted Language Layer;
3. the review reveals a factual answer before learning what the Player thought;
4. the Game summary is operational telemetry rather than a lesson; and
5. the current practice surface offers a very small eligible-resource list
   rather than one coherent recommendation.

Completing only the remaining staging ticket would therefore prove that users
can enter the product, not that the product coaches them.

## Readiness by track

| Track                                      | Current state                                                                                                                                                     | Release interpretation                                              |
| ------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| Chess facts and safety                     | Strong deterministic foundation: completed-game validation, legal board state, Stockfish, Maia, typed causal facts, Grounding Ledger, and fail-closed publication | Preserve this architecture                                          |
| Review exploration                         | Working board, move navigation, evaluation graph, chronological Review Moments, Player-selected moments, Alternative Move exploration, and resumable sessions     | Credible beta foundation                                            |
| Web coaching conversation                  | The normal composer records only the Player message; branch coaching calls a runtime whose hosted Language Layer is deliberately unavailable                      | Blocking                                                            |
| Explanation quality                        | ChatGPT/Claude can use their host model; web renders deterministic fallback prose when no canonical comment exists                                                | Blocking if Web remains a promised beta surface                     |
| Lesson closure                             | Technical count summary plus a three-resource practice allowlist; no single synthesized lesson                                                                    | Blocking for the product promise                                    |
| Coaching-quality evidence                  | Deterministic and heuristic benchmark machinery exists, but the semantic human-coach evaluation is unfinished                                                     | Blocking as a release gate, not necessarily as an algorithm rewrite |
| Beta admission and privacy                 | Twelve implementation tickets under the beta spec are closed; the real staging rollout remains open                                                               | Near ready                                                          |
| Public traffic protection                  | Player, public OAuth/MCP, WAF, and layered staging certification tickets are open                                                                                 | Blocking before a broader community invite wave                     |
| Children                                   | The old PRD mentions child Players while explicitly excluding age collection; current terms contain no adult-only or guardian boundary                            | Release adult-only                                                  |
| Advanced, coach, and commentator workflows | No PGN export/editorial workflow, student permissions, classroom, or broadcast workflow                                                                           | Treat these personas as evaluators, not promised beta users         |

## What is already good enough to build on

### Grounded chess authority

The accepted design correctly separates deterministic chess evidence from
surface-specific prose. ADR 0019 requires Web, Coach Skill, and Coach App to
share the fact boundary, Grounding Gate, failure policy, and publication rule
without requiring byte-identical writing
([ADR 0019](../adr/0019-author-kind-aware-review-moment-comments.md#surface-publication-boundary)).

The implementation follows the important parts of that design:

- Game Import rejects ongoing or unsupported games and validates the completed
  result
  ([`game_eligibility.rs`](../../services/coach-engine/src/game_eligibility.rs));
- Elo maps to beginner, intermediate, or advanced coaching focus
  ([`types.rs`](../../services/coach-engine/src/types.rs#L56));
- automatic Game Review facts are created without trusting generated prose
  ([`game_review.rs`](../../services/coach-engine/src/review_facts/game_review.rs#L207));
- Review Moment publication validates the supplied Grounding Ledger
  ([ADR 0019](../adr/0019-author-kind-aware-review-moment-comments.md#kind-aware-grounding-ledger-and-grounding-gate));
  and
- the Coach App's `discuss_review_moment` flow obtains host-authored prose and
  publishes only admitted output
  (`coach-app-review-moment.ts:129` — historical; that file was renamed after this was written).

This is a real differentiator. It should not be weakened by moving chess truth
into a prompt or by letting generated conversation mutate canonical facts.

### Useful review mechanics

The current web journey already provides much of the V1 interaction shell:

- natural-language-like import of a completed Chess.com game, Lichess game, or
  pasted PGN;
- automatic and Player-selected Review Moments;
- full move navigation, board state, evaluation graph, and opening context;
- Alternative Move evaluation and strongest-reply preview;
- chronological moment navigation; and
- resumable transient Review Sessions.

These are appropriate foundations for the single-game beta. They are also
enough for advanced players, coaches, and commentators to inspect the product
as reviewers without building their complete professional workflows.

### Operational beta work

The beta issue map has delivered its identity, invitation, access, installation,
retention, legal-page, and administration slices. Of its implementation
children, only
[#216, Provision and certify the staging rollout](#216)
remains open.

That is substantial progress, but
[#203, Launch the invite-only beta](#203)
mostly specifies admission and operation. Its staging smoke proves access to
Web, ChatGPT, and Claude; it does not require a Player to complete a useful
coaching conversation or remember a lesson.

## Release-blocking functionality

### P0. Make the web conversation real

The user-facing web control is currently misleading.

`ConversationPanel` labels the area "Review conversation" and invites the
Player to "Describe your plan or ask a follow-up"
([`ConversationPanel.tsx`](../../apps/central-host/src/review-session/ConversationPanel.tsx#L43)).
For a normal discussion, `sendMessage` appends the Player's message and does
nothing else. Only Alternative Move branch messages call `startCoachTurn`
([`ReviewSessionWorkspace.tsx`](../../apps/central-host/src/review-session/ReviewSessionWorkspace.tsx#L395)).

The test suite locks in that incomplete behavior: after the Player explains a
plan, the only commands are `importGame` and `startReviewSession`
([`App.test.tsx`](../../apps/central-host/src/App.test.tsx#L614)).

Branch coaching is not a production workaround. The live Coach Engine runtime
installs `NoHostedLanguageLayer`, which always returns
`ProviderUnavailableReason::LanguageLayer`
([`review_session_runtime.rs`](../../services/coach-engine/src/review_session_runtime.rs#L54)).
The web integration test supplies a mocked completed Coach Turn, so it does not
prove that the deployed provider exists.

The beta needs an authenticated, server-side Web Language Layer that can:

1. author and publish the initial Review Moment Comment from the existing
   bounded authoring context;
2. respond to an ordinary Player plan or follow-up;
3. invoke the existing stateless Player Plan Evaluation only when objective
   counterplay would materially help;
4. keep noncanonical chat transient; and
5. submit every concrete chess claim through the existing grounding/admission
   boundary before it becomes canonical UI content.

Do not put a provider key in the browser, move factual authority into Node, or
reuse `publishCoachTurn` for unrelated root discussion. ADR 0019 deliberately
keeps Alternative Move Assessment publication specific. The shared component
should be the prepared facts and admission contract, not a universal prompt or
model runtime.

**Release test:** sending a root-level plan in the real Web composition produces
a Player-appropriate coach response, the response's chess claims are grounded,
failure has an honest recovery state, and ChatGPT/Claude/Web agree on the
underlying facts.

### P0. Change the learning sequence from answer-first to conversation-first

The current web view renders `openingText` as a coach message immediately
([`ConversationPanel.tsx`](../../apps/central-host/src/review-session/ConversationPanel.tsx#L56)).
When no canonical Language Layer comment exists, the browser constructs prose
such as "Improvement ... the better move was ..." from deterministic facts
([`reviewMoments.ts`](../../apps/central-host/src/review-session/reviewMoments.ts#L71)).

This is safe as a fallback, but it reveals the conclusion before the Player
describes what they saw. That undermines the research-chat promise to
understand the Player's thinking and creates hindsight bias for stronger
players.

For one focal moment, the beta should:

1. show the pre-decision position without revealing the correction;
2. ask what the Player noticed, feared, expected, or calculated;
3. let the Player answer, skip, or request the explanation;
4. reveal the grounded explanation and compare a plausible alternative; and
5. let the Player retry the decision before showing the best line.

This should be a small interaction state around a Review Moment, not the
superseded persistent Move Intent lifecycle. ADR 0026 explicitly keeps Player
intent as conversational context and removes confirmation, correction,
clarification, and durable intent state
([ADR 0026](../adr/0026-replace-move-intent-lifecycle-with-ephemeral-enrichment.md#decision)).

**Release test:** a first-time Player can complete the prompt -> answer/skip ->
grounded explanation -> retry flow without seeing the answer early.

### P0. End every useful review with one lesson and one practice action

The current `GameReview.summary` is:

> Analyzed N plies and selected M Critical Moments for Elo E.

That is useful diagnostic metadata, not a coaching summary
([`rule_extractor.rs`](../../services/coach-engine/src/rule_extractor.rs#L216)).

The practice system is equally incomplete as a learning conclusion:

- its pinned catalog has only Checkmate Patterns I, Piece Checkmates I, and Key
  Squares
  (`practice.rs:8` — historical; that module was removed after this was written);
- the web surface displays every eligible item under "Eligible lessons" rather
  than selecting one action
  (`PracticeRecommendation.tsx:16` — historical; that component was removed after this was written);
  and
- the stated Gotham baseline in
  #127 is 75 of 142 reviews
  producing no eligible lesson.

The Learning Plan issue map is pointed in the right direction:

- #135 still needs the
  Player-facing prototype;
- #136 still needs the
  typed contract and domain decision; and
- no implementation issue exists after those design tickets.

For beta, keep this smaller than longitudinal learning:

- select zero or one primary grounded lesson, with a second track optional
  rather than required;
- cite the supporting moment or plies;
- explain one reusable concept in the selected language depth;
- give one verified learn-or-drill action; and
- distinguish an honest "no grounded recommendation" from a pipeline failure.

Do not add cross-game mastery, schedules, or spaced repetition yet.

**Release test:** the Player can state the intended lesson in their own words
immediately after the review, and the product can ask about recall in a later
research session without pretending that durable mastery exists.

### P0. Establish a human coaching-quality gate

The current Gotham workflow is valuable engineering evidence but not yet a
semantic coaching benchmark.

Its latest recorded run processed 142 games and selected 150 Critical Moments;
62 games selected none. Its keyword comparison matched 44 of 934 detected video
commentary moments within one ply. The issue correctly labels these as noisy
signals rather than a quality score
([#81 progress report](#81#issuecomment-5028673126)).

Zero selected moments can be an honest conservative result, and the product has
a Player-selected fallback. The numbers therefore do not by themselves prove
that the selector is bad. They do prove that a release claim cannot rest on
automated invariants alone.

Before the community invite wave, run task-based reviews with:

- at least five adult players around 800-1200;
- at least three advanced or strong club players;
- at least three coaches, including adult-beginner and junior/school coaching
  experience;
- one safeguarding lead or experienced club organizer; and
- two commentators, streamers, or club annotators using completed games.

Block release on any illegal line, board/prose contradiction, severe
mis-teaching, humiliating tone, cross-user data exposure, or live-game
assistance path. Track review completion, useful follow-ups, explanation
corrections, immediate lesson restatement, and delayed recall. Do not use rating
gain as the first beta gate.

### P0. Make the beta explicitly adult-only

The first PRD describes child Players while making age collection out of scope
([`0001-full-pipeline-game-review.md`](../prd/0001-full-pipeline-game-review.md#out-of-scope)).
The current staging Terms discuss fair play and coaching responsibility but do
not define an age or guardian boundary
(`apps/central-host/terms/index.html#L95`, the static page as it stood at the
time of writing; the Astro migration replaced it with
[`src/public/TermsPage.tsx`](../../apps/central-host/src/public/TermsPage.tsx)).

A community chess release will predictably reach minors. Adding simplified
wording is not a child-safety system. The sourced community research identifies
guardian consent, privacy, controlled communication, reporting, professional
boundaries, and safeguarding operations as a separate capability
([community-needs research](./chenchess-beta-community-needs.md#adult-first-release)).

The smallest honest beta decision is:

- access is for adults only;
- invitation and Terms make that explicit;
- junior coaches may evaluate with adult-owned or appropriately consented,
  de-identified games;
- there is no direct minor account or unmediated minor/AI chat; and
- junior release remains a separate, specialist-reviewed epic.

### P0. Complete public traffic protection and real staging evidence

Before a broad community invite wave, complete:

- [#185, per-Player Coach Engine limits](#185);
- [#186, public OAuth and unauthenticated MCP admission](#186);
- [#187, Cloudflare WAF and origin protection](#187);
  and
- [#188, layered staging certification](#188).

The first two are minimum application controls even for an invite-only product
because OAuth discovery and invalid MCP traffic remain public. The WAF and
layered certification should gate expanding beyond a very small supervised
cohort.

#216 should name the exact
coaching-quality and traffic evidence used for the invite wave, not only prove
identity, email, OAuth, and deployment.

## Persona-specific recommendation

| Persona                             | What the current beta can credibly offer                                                                   | Missing if marketed as a product for that persona                                                                 | Recommendation                                                                                      |
| ----------------------------------- | ---------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| Adult casual Player around 1000 Elo | Grounded completed-game review, useful board, Elo-aware facts, critical decisions, alternative exploration | Real dialogue, answer-before-reveal problem, one lesson, focused practice, quality evidence                       | Primary beta persona after P0                                                                       |
| Advanced Player                     | Full-game navigation, evaluations, Elo-aware Critical Moments, branches, exact lines                       | Concise/evidence mode, analysis provenance, annotations and PGN export, multiple candidate-line control           | Use as adversarial chess reviewer; do not compete with an analysis board in V1                      |
| Adult club coach                    | Fast critical-moment triage and inspectable evidence                                                       | Hide/reveal teaching flow, edit/approve/reject, annotations, export/share, student permission and isolation       | Use as pedagogy reviewer; build a small editorial/export workflow only after the learner loop works |
| Junior/school coach                 | Can evaluate tone and pedagogy using safe test games                                                       | Guardian system, child privacy, controlled communication, reporting, safeguarding operations, play-based pedagogy | Design partner only; no direct child beta                                                           |
| Commentator                         | Completed-game board, real line, alternatives, factual evidence                                            | Multi-game/live feed, delay, clocks, overlays, production layout, contributor controls                            | Test post-game clarity only; keep live broadcast out of scope                                       |

### Explanation depth

The code already derives beginner, intermediate, and advanced coaching focus
from Elo, but the original PRD's `simple`, `standard`, and `advanced`
Explanation Style is not present in the active web request
([PRD decision](../prd/0001-full-pipeline-game-review.md#implementation-decisions)).

Do not turn this into many personas or "coach personalities" before beta. A
reasonable first slice is:

- **guided**: plain language, short lines, one idea at a time; and
- **concise evidence**: chess terminology, exact lines, less explanatory
  scaffolding.

Elo can choose the default; the Player should be able to switch. Child-friendly
must not be treated as a synonym for guided.

## Refactoring priorities

### 1. Introduce a real Web Language Layer composition

The existing contracts already contain most of the deep module:

- Rust prepares authoritative Review Moment comment facts and a Grounding
  Ledger;
- Rust prepares stateless Player Plan Evaluation facts;
- Rust owns Alternative Move evidence and publication;
- Coach App orchestration demonstrates prepare -> host author -> admit; and
- surface prose is intentionally allowed to differ.

The missing module is Web composition. It should live behind the authenticated
server boundary and orchestrate these existing use cases with a configured
provider. Its interface should describe product operations such as authoring a
moment, evaluating a plan, and answering an addressed follow-up—not provider
chat-completion details.

Rust remains authoritative. Node owns protocol/provider adaptation. React owns
transient conversation presentation.

### 2. Separate root discussion from Alternative Move coaching

`startCoachTurn` and `publishCoachTurn` are specific to an Alternative Move
branch. Preserve that depth. Add or expose a root Review Moment discussion use
case that can prepare bounded context and admit grounded claims without
pretending every chat message is an Alternative Move assessment.

General conversational wording may remain transient. When a response needs
objective chess content, the Web Language Layer should explicitly call the
appropriate prepared-facts operation and use only the admitted result.

### 3. Split the web controller along use cases

`ReviewSessionWorkspace.tsx` is 745 lines and currently owns import/start,
resume, moment activation, navigation, board interaction, Alternative Move
exploration, branch coaching, message state, cancellation, retention, and
page composition.

After the conversation contract is fixed, split it into focused controllers or
hooks for:

- session lifecycle;
- Review Moment navigation and reveal state;
- root conversation;
- Alternative Move exploration and coaching; and
- presentation projection.

Do not begin with a broad UI rewrite. Extract along the new application seams
while adding the missing behavior and tests.

### 4. Separate diagnostic summary from learning conclusion

Keep analyzed-ply and selected-moment counts as diagnostics. Add a
Player-facing conclusion whose typed inputs are the selected lesson, supporting
moments, practice action, and honest no-plan reason.

The Language Layer may explain the conclusion but must not select, reorder, or
invent a learning target or resource.

### 5. Delay durable Player Memory

The shared chat's eventual memory idea is valuable only after the single-game
loop works. When it is introduced, keep four authorities distinct:

- Player-stated thought;
- deterministic chess fact;
- explicitly uncertain coach hypothesis; and
- Player- or human-coach-confirmed learning claim.

Generated prose must never write a psychological trait directly into durable
memory. ADR 0026's removal of durable Move Intent state is the correct beta
boundary.

## GitHub pipeline changes recommended

### Add a coaching-quality beta map

The current beta map owns deployment and admission. Add a small parent map for
the Player job, with implementation-ready children for:

1. **Implement the grounded Web Language Layer and real follow-up
   conversation.**
2. **Prototype and implement conversation-first reveal and retry.**
3. **Complete the single-Game Learning Plan and Player-facing conclusion.**
4. **Certify one cross-surface coaching journey with the community review
   panel.**
5. **Restrict the beta to adults and record the future junior-use gate.**

The first issue should explicitly cover the production provider composition;
the current UI tests can otherwise keep passing against a mocked coach while
the live runtime remains unavailable.

### Continue now

- #127,
  #135, and
  #136, narrowed to the
  beta's one-primary-lesson outcome;
- #81, through the semantic
  human-audit stage;
- [#184-188](#184), for bounded
  public traffic; and
- #216, after the
  Player-facing release journey and traffic gates are linked.

### Defer without blocking beta

- [#217, Daily Coaching](#217) and
  its design children: it is explicitly design-only, excludes a new
  LLM-enabled web conversation, and depends on the single-Game Learning Plan;
- [#14, Game Review feedback loop](#14)
  and its children while that map remains explicitly deferred;
- durable history, recurrence, spaced repetition, and mastery;
- full coach/classroom and broadcast products;
- production-region migration; and
- build-cache optimization.

### Triage stale work

Several open issues describe superseded architecture and can distort release
planning:

- #1,
  #11, and
  #12 still refer to the
  older Better Auth/Convex or original deployment shape;
- #93 still requires typed
  intent state and intent-lifecycle controls that ADR 0026 removed; and
- #91 may remain useful for
  future evaluation governance but should not block the community beta unless
  beta data is actually admitted to those datasets.

Close, rewrite, or relabel these; do not silently treat them as release gates.

## Suggested execution order

1. Declare the adult-first beta boundary and create the coaching-quality issue
   map.
2. Implement the Web Language Layer and root conversation, with real provider
   composition tests.
3. Add conversation-first reveal/retry and the one-lesson conclusion.
4. Finish the narrow Learning Plan contract and implementation.
5. Run the semantic corpus audit and moderated community-panel journeys.
6. Complete Player and public-protocol traffic admission.
7. Provision the real staging environment, add the protected custom domain,
   and run both the functional and layered traffic certifications.
8. Invite a small supervised adult cohort; expand only after the first review
   corrections and delayed-recall evidence are inspected.

## Community-beta go/no-go checklist

Release only when all are true:

- [ ] One completed supported game starts a review without operator help.
- [ ] The first focal moment asks for thought before revealing the correction,
      or the Player deliberately skips.
- [ ] A normal web follow-up receives a real coach response.
- [ ] Every concrete chess claim rendered as canonical is grounded and
      reproducible from the admitted position evidence.
- [ ] The Player can retry or compare a plausible move without losing the real
      game line.
- [ ] The review ends with one reusable lesson and one practice action, or an
      honest grounded no-recommendation result.
- [ ] Web, ChatGPT, and Claude preserve the same facts and terminal failure
      meaning.
- [ ] The human review corpus has no illegal line, board/prose contradiction,
      or severe pedagogical error.
- [ ] Adult-only access and the future junior gate are explicit.
- [ ] Per-Player and public protocol traffic limits are active.
- [ ] The custom-domain staging journey and rollback are certified at the exact
      release revision.
- [ ] The first cohort and operator support/incident process are named.

## Bottom line

The repository has done the hard foundational work unusually well: it knows
which layer owns chess truth, it validates generated claims, and it has nearly
completed a careful invite-only operating boundary.

The remaining risk is product-shaped. The current web experience can look like
a conversation while behaving like an analysis report with a text box. The beta
should not broaden into dashboards, classrooms, child accounts, or broadcasts
to compensate. Finish one grounded conversation, one active retry, and one
remembered lesson. Then use the chess community's advanced players, coaches,
and communicators to prove that the result is correct, teachable, and clear.
