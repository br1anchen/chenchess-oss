import { expect, test } from "vitest"

import { gameExplorationRefusalNotice } from "./boardExplorationNotice"

test("a spent allowance and a slow engine are told apart", () => {
  expect(gameExplorationRefusalNotice({ kind: "explorationExhausted" })).toBe(
    "You have used up this game’s exploration for now.",
  )
  expect(gameExplorationRefusalNotice({ kind: "deadlineReached" })).toBe(
    "The engine ran out of time on that line.",
  )
})

test("an illegal move is named as illegal, not as an engine failure", () => {
  expect(gameExplorationRefusalNotice({ kind: "illegalMove" })).toBe(
    "That move is not legal from this position.",
  )
})

test("an unreachable engine does not claim the move was bad", () => {
  expect(gameExplorationRefusalNotice({ kind: "failed" })).toBe(
    "The engine could not be reached. Try that move again.",
  )
  // A mismatch that survived the fresh-identity retry, and a completion for an
  // operation the board never asked for, are both engine faults rather than
  // anything the Player did.
  expect(gameExplorationRefusalNotice({ kind: "idempotencyKeyMismatch" })).toBe(
    "The engine could not be reached. Try that move again.",
  )
})
