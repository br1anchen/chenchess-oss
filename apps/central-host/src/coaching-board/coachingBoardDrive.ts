import { presentationPiecesFromFen } from "@chenchess/review-projection"
import { Chess } from "chessops/chess"
import { makeFen, parseFen } from "chessops/fen"
import { parseUci } from "chessops/util"

import type {
  AlternativeMoveId,
  CanonicalGameMove,
  EngineEvaluation,
  GameImportId,
  GameReview,
  GameReviewCriticalMoment,
  HostTurnShowLine,
  ImportedGame,
} from "@chenchess/coach-engine-sdk"

import {
  browseBoardAtPly,
  moveLabel,
  reviewSideOrientation,
  type BrowseBoardPosition,
} from "@/review-session/model"

import {
  coachingBoardStudy,
  openingStudyState,
  studyAnswered,
  studyRestarted,
  studyViewedPly,
  type CoachingBoardStudyState,
} from "./coachingBoardStudy"
import type { OpeningStudyWorld } from "./openingStudyWorld"

import {
  verifyBoardAnnotation,
  type BoardAnnotationRefusalReason,
  type BoardAnnotationRequest,
  type CoachingBoardMark,
} from "./boardAnnotation"
import { boardConstraints } from "./coachingBoardConstraints"
import {
  linePlaybackPosition,
  resolveStepIndex,
  type CoachingBoardLinePlayback,
  type CoachingBoardLinePlaybackSource,
  type CoachingBoardStepTarget,
} from "./coachingBoardLinePlayback"
import {
  advancedPageRevision,
  coachingBoardSnapshot,
  explorationBranchPath,
  initialPageRevision,
  INITIAL_PAGE_REVISION,
  type CoachingBoardActor,
  type CoachingBoardExplorationBranch,
  type CoachingBoardLineMove,
  type CoachingBoardMainLine,
  type CoachingBoardTreeBranch,
  type CoachingBoardOrientation,
  type CoachingBoardOrigin,
  type CoachingBoardPageRevision,
  type CoachingBoardPendingMove,
  type CoachingBoardSnapshot,
} from "./coachingBoardSnapshot"
import { mergeExplorationBranches } from "./openingContinuationBranches"
import type { OpeningLineRef } from "./openingLineRef"
import { openingBoardPosition, startingBoardPosition } from "./openingMoves"

/**
 * A position this board can be moved to.
 *
 * The `set_board_position` tool accepts more than these — an Opening Line is
 * navigation to another board and an orientation is not a position at all —
 * and the tool layer dispatches those to their own hosts. What reaches the
 * drive as a position target is a position (`CoachingBoardToolTarget`).
 */
export type CoachingBoardPositionTarget =
  | { kind: "ply"; ply: number }
  | { kind: "alternativeMove"; alternativeMoveId: AlternativeMoveId }

/**
 * Why a drive was refused.
 *
 * The two `outside…Vocabulary` reasons answer a call the schema rejected:
 * the board never looked at a position, so it must not say one was
 * unreachable. `unreachablePosition` is reserved for a well-formed target the
 * board checked and could not reach.
 */
export type CoachingBoardDriveRefusalReason =
  | BoardAnnotationRefusalReason
  | "noLineShown"
  | "noRenderOption"
  | "outsideClosedLineUnion"
  | "outsideStepVocabulary"
  | "outsideTargetVocabulary"
  | "unreachablePosition"

export type CoachingBoardDriveRefusal = {
  constraints: CoachingBoardSnapshot["constraints"]
  kind: "refused"
  reason: CoachingBoardDriveRefusalReason
  snapshot: CoachingBoardSnapshot | null
}

