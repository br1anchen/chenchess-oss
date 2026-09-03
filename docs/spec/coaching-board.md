# Coaching Board

## Status

v1 UI lock signed on #484
(2026-08-27). The lock wins over this document's original decisions 4 and 5
and over the original issue sentence that registered tools from the review
workspace.

This specification records the decisions from the Coaching Board grill completed
on 2026-08-26, following the research spike in
#479. ADR 0056 records the
surface commitment and ADR 0057 the opening root; this document supplies the
implementation contract, except where the signed lock supersedes it.

Tracked by #480 with eleven
children, listed under [Children](#children).

## Purpose

A Player studying a Game moves pieces with the mouse and asks questions by
dictating into ChatGPT. Those are two channels, and the language channel is full
of pointers the model cannot resolve — _why is this bad_, _what about that
instead_, _is the first one I tried still winning_. Every one refers to board
state ChatGPT never saw.

The **Coaching Board** closes that gap. The page registers tools with
`document.modelContext`; the agent calls them against the live page and the
Player's own session. It is told apart from the **Coach App** by carrying no
installation: no connector grant, no Beta Coach App Connection, no artifact set,
and no `validateOrigin` entry, because the calls are same-origin to the page that
registered them.

## Decisions (binding)

| #   | Decision                                                                                                                                                                                                                                                                                                                                                        |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Coaching Board is its own domain term, distinguished from Coach App by carrying no installation.                                                                                                                                                                                                                                                                |
| 2   | `/app/board` is a **lobby**, not a board. No Game Import means no grounded position, so it registers lobby reads and staging only: reviewed-game search, the recent-profile read, import staging, and opening find/open. Board-drive tools stay off it.                                                                                                         |
| 3   | The lobby has two exits with deliberately different consent. A Game import is **staged by the agent, committed by the Player** — it is a durable write behind a retention disclosure. An Opening Line, or a Game the Player already reviewed, is **opened directly by the agent** — it creates nothing and discloses nothing, so it is navigation (`open_opening_line`, `open_reviewed_game`; both also reachable as `set_board_position` targets from a board). |
| 4   | **Superseded by the signed v1 UI lock.** Coaching Board is an **own path**. Review Session cannot enter. There is no door, mode switch, or URL rewrite from a live Review Session. `ReviewSessionWorkspace` is not a v1 call site.                                                                                                                              |
| 5   | **Superseded by the signed v1 UI lock.** v1 **hides ConversationPanel**. Do not mount it. Do not keep-and-disable-input. Host-agent only — no web-host LLM session.                                                                                                                                                                                             |
| 6   | The board read returns a **complete snapshot**, no cursor. Exploration is already a retained tree, so there is nothing to journal and no way for a caller to under-read.                                                                                                                                                                                        |
| 7   | The revision is a **page revision** — monotonic for the life of the page, across moments, lines and origins. Equal revisions always mean nothing changed.                                                                                                                                                                                                       |
| 8   | The agent may drive the board only to **grounded positions**: a ply of the Game, a node of the exploration tree, or an Opening Line. Lines pass the existing evaluate-then-show gate.                                                                                                                                                                           |
| 9   | Opening study grounds on a **stateless, identity-free root**. No second aggregate, so ADR 0042's Review Session key stands.                                                                                                                                                                                                                                     |
| 10  | Off-book opening analysis is allowed, bounded by a twelve-ply cap and a per-Player rate limit.                                                                                                                                                                                                                                                                  |
| 11  | The web surface joins the one authored tool-target map rather than gaining a second.                                                                                                                                                                                                                                                                            |
| 12  | Grounding policy travels in **tool descriptions and tool results**, because WebMCP has no instructions channel.                                                                                                                                                                                                                                                 |
| 13  | Every **board** tool result (game or opening origin) carries the current Coaching Board Snapshot, not only the read tool's. Lobby import and find return `kind: "lobby"` plus constraints, not a snapshot; the lobby has no Review Moment or Opening Line origin.                                                                                               |
| 14  | Opening **offer** names only played openings (no imported Game → no offer). Typed **find** returns catalog rows that already match the query, played first, unplayed matches allowed. `open_opening_line` is path navigation and does not re-rank. Analyzing a line is not Player-scoped.                                                                       |
| 15  | `evaluate_opening_continuation` is the opening board's **evaluate-then-show gate** — web-only, rooted at the opened line's end, both sides supplied. Branches are minted in the page with deterministic ids, so the engine route and the generated contract are unchanged. Agent-callable without confirmation: it writes nothing durable and is bounded twice. |
| 16  | `annotate_board` is **verify-then-draw**, the sibling of evaluate-then-show (ADR 0059). The page is the authority on the geometry of the position on screen — attacks, defends, multiAttack, controls, a bare square, and a move already on the board — and refuses a relation that is not there rather than drawing it. Coach Engine remains the sole authority on evaluation. Marks are scoped to one revision and cleared by any move of the board.                                     |
| 17  | `step_line` walks a Review Moment line the board is **already showing**, one ply at a time. It chooses nothing: the snapshot's `linePlayback` names the steps and the depth reached, and is null when there is nothing to walk. Those lines are ordered moves with no positions attached, so the page derives each one, and the two of them root one ply apart. The retained exploration tree is not walked this way — its nodes are already selected one at a time, by the branch strip or by an Alternative Move target.                                     |
| 18  | The snapshot **names the actor**: who advanced this revision, the last revision the Player advanced, and per branch the revision it arrived at and who added it (ADR 0062). WebMCP has no server-to-agent push, so this is how an agent that was idle while the Player moved learns it. The Player's revision is remembered separately because the agent's own next call overwrites who last changed the board. Extends decision 7 rather than narrowing it: the counts are page-scoped like the revision they are made of, so a change of origin advances them and names whoever navigated, and only a reload starts them over.                                     |
| 19  | **Orientation is a `set_board_position` target**, not a tenth tool: the board surface is already nine tools and every name costs description budget. Turning the board is presentation — it reaches no position, grounds nothing, and is the one drive with no hazard to gate — so it clears neither the marks nor a pending move. It advances the revision anyway, because decision 7 reads equal revisions as nothing having changed and the view did change — so a mark held against the revision before the turn is refused as stale, the same as any other advance. Refines decision 16 in both clauses: marks are cleared by a **move** of the board, and turning it is not one, so marks outlive the revision they were drawn at — annotation stays contracted on the revision the agent read, not on the revision the marks carry. A Game board opens from the Player's own side (both sides → White's), an Opening Line board from White's, and the side survives every later move of the board.                                     |
| 20  | **Nothing names the ink.** No result says what colour a mark is drawn in, and no mark carries a tone: every mark is drawn in the one `coach` ink (ADR 0059), so a per-mark tone would broadcast a constant on every snapshot, and a colour word in the result would be a second pin on a design token with nothing holding the two together. Instead the constraints block, which already governs prose, says to name a mark by its square and its label and never by a colour — the piece colours of the position are untouched. The sentence rides on **results only**, where the marks are: a description is read before anything is drawn and would charge nine board tools for it. Refines decision 16: geometry is gated by construction, but describing what was drawn is prose, and prose is governed the way every other sentence on this surface is.                                     |

