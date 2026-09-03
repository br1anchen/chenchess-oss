import { decodeGameReviewSnapshot } from "@chenchess/coach-engine-sdk"
import { describe, expect, test } from "vitest"

import { projectGameReviewSnapshot } from "@chenchess/review-projection"
import { completionFixture } from "./reviewCompletionFixtures"

describe("Game Review snapshot projection", () => {
  test("renders a whole Game Review from its address alone", () => {
    const completion = completionFixture("gameReviewSnapshotRead")

    const snapshot = projectGameReviewSnapshot(completion)

    expect(() => decodeGameReviewSnapshot(snapshot)).not.toThrow()
    expect(snapshot).toMatchObject({
      eloRating: completion.importedGame.eloProfile.rating,
      gameImportId: completion.gameImportId,
      reviewSide: completion.importedGame.reviewSide,
      version: "v1",
    })
    expect(snapshot.moments).toHaveLength(completion.reviewMoments.length)
    expect(JSON.stringify(snapshot)).not.toContain("sessionId")
  })

  test("orders moments so the next Critical Moment is an index step", () => {
    const completion = completionFixture("gameReviewSnapshotRead")

    const snapshot = projectGameReviewSnapshot(completion)

    const plies = snapshot.moments.map(({ ply }) => ply)
    expect([...plies].sort((left, right) => left - right)).toEqual(plies)
  })

  /**
   * The selector renders no line, so a moment names the continuations it offers
   * and stops there. Their moves are read at the moment's own address when the
   * Player opens it, which keeps every other moment's moves out of a payload
   * that rides on every `list_critical_moments`.
   */
  test("names the continuations each moment offers", () => {
    const completion = completionFixture("gameReviewSnapshotRead")

    const first = projectGameReviewSnapshot(completion)

    // A moment whose lines went missing would render a card with no line
    // buttons; the decoder rejects a repeated kind, so uniqueness is its test.
    for (const moment of first.moments) {
      expect(moment.sequenceKinds.length).toBeGreaterThan(0)
    }
  })

  test("refuses a Review Moment the Game Review does not carry", () => {
    const completion = completionFixture("gameReviewSnapshotRead")
    completion.review.criticalMoments = []

    expect(() => projectGameReviewSnapshot(completion)).toThrow(
      /no canonical Game Review presentation/,
    )
  })
})
