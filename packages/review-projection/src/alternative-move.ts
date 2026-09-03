import type { AlternativeMoveResult } from "@chenchess/coach-engine-sdk"

import {
  PLAYER_VISIBLE_MOVE_FALLBACK,
  playerVisibleSanFromLegalUci,
  playerVisibleStrongestReply,
  type PlayerVisibleSan,
} from "./player-visible-san.js"

export type ModelStrongestReply =
  | {
      kind: "offered"
      uci: Extract<
        AlternativeMoveResult["strongestReply"],
        { kind: "offered" }
      >["uci"]
      san: PlayerVisibleSan
    }
  | { kind: "terminal" }

export type ModelAlternativeMove = {
  alternativeMoveId: AlternativeMoveResult["alternativeMoveId"]
  bestMoveSan: PlayerVisibleSan
  branchRef: AlternativeMoveResult["branchRef"]
  evaluation: AlternativeMoveResult["evaluation"]
  moveSan: PlayerVisibleSan
  moveUci: AlternativeMoveResult["moveUci"]
  parent: AlternativeMoveResult["parent"]
  resultingPositionRef: AlternativeMoveResult["resultingPosition"]["positionRef"]
  resultingSideToMove: AlternativeMoveResult["resultingPosition"]["sideToMove"]
  sourcePositionRef: AlternativeMoveResult["sourcePositionRef"]
  strongestReply: ModelStrongestReply
}

export type AlternativeMoveChatTarget = {
  alternativeMove: ModelAlternativeMove
  alternativeMoveId: string
  gameImportId: string
  reviewMomentId: string
}

/** The one model-safe Alternative Move shape shared by tools and chat. */
export function projectAlternativeMove(
  alternativeMove: AlternativeMoveResult,
  sourceFen?: string,
): ModelAlternativeMove {
  const moveSan = sourceFen
    ? playerVisibleSanFromLegalUci(sourceFen, alternativeMove.moveUci)
    : PLAYER_VISIBLE_MOVE_FALLBACK
  const bestMoveSan = sourceFen
    ? playerVisibleSanFromLegalUci(
        sourceFen,
        alternativeMove.evaluation.bestMoveUci,
      )
    : PLAYER_VISIBLE_MOVE_FALLBACK
  return {
    alternativeMoveId: alternativeMove.alternativeMoveId,
    bestMoveSan,
    branchRef: alternativeMove.branchRef,
    evaluation: alternativeMove.evaluation,
    moveSan,
    moveUci: alternativeMove.moveUci,
    parent: alternativeMove.parent,
    resultingPositionRef: alternativeMove.resultingPosition.positionRef,
    resultingSideToMove: alternativeMove.resultingPosition.sideToMove,
    sourcePositionRef: alternativeMove.sourcePositionRef,
    strongestReply: modelStrongestReply(alternativeMove),
  }
}

function modelStrongestReply(
  alternativeMove: AlternativeMoveResult,
): ModelStrongestReply {
  const strongestReply = alternativeMove.strongestReply
  if (strongestReply.kind !== "offered") return { kind: "terminal" }
  return {
    kind: "offered",
    san: playerVisibleStrongestReply(
      strongestReply,
      alternativeMove.resultingPosition.fen,
    ),
    uci: strongestReply.uci,
  }
}