## Architecture

One registration hook is called from every Coaching Board surface — the lobby,
the game board, and the opening board. Each call site is keyed by
`authorizedPlayerId` after **Beta Access**. The hook registers on an effect
and tears down through the `AbortSignal` that `registerTool` accepts.
`ReviewSessionWorkspace` is **not** a v1 call site. Nothing registers at
module load: the `/app/` gate is asynchronous, so early registration would
let an agent call board tools while identity is still resolving. Sign-in and
beta-admission pages register session-status only
(#486), never board tools.

Anonymous visits to `/app/board` and the two board addresses are allowed.
Other `/app/` Game Review addresses stay gated. Tools still do not register
while identity is loading, signed out, unverified, or beta-unauthorized.
Anonymous staging of the lobby import form is capped at **ten attempts per
rolling hour per client** (conservative pick; the lock did not name a number).
A Sign-in refusal for the durable Game import does not spend that allowance.
The unused opening-analysis allowance is retired until an anonymous analysis
route exists.

The board state the tools project already exists. Exploration is a retained tree
— `BranchParent` is `root(positionRef) | move(branchRef)`, `AlternativeMoveResult`
carries its `parent`, and the client keeps branches per critical ply without ever
removing one. A takeback moves the active branch; it erases nothing. That is why
the read is a snapshot rather than a journal: the abandoned sibling line is
usually what _"the first one I tried"_ points at, and it is already there.

Addresses:

- `/app/board` — the lobby
- `/app/board/games/<gameImportId>` — Coaching Board over a Game Import
- `/app/board/openings/<eco>-<name-slug>-<digest4>` — Coaching Board over an
  Opening Line

The ordinary Game Review address is unaffected.

## The snapshot

A **Coaching Board Snapshot** is a board artifact. Returned whole on every
board read, and carried on every other **board** tool's result (game or
opening origin):