export type CoachingBoardDriveState = {
  activeBranchId: AlternativeMoveId | null
  branches: readonly CoachingBoardTreeBranch[]
  /** The frozen Review's evaluation at each ply of the Game's own line.
   * Empty on an Opening Line, which the Review never saw. */
  evaluationByPly: ReadonlyMap<number, EngineEvaluation>
  /** A legal move the Player has played that Coach Engine has not confirmed.
   * The board draws it; the snapshot names it as unconfirmed (ADR 0060). */
  pendingMove: CoachingBoardPendingMove | null
  /** How far into a shown Review Moment line the board has walked. An
   * exploration path reads its own depth from the active branch instead. */
  lineIndex: number
  marks: readonly CoachingBoardMark[]
  momentByPly: ReadonlyMap<number, GameReviewCriticalMoment>
  /** The Game's own moves, or the Opening Line's, by the ply they are played
   * at — what the caption names and what `mainLine` reports. */
  movesByPly: ReadonlyMap<number, CanonicalGameMove>
  orientation: CoachingBoardOrientation
  origin: CoachingBoardOrigin
  playerChangedAtRevision: number | null
  positionsByPly: ReadonlyMap<number, BrowseBoardPosition>
  revision: number
  revisionChangedBy: CoachingBoardActor | null
  shownLine: HostTurnShowLine | null
  /** The opening study session, on a line that has an authored world. */
  study: CoachingBoardStudyState | null
  viewedPly: number
}

export type CoachingBoardDriveResult =
  | {
      kind: "applied"
      snapshot: CoachingBoardSnapshot
      state: CoachingBoardDriveState
    }
  | CoachingBoardDriveRefusal

export type CoachingBoardToolResult =
  | CoachingBoardSnapshot
  | CoachingBoardDriveRefusal

/**
 * A change to the board that is not a move of it.
 *
 * Drawing and turning both land here: the position on screen is the same one
 * afterwards, so nothing about it is cleared and a caller states exactly what
 * it changed. The revision still advances, because equal revisions mean
 * nothing changed (spec decisions 7 and 19).
 */
function changedBoard(
  state: CoachingBoardDriveState,
  by: CoachingBoardActor,
  changes: Partial<CoachingBoardDriveState>,
): CoachingBoardDriveState {
  return { ...state, ...changes, ...advancedPageRevision(state, by) }
}

/**
 * A change that moves the board off the position it was showing.
 *
 * It drops the marks and the move in flight with it: both belong to the
 * position they were made on, so nothing can survive onto a different one
 * (ADR 0059, ADR 0060).
 */
function movedBoard(
  state: CoachingBoardDriveState,
  by: CoachingBoardActor,
  changes: Partial<CoachingBoardDriveState>,
): CoachingBoardDriveState {
  return changedBoard(state, by, {
    lineIndex: 0,
    // Before `changes` so a caller can set one; every other transition clears
    // it.
    pendingMove: null,
    ...changes,
    marks: [],
  })
}

/**
 * Draw the Player's legal move before Coach Engine has confirmed it.
 *
 * The position is derived in the page, which ADR 0058 permits because deriving
 * a position is not authoring a fact. Nothing evaluates it and the snapshot
 * says so; an illegal move leaves the board alone rather than guessing
 * (ADR 0060).
 */
export function applyPendingMove(
  state: CoachingBoardDriveState,
  uci: string,
): CoachingBoardDriveState {
  const derived = derivedPositionAfter(
    driveCurrentBoardPosition(state).fen,
    uci,
  )
  return derived
    ? movedBoard(state, "player", {
        pendingMove: { derivedPosition: derived, uci },
      })
    : state
}

function derivedPositionAfter(fen: string, uci: string) {
  const setup = parseFen(fen)
  if (setup.isErr) return null
  const position = Chess.fromSetup(setup.value)
  if (position.isErr) return null
  const move = parseUci(uci)
  const chess = position.value
  if (!move || !chess.isLegal(move)) return null
  chess.play(move)
  return {
    fen: makeFen(chess.toSetup()),
    sideToMove:
      chess.turn === "white" ? ("white" as const) : ("black" as const),
  }
}

export function applyShowLine(
  state: CoachingBoardDriveState,
  by: CoachingBoardActor,
  line: HostTurnShowLine,
): CoachingBoardDriveResult {
  const render = lineRender(state, line)
  if (!render) {
    return refuseDrive("noRenderOption", state)
  }
  const next = movedBoard(state, by, {
    activeBranchId:
      line.kind === "alternativeMove" ? line.alternativeMoveId : null,
    shownLine: line,
  })
  return { kind: "applied", snapshot: snapshotFromDrive(next), state: next }
}

