import { describe, expect, test } from "vitest"

import { parseReviewResourceUri } from "./reviewResourceUri"

describe("Game Review resource addresses", () => {
  test("reads the review a snapshot URI names", () => {
    expect(
      parseReviewResourceUri(
        "chenchess://game-review/game-import%3Afixture%3A1",
      ),
    ).toEqual({ gameImportId: "game-import:fixture:1", kind: "gameReview" })
  })

  test("reads the whole address a Move Sequence URI names", () => {
    expect(
      parseReviewResourceUri(
        "chenchess://game-review/game-import%3Afixture%3A1/moment/review-moment%3Afixture%3Aone/sequence/engineBest",
      ),
    ).toEqual({
      gameImportId: "game-import:fixture:1",
      kind: "moveSequence",
      reviewMomentId: "review-moment:fixture:one",
      sequenceKind: "engineBest",
    })
  })

  test("refuses everything it is not the address of", () => {
    for (const uri of [
      "chenchess://game-review/",
      "chenchess://game-review/game-import%3Afixture%3A1/moment/review-moment%3Aone",
      "chenchess://game-review/game-import%3Afixture%3A1/moment/review-moment%3Aone/explanation",
      "chenchess://game-review/game-import%3Afixture%3A1/moment/review-moment%3Aone/sequence/inventedKind",
      "chenchess://game-review/not%2Fa%2Fhandle",
      "https://coach.chenchess.example/game-review/game-import%3Afixture%3A1",
      "chenchess://game-review/%",
    ]) {
      expect(parseReviewResourceUri(uri)).toBeUndefined()
    }
  })
})
