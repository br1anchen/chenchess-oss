import type { AlternativeMoveId } from "@chenchess/coach-engine-sdk"

import type { CoachingBoardExplorationBranch } from "./coachingBoardSnapshot"

import type { OpeningLineRef } from "./openingLineRef"

/**
 * Exploration retained per Opening Line, the way the game board retains it
 * per critical ply: nothing durable, re-exploring is a cached stateless
 * call, so the five most recent lines keep their exploration and the oldest
 * is evicted.
 */
export const OPENING_EXPLORATION_RETENTION_LIMIT = 5

export type RetainedOpeningExploration = {
  activeBranchId: AlternativeMoveId | null
  branches: readonly CoachingBoardExplorationBranch[]
  viewedPly: number
}

const retained = new Map<OpeningLineRef, RetainedOpeningExploration>()

/**
 * Who the retained exploration belongs to.
 *
 * A board surface recalls during render, while the Player boundary clears in
 * an effect, so the next identity on this tab would otherwise mount holding
 * the previous one's exploration. Owning the retention by Player makes that
 * unrepresentable rather than a matter of effect ordering.
 */
let retainedPlayerId: string | null = null

export function retainOpeningExploration(
  playerId: string | null,
  openingLineRef: OpeningLineRef,
  exploration: RetainedOpeningExploration,
) {
  if (playerId !== retainedPlayerId) {
    retained.clear()
    retainedPlayerId = playerId
  }
  retained.delete(openingLineRef)
  retained.set(openingLineRef, exploration)
  while (retained.size > OPENING_EXPLORATION_RETENTION_LIMIT) {
    const oldest = retained.keys().next().value
    if (oldest === undefined) break
    retained.delete(oldest)
  }
}

export function recallOpeningExploration(
  playerId: string | null,
  openingLineRef: OpeningLineRef,
): RetainedOpeningExploration | undefined {
  if (playerId !== retainedPlayerId) return undefined
  const exploration = retained.get(openingLineRef)
  if (exploration) {
    // A recall is a use: the line moves to the most-recent end.
    retained.delete(openingLineRef)
    retained.set(openingLineRef, exploration)
  }
  return exploration
}

export function clearOpeningExplorationRetention() {
  retained.clear()
  retainedPlayerId = null
}