export function applySetPosition(
  state: CoachingBoardDriveState,
  by: CoachingBoardActor,
  target: CoachingBoardPositionTarget,
): CoachingBoardDriveResult {
  switch (target.kind) {
    case "ply": {
      if (!state.positionsByPly.has(target.ply)) {
        return refuseDrive("unreachablePosition", state)
      }
      const next = movedBoard(state, by, {
        activeBranchId: null,
        origin: originAtPly(state, target.ply),
        shownLine: null,
        viewedPly: target.ply,
      })
      return { kind: "applied", snapshot: snapshotFromDrive(next), state: next }
    }
    case "alternativeMove": {
      if (!exploredBranch(state, target.alternativeMoveId)) {
        return refuseDrive("unreachablePosition", state)
      }
      const next = movedBoard(state, by, {
        activeBranchId: target.alternativeMoveId,
        shownLine: null,
      })
      return { kind: "applied", snapshot: snapshotFromDrive(next), state: next }
    }
    default: {
      const _exhaustive: never = target
      return _exhaustive
    }
  }
}

/**
 * The Player answers the study card they are on.
 *
 * One transition, one revision: the verdict is graded and the board moves to
 * where the next card is asked from (or to the end of the line when the world
 * comes apart), the way a Player browsing moves it. An agent reading the
 * next snapshot sees the answer, the verdict, and a Player-advanced revision.
 * Only the Player answers, so this takes no actor. A board with no study
 * session is left alone.
 */
export function applyStudyAnswer(
  state: CoachingBoardDriveState,
  answer: string,
): CoachingBoardDriveState {
  if (!state.study) return state
  return studyMoved(state, studyAnswered(state.study, answer))
}

/** Build the world again: the same cards, no answers, the board on the first. */
export function applyStudyRestart(
  state: CoachingBoardDriveState,
): CoachingBoardDriveState {
  if (!state.study) return state
  return studyMoved(state, studyRestarted(state.study))
}

function studyMoved(
  state: CoachingBoardDriveState,
  study: CoachingBoardStudyState,
): CoachingBoardDriveState {
  // Every card's ply is on the line: the world verifier admits no slot asked
  // from before the first move, and the other cards sit at the line's end.
  return movedBoard(state, "player", {
    activeBranchId: null,
    shownLine: null,
    study,
    viewedPly: studyViewedPly(study),
  })
}

/**
 * Turn the board around.
 *
 * Presentation, and the one drive that cannot refuse: the position, the tree
 * and what the coach drew about the position on screen are all still true from
 * the other chair, so this changes the board without moving it.
 */
export function applyOrientation(
  state: CoachingBoardDriveState,
  by: CoachingBoardActor,
  orientation: CoachingBoardOrientation,
): CoachingBoardDriveState {
  return changedBoard(state, by, { orientation })
}

/**
 * Fold analyzed branches into the tree without showing any of them.
 *
 * Evaluation is the gate's first half: the active branch and the shown line
 * are left exactly where the Player put them, and a separate show or
 * set-position call is what moves the board.
 */
export function applyExplorationBranches(
  state: CoachingBoardDriveState,
  by: CoachingBoardActor,
  minted: readonly CoachingBoardExplorationBranch[],
): CoachingBoardDriveState {
  if (minted.length === 0) return state
  return foldedBoard(state, by, minted, {})
}

/**
 * Fold branches into the tree, stamped with the revision they arrive at.
 *
 * The merge is the only step that knows which branch is new, so it is the one
 * that stamps: a re-analyzed branch keeps the arrival it already had, because
 * it is not new to an agent that has seen it and re-analysing an old line must
 * not read as the Player having just played it. The stamp is read off the
 * moved state rather than recomputed, so a branch cannot claim a revision the
 * board never landed on.
 */
function foldedBoard(
  state: CoachingBoardDriveState,
  by: CoachingBoardActor,
  minted: readonly CoachingBoardExplorationBranch[],
  changes: Partial<CoachingBoardDriveState>,
): CoachingBoardDriveState {
  const moved = movedBoard(state, by, changes)
  return {
    ...moved,
    branches: mergeExplorationBranches(state.branches, minted, (branch) => ({
      ...branch,
      addedAtRevision: moved.revision,
      addedBy: by,
    })),
  }
}

