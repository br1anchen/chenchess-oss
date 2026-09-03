import { expect, test } from "vitest"

import {
  reviewMomentCountLabel,
  type ReviewMomentMarkerPresentation,
} from "./reviewNavigationPresentation"

const frozen: ReviewMomentMarkerPresentation = {
  glyph: "↗",
  label: "Improvement opportunity",
  moveLabel: "11. Nf3",
  ply: 21,
  tone: "improvement",
}

const nominated: ReviewMomentMarkerPresentation = {
  countsInTotal: false,
  glyph: "!",
  label: "Positive highlight",
  moveLabel: "13. Nxd4",
  ply: 26,
  tone: "positive",
}

test("nominated extras stay outside the x/N total", () => {
  expect(reviewMomentCountLabel([frozen, nominated], 21)).toBe("1/1")
  expect(reviewMomentCountLabel([frozen, nominated], 26)).toBe("+/1")
})
