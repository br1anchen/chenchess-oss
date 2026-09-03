import { useRef, useState } from "react"
import type { MoveInput } from "@chenchess/coach-engine-sdk"
import { fromGameImportId } from "@chenchess/coach-engine-sdk"

import type { PlayerLineCommandOutcome } from "../../server/board/player-line-evaluate"

import { alternativeProgressLabels } from "@/review-session/useReviewSessionCommands"

import type { CoachingBoardCommandExecute } from "./coachingBoardCoachTools"
import {
  playerLineExecuteFor,
  webPlayerLineKeys,
} from "./coachingBoardCoachTools"
import {
  BOARD_EXPLORATION_MOVE_LIMIT,
  boardExplorationLimitNotice,
  boardExplorationUnreachableNotice,
  gameExplorationRefusalNotice,
} from "./boardExplorationNotice"
import {
  explorationIdentity,
  explorationRoots,
  explorationSource,
  exploreGameMove,
  refusalInvalidatesSession,
  type GameExplorationIdentity,
} from "./gameBoardExploration"
import type {
  CoachingBoardExplorationBranch,
  CoachingBoardSnapshot,
} from "./coachingBoardSnapshot"

/**
 * The Player's exploration line from one ply of the reviewed Game.
 *
 * Each move is one command, parented to the branch the board is standing on,
 * against a moment root read once per ply and kept. This replaces
 * re-submitting the whole line on every drag, which cost `k + 4` sequential
 * round trips at the k-th move: four to re-establish the moment root, then one
 * per ply already walked. The walked plies deduplicated on the engine and cost
 * no second search, but each still cost a request.
 *
 * `line` is the drive's own account of how the board reached this position, so
 * a position the host agent set is extended by the Player's next move rather
 * than by a line this hook remembered separately. Where the board ends up is
 * the drive's business too: `followExploredLine` folds the new branch in and
 * walks to it.
 */
export function useGameLineExploration({
  activeBranch,
  applyBranches,
  beginPendingMove,
  execute,
  followExploredLine,
  gameImportId,
  line,
  playerId,
  signal,
  viewedPly,
}: {
  activeBranch: CoachingBoardExplorationBranch | null
  applyBranches: (
    minted: readonly CoachingBoardExplorationBranch[],
  ) => CoachingBoardSnapshot
  beginPendingMove: (uci: string) => CoachingBoardSnapshot
  execute: CoachingBoardCommandExecute | null
  followExploredLine: (
    minted: readonly CoachingBoardExplorationBranch[],
  ) => CoachingBoardSnapshot
  gameImportId: string
  line: readonly string[]
  playerId: string
  signal: AbortSignal
  viewedPly: number
}) {
  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState<string | null>(null)
  const [progress, setProgress] = useState<string | null>(null)
  const [roots] = useState(explorationRoots)
  const inFlight = useRef<GameExplorationIdentity | null>(null)
  // The Player may browse while an evaluation is in flight, so where they are
  // when it lands is read at that moment rather than captured at dispatch.
  const viewedPlyRef = useRef(viewedPly)
  viewedPlyRef.current = viewedPly

  async function explore(uci: string) {
    if (!execute || busy) return
    if (line.length >= BOARD_EXPLORATION_MOVE_LIMIT) {
      setNotice(boardExplorationLimitNotice)
      return
    }
    const keys = webPlayerLineKeys(playerId, signal)
    const runner = playerLineExecuteFor(execute)
    const startedAtPly = viewedPly
    // The piece lands now. Waiting for the engine to confirm it cost a
    // measured 448 ms median against a 100 ms budget (ADR 0060).
    beginPendingMove(uci)
    setBusy(true)
    setNotice(null)
    setProgress(null)
    try {
      const root = await roots.ensure({
        execute: runner,
        gameImportId,
        keys,
        ply: startedAtPly,
      })
      if (root.kind === "refused") return refuse(root.outcome)

      const move: MoveInput = { kind: "uci", uci }
      const movePath: MoveInput[] = [
        ...line.map((played) => ({ kind: "uci" as const, uci: played })),
        move,
      ]
      const identity = explorationIdentity({
        gameImportId,
        keys,
        movePath,
        reviewMomentId: root.value.reviewMomentId,
      })
      inFlight.current = identity
      const explored = await exploreGameMove({
        execute: runner,
        gameImportId,
        identity,
        keys,
        move,
        observe: (envelope) => {
          const event = envelope.event
          if (event.kind !== "progress") return
          if (event.stage.kind !== "alternativeMove") return
          if (!signal.aborted) {
            setProgress(alternativeProgressLabels[event.stage.stage])
          }
        },
        reviewMomentId: root.value.reviewMomentId,
        source: explorationSource(root.value, activeBranch),
      })
      if (signal.aborted) return
      if (explored.kind === "refused") return refuse(explored.outcome)

      // Browsing away while Stockfish worked is not a reason to lose the
      // evaluation, but it is a reason not to drag the board back: the branch
      // is folded into the tree either way and only followed if the Player is
      // still standing where they played it.
      if (viewedPlyRef.current === startedAtPly) {
        followExploredLine([explored.value])
      } else {
        applyBranches([explored.value])
      }
    } catch {
      // The command channel throws on a dropped connection. The board cannot
      // move, so it says why rather than leaving the click unanswered.
      if (!signal.aborted) {
        roots.forget()
        setNotice(boardExplorationUnreachableNotice)
      }
    } finally {
      inFlight.current = null
      if (!signal.aborted) {
        setBusy(false)
        setProgress(null)
      }
    }
  }

  function refuse(outcome: PlayerLineCommandOutcome) {
    if (refusalInvalidatesSession(outcome)) roots.forget()
    if (!signal.aborted) setNotice(gameExplorationRefusalNotice(outcome))
  }

  /**
   * Cancel the evaluation in flight.
   *
   * `CancelOperation` is the engine's only cancellation authority, and both
   * keys are minted deterministically from the move path, so the board can
   * name the operation it started without holding a handle the engine gave it.
   */
  function cancel() {
    const operation = inFlight.current
    if (!execute || !operation) return
    void execute({
      gameImportId: fromGameImportId(gameImportId),
      idempotencyKey: operation.idempotencyKey,
      kind: "cancelOperation",
      operationId: operation.operationId,
    })
  }

  return {
    busy,
    cancel,
    clearNotice: () => setNotice(null),
    explore,
    notice,
    progress,
  }
}
