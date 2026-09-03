import { existsSync, readFileSync } from "node:fs"

import { expect, test } from "vitest"

import {
  parseGroundingSentences,
  parseSharedLimits,
  sharedGroundingSentences,
  sharedLimits,
} from "./index.js"
import {
  canonicalGamePgnPath,
  canonicalGameRawPgnPath,
  canonicalGameRecordingPath,
} from "./paths.js"

test("grounding sentences are the closed shared list", () => {
  expect(sharedGroundingSentences).toHaveLength(7)
  expect(() => parseGroundingSentences([""])).toThrow(
    /Invalid length: Expected >=1 but received 0/,
  )
  expect(() => parseGroundingSentences("nope")).toThrow(
    /Invalid type: Expected Array but received "nope"/,
  )
})

test("shared limits are the V1 snapshot both deployables import", () => {
  expect(sharedLimits.commentAuthoringDeadlineSeconds).toBe(10)
  expect(sharedLimits.hostTurnMaxPriorTurns).toBe(4)
  expect(() => parseSharedLimits({})).toThrow(
    /Expected "commentAuthoringDeadlineSeconds"/,
  )
  expect(() =>
    parseSharedLimits({
      commentAuthoringDeadlineSeconds: 0,
      hostTurnMaxPriorTurns: 4,
    }),
  ).toThrow(/Invalid value: Expected >=1 but received 0/)
})

test("canonical Game files live in this package", () => {
  expect(existsSync(canonicalGamePgnPath)).toBe(true)
  expect(existsSync(canonicalGameRawPgnPath)).toBe(true)
  expect(existsSync(canonicalGameRecordingPath)).toBe(true)
  expect(readFileSync(canonicalGamePgnPath, "utf8")).toContain("Synthet1")
})
