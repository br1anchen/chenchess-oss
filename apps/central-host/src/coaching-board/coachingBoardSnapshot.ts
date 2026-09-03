import type {
  AlternativeMoveEvaluation,
  AlternativeMoveId,
  BranchParent,
  BranchRef,
  EngineEvaluation,
  GameImportId,
  HostTurnShowLine,
  PositionSnapshot,
  ReviewSide,
  StrongestReply,
} from "@chenchess/coach-engine-sdk"

import type { CoachingBoardMark } from "./boardAnnotation"
import type { CoachingBoardLinePlayback } from "./coachingBoardLinePlayback"
import type { CoachingBoardStudy } from "./coachingBoardStudy"
import type { OpeningLineRef } from "./openingLineRef"

/**
 * What the board actually reads from an exploration branch.
 *
 * An engine `AlternativeMoveResult` satisfies this, so game exploration
 * passes through unchanged. Opening exploration is built in the page from
 * the stateless analysis route, which grounds a position as one FEN and has
 * no `PositionSnapshot` to give (ADR 0058).
 *
 * Every branch carries a `positionRef` for the position it produced, which is
 * what lets the Player's next move be evaluated from this branch alone instead
 * of re-walking the line. A game branch carries the engine's own reference; an
 * opening branch mints one from its FEN, as it already does for its root
 * (ADR 0058 — the web mints identifiers where the engine offers none).
 */
export type CoachingBoardExplorationBranch = {
  alternativeMoveId: AlternativeMoveId
  branchRef: BranchRef
  evaluation: AlternativeMoveEvaluation
  moveUci: string
  parent: BranchParent
  resultingPosition: Pick<
    PositionSnapshot,
    "fen" | "occupied" | "positionRef" | "sideToMove"
  >
  /** The engine answers a Player line with its strongest reply; opening
   * analysis returns none, so the board draws nothing for those. */
  strongestReply?: StrongestReply
}

/**
 * Who caused the board to change.
 *
 * WebMCP has no server-to-agent push, so an agent cannot be told that the
 * Player dragged a piece while it was idle. Naming the actor on the next
 * result is the honest substitute: a stale agent reads who moved the board
 * instead of re-deriving that something did.
 */
export type CoachingBoardActor = "agent" | "player"

/** When a branch joined the tree, and who put it there. Null for the branches
 * the board loaded with, which nobody added while the agent was watching. */
type CoachingBoardBranchArrival = {
  addedAtRevision: number
  addedBy: CoachingBoardActor | null
}

/**
 * A branch as the board's tree holds it: what the engine minted, plus the
 * arrival only the board knows.
 *
 * Named for the tree rather than for exploration because
 * `CoachingBoardExplorationBranch` is the structural shape an engine
 * `AlternativeMoveResult` already satisfies, and this one deliberately does
 * not — nothing outside this page can produce it.
 */
export type CoachingBoardTreeBranch = CoachingBoardExplorationBranch &
  CoachingBoardBranchArrival

export type CoachingBoardOrigin =
  | {
      gameImportId: GameImportId
      kind: "reviewMoment"
      ply: number
      reviewMomentId: string | null
      /** Whose moves are the Player's own in this Game. */
      reviewSide: ReviewSide
    }
  | {
      kind: "openingLine"
      openingLineRef: OpeningLineRef
      eco: string
      name: string
    }

export type CoachingBoardPosition = {
  fen: string
  sideToMove: "black" | "white"
}

/** One move of the Game's own line or the Opening Line, named the way the
 * caption under the board names it ("9. Nxc4", "8… Nxc4"). */
export type CoachingBoardLineMove = {
  label: string
  ply: number
  san: string
  side: "black" | "white"
  uci: string
}

/**
 * Where the viewed ply sits on the Game's own line or the Opening Line.
 *
 * The board at `viewedPly` shows the position *before* the move at that ply
 * is played — a Critical Moment is named by the move played from it — so
 * `continuesWith` is the move the caption shows and `reachedBy` is the one
 * that produced the position. Without this the agent has a FEN and a number,
 * and is forbidden from reconstructing "what did I play here" from either.
 * The evaluation is the frozen Review's, for the position at this ply; an
 * Opening Line carries none, and neither does a Game with no Review.
 */