/**
 * Fold an evaluated line in and follow it to its end.
 *
 * The Player's own move is the other half of the gate: they played it, so the
 * board goes there. An agent evaluating a line folds it without moving
 * (`applyExplorationBranches`) and shows it in a separate call, which is why
 * this one is the Player's and takes no actor.
 *
 * Folding and following are one revision, not two. The revision between them
 * is never published, and a branch stamped with it would report arriving at a
 * revision no reader ever sees — while the agent's own evaluation, a single
 * transition, stamps the revision its result carries.
 */
export function applyExploredLine(
  state: CoachingBoardDriveState,
  minted: readonly CoachingBoardExplorationBranch[],
): CoachingBoardDriveState {
  const leaf = minted.at(-1)
  if (!leaf) return state
  return foldedBoard(state, "player", minted, {
    activeBranchId: leaf.alternativeMoveId,
    shownLine: null,
  })
}

/**
 * Verify what the coach wants to point at, then draw it or refuse.
 *
 * ADR 0059's verify-then-draw gate, and the sibling of evaluate-then-show:
 * the page checks each relation against the FEN on screen, so a claim the
 * position does not support never becomes an arrow. `revision` is what the
 * agent believed it was annotating — a mismatch means the board moved between
 * the read and the draw, and the marks would describe a different position.
 */
export function applyBoardAnnotation(
  state: CoachingBoardDriveState,
  request: { requests: readonly BoardAnnotationRequest[]; revision: number },
): CoachingBoardDriveResult {
  if (request.revision !== state.revision) {
    return refuseDrive("staleRevision", state)
  }
  const outcome = verifyBoardAnnotation({
    fen: driveCurrentBoardPosition(state).fen,
    groundedMoveUcis: groundedMoveUcis(state),
    requests: request.requests,
  })
  if (outcome.kind === "refused") {
    return refuseDrive(outcome.reason, state)
  }
  // Drawing does not move the board off the position, so it changes the board
  // rather than moving it, and states the one thing it clears. Only the agent
  // draws.
  const next = changedBoard(state, "agent", {
    marks: outcome.marks,
    pendingMove: null,
  })
  return { kind: "applied", snapshot: snapshotFromDrive(next), state: next }
}

/**
 * The moves ChenChess has already put on this board.
 *
 * A `move` mark may name one of these and nothing else; the verifier then
 * discards any that are not playable in the position on screen, so a move
 * from elsewhere in the tree cannot be drawn here.
 */
function groundedMoveUcis(state: CoachingBoardDriveState): ReadonlySet<string> {
  const ucis = new Set(state.branches.map((branch) => branch.moveUci))
  const branch = activeExploredBranch(state)
  if (branch?.strongestReply?.kind === "offered") {
    ucis.add(branch.strongestReply.uci)
  }
  const lines = viewedLines(state)
  for (const move of [...(lines?.best ?? []), ...(lines?.refutation ?? [])]) {
    ucis.add(move.uci)
  }
  return ucis
}

/** Which of the Review Moment's lines each source names. */
const momentLineBySource = {
  engineBest: "best",
  playedMoveRefutation: "refutation",
} as const satisfies Record<
  CoachingBoardLinePlaybackSource,
  "best" | "refutation"
>

/**
 * A shown Review Moment line, with the position it is played from.
 *
 * The best line replaces the move at this ply, so it starts before it. The
 * refutation answers the move that was played, so it starts after it — the
 * two root one ply apart, and walking either from the wrong one is a sequence
 * of illegal moves. A line the board cannot root cannot be walked at all; the
 * arrow still names its first move, and only the transport goes away.
 */
