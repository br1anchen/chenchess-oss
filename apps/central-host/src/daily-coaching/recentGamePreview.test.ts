import { fromGameImportId } from "@chenchess/coach-engine-sdk"
import { expect, test } from "vitest"

import { previewBoardFromReview } from "./recentGamePreview"

const startingFen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"

test("builds a preview board from the first Critical Moment position", () => {
  const board = previewBoardFromReview(
    {
      criticalMoments: [
        {
          criticalMomentId: "critical-moment:ba6",
          moveNumber: 11,
          playedSan: "Ba6",
          side: "black",
        },
      ],
      positionViews: [
        {
          criticalMomentId: "critical-moment:ba6",
          positionSnapshot: { fen: startingFen },
        },
      ],
    },
    {
      gameImportId: fromGameImportId("game-import:fixture:1"),
      reviewSide: "black",
    },
  )

  expect(board.orientation).toBe("black")
  expect(board.fen).toBe(startingFen)
  expect(board.pieces.length).toBe(32)
  expect(board.announcement).toBe("...Ba6 at move 11")
  expect(board.disabled).toBe(true)
})
