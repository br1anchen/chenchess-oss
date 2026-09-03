import { sharedGroundingSentences } from "@chenchess/shared-assets"

import { BOARD_ANNOTATION_MARK_LIMIT } from "./boardAnnotation"

import {
  evaluatePlayerLineDescription,
  listCriticalMomentsDescription,
  openReviewMomentInPlaceDescription,
} from "../../server/board/conversation-policy"

import type {
  CoachingBoardConstraints,
  CoachingBoardLobbyResult,
} from "./coachingBoardSnapshot"

const boardConstraintSentences = [
  "Speak only from this Coaching Board Snapshot. Do not reconstruct the board, a line, or an evaluation from chat prose or model knowledge.",
  "Every sibling Alternative Move remains in the tree, including abandoned ones. A takeback changes the active branch; it erases nothing.",
  "Name only Positions this snapshot grounds: a ply of the Game Import, a node of the retained Alternative Move Exploration, or the Opening Line and lines already evaluated from it.",
  "Never present a Player Line as a recommendation or a canonical Move Sequence.",
  "A pendingMove is a move the Player has played that the Engine has not confirmed. It carries no evaluation and none may be inferred from it. Coach on currentPosition, which is the last Position the Engine confirmed.",
  "mainLine names where viewedPly sits on the Game's own line or the Opening Line: reachedBy is the move that produced the position on screen, continuesWith is the move the line went on to play from it and the move the caption under the board shows, and evaluation is the Review's verdict on this position when it has one. Say what was played from these facts, never from the FEN. When a branch is active, currentPosition is off that line and pathFromRoot says how it got there.",
  "study, when present, is the Player's opening study session on this line: the card they are on in the exact words the page asks it, and every answer already given with the verdict the page gave. An ungraded verdict is a plan in the Player's own words for you to mark against its rubric — credit what it names, name what it misses. Never regrade a card the page graded, and never answer a card for the Player: ask, then read the board again once they have answered.",
] as const

/**
 * Sentences that ride on results only, never on a tool description.
 *
 * A description is read once, before there is a snapshot to compare or a mark
 * to describe, so a rule about the evidence in front of the agent has nothing
 * to attach to there — and every sentence in `boardConstraintSentences` is
 * paid for nine times over, once per registered board tool.
 */
const resultOnlySentences = [
  "revisionChangedBy names who advanced the board to this revision, and each branch says which revision it arrived at and who added it. Read the Player's activity from playerChangedAtRevision, not from revisionChangedBy, which your own call overwrites.",
  "A playerChangedAtRevision higher than a revision you read means the Player changed the board while you were away: say so, and answer from this snapshot rather than the board you last read. Both counts belong to the page rather than to one board, so that comparison holds across a change of origin too — navigating is one of the changes it reports, and revisionChangedBy names who navigated. A revision lower than one you read is a reloaded page, not a board that moved backwards.",
  "Name a mark by its square and its label, never by a colour: no field of this snapshot says what ink the board draws marks in, and a guessed colour sends the Player hunting for something that is not on screen. The colour of a piece is a fact of the position and this does not touch it.",
] as const

const lobbyConstraintSentences = [
  "This result is a lobby. It has no Review Moment or Opening Line origin and does not carry a Coaching Board Snapshot.",
  "A staged Game import is a proposal. The Player commits it. Do not claim the Game is imported until the Player does.",
  "Opening find ranks matching catalog rows. Played matches come first only when the Player is signed in. Open navigates a path; it does not re-rank.",
] as const

export function boardConstraints(): CoachingBoardConstraints {
  return {
    kind: "constraints",
    sentences: [
      ...sharedGroundingSentences,
      ...boardConstraintSentences,
      ...resultOnlySentences,
    ],
  }
}

export function lobbyConstraints(): CoachingBoardConstraints {
  return {
    kind: "constraints",
    sentences: [...sharedGroundingSentences, ...lobbyConstraintSentences],
  }
}

export function lobbyResult(): CoachingBoardLobbyResult {
  return { constraints: lobbyConstraints(), kind: "lobby" }
}

export function unavailableLobbyResult(): CoachingBoardLobbyResult & {
  outcome: "unavailable"
} {
  return { ...lobbyResult(), outcome: "unavailable" }
}

/**
 * A refusal the agent can correct, distinct from unavailable: the call shape
 * was wrong, the backend was never asked.
 */
export function refusedLobbyResult(reason: "invalidFields" | "invalidFilters") {
  return {
    ...lobbyResult(),
    outcome: "refused" as const,
    reason: { kind: reason },
  }
}

export const showLineDescription = [
  "Show a line already on this Coaching Board. Accept only the closed HostTurnShowLine union the web coach uses: engineBest, playedMoveRefutation, or an Alternative Move already evaluated and on screen. The vocabulary cannot express an invented line.",
  "A line with no render option cannot be shown. This tool does not evaluate, does not spend engine compute, and writes nothing durable. Return the updated Coaching Board Snapshot and constraints. The page revision advances when the line is shown.",
  ...sharedGroundingSentences,
  ...boardConstraintSentences,
].join(" ")

