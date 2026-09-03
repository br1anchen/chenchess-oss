/**
 * Projects a started or opened Review Session into the compact presentation
 * both surfaces render.
 *
 * Cache and telemetry stay in Central Host. This module is the function the
 * Coach App preview fixture generator freezes.
 */
import {
  decodeReviewSessionPresentation,
  type GameImportId,
  type GameReviewCriticalMoment,
  type IdempotencyKey,
  type ImportedGame,
  type OperationCompletion,
  type PositionSnapshot,
  type ReviewMomentLearningMaterial,
  type ReviewMomentOccurrence,
  type ReviewSessionMoment,
  type ReviewSessionPresentation,
  type ReviewSessionPresentationAddition,
  type ReviewSessionPresentationMoment,
  type ReviewSessionPresentationSource,
} from "@chenchess/coach-engine-sdk"

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
} from "./review-moment-board.js"

type SessionCompletion = Extract<
  OperationCompletion,
  { kind: "reviewSessionStarted" }
>

type MomentCompletion = Extract<
  OperationCompletion,
  { kind: "reviewMomentOpened" }
>

/** One Review Moment as a presentation renders it, whatever payload carried it. */
type PresentedMoment = {
  authoringReadiness: "pending" | "prepared"
  criticalMoment: GameReviewCriticalMoment
  learningMaterial: ReviewMomentLearningMaterial
  occurrence: ReviewMomentOccurrence
  positionSnapshot: PositionSnapshot
}

export function projectReviewSessionPresentation(
  result: SessionCompletion,
  idempotencyKey: IdempotencyKey,
): ReviewSessionPresentation {
  const content = result.importedGame
  const moments = result.reviewMoments.map((admitted) =>
    projectMoment(
      admittedMoment(admitted, result.review.criticalMoments),
      content,
      result.gameImportId,
      idempotencyKey,
    ),
  )
  const selectedMomentId = moments[0]?.momentId ?? null
  const orientation = boardOrientation(
    content,
    result.reviewMoments[0]?.positionSnapshot,
  )
  return decodeReviewSessionPresentation({
    animation: null,
    eloRating: content.eloProfile.rating,
    evaluationTimeline: result.review.evaluationTimeline,
    handoffState: "ready",
    maxPly: content.game.moves.at(-1)?.ply ?? 1,
    moments,
    opening: content.game.opening,
    orientation,
    presentationRevision: result.sessionRevision,
    reviewSide: content.reviewSide,
    selectedMomentId,
    gameImportId: result.gameImportId,
    sessionRevision: result.sessionRevision,
    source: presentationSource(content.provenance.kind),
    version: "v1",
  })
}

export function projectReviewSessionPresentationAddition(
  result: MomentCompletion,
  idempotencyKey: IdempotencyKey,
): ReviewSessionPresentationAddition {
  const core = result.reviewMoment
  if (result.criticalMoment.criticalMomentId !== core.reviewMoment.momentId) {
    throw new Error(
      `Opened Review Moment ${core.reviewMoment.momentId} carries another moment's Game Review entry`,
    )
  }
  const changedMomentId = core.reviewMoment.momentId
  if (
    result.revisionDelta.changedMomentIds.length !== 1 ||
    result.revisionDelta.changedMomentIds[0] !== changedMomentId ||
    result.revisionDelta.resultingRevision !== result.sessionRevision
  ) {
    throw new Error(
      `Opened Review Moment ${changedMomentId} has an incompatible revision delta`,
    )
  }
  const moment = projectMoment(
    {
      authoringReadiness: "prepared",
      criticalMoment: result.criticalMoment,
      learningMaterial: result.criticalMoment.learningMaterial,
      occurrence: core.reviewMoment,
      positionSnapshot: core.positionSnapshot,
    },
    core.importedGame,
    result.gameImportId,
    idempotencyKey,
  )
  return {
    animation: null,
    changedFields: ["animation", "moment", "selectedMomentId"],
    changedMomentIds: result.revisionDelta.changedMomentIds,
    fullRefreshRequired: result.revisionDelta.fullRefreshRequired,
    moment,
    priorRevision: result.revisionDelta.priorRevision,
    resultingRevision: result.revisionDelta.resultingRevision,
    gameImportId: result.gameImportId,
    version: "v1",
  }
}

function presentationSource(
  kind: ImportedGame["provenance"]["kind"],
): ReviewSessionPresentationSource {
  switch (kind) {
    case "lichess":
      return "lichess"
    case "chessCom":
      return "chessCom"
    case "pastedPgn":
    case "localPgn":
      return "pgn"
    default: {
      const exhaustive: never = kind
      return exhaustive
    }
  }
}

function admittedMoment(
  admitted: ReviewSessionMoment,
  criticalMoments: readonly GameReviewCriticalMoment[],
): PresentedMoment {
  return {
    ...boardSourceMoment(admitted, criticalMoments),
    authoringReadiness:
      admitted.authoring.kind === "prepared" ? "prepared" : "pending",
    learningMaterial: admitted.learningMaterial,
  }
}

function projectMoment(
  presented: PresentedMoment,
  importedGame: ImportedGame,
  gameImportId: GameImportId,
  idempotencyKey: IdempotencyKey,
): ReviewSessionPresentationMoment {
  const { criticalMoment: moment, occurrence } = presented
  const boardFacts = projectBoardFacts(presented, importedGame)
  return {
    arrows: projectArrows(moment, importedGame.eloProfile.rating),
    authoringReadiness: presented.authoringReadiness,
    bestEvaluation: moment.display.bestEvaluation,
    board: boardFacts,
    glyph: momentGlyph(moment),
    handoff: {
      momentId: occurrence.momentId,
      ply: occurrence.ply,
      idempotencyKey,
      selection: occurrence.selection,
      gameImportId,
    },
    kind:
      occurrence.selection.kind === "pipelineCriticalMoment"
        ? "automatic"
        : "playerSelected",
    decisionLearningOutcome: moment.decisionLearningOutcome,
    learningMaterial: presented.learningMaterial,
    momentId: occurrence.momentId,
    moveLabel: moveLabel(moment),
    playedEvaluation: moment.display.playedEvaluation,
    ply: occurrence.ply,
    summary: momentSummary(moment),
    title: classificationLabel(moment.classification.kind),
    tone: momentTone(moment),
  }
}
