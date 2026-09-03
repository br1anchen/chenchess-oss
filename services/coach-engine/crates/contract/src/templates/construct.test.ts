import { describe, expect, test } from "vitest"

import {
  mintOperationId,
  mintRequestId,
  parseBranchParent,
  parseCriticalMomentId,
  parseGameImportId,
  parseJsonObject,
  parseLearningPathFeedbackState,
  parseMoveInput,
  readGameImportId,
  readJsonObject,
  readLearningPathFeedbackState,
} from "./construct.js"

describe("branded id constructors", () => {
  test("parseGameImportId requires the game-import prefix", () => {
    expect(() => parseGameImportId("game-import:fixture:1")).not.toThrow()
    expect(() => parseGameImportId("fixture:1")).toThrow(/GameImportId/)
    expect(readGameImportId("fixture:1")).toBeUndefined()
  })

  test("critical moment constructors accept both minted prefixes", () => {
    expect(
      parseCriticalMomentId(
        "review-moment:7eb4b0803c2b4fca8d80b3968928fe856bf15999626a402d9651694c0e80c799:10",
      ),
    ).toMatch(/^review-moment:/)
    expect(parseCriticalMomentId("critical-moment:curriculum")).toMatch(
      /^critical-moment:/,
    )
  })

  test("mint helpers produce the operation and request namespaces", () => {
    expect(mintOperationId("coach-app", "abc")).toBe("operation:coach-app:abc")
    expect(mintRequestId("web", "1")).toBe("request:web:1")
  })
})

describe("composite constructors", () => {
  test("move input and branch parent reject incomplete discriminators", () => {
    expect(() => parseMoveInput({ kind: "uci" })).toThrow(
      "Invalid type: Expected Object but received Object",
    )
    expect(() =>
      parseBranchParent({
        kind: "move",
        branchRef: "not-a-branch",
      }),
    ).toThrow('Invalid start: Expected "branch:" but received "not-a-b"')
  })

  test("learning path feedback is one object, not field checks", () => {
    expect(() =>
      parseLearningPathFeedbackState({
        currentVote: "thumbsUp",
        exposedSurfaces: ["coachApp"],
        learningPathRef: "learning-path:fixture:1",
      }),
    ).not.toThrow()
    expect(readLearningPathFeedbackState({ learningPathRef: "nope" })).toBe(
      undefined,
    )
  })
})

describe("JSON constructors", () => {
  test("parseJsonObject drops undefined keys and rejects non-JSON values", () => {
    expect(parseJsonObject({ keep: "yes", skip: undefined })).toEqual({
      keep: "yes",
    })
    expect(readJsonObject("nope")).toBeUndefined()
    expect(readJsonObject({ keep: "yes", fn: () => {} })).toBeUndefined()
    expect(readJsonObject({ keep: "yes", when: new Date(0) })).toBeUndefined()
    expect(() => parseJsonObject("nope")).toThrow(
      `Invalid type: Expected Object but received "nope"`,
    )
    expect(() => parseJsonObject({ keep: "yes", fn: () => {} })).toThrow(
      "JSON value cannot contain function, symbol, or bigint",
    )
    expect(() => parseJsonObject({ keep: "yes", when: new Date(0) })).toThrow(
      "JSON object must be a plain object",
    )
  })
})
