/**
 * Projects one Game Review into the immutable snapshot a surface renders.
 *
 * Everything here is a function of the frozen Game Review and the imported
 * Game. There is no Review Session to reconcile, no revision to compare, and
 * no `_meta` to depend on, so the same address answers with the same bytes on
 * first paint, after a reload, and in a year-old conversation.
 */
import {
  decodeGameReviewSnapshot,
  type GameReviewSnapshot,
  type GameReviewSnapshotMoment,
  type ImportedGame,
  type OperationCompletion,
} from "@chenchess/coach-engine-sdk"

import { canonicalMomentLines } from "./move-sequence-lines.js"
import {
  boardOrientation,
  boardSourceMoment,
  classificationLabel,
  momentGlyph,
  momentSummary,
  momentTone,
  moveLabel,
  projectArrows,
  projectBoardFacts,
  type BoardSourceMoment,
} from "./review-moment-board.js"

type SnapshotCompletion = Extract<
  OperationCompletion,
  { kind: "gameReviewSnapshotRead" }
>

export function projectGameReviewSnapshot(
  result: SnapshotCompletion,
): GameReviewSnapshot {
  const content = result.importedGame
  return decodeGameReviewSnapshot({
    eloRating: content.eloProfile.rating,
    evaluationTimeline: result.review.evaluationTimeline,
    gameImportId: result.gameImportId,
    maxPly: content.game.moves.at(-1)?.ply ?? 1,
    moments: result.reviewMoments.map((admitted, index) =>
      projectSnapshotMoment(
        index,
        boardSourceMoment(admitted, result.review.criticalMoments),
        content,
      ),
    ),
    opening: content.game.opening,
    orientation: boardOrientation(
      content,
      result.reviewMoments[0]?.positionSnapshot,
    ),
    reviewSide: content.reviewSide,
    source:
      content.provenance.kind === "lichess"
        ? "lichess"
        : content.provenance.kind === "chessCom"
          ? "chessCom"
          : "pgn",
    version: "v1",
  })
}

function projectSnapshotMoment(
  index: number,
  presented: BoardSourceMoment,
  importedGame: ImportedGame,
): GameReviewSnapshotMoment {
  const { criticalMoment: moment, occurrence } = presented
  return {
    arrows: projectArrows(moment, importedGame.eloProfile.rating),
    bestEvaluation: moment.display.bestEvaluation,
    board: projectBoardFacts(presented, importedGame),
    decisionLearningOutcome: moment.decisionLearningOutcome,
    glyph: momentGlyph(moment),
    index,
    kind:
      occurrence.selection.kind === "pipelineCriticalMoment"
        ? "automatic"
        : "playerSelected",
    learningMaterial: moment.learningMaterial,
    momentId: occurrence.momentId,
    moveLabel: moveLabel(moment),
    playedEvaluation: moment.display.playedEvaluation,
    ply: occurrence.ply,
    selection: occurrence.selection,
    // Named, not carried: the selector renders no line, so the moves behind
    // these kinds are read at the moment's own address when the Player opens it.
    sequenceKinds: canonicalMomentLines(presented, importedGame).map(
      ({ kind }) => kind,
    ),
    summary: momentSummary(moment),
    title: classificationLabel(moment.classification.kind),
    tone: momentTone(moment),
  }
}