function rootedPlayback(state: CoachingBoardDriveState): RootedPlayback | null {
  const shown = state.shownLine
  if (shown?.kind !== "engineBest" && shown?.kind !== "playedMoveRefutation") {
    return null
  }
  const steps = viewedLines(state)?.[momentLineBySource[shown.kind]] ?? []
  if (steps.length === 0) return null
  const ply =
    shown.kind === "playedMoveRefutation"
      ? state.viewedPly + 1
      : state.viewedPly
  const base = state.positionsByPly.get(ply)
  if (!base) return null
  return {
    baseFen: base.fen,
    // Every transition that could invalidate it resets the index to zero, so
    // it is always inside the line it names.
    index: state.lineIndex,
    source: shown.kind,
    steps: steps.map((move) => ({ san: move.san, uci: move.uci })),
  }
}

type RootedPlayback = CoachingBoardLinePlayback & { baseFen: string }

/** The line the board can walk right now, if it is showing one. */
export function drivePlayback(
  state: CoachingBoardDriveState,
): CoachingBoardLinePlayback | null {
  const rooted = rootedPlayback(state)
  if (!rooted) return null
  return { index: rooted.index, source: rooted.source, steps: rooted.steps }
}

/**
 * Walk the shown line one step, or to a named end of it.
 *
 * The position is derived here rather than on the read path, so a line that
 * does not play from its root is refused at the transition instead of
 * throwing while the board renders.
 */
export function applyStepLine(
  state: CoachingBoardDriveState,
  by: CoachingBoardActor,
  target: CoachingBoardStepTarget,
): CoachingBoardDriveResult {
  const playback = rootedPlayback(state)
  if (!playback) return refuseDrive("noLineShown", state)
  const index = resolveStepIndex(playback, target)
  if (index === null) return refuseDrive("unreachablePosition", state)
  if (!walkedPosition(playback, index)) {
    return refuseDrive("unreachablePosition", state)
  }
  const next = movedBoard(state, by, { lineIndex: index })
  return { kind: "applied", snapshot: snapshotFromDrive(next), state: next }
}

/**
 * The position a line has reached at `index`, or null if it does not play.
 *
 * Index 0 is the line's own root rather than a special case: walking zero
 * moves from the root *is* the root. That matters for the refutation, which
 * roots one ply later than the moment — rendering the viewed ply there would
 * show the position before the move the line answers.
 */
function walkedPosition(playback: RootedPlayback, index: number) {
  return linePlaybackPosition(
    playback.baseFen,
    playback.steps.slice(0, index).map((step) => step.uci),
  )
}

export function snapshotFromDrive(
  state: CoachingBoardDriveState,
): CoachingBoardSnapshot {
  return coachingBoardSnapshot({
    activeBranchId: state.activeBranchId,
    branches: state.branches,
    constraints: boardConstraints(),
    currentPosition: driveCurrentPosition(state),
    linePlayback: drivePlayback(state),
    mainLine: driveMainLine(state),
    marks: state.marks,
    orientation: state.orientation,
    origin: state.origin,
    pendingMove: state.pendingMove,
    playerChangedAtRevision: state.playerChangedAtRevision,
    revision: state.revision,
    revisionChangedBy: state.revisionChangedBy,
    shownLine: state.shownLine,
    study: coachingBoardStudy(state.study),
    viewedPly: state.viewedPly,
  })
}

/**
 * Where the viewed ply sits on the Game's own line or the Opening Line.
 *
 * Read off the moves rather than the position so the agent gets the move in
 * the caption's own words: the position at `viewedPly` is the one before the
 * move at that ply, so the move at `viewedPly` is what the line went on to
 * play and the one before it is what reached the position.
 */
export function driveMainLine(
  state: CoachingBoardDriveState,
): CoachingBoardMainLine {
  return {
    continuesWith: lineMoveAt(state, state.viewedPly),
    evaluation: state.evaluationByPly.get(state.viewedPly) ?? null,
    lastPly: Math.max(0, ...state.movesByPly.keys()),
    reachedBy: lineMoveAt(state, state.viewedPly - 1),
  }
}

function lineMoveAt(
  state: CoachingBoardDriveState,
  ply: number,
): CoachingBoardLineMove | null {
  const move = state.movesByPly.get(ply)
  if (!move) return null
  return {
    label: moveLabel(move),
    ply: move.ply,
    san: move.san,
    side: move.side,
    uci: move.uci,
  }
}

