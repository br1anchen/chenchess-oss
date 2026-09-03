import { beforeAll, expect, test } from "vitest"

import type { GameReview } from "@chenchess/coach-engine-sdk"

import {
  FIXTURE_GAME_IMPORT_ID,
  fixtureCore,
  fixtureGameReview,
  loadReviewSessionFixtures,
} from "@/review-session/reviewSessionStreamFixtures"

import { boardMomentCommentary } from "./boardMomentCommentary"
import { applySetPosition, gameBoardDrive } from "./coachingBoardDrive"

beforeAll(async () => {
  await loadReviewSessionFixtures()
})

function boardAt(review: GameReview, ply: number) {
  const drive = gameBoardDrive({
    gameImportId: FIXTURE_GAME_IMPORT_ID,
    importedGame: fixtureCore().importedGame,
    review,
  })
  if (drive.viewedPly === ply) return drive
  const moved = applySetPosition(drive, "player", { kind: "ply", ply })
  if (moved.kind !== "applied") throw new Error("the fixture Game has that ply")
  return moved.state
}

test("a ply the Review said nothing about carries no commentary", () => {
  const review = fixtureGameReview()
  const moments = new Set(review.criticalMoments.map((moment) => moment.ply))
  const quiet = fixtureCore().importedGame.game.moves.find(
    (move) => !moments.has(move.ply),
  )?.ply
  if (quiet === undefined) throw new Error("the fixture has a quiet ply")
  expect(boardMomentCommentary(boardAt(review, quiet))).toBeNull()
})

test("hosted prose is preferred over the re-derived safe rendering", () => {
  const review = fixtureGameReview()
  const moment = review.criticalMoments[0]
  if (!moment) throw new Error("the fixture has a Critical Moment")
  moment.comment = { text: "The knight was hanging after this." }
  expect(boardMomentCommentary(boardAt(review, moment.ply))).toBe(
    "The knight was hanging after this.",
  )
})

test("a moment the Language Layer never authored falls back to the facts", () => {
  const review = fixtureGameReview()
  const moment = review.criticalMoments[0]
  if (!moment) throw new Error("the fixture has a Critical Moment")
  moment.comment = null
  const commentary = boardMomentCommentary(boardAt(review, moment.ply))
  expect(commentary).toContain(moment.playedSan)
})
