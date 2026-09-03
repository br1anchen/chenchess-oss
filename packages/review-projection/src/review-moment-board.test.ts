import { expect, test } from "vitest"

import { fromPositionRef } from "@chenchess/coach-engine-sdk"

import {
  occurrenceMoveLabel,
  reviewMomentToneFromClassificationKind,
  toneFromClassificationKind,
} from "./review-moment-board.js"

test("toneFromClassificationKind paints Improvement as improvement", () => {
  expect(toneFromClassificationKind("positiveHighlight")).toBe("positive")
  expect(toneFromClassificationKind("improvementOpportunity")).toBe(
    "improvement",
  )
  expect(toneFromClassificationKind("neutral")).toBe("selected")
  expect(reviewMomentToneFromClassificationKind("improvementOpportunity")).toBe(
    "critical",
  )
})

test("occurrenceMoveLabel uses SAN, never ply vocabulary", () => {
  expect(
    occurrenceMoveLabel({
      precedingMove: {
        afterPositionRef: fromPositionRef("sha256:after"),
        beforePositionRef: fromPositionRef("sha256:before"),
        moveNumber: 2,
        ply: 3,
        san: "Nf3",
        side: "white",
        uci: "g1f3",
      },
    }),
  ).toBe("2. Nf3")
  expect(occurrenceMoveLabel({ precedingMove: null })).toBeUndefined()
})
