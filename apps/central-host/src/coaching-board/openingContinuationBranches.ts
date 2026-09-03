import type {
  BranchParent,
  OpeningAnalyzedPly,
} from "@chenchess/coach-engine-sdk"
import {
  fromAlternativeMoveId,
  fromBranchRef,
} from "@chenchess/coach-engine-sdk"

import type { CoachingBoardExplorationBranch } from "./coachingBoardSnapshot"
import type { OpeningLineRef } from "./openingLineRef"
import { openingPositionFromFen, positionRefForFen } from "./openingMoves"
import { webFingerprint } from "./webFingerprint"

/**
 * Turn analyzed plies into branches of the board's exploration tree.
 *
 * The analysis route is stateless and has no actor to mint ids from, so the
 * page mints them — deterministically, over the Opening Line and the move
 * path walked from its end, so re-walking a shared prefix converges on the
 * branches already on the board instead of growing a second copy of the
 * same line (ADR 0058).
 *
 * Each ply chains onto the one before it, so the analyzed continuation lands
 * as a path through the tree rather than a flat list of siblings.
 */
export function openingContinuationBranches({
  openingLineRef,
  plies,
  rootFen,
}: {
  openingLineRef: OpeningLineRef
  plies: readonly OpeningAnalyzedPly[]
  rootFen: string
}): CoachingBoardExplorationBranch[] {
  const branches: CoachingBoardExplorationBranch[] = []
  const walked: string[] = []
  let parent: BranchParent = {
    kind: "root",
    positionRef: positionRefForFen(rootFen),
  }
  for (const ply of plies) {
    walked.push(ply.moveUci)
    const fingerprint = webFingerprint(`${openingLineRef} ${walked.join(" ")}`)
    const branchRef = fromBranchRef(`branch:web-opening-${fingerprint}`)
    branches.push({
      alternativeMoveId: fromAlternativeMoveId(
        `alternative-move:web-opening-${fingerprint}`,
      ),
      branchRef,
      evaluation: ply.evaluation,
      moveUci: ply.moveUci,
      parent,
      resultingPosition: {
        ...openingPositionFromFen(ply.resultingFen),
        positionRef: positionRefForFen(ply.resultingFen),
      },
    })
    parent = { kind: "move", branchRef }
  }
  return branches
}

/**
 * Fold freshly analyzed branches into the ones already retained.
 *
 * Ids are deterministic, so a re-analyzed branch replaces itself in place and
 * only genuinely new plies extend the tree. Order is retention order: the
 * branches the Player already explored keep their positions.
 *
 * This is the only step that knows which branch is new, so it is the one that
 * builds them: `arriving` makes whatever the caller's tree holds beyond the
 * engine's facts. A re-analyzed branch takes the fresh facts over the ones the
 * tree already carried, so nothing the caller added to it is lost.
 */
export function mergeExplorationBranches<
  Branch extends CoachingBoardExplorationBranch,
>(
  existing: readonly Branch[],
  minted: readonly CoachingBoardExplorationBranch[],
  arriving: (branch: CoachingBoardExplorationBranch) => Branch,
): Branch[] {
  const mintedById = new Map(
    minted.map((branch) => [branch.alternativeMoveId, branch] as const),
  )
  const known = new Set(existing.map((branch) => branch.alternativeMoveId))
  return [
    ...existing.map((branch) => {
      const again = mintedById.get(branch.alternativeMoveId)
      return again ? { ...branch, ...again } : branch
    }),
    ...minted
      .filter((branch) => !known.has(branch.alternativeMoveId))
      .map(arriving),
  ]
}
