# Coach the web board through site-registered tools

## Status

Accepted.

This decision introduces the **Coaching Board** as a delivery surface distinct
from the **Coach App**, adds a third value to `CoachToolTarget`, and moves the
grounding policy for that surface out of a single instructions blob and into
per-tool descriptions and per-result constraint blocks.

[ADR 0059](./0059-verify-board-annotations-in-the-page.md) extends this one:
the page verifies the geometry of the position on screen so the agent can point
at it, while evaluation stays where this decision leaves it.

It does not disturb ADR 0040 or ADR 0043. The Coach App keeps its remote MCP
transport, its OAuth grant, and its artifact set unchanged.

## Context

A Player studying a Game on `/app/game-reviews/<gameImportId>` moves pieces with
the mouse and asks questions by dictating into ChatGPT. Those are two channels,
and the language channel is full of pointers the model cannot resolve: *why is
this bad, what about that instead, is the first one I tried still winning.* Every
one of those refers to board state ChatGPT never saw.

WebMCP closes that gap: the page registers tools with `document.modelContext`
and the agent calls them against the live page and the Player's own session. The
mechanism is not in dispute. Three things about fitting it to this codebase were.

**It resembles the Coach App closely enough to be confused with it.** The Coach
App is defined as a surface that "uses the host model as its Language Layer and
pairs the native conversation with an inline chess workspace." The Coaching Board
does both. What differs is everything around it: no connector installation, no
Coach OAuth grant, no **Beta Coach App Connection**, no `ui://` artifact set, and
no `validateOrigin` allowlist entry, because the calls are same-origin to the
page that registered them. Authorization is an ordinary web sign-in plus **Beta
Access**.

**One authored map already declares who may call each tool.** `coachToolSurface`
maps every Coach tool to `["model"]`, `["app"]`, or both, and the registration
lists derive from it. The project has four hand-written pins that must agree with
that map already; a fifth, maintained beside it, would rot the same way.

**The grounding policy has no channel on this surface.** `coachMcpInstructions`
is roughly nineteen kilobytes delivered once in the MCP initialize response, and
it is what stops a coach inventing a canonical line, presenting a Player Line as
a recommendation, or replacing a failed render with invented chess facts.
`document.modelContext.registerTool` accepts a description and annotations. There
is no site-level instructions channel to put those sentences in.

There is also a hazard specific to `/app/`. Its routes pass a four-stage
asynchronous gate, and registering tools at module load would let an agent
discover and call board tools while identity is still resolving, or while the
Player is signed out — executing against no Player at all.

## Decision

**Name it separately.** The Coaching Board is its own term, told apart from the
Coach App by carrying no installation. Both terms stay in the glossary with
their `_Avoid_` lists intact, including Coach App's `_Avoid_: web application`.

**Widen the one map rather than adding a second.** `CoachToolTarget` gains
`"web"`. Two corrections come with it, and the first is a live defect:
`coachAppOnlyToolNames` was derived as *not model*, so any web-only tool would
have landed in the app list, where the surface tests assert every entry is
advertised and registered on the MCP server. It is now derived as
`coachToolNamesByTarget("app")`. And `measureModelToolSurface` used
`Buffer.byteLength`, which the browser cannot import; it uses `TextEncoder`, so
one module serves both registries.

**Register inside the gate, never at module load.** One hook, called from
every Coaching Board surface — lobby, game board, and opening board — each
already inside `BetaAccessBoundary` and keyed by `authorizedPlayerId`,
registers on an effect and tears down through the `AbortSignal` that
`registerTool` accepts. `ReviewSessionWorkspace` is one call site, not the
only one. There is no code path that reaches registration before the gate
resolves, so the race is closed by construction rather than by care.
Sign-out fires the signal, `toolchange` tells the agent, and the tools stop
existing. Sign-in and beta-admission pages register session-status only
(#486), never board
tools.

**Deliver the policy twice, deliberately.** Each web tool's description carries
the sentences that govern calling it, drawn from the same authored source as
`coachMcpInstructions` so the surfaces cannot drift. Each tool *result*
additionally carries a constraint block covering the facts it just returned.
Descriptions are host-summarised context seen once; results are read fresh on
every call, so the rules that matter most travel with the evidence they govern.

**Carry the board snapshot on every board-tool result, not only the read
tool's.** Nothing in WebMCP obliges an agent to read state before answering,
and an agent that skips the read produces fluent, confident, wrong coaching
for a Player who cannot detect it. Putting the snapshot on every *board*
tool result (game or opening origin) means any board call refreshes the
agent's picture of the board, which reduces the failure surface to the
single path where it calls nothing at all. Lobby import and find are not
board tools: they return `kind: "lobby"` plus constraints, not a Coaching
Board Snapshot. The lobby has no Review Moment or Opening Line origin.

**Keep durable writes off the surface.** The agent stages a Game import; the
Player commits it by pressing the button that already exists, having seen the
retention disclosure. Comments, votes and learning-path writes are not exposed.

## Consequences

The policy exists in two shapes and must be generated from one source, or the
web surface will drift from the Coach App's rules silently. A test asserts the
registered names equal the `"web"` projection of `coachToolSurface`, in map
order; it lives in `apps/central-host/src/` and therefore needs no turbo task,
because `$TURBO_DEFAULT$` already covers a test that reads only files inside its
own package.

Every *board* tool result is larger than its facts, because it carries
constraints and a snapshot. Lobby results carry constraints without a
snapshot. That is a deliberate cost paid against confident-wrong coaching
on a board, without inventing an origin the lobby does not have.

The Coaching Board and the Coach App can both be present to one agent, with
overlapping names on `list_critical_moments`, `open_review_moment` and
`evaluate_player_line`. They are the same operations against the same durable
data, so overlap is answered by distinct descriptions rather than by renaming.

Nothing forces an agent to read the board before answering. This is a measured
risk, not a solved one: a fixed scripted suite exercises each referent class,
including one prompt that must *not* trigger a call.