export type CoachingBoardMainLine = {
  continuesWith: CoachingBoardLineMove | null
  evaluation: EngineEvaluation | null
  /** The last ply the line has, so "end" needs no guessing. */
  lastPly: number
  reachedBy: CoachingBoardLineMove | null
}

/**
 * Which side of the board the Player is looking from.
 *
 * Presentation and nothing else: it changes no position, reaches no new one,
 * and grounds nothing. A Game reviewed from both sides is drawn from White's,
 * the same way the board has always drawn it.
 */
export const COACHING_BOARD_ORIENTATIONS = ["black", "white"] as const

export type CoachingBoardOrientation =
  (typeof COACHING_BOARD_ORIENTATIONS)[number]

/**
 * A move the Player has played on the board that Coach Engine has not confirmed.
 *
 * The board draws it immediately so a legal drag lands in a frame rather than
 * after a round trip, and this field is how the snapshot stays honest about it:
 * `currentPosition` is still the last position the Engine confirmed, and this
 * says what the Player is additionally looking at. There is deliberately no
 * evaluation here and none may be inferred — the resulting position has not
 * been evaluated by anything (ADR 0060).
 */
export type CoachingBoardPendingMove = {
  derivedPosition: CoachingBoardPosition
  uci: string
}

export type CoachingBoardBranch = {
  /** The revision this branch joined the tree at, and who added it — null for
   * the branches the board loaded with. */
  addedAtRevision: number
  addedBy: CoachingBoardActor | null
  active: boolean
  alternativeMoveId: AlternativeMoveId
  evaluation: EngineEvaluation
  moveUci: string
  parent: BranchParent
}

export type CoachingBoardConstraints = {
  kind: "constraints"
  sentences: readonly string[]
}

export type CoachingBoardSnapshot = {
  constraints: CoachingBoardConstraints
  /** The last position Coach Engine confirmed, which is the position on screen
   * except while a move is in flight — see `pendingMove` (ADR 0060). */
  currentPosition: CoachingBoardPosition
  /** The line the board can walk, and how far into it the board has come.
   * Null when no line is shown and no branch is active. */
  linePlayback: CoachingBoardLinePlayback | null
  /** What the coach drew about the position on screen. Scoped to one
   * revision: any move of the board clears it (ADR 0059). */
  marks: readonly CoachingBoardMark[]
  exploration: {
    activeBranchId: AlternativeMoveId | null
    branches: readonly CoachingBoardBranch[]
    pathFromRoot: readonly AlternativeMoveId[]
  }
  kind: "coachingBoard"
  mainLine: CoachingBoardMainLine
  origin: CoachingBoardOrigin
  orientation: CoachingBoardOrientation
  /** Null whenever nothing is in flight, which is almost always. */
  pendingMove: CoachingBoardPendingMove | null
  /**
   * The last revision the Player advanced the board to.
   *
   * `revisionChangedBy` only names the most recent change, so the agent's own
   * next call overwrites it — and the three things the Player does that add no
   * branch (browse a ply, select a branch, walk a line) would leave no trace
   * at all. This one survives the agent's calls: higher than a revision the
   * agent already saw means the Player changed the board while it was away,
   * and the moves they added are the branches whose `addedAtRevision` is
   * higher than it.
   */
  playerChangedAtRevision: number | null
  revision: number
  /** Who advanced the revision to this one, and null while it is still the
   * revision the page loaded on — a board mounted by a navigation opens on
   * that navigation's actor, not on null. */
  revisionChangedBy: CoachingBoardActor | null
  shownLine: HostTurnShowLine | null
  /** The opening study session on this line, when the line has an authored
   * world (ADR 0063). Null on a Game board and on an opening with no world. */
  study: CoachingBoardStudy | null
  viewedPly: number
}

export type CoachingBoardLobbyResult = {
  constraints: CoachingBoardConstraints
  kind: "lobby"
}

