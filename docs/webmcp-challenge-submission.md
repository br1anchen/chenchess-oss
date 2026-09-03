# WebMCP Challenge submission kit

Materials for the [WebMCP Challenge](https://webmcp.devpost.com/) entry
(deadline Sep 3, 2026 1:00 PM PDT; repo and licence work is tracked in
#521). The video is a
screen recording of `staging.example/app/board` driven from Chrome with
WebMCP on, with the script below read as voice-over. Under three minutes:
about 420 spoken words at a coaching pace.

Everything the script claims is something the tools return today. Where a
line quotes the coach, the words are the kind an agent produced in the
measured transcripts (withheld from this snapshot),
not a promise about a specific model.

## Demo video script

Shots are in order. `[…]` is what is on screen; the text is the voice-over.

### 0:00 — the gap (hook)

`[A chess board on the left, a chat window on the right. The Player drags a
knight. Types "why is that bad?"]`

Every chess player has had this conversation. You are looking at a position.
Your coach is in another window. And the whole question is "this" — this
move, this square, the line I tried a second ago. The chat never saw the
board.

ChenChess is a chess coach. With WebMCP, the coach and the player finally
look at the same board.

### 0:25 — the lobby

`[Chrome at /app/board. The WebMCP tools panel shows seven site tools.]`

This is the Coaching Board. The page registers its tools with the browser —
no connector, no OAuth, no extension. The agent calls them against the live
page and my own signed-in session.

`[Agent: "open my last game against dagdraum." Tool calls: search_reviewed_games,
then open_reviewed_game — the board navigates.]`

In the lobby the agent can search my reviewed games, read my connected
Lichess or Chess.com profile, or find an opening in the catalog. It can open a
reviewed game or an opening directly. A new game import it can only stage — I
commit it, because that's the one durable write.

### 0:55 — reading and driving a game

`[Game board. Agent: "what happened here?" Tool call: read_coaching_board. The
result panel shows currentPosition, mainLine, exploration.]`

Every board tool returns a snapshot: the position, which move reached it,
which move I played next, the engine's evaluation, the whole tree of
alternatives I've tried — and a revision that says who moved the board last,
me or the agent. The agent never reconstructs the board from chat. It reads
it.

`[Agent: "show me the better move." Tool calls: open_review_moment_in_place,
show_line, step_line. The board walks the line, the arrow moves.]`

It can move the board — to a ply, to a branch, along a line the engine
already established. It can't invent a line. Evaluate first, then show. That
gate is the product.

`[Agent: "what does that knight actually hit?" Tool call: annotate_board.
Arrows appear.]`

And it can point. Every mark is verified against the position on screen
before a pixel is drawn. Ask it to draw a relation that isn't there, and the
page refuses.

`[Player drags a piece. Board shows the pending move; the agent's next read
shows pendingMove and playerChangedAtRevision.]`

When I move, the agent's next read says so. WebMCP has no push. The snapshot
is honest instead.

### 1:50 — opening study

`[/app/board/openings/… Italian Game. The "Build the world" card: "Where does
the king's knight belong?" Options f3, e2, c3.]`

Opening study is where this gets interesting. Instead of a line to memorise,
the page runs a session: build the position piece by piece, say the plan in
your own words, choose the pawn break, then answer when the opponent leaves
the book.

`[Player clicks e2. Badge: "Not that". The agent reads the board: study.answered
shows the wrong slot with the page's verdict.]`

The page grades what a board can grade. The agent reads every answer and
every verdict from the same snapshot.

`[Plan card. Player types a plan. Presses "Ask the coach to mark my plan".
Pastes into chat.]`

The one thing a board can't grade is a plan in your own words. That card
hands off to the coach: the plan and its rubric are in the snapshot, and one
press copies the referent.

`[Agent reads the board; replies: "Half credit — castling early and the f7
diagonal, yes. You missed d4 — that's what c3 is for."]`

The coach marks it against the rubric the page authored. Then the opponent
goes off book, and the session asks what you'd play — from the plan, not from
a line.

### 2:35 — how it's built

`[Code: document.modelContext.registerTool, the tool-surface map, a
constraints block in a result.]`

Sixteen tools, registered only after the player is authorised. Descriptions
carry the grounding rules, because WebMCP has no instructions channel; every
result carries a constraints block for the facts it just returned. The chess
facts come from our engine. The geometry comes from the page. The words come
from whichever agent you brought.

`[Back to the shared board.]`

One board. Two of you looking at it. That's ChenChess on WebMCP.

## Devpost fields

**Inspiration.** A player and a coach in different windows, and a chat full of
pointers — "this", "that one", "the first thing I tried" — to a board the
model never saw.

**What it does.** The ChenChess Coaching Board registers site tools with
`document.modelContext` so any WebMCP-capable agent can read the live board,
move it to grounded positions, walk engine lines, draw verified marks, run an
opening study session with the player, and stage (never commit) a game
import. Every board tool returns the complete Coaching Board Snapshot with a
revision that names who changed the board.

**How we used WebMCP.** One registration hook per board surface, inside the
auth gate, torn down through `AbortSignal`. Tool visibility comes from one
authored map shared with our MCP server, so the web surface is the same
vocabulary the ChatGPT app uses. Descriptions carry the grounding policy and
results carry constraint blocks, because the standard has no instructions
channel. Arguments are validated by the same valibot schemas that generate
the advertised JSON Schema.

**Challenges.** Nothing obliges an agent to read before answering. We
mitigated it structurally — the snapshot rides on every result — and measured
it with a scripted deixis suite. Malformed calls used to read as
"unreachable position"; they are now refused for their shape. The opening
study session lived in component state and was invisible to the agent; it now
lives in the board's drive and every snapshot carries it.

**What's next.** Off-book deviation cards for more openings, and the
transposition card once the catalog is prefix-closed.

**Built with.** TypeScript, React, Astro, valibot, chessops, Rust (Coach
Engine, Stockfish and Maia), Firebase Auth, Railway.
