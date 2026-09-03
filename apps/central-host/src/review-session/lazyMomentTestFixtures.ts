import {
  fromCriticalMomentId,
  type ImportedGame,
  type GameReview,
} from "@chenchess/coach-engine-sdk"

export function reviewWithLazyMoment(
  source: GameReview,
  snapshot: ImportedGame,
): GameReview {
  const review = structuredClone(source)
  const entry = review.criticalMoments[0]
  const move = snapshot.game.moves.find((candidate) => candidate.ply === 3)
  if (!entry || !move)
    throw new Error("lazy fixture requires entry and later moves")
  review.criticalMoments.push({
    ...structuredClone(entry),
    criticalMomentId: fromCriticalMomentId(
      `review-moment:${snapshot.game.gameRef}:${move.ply}`,
    ),
    ply: move.ply,
    moveNumber: move.moveNumber,
    side: move.side,
    playedSan: move.san,
    comment: null,
  })
  return review
}