- origin — a Review Moment or an Opening Line, the only two roots a board may have
- viewed ply and current Position
- the retained exploration tree: each branch's parent, move, evaluation and
  strongest reply, plus the revision it arrived at and who added it
- which branch is active, and the ordered path from root to current
- the shown line, if any
- the side the board is drawn from
- the marks the coach drew about this position, cleared by any move of the board
- the line the board can walk, its ordered steps, and how far into it the board has come
- where the viewed ply sits on the Game's own line or the Opening Line: the
  move that reached the position, the move the line went on to play (the
  caption's move — the board stands before it), the Review's evaluation of
  the position when it has one, and the last ply. The Game origin names the
  review side (ADR 0064)
- on an opening board with an authored world, the study session: the card the
  Player is on in the words the page asks it, every answer with the verdict
  the page gave, and the tally. The plan card's `ungraded` verdict carries its
  rubric for the host agent to mark, and the page offers the Player a press
  that copies the referent to ask for it (ADR 0063, ADR 0064)
- a monotonic page revision, whether the Player or the agent advanced it, and
  the last revision the Player advanced — each null while the page is still on
  the revision it loaded with, so a board mounted by a navigation opens naming
  that navigation's actor. WebMCP has no server-to-agent push, so these and
  the branch arrivals are how an agent that was idle while the Player moved
  reads what happened instead of answering from the board it last saw. The
  Player's revision is remembered separately because the agent's own next call
  overwrites who last changed the board, and browsing a ply, selecting a branch
  and walking a line add no branch to point at. The page holds the count, not
  the board showing it: changing origin tears the board down, and the next one
  picks the count up where the last left it, advanced once and stamped with
  whoever navigated. Only a reload is a new page
- a constraints block stating what may be said about the facts returned

Lobby tools (search, recent-profile read, import staging, opening find and
open) return `kind: "lobby"` plus a constraints block,
not a Coaching Board Snapshot. The lobby has no Review Moment or Opening Line
origin.

Game and opening exploration are one consumed shape. A game branch is an
engine `AlternativeMoveResult`; an opening branch is built in the page from
the stateless analysis route, which grounds a position as one FEN and mints
no ids. Both satisfy the narrower **Coaching Board Exploration Branch** the
board reads — parent, move, evaluation, and the resulting position's FEN,
occupied squares, side to move, and position reference (ADR 0058, as
amended). Per-ply evaluations that the snapshot's branch list drops,
including the comparison against the best move, ride on the evaluating tool's
own result.

The position reference is what lets the Player's own next move be parented to
the branch the board is standing on, rather than re-walking the line from the
moment on every move. A game branch carries the engine's reference; an opening
branch mints one from its resulting FEN, as it already does for its root
parent.

## Grounding policy without an instructions channel

`coachMcpInstructions` is roughly nineteen kilobytes delivered once in the MCP
initialize response, and it is what stops a coach inventing a canonical line,
presenting a Player Line as a recommendation, or replacing a failed render with
invented chess facts. `registerTool` accepts a description and annotations, and
nothing else.

So each web tool's description carries the sentences governing its own use,
assembled from the same authored source rather than hand-copied, and each result
carries a constraint block for the facts it just returned. Descriptions are
host-summarised context seen once; results are read fresh on every call, so the
rules that matter most travel with the evidence they govern.

## Driving limits

Our own web coach may only show a line ChenChess established or one already
evaluated and on screen — `HostTurnShowLine` is a closed union and deliberately
cannot express an invented line. The agent gets that vocabulary and no more.

A line that has been shown can be **walked**, which chooses nothing the gate
did not already allow: `step_line` moves along the steps `show_line` accepted,
or along the exploration path already on screen, and refuses when there is no
line to walk. The Player has the same transport as a row of buttons, so being
told to look at a line does not mean asking the coach to advance it.

Pointing at the board is the same shape one level down. `annotate_board` takes
at most six marks from a closed vocabulary, and the page settles each against
the FEN it is rendering before a pixel is drawn: a relation the position does
not support is a typed refusal, not an arrow. `multiAttack` is named for what
is checked — that one piece bears on two enemies — never for whether the fork
is worth having; that judgement lives in the label, which the constraints block
governs like any other prose. Marks carry their own ink, so an agent's claim
never reads as the Player's own exploration — and the result says nothing about
what that ink is, so the constraints block tells the coach to name a mark by its
square and its label rather than by a colour. A guessed colour is the same
confident-wrong detail verify-then-draw exists to prevent, reaching the Player
through the one channel geometry cannot check.

Turning the board is the one drive with nothing to gate. `set_board_position`
takes an **orientation** target beside its position targets: it reaches no new
position, so there is nothing to ground and nothing to refuse, and it is not a
move of the board — the marks and a pending move stay exactly where they were.
The revision still advances, because what the Player is looking at did change.

A Player Line the agent proposes goes through the existing evaluation path: it
takes ordered SAN or UCI with an explicit `opponentReplies` choice, caps at twelve
plies, and returns an exact render option that is the only thing allowed to be
shown. Evaluate, then show. An unevaluated line has no render option, so it
cannot reach the board. That is the gate, and it already exists.

## The opening root

Every engine command is keyed by a Game Import, and exploration additionally by a
Review Moment. Nothing accepts a bare position. Opening study therefore needed a
root, and it is deliberately not an aggregate: a stateless route that, given an
Opening Line and an ordered continuation from it, returns the position and
per-ply evaluations rooted at the initial position. No actor, no key, no
residency policy, no Player-owned state.

**Two keys, two jobs.** An Opening Line is addressed by its move path — across
the pinned catalog's 3,690 rows there are 499 distinct ECO codes, 3,160 distinct
names and 3,313 distinct ECO-and-name pairs, but 3,690 distinct paths, so only
the path identifies a line. Analysis is addressed by normalized position, with no
owner and no session segment, mirroring the prepared-analysis cache.
Transpositions therefore collapse onto one cache entry, which is right because
two move orders reaching one position are one board, and stay distinct addresses,
which is also right.

Compute is bounded twice, because there is no Review Session allowance to scope
it: twelve plies, and a per-Player rate limit.

**Offer, find, and open are three verbs.** Empty-state offer names only
openings the Player has played; a Player with no imported Game is offered
none — that is “names no opening it cannot attribute.” Typed find is
authenticated and returns catalog rows that already match the query, played
matches first, unplayed matches allowed. `open_opening_line` navigates a
path-identified catalog line; find ranks, open does not re-rank. The
analysis route and its cache know nothing about who asked. A played opening
is known only as an ECO code and a name — the pair that collides — so it
ranks rows and never identifies one, resolving to the shortest path among
the rows sharing it.

## Validation

Three tiers, labelled, because they buy different things.

**Deterministic** — vitest with a jsdom polyfill of `document.modelContext`,
inside the app package and covered by its existing test task. Registration
timing and teardown, the drift assertion against the authored map, snapshot
correctness for a multi-branch fixture, revision monotonicity across moment and
line switches, refusals, and staging never clobbering Player input. No model.
Annotation joins it: each mark kind verified and refused against a fixture
position, the stale-revision refusal, the six-mark cap, a call outside the mark
vocabulary, and marks clearing across a position change. Playback joins it too:
the steps a shown line offers, each derived position, the named directions
clamping at the ends, an index outside the line refused, no line to walk
refused, and the refutation walking from after the played move rather than
before it.
Opening branch minting joins it: ids deterministic per line and move path, a
shared prefix converging rather than duplicating, a partial verdict keeping
its evaluated prefix, a continuation naming another line refused before the
engine is asked, and evaluation advancing the revision without activating a
branch or moving the board.
Naming the actor joins it: each transition reporting who advanced the revision,
drawing naming the agent without moving the board, the agent's own call not
erasing what the Player did before it, a branch carrying the revision it
arrived at, a re-analyzed branch keeping the arrival it already had, and the
Player's own explored move landing on one revision rather than a revision no
reader ever sees. Which affordance names which actor is asserted against the
rendered board rather than the drive, because the mapping from a Player's click
to an actor is where it can go wrong. Changing origin joins it there too: the
count climbing across the switch instead of restarting, and the navigation
naming the Player or the agent, asserted at the mount, which is the only thing
that outlives the board being torn down.
Turning the board joins it: the side each origin opens from, both sides of the
turn reaching the rendered board, the marks and a pending move surviving it,
the side surviving a later move of the board, and a mark held against the
revision before the turn refused as stale. How to name a mark joins it: the rule
riding once on the board constraints and on no registered tool description,
asserted against the descriptions actually registered on both surfaces rather
than a hand-picked pair, and not on the lobby, which draws nothing.

**Boundary** — engine integration: opening analysis statelessness, transposition
cache hits, the ply cap retaining its evaluated prefix, the rate limit tripping
and recovering, and no cache document carrying an owner or session segment.

**Behavioural** — a WebMCP journey kind in the existing journey harness, driving
a fixed suite of scripted prompts through a real agent and recording whether the
board read preceded the answer. It covers each referent class and includes one
non-deictic prompt that must _not_ trigger a read, because over-calling is a
failure too. The annotation referent classes join it, with one trap of their
own: a mark the position does not support, which must come back refused and be
reported refused rather than drawn. Reported as measured with a model; not wired as a gate.
Each run also records whether the coach drew on a prompt that did not ask for
a drawing, as a count beside the number of prompts that did not ask. It is a
measurement, not a criterion: the mitigation for over-drawing is structural —
the mark cap and clearing on every move — and a rate would be a number nobody
acts on (#533).

The suite passes whole rather than clearing a rate. A pass rate would cost model
spend to produce a number nobody would act on differently at 85% than at 92%,
and the architectural mitigation — the snapshot riding on every board-tool
result — already does the work a threshold cannot.

## Top risk

Nothing in WebMCP obliges an agent to read the board before answering. An agent
that skips the read answers from the position it last saw and produces fluent,
confident, wrong coaching for a Player who is least able to detect it. This is a
product risk, not a technical one, and it is not solved — it is mitigated by
carrying the snapshot on every board-tool result, so any board call refreshes
the agent's picture, and measured by the behavioural tier.

## Children

- #484 an agent can read the live board on the deployed origin
- #485 Coaching Board mode and its address
- #486 answer the locked state on the sign-in and beta admission pages
- #487 the agent can drive the board within grounded limits
- #488 existing coach tools reach the web surface
- #489 measure deixis reliability with a scripted journey
- #490 the lobby, with a staged Game import
- #491 import my latest game from a connected playing profile
- #492 find an Opening Line in the pinned catalog
- #493 the opening Coaching Board on a stateless identity-free root
- #494 played-opening hints and match-tier ranking

## Out of scope

- **Anonymous tool registration.** Page visits to `/app/board` and the two
  board addresses are allowed without Firebase sign-in. Tools still do not
  register until Beta Access authorizes a Player. Anonymous lobby import-form
  staging is rate-limited (ten per rolling hour per client). A Sign-in
  refusal for the durable Game import does not spend that allowance.
- **Durable writes by the agent.** Comments, votes and learning-path writes stay
  off this surface. The one durable write reachable from it — a Game import — is
  staged by the agent and committed by the Player.
- **Arbitrary FEN.** A position outside the Game Import and outside the catalog
  has no engine root, and legality is not grounding.
- **A separate voice path.** ChatGPT voice is speech-to-text into the ordinary
  chat, so a dictated tool call is the same text path as a typed one. Nothing
  here needs a voice-specific test.
- **A Chrome-extension host.** The built-in browser carries the Firebase session,
  so the hedge against an isolated browsing container is against a measured-false
  risk.
- **A trail of what the coach drew.** Marks clear on every move of the board
  (ADR 0059) because a mark beside a board that has moved on is the one false
  thing annotation could show, and a stack of past marks under the board is
  that by construction. The transcript already holds each drawing as a
  sentence addressed by square and label, never by a colour, so the Player
  can ask for it again by name, and the board column is the crowded surface.
  Decided under #531: the coach says it again; the board does not remember.
