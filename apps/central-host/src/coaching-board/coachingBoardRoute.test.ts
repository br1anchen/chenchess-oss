import { describe, expect, test } from "vitest"

import { fromGameImportId } from "@chenchess/coach-engine-sdk"

import {
  coachingBoardGamePath,
  coachingBoardOpeningPath,
  coachingBoardPath,
  parseCoachingBoardRoute,
} from "./coachingBoardRoute"
import { openingLineRefFromPath } from "./openingLineRef"

const gameImportId = fromGameImportId("game-import:fixture:board")

describe("Coaching Board routes", () => {
  test("no-target /app/board is the empty Coaching Board", () => {
    expect(parseCoachingBoardRoute("/app/board")).toEqual({ kind: "empty" })
    expect(parseCoachingBoardRoute("/app/board/")).toEqual({ kind: "empty" })
    expect(coachingBoardPath()).toBe("/app/board")
  })

  test("does not overload Game Review routes", () => {
    expect(
      parseCoachingBoardRoute(
        "/app/game-reviews/game-import%3Afixture%3Aboard",
      ),
    ).toEqual({ kind: "none" })
    expect(parseCoachingBoardRoute("/app/")).toEqual({ kind: "none" })
  })

  test("addresses a Game Import on the own path", () => {
    const path = coachingBoardGamePath(gameImportId)
    expect(path).toBe("/app/board/games/game-import%3Afixture%3Aboard")
    expect(parseCoachingBoardRoute(path)).toEqual({
      gameImportId,
      kind: "game",
    })
  })

  test("addresses an Opening Line on the own path", () => {
    const ref = openingLineRefFromPath(
      "C41",
      "Philidor Defense",
      "1. e4 e5 2. Nf3 d6",
    )
    const path = coachingBoardOpeningPath(ref)
    expect(parseCoachingBoardRoute(path)).toEqual({
      kind: "opening",
      openingLineRef: ref,
    })
  })

  test("refuses a malformed board address", () => {
    expect(parseCoachingBoardRoute("/app/board/games/")).toEqual({
      kind: "invalid",
    })
    expect(parseCoachingBoardRoute("/app/board/games/not-an-id")).toEqual({
      kind: "invalid",
    })
    expect(parseCoachingBoardRoute("/app/board/openings/not-a-line")).toEqual({
      kind: "invalid",
    })
    expect(parseCoachingBoardRoute("/app/board/review")).toEqual({
      kind: "invalid",
    })
  })
})
