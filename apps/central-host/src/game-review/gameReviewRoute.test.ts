import { describe, expect, test, vi } from "vitest"

import {
  fromCriticalMomentId,
  fromGameImportId,
} from "@chenchess/coach-engine-sdk"

import {
  gameReviewPath,
  moveSequencePath,
  parseGameReviewRoute,
  replaceGameReviewPath,
  reviewMomentPath,
  parseViewedPly,
  replaceViewedPly,
} from "./gameReviewRoute"

const gameImportId = fromGameImportId("game-import:fixture:cross-surface")
const reviewMomentId = fromCriticalMomentId("review-moment:fixture:one")

describe("authenticated Game Review routes", () => {
  test("round-trips the canonical Game Review locator", () => {
    const path = gameReviewPath(gameImportId)

    expect(path).toBe("/app/game-reviews/game-import%3Afixture%3Across-surface")
    expect(parseGameReviewRoute(path)).toEqual({
      gameImportId,
      kind: "gameReview",
    })
  })

  test("hangs one Review Moment off the review that contains it", () => {
    const path = reviewMomentPath(gameImportId, reviewMomentId)

    expect(path).toBe(
      "/app/game-reviews/game-import%3Afixture%3Across-surface/moments/review-moment%3Afixture%3Aone",
    )
    expect(parseGameReviewRoute(path)).toEqual({
      gameImportId,
      kind: "reviewMoment",
      reviewMomentId,
    })
  })

  test("hangs one canonical continuation off the moment that offers it", () => {
    const path = moveSequencePath(gameImportId, reviewMomentId, "engineBest")

    expect(path).toBe(
      "/app/game-reviews/game-import%3Afixture%3Across-surface/moments/review-moment%3Afixture%3Aone/sequences/engineBest",
    )
    expect(parseGameReviewRoute(path)).toEqual({
      gameImportId,
      kind: "moveSequence",
      reviewMomentId,
      sequenceKind: "engineBest",
    })
  })

  test("no longer answers the deleted Review Session route", () => {
    expect(
      parseGameReviewRoute(
        "/app/review-sessions/game-import%3Afixture%3Across-surface",
      ),
    ).toEqual({ kind: "none" })
  })

  test("distinguishes unrelated paths from malformed review links", () => {
    expect(parseGameReviewRoute("/")).toEqual({ kind: "none" })
    for (const pathname of [
      "/app/game-reviews/",
      "/app/game-reviews/not%2Fa%2Fhandle",
      "/app/game-reviews/%",
      "/app/game-reviews/game-import%3A1/moments/",
      "/app/game-reviews/game-import%3A1/moments/review-moment%3A1/sequences/inventedKind",
      "/app/game-reviews/game-import%3A1/moments/review-moment%3A1/sequences",
      "/app/game-reviews/game-import%3A1/annotations/review-moment%3A1",
    ]) {
      expect(parseGameReviewRoute(pathname)).toEqual({ kind: "invalid" })
    }
  })

  test("updates the browser to the canonical path without blocking a review", () => {
    const replaceState = vi.fn()
    replaceGameReviewPath(gameImportId, { replaceState })
    expect(replaceState).toHaveBeenCalledWith(
      null,
      "",
      "/app/game-reviews/game-import%3Afixture%3Across-surface",
    )

    expect(() =>
      replaceGameReviewPath(gameImportId, {
        replaceState: () => {
          throw new Error("history denied")
        },
      }),
    ).not.toThrow()
  })
})

describe("the board's position in the address", () => {
  test("a ply in the address is what the board opens on", () => {
    expect(parseViewedPly("?ply=31")).toBe(31)
    expect(parseViewedPly("?ply=31&other=x")).toBe(31)
  })

  test("no ply, or one no Player could have produced, defers to the moment", () => {
    for (const search of [
      "",
      "?other=x",
      "?ply=",
      "?ply=0",
      "?ply=-4",
      "?ply=2.5",
      "?ply=abc",
      "?ply=9007199254740993",
    ]) {
      expect(parseViewedPly(search)).toBeNull()
    }
  })

  test("walking the line replaces the address rather than stacking history", () => {
    const replaceState = vi.fn()
    replaceViewedPly(
      31,
      { pathname: "/app/game-reviews/one/moments/two", search: "" },
      { replaceState },
    )
    expect(replaceState).toHaveBeenCalledWith(
      null,
      "",
      "/app/game-reviews/one/moments/two?ply=31",
    )
  })

  test("an existing query survives, and leaving the board clears only the ply", () => {
    const replaceState = vi.fn()
    replaceViewedPly(
      12,
      { pathname: "/app/game-reviews/one", search: "?from=digest" },
      { replaceState },
    )
    expect(replaceState).toHaveBeenCalledWith(
      null,
      "",
      "/app/game-reviews/one?from=digest&ply=12",
    )

    replaceViewedPly(
      null,
      { pathname: "/app/game-reviews/one", search: "?from=digest&ply=12" },
      { replaceState },
    )
    expect(replaceState).toHaveBeenLastCalledWith(
      null,
      "",
      "/app/game-reviews/one?from=digest",
    )
  })

  test("a browser that denies history updates does not break the board", () => {
    expect(() =>
      replaceViewedPly(
        4,
        { pathname: "/app/game-reviews/one", search: "" },
        {
          replaceState: () => {
            throw new Error("history denied")
          },
        },
      ),
    ).not.toThrow()
  })
})
