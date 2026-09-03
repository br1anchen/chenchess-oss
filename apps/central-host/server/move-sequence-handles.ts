/**
 * Which canonical continuations each Review Moment of a review offers.
 *
 * A pure projection of what the Coach Engine answered: no store, no minted
 * reference, no expiry, and nothing scoped to a caller or a Review Session. A
 * Review Moment offers at most one line of each kind, so the kind is the whole
 * reference — `render_move_sequence` and the sequence resource both address a
 * line by `(gameImportId, reviewMomentId, kind)` and resolve it from the frozen
 * review rather than from anything held in memory between two calls.
 */
import type {
  ImportedGame,
  OperationCompletion,
} from "@chenchess/coach-engine-sdk"

import {
  type BoardSourceMoment,
  canonicalMomentLines,
} from "@chenchess/review-projection"

type SequenceSourceCompletion = Extract<
  OperationCompletion,
  { kind: "reviewMomentOpened" | "reviewSessionStarted" }
>

export type MoveSequenceHandle = {
  kind: "engineBest" | "playedMoveRefutation"
  moveCount: number
  san: readonly string[]
  title: string
}

export function canonicalSequenceHandles(completion: SequenceSourceCompletion) {
  const { content, moments } = sequenceSourceMoments(completion)
  const handles: Record<string, MoveSequenceHandle[]> = {}
  for (const admitted of moments) {
    const lines = canonicalMomentLines(admitted, content).map((line) => ({
      kind: line.kind,
      moveCount: line.moves.length,
      san: line.moves.map(({ san }) => san),
      title: line.title,
    }))
    if (lines.length > 0) handles[admitted.occurrence.momentId] = lines
  }
  return handles
}

type SequenceSourceMoments = {
  content: ImportedGame
  moments: BoardSourceMoment[]
}

function sequenceSourceMoments(
  completion: SequenceSourceCompletion,
): SequenceSourceMoments {
  if (completion.kind === "reviewMomentOpened") {
    const core = completion.reviewMoment
    return {
      content: core.importedGame,
      moments: [
        {
          criticalMoment: completion.criticalMoment,
          occurrence: core.reviewMoment,
          positionSnapshot: core.positionSnapshot,
        },
      ],
    }
  }
  return {
    content: completion.importedGame,
    moments: completion.reviewMoments.flatMap((admitted) => {
      const criticalMoment = completion.review.criticalMoments.find(
        ({ criticalMomentId }) =>
          criticalMomentId === admitted.reviewMoment.momentId,
      )
      return criticalMoment
        ? [
            {
              criticalMoment,
              occurrence: admitted.reviewMoment,
              positionSnapshot: admitted.positionSnapshot,
            },
          ]
        : []
    }),
  }
}