/**
 * What the snapshot reports as `currentPosition`.
 *
 * Deliberately blind to `pendingMove`: this is the last Position Coach Engine
 * confirmed, and an agent reading it is reasoning about something the Engine
 * has evaluated. Reporting the derived position here instead would leave
 * `pathFromRoot` unable to explain how the board reached it, which is the
 * alternative ADR 0060 rejects.
 */
export function driveCurrentPosition(state: CoachingBoardDriveState) {
  const position = confirmedBoardPosition(state)
  return { fen: position.fen, sideToMove: position.sideToMove }
}

/**
 * What the board draws.
 *
 * The Player's own move outranks everything here: they played it, so the board
 * shows it while the Engine confirms it. The snapshot does not follow — see
 * `driveCurrentPosition` (ADR 0060).
 */
export function driveCurrentBoardPosition(state: CoachingBoardDriveState) {
  const pending = state.pendingMove
  if (pending) {
    return {
      fen: pending.derivedPosition.fen,
      occupied: presentationPiecesFromFen(pending.derivedPosition.fen).map(
        ({ piece, square }) => ({ piece, square }),
      ),
      sideToMove: pending.derivedPosition.sideToMove,
    }
  }
  return confirmedBoardPosition(state)
}

function confirmedBoardPosition(state: CoachingBoardDriveState) {
  const walked = walkedLinePosition(state)
  if (walked) return walked
  const branch = activeExploredBranch(state)
  if (branch) {
    return {
      fen: branch.resultingPosition.fen,
      occupied: branch.resultingPosition.occupied,
      sideToMove: branch.resultingPosition.sideToMove,
    }
  }
  return state.positionsByPly.get(state.viewedPly) ?? startingBoardPosition()
}

/**
 * Where a partly walked Review Moment line has reached.
 *
 * Only those lines need this: an exploration path stands on a branch whose
 * position the engine already returned.
 */
function walkedLinePosition(state: CoachingBoardDriveState) {
  const playback = rootedPlayback(state)
  if (!playback) return null
  return walkedPosition(playback, playback.index)
}

/**
 * The moves between the viewed ply and the position on the board.
 *
 * Read from the branch tree rather than remembered separately, so a position
 * the host agent set is described by the same line the Player's next move
 * extends.
 */
export function explorationLineUcis(
  state: CoachingBoardDriveState,
): readonly string[] {
  return explorationBranchPath(state.branches, state.activeBranchId).map(
    (branch) => branch.moveUci,
  )
}

/**
 * The engine's move on the shown position, when the engine has spoken about
 * it and the board is not already showing a line of its own.
 *
 * An explored position is answered by the StrongestReply that came back with
 * the branch; a position on the Game's own line by the Review's best line,
 * which exists at Critical Moments. While a line is shown the board is making
 * a different point, and a second engine move would argue with it.
 */
export function engineArrowUci(
  state: CoachingBoardDriveState,
): string | undefined {
  if (state.shownLine) return undefined
  const branch = activeExploredBranch(state)
  if (branch) {
    return branch.strongestReply?.kind === "offered"
      ? branch.strongestReply.uci
      : undefined
  }
  return viewedLines(state)?.best[0]?.uci
}

/**
 * The position a branch's move was played from.
 *
 * A move only reads as a move beside the position it was played in, so the
 * board names an explored move from its parent rather than from the position
 * it produced.
 */
export function branchSourceFen(
  state: CoachingBoardDriveState,
  branch: CoachingBoardExplorationBranch,
): string {
  const path = explorationBranchPath(state.branches, branch.alternativeMoveId)
  const parent = path.at(-2)
  if (parent) return parent.resultingPosition.fen
  // A branch with no parent branch was played from the viewed ply, and every
  // reachable ply has a position: `applySetPosition` refuses the ones that do
  // not.
  return (state.positionsByPly.get(state.viewedPly) ?? startingBoardPosition())
    .fen
}

export function activeExploredBranch(state: CoachingBoardDriveState) {
  return state.activeBranchId
    ? exploredBranch(state, state.activeBranchId)
    : undefined
}