export const annotateBoardDescription = [
  `Point at the position the Coaching Board is showing. Take up to ${BOARD_ANNOTATION_MARK_LIMIT} marks from a closed vocabulary, each with a short Player-visible label: attacks and defends (one piece bearing on one occupied square), multiAttack (one piece bearing on two or more enemy pieces), controls (a bishop, rook or queen reaching along a line), square (a bare highlight that asserts no chess relation), and move (an arrow for a move this board already put on screen).`,
  "The page verifies every mark against the position before drawing it. A relation that is not on this board is refused, not drawn, and the refusal is the answer: say the position does not support the claim rather than retrying with another mark. multiAttack is named for what is checked — that the piece hits two enemies — and never for whether the fork is worth having; that judgement belongs in the label and in what you say, not in the tool.",
  "Send the revision from the Coaching Board Snapshot you are annotating. A board that moved since then refuses with staleRevision: read it again and decide whether the marks still apply. Marks belong to one position and are cleared by any move of the board, so they can never describe a position other than the one they were drawn on.",
  "This checks geometry the page can settle from the position on screen. It spends no engine compute, asks for no evaluation, and writes nothing durable. Return the updated Coaching Board Snapshot and constraints.",
  ...sharedGroundingSentences,
  ...boardConstraintSentences,
].join(" ")

export const stepLineDescription = [
  "Walk the line the Coaching Board is already showing, one ply at a time. Take `to` as a step index from 0 (the position the line starts from) up to its length, or one of start, previous, next, end. The board moves to that point in the line and the arrow names the move that comes next.",
  "This walks; it never chooses a line. Show one first with show_line, or open an Alternative Move — the snapshot's linePlayback names the steps and how far in the board has come, and is null when there is nothing to walk, which this tool then refuses. The named directions stop at the ends rather than erroring; an explicit index outside the line is refused as unreachablePosition, and a `to` that is neither an index nor a named direction as outsideStepVocabulary.",
  "This spends no engine compute, evaluates nothing, and writes nothing durable. Return the updated Coaching Board Snapshot and constraints; the page revision advances because the board moved.",
  ...sharedGroundingSentences,
  ...boardConstraintSentences,
].join(" ")

export const setBoardPositionDescription = [
  "Move the Coaching Board to a grounded position: a ply of the loaded Game Import, a node of the retained Alternative Move Exploration, a path-identified Opening Line, or a Game the Player already reviewed by its exact gameImportId. Every reachable position already has an engine evaluation.",
  "An orientation target turns the board to be seen from White's or Black's side. It is presentation: it moves nothing, reaches no new position, and grounds nothing, so what the coach drew stays on screen. Use it when the Player asks to look from the other side; the snapshot's orientation says which side they are looking from now.",
  "An Opening Line or a reviewed Game target is navigation to that board; the new snapshot arrives from read_coaching_board after the page settles. An unreachable target is a typed refusal and the board is left unchanged. A call outside the five target kinds is refused as outsideTargetVocabulary before any position is looked for: fix the call rather than concluding the position is unreachable. This tool does not spend engine compute and writes nothing durable. Ply, exploration and orientation targets return the updated Coaching Board Snapshot and constraints, and the page revision advances because the board changed.",
  ...sharedGroundingSentences,
  ...boardConstraintSentences,
].join(" ")

export const evaluateOpeningContinuationDescription = [
  "Evaluate an ordered continuation from the Opening Line this board is on, and keep the evaluated plies as branches of its Alternative Move Exploration. Take one to twelve plies as SAN or UCI, both sides supplied: every ply is evaluated, so there is no opponent-reply choice to make.",
  "The continuation is rooted at the END of the opened line, never partway through it. To ask about a deviation earlier than that, open the shorter Opening Line that ends where the question starts. Do not retry this tool with a shorter continuation; it will be read from the line's end again.",
  "This evaluates and shows nothing: no branch becomes active and the board does not move. Show one afterwards with show_line or set_board_position, addressing an Alternative Move id this result returns. An illegal move keeps the plies evaluated before it and reports the outcome; a rate limit returns a typed retry. A continuation longer than twelve plies is not accepted at all and spends no compute, so send at most twelve.",
  "This spends Game Review Engine compute and writes nothing durable, so it needs no Player confirmation. It answers only on an Opening Line origin. On a Game board it is unavailable, where evaluate_player_line is the equivalent gate.",
  ...sharedGroundingSentences,
  ...boardConstraintSentences,
].join(" ")

export const listCriticalMomentsWebDescription = [
  listCriticalMomentsDescription,
  ...sharedGroundingSentences,
  ...boardConstraintSentences,
].join(" ")

