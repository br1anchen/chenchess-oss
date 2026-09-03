import type { GameImportId } from "@chenchess/coach-engine-sdk"
import { piecesFromFen } from "@chenchess/ui/board"
import type { BoardPresentation } from "@chenchess/ui/contracts"

import { reviewSideOrientation } from "@/review-session/model"

type PreviewReview = {
  criticalMoments: readonly {
    criticalMomentId: string
    moveNumber: number
    playedSan: string
    side: "black" | "white"
  }[]
  positionViews: readonly {
    criticalMomentId: string
    positionSnapshot: { fen: string }
  }[]
}

type PreviewGame = {
  gameImportId: GameImportId
  reviewSide: "black" | "both" | "white"
}

export function previewBoardFromReview(
  review: PreviewReview,
  game: PreviewGame,
): BoardPresentation {
  const moment = review.criticalMoments[0]
  const view =
    (moment &&
      review.positionViews.find(
        (candidate) => candidate.criticalMomentId === moment.criticalMomentId,
      )) ??
    review.positionViews[0]
  if (view === undefined) {
    throw new Error("The frozen Game Review has no position views.")
  }
  const fen = view.positionSnapshot.fen
  return {
    announcement: moment
      ? `${moment.side === "black" ? "..." : ""}${moment.playedSan} at move ${moment.moveNumber}`
      : "First key moment",
    checkSquare: null,
    disabled: true,
    fen,
    id: `${game.gameImportId}:${view.criticalMomentId}`,
    lastMove: null,
    legalDestinations: [],
    orientation: reviewSideOrientation(game.reviewSide),
    pieces: piecesFromFen(fen),
    promotion: null,
    selectedSquare: null,
  }
}
