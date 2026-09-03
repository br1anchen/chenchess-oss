/**
 * Projects one grounded Review Moment into the two things addressable under it:
 * the moment's own detail, and one continuation played out.
 *
 * Both are pure functions of what the Coach Engine answered, so the same
 * address renders the same bytes on first paint, after a reload, and in a
 * year-old conversation. Neither carries the review that contains the moment —
 * a surface holding one already read that.
 */
import {
  decodeMoveSequenceSnapshot,
  decodeReviewMomentSnapshot,
  type GroundedReviewMomentDetail,
  type MoveSequencePresentationKind,
  type MoveSequenceSnapshot,
  type ReviewMomentSnapshot,
} from "@chenchess/coach-engine-sdk"

import {
  canonicalMovesFromFen,
  canonicalLinesFrom,
  sequenceOrientation,
  type CanonicalMomentLine,
} from "./move-sequence-lines.js"
import { projectSequenceMoves } from "./move-sequence-presentation.js"
import {
  decodePlayerLineSequenceSnapshot,
  type PlayerLineSequenceSnapshot,
} from "./rendered-sequence-snapshot.js"

export function projectReviewMomentSnapshot(
  detail: GroundedReviewMomentDetail,
): ReviewMomentSnapshot {
  const lines = momentLines(detail)
  const snapshot = {
    gameImportId: detail.gameImportId,
    orientation: sequenceOrientation(detail.continuation),
    ply: detail.ply,
    reviewMomentId: detail.reviewMomentId,
    sequences: lines.map((line) => ({
      kind: line.kind,
      moveCount: line.moves.length,
      san: line.moves.map(({ san }) => san),
      title: line.title,
    })),
    version: "v1" as const,
  }
  if (detail.explanation && detail.explanationRef) {
    return decodeReviewMomentSnapshot({
      ...snapshot,
      explanation: detail.explanation,
      explanationRef: detail.explanationRef,
    })
  }
  if (detail.explanation) {
    return decodeReviewMomentSnapshot({
      ...snapshot,
      explanation: detail.explanation,
    })
  }
  if (detail.explanationRef) {
    return decodeReviewMomentSnapshot({
      ...snapshot,
      explanationRef: detail.explanationRef,
    })
  }
  return decodeReviewMomentSnapshot(snapshot)
}

/**
 * A continuation this Review Moment does not offer is not an error to render
 * around; the caller decides whether a missing line is a miss or a choice.
 */
export function projectMoveSequenceSnapshot(
  detail: GroundedReviewMomentDetail,
  kind: MoveSequencePresentationKind,
): MoveSequenceSnapshot | undefined {
  const line = momentLines(detail).find((candidate) => candidate.kind === kind)
  if (!line) return undefined
  return decodeMoveSequenceSnapshot({
    gameImportId: detail.gameImportId,
    kind: line.kind,
    moves: projectSequenceMoves(line.initialFen, line.moves),
    orientation: line.orientation,
    reviewMomentId: detail.reviewMomentId,
    title: line.title,
    version: "v1",
  })
}

/**
 * Replays an evaluated Player Line from the durable Review Moment origin.
 *
 * UCI is the evaluated tool result's authoritative notation. SAN and every
 * board are recomputed here, so neither the model nor the earlier evaluation
 * result can smuggle presentation state into a reopened card.
 */
export function projectPlayerLineSequenceSnapshot(
  detail: GroundedReviewMomentDetail,
  uci: readonly string[],
): PlayerLineSequenceSnapshot | undefined {
  if (uci.length < 1 || uci.length > 12) return undefined
  const canonical = canonicalMovesFromFen(
    detail.continuation.fen,
    uci.map((move) => ({ uci: move })),
  )
  if (!canonical) return undefined
  return decodePlayerLineSequenceSnapshot({
    gameImportId: detail.gameImportId,
    kind: "playerLine",
    moves: projectSequenceMoves(detail.continuation.fen, canonical),
    orientation: sequenceOrientation(detail.continuation),
    reviewMomentId: detail.reviewMomentId,
    title: "Your line",
    uci: canonical.map((move) => move.uci),
    version: "v1",
  })
}

function momentLines(
  detail: GroundedReviewMomentDetail,
): CanonicalMomentLine[] {
  return canonicalLinesFrom(detail.continuation, detail.objectiveLines)
}