export type CoachingBoardSnapshotInput = {
  activeBranchId: AlternativeMoveId | null
  branches: readonly CoachingBoardTreeBranch[]
  constraints: CoachingBoardConstraints
  currentPosition: CoachingBoardPosition
  linePlayback: CoachingBoardLinePlayback | null
  mainLine: CoachingBoardMainLine
  marks: readonly CoachingBoardMark[]
  orientation: CoachingBoardOrientation
  origin: CoachingBoardOrigin
  pendingMove: CoachingBoardPendingMove | null
  playerChangedAtRevision: number | null
  revision: number
  revisionChangedBy: CoachingBoardActor | null
  shownLine: HostTurnShowLine | null
  study: CoachingBoardStudy | null
  viewedPly: number
}

export function coachingBoardSnapshot(
  input: CoachingBoardSnapshotInput,
): CoachingBoardSnapshot {
  const branches = input.branches.map((branch) => ({
    addedAtRevision: branch.addedAtRevision,
    addedBy: branch.addedBy,
    active: branch.alternativeMoveId === input.activeBranchId,
    alternativeMoveId: branch.alternativeMoveId,
    evaluation: branch.evaluation.selectedMove,
    moveUci: branch.moveUci,
    parent: branch.parent,
  }))
  return {
    constraints: input.constraints,
    currentPosition: input.currentPosition,
    exploration: {
      activeBranchId: input.activeBranchId,
      branches,
      pathFromRoot: pathFromRoot(input.branches, input.activeBranchId),
    },
    kind: "coachingBoard",
    linePlayback: input.linePlayback,
    mainLine: input.mainLine,
    marks: input.marks,
    orientation: input.orientation,
    origin: input.origin,
    pendingMove: input.pendingMove,
    playerChangedAtRevision: input.playerChangedAtRevision,
    revision: input.revision,
    revisionChangedBy: input.revisionChangedBy,
    shownLine: input.shownLine,
    study: input.study,
    viewedPly: input.viewedPly,
  }
}

/** The revision a page loads on, before anyone has changed anything. */
export const INITIAL_PAGE_REVISION = 1

/**
 * How far the page's revision has come, and who brought it here.
 *
 * The three travel together because they only ever change together: a
 * revision handed on without its actor is exactly the silent "nothing
 * changed" decision 18 exists to prevent.
 */
export type CoachingBoardPageRevision = {
  playerChangedAtRevision: number | null
  revision: number
  revisionChangedBy: CoachingBoardActor | null
}

/** A page nobody has changed yet: the revision it loads on, and no actor. */
export const initialPageRevision: CoachingBoardPageRevision = {
  playerChangedAtRevision: null,
  revision: INITIAL_PAGE_REVISION,
  revisionChangedBy: null,
}

/**
 * Advance the page revision and say who advanced it.
 *
 * Every transition that changes the board goes through here, so the next one
 * added cannot forget to say who (ADR 0059). `playerChangedAtRevision` is
 * remembered rather than derived from `revisionChangedBy`, which the agent's
 * own next call overwrites — see the snapshot's field.
 */
export function advancedPageRevision(
  current: CoachingBoardPageRevision,
  by: CoachingBoardActor,
): CoachingBoardPageRevision {
  const revision = current.revision + 1
  return {
    playerChangedAtRevision:
      by === "player" ? revision : current.playerChangedAtRevision,
    revision,
    revisionChangedBy: by,
  }
}

function pathFromRoot(
  branches: readonly CoachingBoardExplorationBranch[],
  activeBranchId: AlternativeMoveId | null,
): AlternativeMoveId[] {
  return explorationBranchPath(branches, activeBranchId).map(
    (branch) => branch.alternativeMoveId,
  )
}

/** The branches from the origin down to the active one, in play order. */
export function explorationBranchPath(
  branches: readonly CoachingBoardExplorationBranch[],
  activeBranchId: AlternativeMoveId | null,
): CoachingBoardExplorationBranch[] {
  if (!activeBranchId) return []
  const byRef = new Map(
    branches.map((branch) => [branch.branchRef, branch] as const),
  )
  const byId = new Map(
    branches.map((branch) => [branch.alternativeMoveId, branch] as const),
  )
  const path: CoachingBoardExplorationBranch[] = []
  let current = byId.get(activeBranchId)
  const seen = new Set<AlternativeMoveId>()
  while (current && !seen.has(current.alternativeMoveId)) {
    seen.add(current.alternativeMoveId)
    path.push(current)
    if (current.parent.kind === "root") break
    current = byRef.get(current.parent.branchRef)
  }
  return path.reverse()
}