export function shownLineMoveUci(state: CoachingBoardDriveState) {
  const shown = state.shownLine
  if (!shown) return null
  switch (shown.kind) {
    // The arrow names the line's next move, so walking the line moves it
    // along rather than leaving it on the first move for ever.
    case "engineBest":
    case "playedMoveRefutation": {
      const playback = drivePlayback(state)
      if (playback) return playback.steps[playback.index]?.uci ?? null
      return (
        viewedLines(state)?.[momentLineBySource[shown.kind]][0]?.uci ?? null
      )
    }
    case "alternativeMove":
      return exploredBranch(state, shown.alternativeMoveId)?.moveUci ?? null
    default: {
      const _exhaustive: never = shown
      return _exhaustive
    }
  }
}

export function driveRefusal(
  reason: CoachingBoardDriveRefusalReason,
  snapshot: CoachingBoardSnapshot | null,
): CoachingBoardDriveRefusal {
  return {
    constraints: boardConstraints(),
    kind: "refused",
    reason,
    snapshot,
  }
}

function refuseDrive(
  reason: CoachingBoardDriveRefusalReason,
  state: CoachingBoardDriveState,
): CoachingBoardDriveRefusal {
  return driveRefusal(reason, snapshotFromDrive(state))
}

/**
 * Where a board mounting now picks up.
 *
 * `CoachingBoardMount` keys its children on the target, so changing origin
 * tears the drive down — and the revision is a *page* revision, monotonic
 * across moments, lines and origins (spec decision 7). The page hands the
 * next board the revision the last one reached, and names whoever navigated
 * there. A board with no page above it starts one.
 */
export function gameBoardDrive({
  branches = [],
  gameImportId,
  importedGame,
  pageRevision = initialPageRevision,
  review,
  viewedPly,
}: {
  branches?: readonly CoachingBoardExplorationBranch[]
  gameImportId: GameImportId
  importedGame: ImportedGame
  pageRevision?: CoachingBoardPageRevision
  review: GameReview | null
  viewedPly?: number
}): CoachingBoardDriveState {
  const moves = importedGame.game.moves
  const ply = viewedPly ?? review?.criticalMoments[0]?.ply ?? moves[0]?.ply ?? 1
  const positionsByPly = positionsAlongMoves(moves, ply)
  const momentByPly = new Map(
    (review?.criticalMoments ?? []).map(
      (moment) => [moment.ply, moment] as const,
    ),
  )
  return {
    activeBranchId: null,
    branches: loadedBranches(branches),
    evaluationByPly: new Map(
      (review?.evaluationTimeline ?? []).map(
        (point) => [point.ply, point.evaluation] as const,
      ),
    ),
    lineIndex: 0,
    marks: [],
    momentByPly,
    movesByPly: movesByPly(moves),
    // The Player's own side, the side the board has always opened from.
    orientation: reviewSideOrientation(importedGame.reviewSide),
    origin: {
      gameImportId,
      kind: "reviewMoment",
      ply,
      reviewMomentId: momentByPly.get(ply)?.criticalMomentId ?? null,
      reviewSide: importedGame.reviewSide,
    },
    playerChangedAtRevision: pageRevision.playerChangedAtRevision,
    positionsByPly,
    pendingMove: null,
    revision: pageRevision.revision,
    revisionChangedBy: pageRevision.revisionChangedBy,
    shownLine: null,
    study: null,
    viewedPly: ply,
  }
}

function movesByPly(moves: readonly CanonicalGameMove[]) {
  return new Map(moves.map((move) => [move.ply, move] as const))
}

