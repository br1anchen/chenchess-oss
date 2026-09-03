import type {
  OpeningAnalysisOutcome,
  OpeningAnalysisRequest,
} from "@chenchess/coach-engine-sdk"

import {
  unavailableBoardCoachResult,
  wrapBoardCoachResult,
} from "./coachingBoardCoachTools"
import { driveRefusal } from "./coachingBoardDrive"
import type {
  CoachingBoardExplorationBranch,
  CoachingBoardSnapshot,
} from "./coachingBoardSnapshot"
import { openingContinuationBranches } from "./openingContinuationBranches"
import type { OpeningLineRef } from "./openingLineRef"

export type OpeningContinuationInput = {
  continuation: ({ kind: "san"; san: string } | { kind: "uci"; uci: string })[]
  openingLineRef: string
}

/**
 * The opening board's evaluate-then-show gate.
 *
 * Analyzed plies become branches of this board's exploration tree and
 * nothing else happens: no branch is activated, the board does not move, and
 * showing one is a separate call. A continuation naming another line is
 * refused rather than analyzed, because the branches would have no root on
 * the board the Player is looking at (ADR 0058).
 */
export async function evaluateOpeningContinuationOnBoard({
  analyze,
  applyBranches,
  boardLineRef,
  input,
  snapshot,
}: {
  analyze: (request: OpeningAnalysisRequest) => Promise<OpeningAnalysisOutcome>
  applyBranches: (
    minted: readonly CoachingBoardExplorationBranch[],
  ) => CoachingBoardSnapshot
  boardLineRef: OpeningLineRef
  input: OpeningContinuationInput
  snapshot: CoachingBoardSnapshot | null
}) {
  if (input.openingLineRef !== boardLineRef) {
    return driveRefusal("unreachablePosition", snapshot)
  }
  const outcome = await analyze({
    continuation: input.continuation,
    openingLineRef: boardLineRef,
  })
  switch (outcome.outcome) {
    case "analyzed": {
      const minted = openingContinuationBranches({
        openingLineRef: boardLineRef,
        plies: outcome.plies,
        rootFen: outcome.root.fen,
      })
      return wrapBoardCoachResult(
        {
          branches: minted.map((branch) => ({
            alternativeMoveId: branch.alternativeMoveId,
            evaluation: branch.evaluation,
            moveUci: branch.moveUci,
          })),
          kind: "openingContinuationEvaluated" as const,
          line: outcome.line,
          root: outcome.root,
          verdict: outcome.verdict,
        },
        applyBranches(minted),
      )
    }
    case "rateLimited":
      return wrapBoardCoachResult(
        { kind: "rateLimited" as const, retry: outcome.retry },
        snapshot,
      )
    case "unknownOpeningLine":
      return wrapBoardCoachResult(
        { kind: "unknownOpeningLine" as const },
        snapshot,
      )
    case "unavailable":
      return unavailableBoardCoachResult(snapshot)
    default: {
      const _exhaustive: never = outcome
      return _exhaustive
    }
  }
}
