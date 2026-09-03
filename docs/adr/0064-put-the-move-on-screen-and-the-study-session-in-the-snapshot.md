# Put the move on screen and the study session in the snapshot

## Status

Accepted (2026-09-02). Found by driving the deployed Coaching Board through
Chrome's WebMCP channel as a host agent and trying to discuss a Game position
and run an opening study from what the tools returned.

This decision extends ADR 0056 (every board-tool result carries the Coaching
Board Snapshot) and makes a claim in ADR 0063 true — that the host agent
grades the Player's plan "against the snapshot it already holds". It has the
shape of ADR 0060 and ADR 0062: fields added so the snapshot stays honest
about something the Player can see and the agent could not.

## Context

Two things the Player looks at were missing from what the agent reads.

**The move.** At a ply that is not a Critical Moment, the snapshot carried a
FEN and `viewedPly`. The Player sees "10… b5" highlighted and an evaluation
graph; the agent is forbidden by the constraints from reconstructing either
from model knowledge, so it could not say what was played here, what came
next, or how the Review judged the position. Outside the six moments of a
Game it was mute on exactly the questions a Player browsing their own game
asks.

**The study.** ADR 0063 runs the opening study as a sequence of cards in
page state and defers the one card a board cannot grade — the plan, in the
Player's own words — to the host agent. The page even said so: "Ask the coach
on this page to read your plan". No tool returned the card, the plan, the
rubric, or a verdict. The #526 transcript passed only because the driver
handed the agent the world content out of band; a blind host agent on the
live board had nothing to mark.

A third, smaller thing: a `set_board_position` call the schema rejected —
the arguments wrapped one level too deep — came back `unreachablePosition`,
which told the agent the ply did not exist rather than that the call was
malformed.

## Decision

**`mainLine` on every snapshot.** Where `viewedPly` sits on the Game's own
line or the Opening Line: `reachedBy` (the move that produced the position on
screen), `continuesWith` (the move the line went on to play, which is also
the move the caption under the board names), the Review's `evaluation` of
this position when it has one, and `lastPly`. The board at `viewedPly` shows
the position *before* the move at that ply, and the constraint sentence says
so, because a Critical Moment is named by the move played from it. The Game
origin also names `reviewSide`, so "your move" is derivable. Moves come from
the imported Game or the catalog path the page already holds; the evaluation
is the frozen Review's. No new authority over chess facts.

**`study` on an opening snapshot with an authored world.** The session moves
out of the card component into the drive, beside the position. A Player
answering a card is a change of the board like a browse: one transition, one
Player-advanced revision, the board moved to where the next card is asked
from. The snapshot projects the card the Player is on — the authored card plus
the exact prompt the page shows, so coach and page ask the same words — every
answer with the verdict the page gave, the tally, and the side. The `ungraded`
verdict carries its rubric; the constraint sentence tells the agent to mark
that one, never to regrade what the page graded, and never to answer a card
for the Player. The agent has no tool to answer a card, by construction.

**The plan card hands off.** Beside the "For your coach to mark" note the
Player gets an "Ask the coach to mark my plan" press that copies a referent —
the same clipboard mechanism as "Ask about this position" (#530), extracted
into one shared affordance. The plan is not repeated in the referent: the
coach reads it from the board, where the page put it.

**A reviewed Game is navigation.** The lobby could search reviewed Games
and could not open one; "open my game against X" ended with the Player
clicking. `open_reviewed_game` takes the exact Game Import id a search or the
recent-games read returned and navigates, the same consent class as
`open_opening_line` (spec decision 3): nothing is created or disclosed. From
a board it is a `set_board_position` target of kind `game`, beside the
Opening Line target.

**Honest refusals.** A call the schema rejects is refused as
`outsideTargetVocabulary` (`set_board_position`) or `outsideStepVocabulary`
(`step_line`). `unreachablePosition` is reserved for a well-formed target the
board checked and could not reach.

## Consequences

- An agent can discuss any ply of a Game from the snapshot alone: what was
  played, what came next, whose move it was, and the Review's evaluation.
- The opening study is runnable by a blind host agent: it reads the card,
  waits for the Player, reads the verdict, and marks the plan against the
  rubric the page authored. The measured #526 transcript no longer needs the
  world supplied out of band.
- Driving the deployed lobby showed the agent's navigation arriving as a
  document load, so the opened board read as revision 1 with nobody having
  navigated — ADR 0062's actor-on-navigation never had a page to land on,
  because the app replaced the document for every address. The board's own
  path is now pushed onto history and rendered in place
  (`useCoachingBoardNavigation`); sign-in, the dashboard and malformed
  addresses still leave the document.
- The session's first card is arranged where the board is built rather than
  by an effect after mount, so a freshly opened line reads to the agent as a
  page nobody has changed yet. The card component renders the same
  projection the agent reads; there is one read model of the session.
- Nothing durable is written; the session still comes apart with the board.
  The retention of opening exploration is unchanged — the study is not
  retained, which is ADR 0063's design.
- The snapshot grows: the current card carries its authored key (accepted
  squares, the primary break, the deviation answer). The coach is the marker,
  and the #526 drive gave it the whole world; a card key is less than that.
- The "Ask about this position" referent said "after 9. Nxc4" for a board
  standing before 9. Nxc4. It now says where the board stands relative to the
  caption's move — "before" on the Game's own line and an engine line,
  "after" on a branch and on the refutation of the played move — so the
  Player's own words and `mainLine` agree.