/** Seeded from the page the same way a game board is — see `gameBoardDrive`. */
export function openingBoardDrive({
  activeBranchId = null,
  branches = [],
  eco,
  moves,
  name,
  openingLineRef,
  pageRevision = initialPageRevision,
  viewedPly,
  world = null,
}: {
  activeBranchId?: AlternativeMoveId | null
  branches?: readonly CoachingBoardExplorationBranch[]
  eco: string
  moves: readonly CanonicalGameMove[]
  name: string
  openingLineRef: OpeningLineRef
  pageRevision?: CoachingBoardPageRevision
  viewedPly: number
  /** The authored study world for this line, when it has one (ADR 0063). */
  world?: OpeningStudyWorld | null
}): CoachingBoardDriveState {
  const lastPly = moves.at(-1)?.ply
  const positionsByPly = positionsAlongMoves(moves, viewedPly)
  if (lastPly !== undefined) {
    positionsByPly.set(lastPly + 1, openingBoardPosition(moves, lastPly + 1))
  }
  const study = world ? openingStudyState(world, moves.length) : null
  return {
    activeBranchId,
    branches: loadedBranches(branches),
    evaluationByPly: new Map(),
    lineIndex: 0,
    marks: [],
    momentByPly: new Map(),
    movesByPly: movesByPly(moves),
    study,
    // An Opening Line belongs to whoever plays it, so it opens from White's
    // side and the agent turns it when the Player asks for the other chair.
    orientation: "white",
    origin: {
      eco,
      kind: "openingLine",
      name,
      openingLineRef,
    },
    playerChangedAtRevision: pageRevision.playerChangedAtRevision,
    positionsByPly,
    pendingMove: null,
    revision: pageRevision.revision,
    revisionChangedBy: pageRevision.revisionChangedBy,
    shownLine: null,
    // A session opens on its first card, but only when nobody asked for a
    // position: the address, the Player's own navigation, and the agent's
    // set_board_position all name a ply other than the line's end, and a
    // board mounted on a named ply stays there (ADR 0063). Arranged here
    // rather than in an effect so a fresh page reads as unchanged by anyone.
    viewedPly:
      study && viewedPly === moves.length ? studyViewedPly(study) : viewedPly,
  }
}

function lineRender(
  state: CoachingBoardDriveState,
  line: HostTurnShowLine,
): boolean {
  switch (line.kind) {
    case "engineBest":
      return (viewedLines(state)?.best.length ?? 0) > 0
    case "playedMoveRefutation":
      return (viewedLines(state)?.refutation.length ?? 0) > 0
    case "alternativeMove":
      return exploredBranch(state, line.alternativeMoveId) !== undefined
    default: {
      const _exhaustive: never = line
      return _exhaustive
    }
  }
}

/** The Critical Moment on the ply the board is showing, if the Review found
 * one there. */
export function viewedMoment(state: CoachingBoardDriveState) {
  return state.momentByPly.get(state.viewedPly) ?? null
}

export function viewedLines(state: CoachingBoardDriveState) {
  return viewedMoment(state)?.objective.lines ?? null
}

function exploredBranch(
  state: CoachingBoardDriveState,
  alternativeMoveId: AlternativeMoveId,
) {
  return state.branches.find(
    (branch) => branch.alternativeMoveId === alternativeMoveId,
  )
}

function positionsAlongMoves(
  moves: readonly CanonicalGameMove[],
  viewedPly: number,
) {
  const positionsByPly = new Map(
    moves.map((move) => [move.ply, browseBoardAtPly(moves, move.ply)] as const),
  )
  if (!positionsByPly.has(viewedPly)) {
    positionsByPly.set(viewedPly, browseBoardAtPly(moves, viewedPly))
  }
  return positionsByPly
}

function originAtPly(
  state: CoachingBoardDriveState,
  ply: number,
): CoachingBoardOrigin {
  if (state.origin.kind === "openingLine") return state.origin
  return {
    ...state.origin,
    ply,
    reviewMomentId: state.momentByPly.get(ply)?.criticalMomentId ?? null,
  }
}

/**
 * Stamp the branches a board loads with as already there.
 *
 * Recalled opening exploration and a game board handed its branches both
 * predate the page, so no actor added them: an agent reading `addedBy: null`
 * learns the branch was on the board before it looked, not that the Player
 * played it while it was away.
 */
export function loadedBranches(
  branches: readonly CoachingBoardExplorationBranch[],
): CoachingBoardTreeBranch[] {
  return branches.map((branch) => ({
    ...branch,
    addedAtRevision: INITIAL_PAGE_REVISION,
    addedBy: null,
  }))
}
