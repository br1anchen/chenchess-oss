import { describe, expect, test } from "vitest"

import { isCriticalMomentId, isGameImportId } from "@/game-review/reviewAddress"

import {
  coachAppDestinationForGameReview,
  coachingBoardDestination,
  verifiedIdentityDestination,
  withInvitationFragment,
} from "./verifiedIdentityDestination"

describe("where a verified identity lands", () => {
  test("defaults to the Coaching Board", () => {
    expect(verifiedIdentityDestination("")).toEqual(coachingBoardDestination)
    expect(verifiedIdentityDestination("?return_to=nonsense")).toEqual(
      coachingBoardDestination,
    )
  })

  test("returns an addressed Game Review to itself, moment and all", () => {
    const gameImportId = `game-import:${"a".repeat(64)}:${"b".repeat(32)}`
    const reviewMomentId = `review-moment:${"c".repeat(64)}:24`
    const search = new URLSearchParams({
      game_review: gameImportId,
      return_to: "app",
      review_moment: reviewMomentId,
      sequence: "engineBest",
    })

    const destination = verifiedIdentityDestination(`?${search}`)

    // The guards are the only way into the branded ids, so the expectation is
    // built from the same parse the resolver runs rather than from a cast.
    if (!isGameImportId(gameImportId) || !isCriticalMomentId(reviewMomentId)) {
      throw new Error("the fixture ids must satisfy their own address guards")
    }
    expect(destination).toEqual(
      coachAppDestinationForGameReview({
        gameImportId,
        kind: "moveSequence",
        reviewMomentId,
        sequenceKind: "engineBest",
      }),
    )
    expect(destination.requiresBetaAccess).toBe(true)
  })

  test("a malformed review address falls back rather than half-resolving", () => {
    expect(
      verifiedIdentityDestination("?return_to=app&game_review=not-an-id"),
    ).toEqual(coachingBoardDestination)
  })

  test("an invitation code rides the fragment, and nothing rides without one", () => {
    expect(withInvitationFragment("/join/", "abc")).toBe("/join/#invite=abc")
    expect(withInvitationFragment("/join/", null)).toBe("/join/")
  })
})
