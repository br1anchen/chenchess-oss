import { expect, test } from "vitest"

import { authenticatedGameReviewUrl } from "./game-review-url"
import { fromGameImportId } from "@chenchess/coach-engine-sdk"

test("creates an authenticated durable Game Review locator", () => {
  expect(
    authenticatedGameReviewUrl(
      new URL("https://coach.chenchess.example"),
      fromGameImportId("game-import:fixture:cross-surface"),
    ),
  ).toBe(
    "https://coach.chenchess.example/app/game-reviews/game-import%3Afixture%3Across-surface",
  )
})
