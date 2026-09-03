import { useRef, useState } from "react"
import type { HostTurnShowLine } from "@chenchess/coach-engine-sdk"

import { unavailableBoardCoachResult } from "./coachingBoardCoachTools"
import { lobbyResult } from "./coachingBoardConstraints"
import type { CoachingBoardPage } from "./coachingBoardPage"
import type { BoardAnnotationRequest } from "./boardAnnotation"
import type { CoachingBoardStepTarget } from "./coachingBoardLinePlayback"
import {
  applyBoardAnnotation,
  applyOrientation,
  applyPendingMove,
  applyStepLine,
  applyExplorationBranches,
  applyExploredLine,
  applySetPosition,
  applyShowLine,
  applyStudyAnswer,
  applyStudyRestart,
  snapshotFromDrive,
  type CoachingBoardDriveState,
  type CoachingBoardPositionTarget,
  type CoachingBoardToolResult,
} from "./coachingBoardDrive"
import type {
  CoachingBoardActor,
  CoachingBoardExplorationBranch,
  CoachingBoardOrientation,
} from "./coachingBoardSnapshot"
import type { CoachingBoardToolHost } from "./useCoachingBoardTools"

export function useCoachingBoardDrive(
  initial: CoachingBoardDriveState | (() => CoachingBoardDriveState),
  page?: CoachingBoardPage,
) {
  const [state, setState] = useState(initial)
  const stateRef = useRef(state)
  stateRef.current = state
  const snapshot = snapshotFromDrive(state)
  const snapshotRef = useRef(snapshot)
  snapshotRef.current = snapshot

  /**
   * Tell the page where the board has come to, at the moment it gets there.
   *
   * Not on render and not in an effect: an agent can navigate in the same
   * task as the call that moved the board, and the page would then seed the
   * next board from a revision this one had already left behind. The three
   * counts are copied out rather than the state handed over whole, so a page
   * that outlives this board does not keep its positions and moments alive
   * with them.
   */
  function reached(next: CoachingBoardDriveState) {
    page?.reachedRevision({
      playerChangedAtRevision: next.playerChangedAtRevision,
      revision: next.revision,
      revisionChangedBy: next.revisionChangedBy,
    })
  }

  /** A transition that may have been refused. A refusal commits nothing. */
  function commit(
    result: ReturnType<typeof applyShowLine>,
  ): CoachingBoardToolResult {
    return result.kind === "applied" ? commitDrive(result.state) : result
  }

  function annotateBoard(request: {
    requests: readonly BoardAnnotationRequest[]
    revision: number
  }) {
    return commit(applyBoardAnnotation(stateRef.current, request))
  }

  /**
   * The board's transitions, as one actor.
   *
   * Which of the two facades a caller reaches for is the whole answer to who
   * moved the board, so no call site names an actor and none can name the
   * wrong one — which is exactly how the Player's branch strip came to drive
   * the board as the agent.
   */
  function boundDrive(by: CoachingBoardActor) {
    return {
      applyBranches: (minted: readonly CoachingBoardExplorationBranch[]) =>
        commitDrive(applyExplorationBranches(stateRef.current, by, minted)),
      setBoardPosition: (target: CoachingBoardPositionTarget) =>
        commit(applySetPosition(stateRef.current, by, target)),
      showLine: (line: HostTurnShowLine) =>
        commit(applyShowLine(stateRef.current, by, line)),
      stepLine: (target: CoachingBoardStepTarget) =>
        commit(applyStepLine(stateRef.current, by, target)),
      turnBoard: (orientation: CoachingBoardOrientation) =>
        commitDrive(applyOrientation(stateRef.current, by, orientation)),
    }
  }

  /** The one place a new board state becomes the board. */
  function commitDrive(next: CoachingBoardDriveState) {
    stateRef.current = next
    snapshotRef.current = snapshotFromDrive(next)
    reached(next)
    setState(next)
    return snapshotRef.current
  }

  /** Draw the Player's move now; the engine's answer replaces it. */
  function beginPendingMove(uci: string) {
    return commitDrive(applyPendingMove(stateRef.current, uci))
  }

  function followExploredLine(
    minted: readonly CoachingBoardExplorationBranch[],
  ) {
    return commitDrive(applyExploredLine(stateRef.current, minted))
  }

  const agent = boundDrive("agent")
  const player = {
    ...boundDrive("player"),
    // The study session is the Player's alone: the agent reads it from the
    // snapshot and never answers a card for them.
    answerStudyCard: (answer: string) =>
      commitDrive(applyStudyAnswer(stateRef.current, answer)),
    restartStudy: () => commitDrive(applyStudyRestart(stateRef.current)),
  }

  const host: CoachingBoardToolHost = {
    annotateBoard,
    // Only an Opening Line origin can evaluate a continuation; the game
    // surface leaves this default in place and answers unavailable.
    evaluateOpeningContinuation: () =>
      unavailableBoardCoachResult(snapshotRef.current),
    evaluatePlayerLine: () => unavailableBoardCoachResult(snapshotRef.current),
    findOpeningLine: () => lobbyResult(),
    listCriticalMoments: () => unavailableBoardCoachResult(snapshotRef.current),
    listPlayedOpenings: () => lobbyResult(),
    listRecentProfileGames: () => lobbyResult(),
    openOpeningLine: () => lobbyResult(),
    openReviewedGame: () => lobbyResult(),
    openReviewMomentInPlace: () =>
      unavailableBoardCoachResult(snapshotRef.current),
    readSnapshot: () => snapshotRef.current,
    searchReviewedGames: () => lobbyResult(),
    setBoardPosition: agent.setBoardPosition,
    showLine: agent.showLine,
    stepLine: agent.stepLine,
    turnBoard: agent.turnBoard,
    stageGameImport: () => lobbyResult(),
  }

  return {
    agent,
    beginPendingMove,
    followExploredLine,
    host,
    player,
    selectPly: (ply: number) => player.setBoardPosition({ kind: "ply", ply }),
    snapshot,
    state,
  }
}