export const openReviewMomentInPlaceWebDescription = [
  openReviewMomentInPlaceDescription,
  ...sharedGroundingSentences,
  ...boardConstraintSentences,
].join(" ")

export const evaluatePlayerLineWebDescription = [
  evaluatePlayerLineDescription,
  ...sharedGroundingSentences,
  ...boardConstraintSentences,
].join(" ")

export const readCoachingBoardDescription = [
  "Read the live Coaching Board. Takes no arguments and returns the whole snapshot: origin, viewed ply, current Position, the retained Alternative Move Exploration with each branch's parent, move, evaluation, and the revision it arrived at, the active branch, the ordered path from root to current, the shown line if any, the monotonic page revision with who advanced it and the last revision the Player advanced, and the constraints that govern these facts.",
  "Call this before answering any question that points at the board. A question that carries no board referent needs no read: general chess advice, study habits, and which time control to play are answerable without looking, and reading anyway spends a call and puts board state into an answer that did not ask for it. WebMCP has no instructions channel; the constraints in the result govern the facts just returned.",
  ...sharedGroundingSentences,
  ...boardConstraintSentences,
].join(" ")

export const listRecentProfileGamesDescription = [
  "List recent completed Games from the Player's connected Lichess or Chess.com Playing Profile. This is a read. It imports nothing, spends no Game Review Engine compute, and writes nothing durable.",
  "A Player with no Playing Profile Connection receives outcome noPlayingProfile. When Games are found, list them, then stage one with stage_game_import. The Player confirms the import.",
  "Return kind lobby plus constraints, not a Coaching Board Snapshot.",
  ...sharedGroundingSentences,
  ...lobbyConstraintSentences,
].join(" ")

export const stageGameImportDescription = [
  "Stage one Game import as a proposal on the Coaching Board lobby. The Player commits the durable write after seeing the retention disclosure. Never claim the Game is imported, and never overwrite source the Player is already typing.",
  "Accept a Chess.com URL, a Lichess URL, or pasted PGN. Return kind lobby plus constraints, not a Coaching Board Snapshot.",
  ...sharedGroundingSentences,
  ...lobbyConstraintSentences,
].join(" ")

export const findOpeningLineDescription = [
  "Find Opening Lines in the pinned catalog that already match the typed query. Played matches rank first when the Player is signed in; unplayed matches are allowed. A played opening never surfaces for a query it does not match.",
  "This ranks. It does not open a line and does not re-rank when opening. Return kind lobby plus constraints, not a Coaching Board Snapshot.",
  ...sharedGroundingSentences,
  ...lobbyConstraintSentences,
].join(" ")

export const searchReviewedGamesWebDescription = [
  "Search the Player's reviewed Games across Coaching Digests and manual imports. All supplied filters are AND-ed; results are always newest-first and capped at 20. Coverage describes reviewed Games only; a truncated result supplies the exact boundary needed to narrow the next search.",
  "This is a read. It imports nothing and spends no Game Review Engine compute. Stage a matching Game with stage_game_import; the Player confirms the import.",
  "Return kind lobby plus constraints, not a Coaching Board Snapshot.",
  ...sharedGroundingSentences,
  ...lobbyConstraintSentences,
].join(" ")

export const listPlayedOpeningsDescription = [
  "Return the Player's played openings, aggregated over every imported Game with no recency window: a play count and a last-played time per ECO-and-name pair, sorted by count then most recent. Use this instead of aggregating over a truncated reviewed-game search page.",
  "Each played opening resolves to the shortest move path among the catalog rows sharing its ECO and name — the canonical order of that named line. It ranks rows; it never claims the Player played that exact line.",
  "A Player with no imported Games gets an empty list; offer the opening find, never a curated fallback. Return kind lobby plus constraints, not a Coaching Board Snapshot.",
  ...sharedGroundingSentences,
  ...lobbyConstraintSentences,
].join(" ")

export const openReviewedGameDescription = [
  "Open a Game the Player already reviewed on the Coaching Board, by the exact gameImportId a reviewed-game search or the recent-games read returned. This is navigation: it imports nothing, spends no Game Review Engine compute, and writes nothing durable, so it needs no Player confirmation. A Game not yet imported must be staged with stage_game_import instead.",
  "Return kind lobby plus constraints when called from the lobby. The board snapshot arrives after navigation, from read_coaching_board.",
  ...sharedGroundingSentences,
  ...lobbyConstraintSentences,
].join(" ")

export const openOpeningLineDescription = [
  "Open a path-identified Opening Line on the Coaching Board. This is navigation. It does not re-rank find results and does not refuse an unplayed catalog path.",
  "Return kind lobby plus constraints when called from the lobby. The board snapshot arrives after navigation, from read_coaching_board.",
  ...sharedGroundingSentences,
  ...lobbyConstraintSentences,
].join(" ")
