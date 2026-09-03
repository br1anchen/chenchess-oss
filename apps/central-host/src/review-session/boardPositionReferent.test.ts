import { expect, test } from "vitest"

import {
  containsRawUci,
  PLAYER_VISIBLE_MOVE_FALLBACK,
  playerVisibleSanFromLegalUci,
  playerVisibleSanLiteral,
} from "@chenchess/review-projection"

import { boardPositionReferent } from "./boardPositionReferent"

const AFTER_E4_FEN =
  "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1"

// The sentence is what a Player pastes in front of a question, so each form
// is pinned as the Player reads it: the board's own kind and move, nothing
// from the snapshot the coach will read for itself.
test("names the Game's own position by the move the caption shows, which the board stands before", () => {
  expect(
    boardPositionReferent({
      heading: playerVisibleSanLiteral("23… Nf6"),
      kind: null,
      lineStep: 0,
      played: false,
    }),
  ).toBe("About the position on my Coaching Board (before 23… Nf6):")
})

test("names an off-game position by its kind first", () => {
  expect(
    boardPositionReferent({
      heading: playerVisibleSanLiteral("21. Rd1"),
      kind: "Alternative branch",
      lineStep: 0,
      played: true,
    }),
  ).toBe(
    "About the position on my Coaching Board (alternative branch, after 21. Rd1):",
  )
})

test("a shown line stands before the caption's move unless it refutes the played one", () => {
  expect(
    boardPositionReferent({
      heading: playerVisibleSanLiteral("23… Nf6"),
      kind: "Engine line",
      lineStep: 0,
      played: false,
    }),
  ).toBe(
    "About the position on my Coaching Board (engine line, before 23… Nf6):",
  )
  expect(
    boardPositionReferent({
      heading: playerVisibleSanLiteral("23… Nf6"),
      kind: "Played refutation",
      lineStep: 0,
      played: true,
    }),
  ).toBe(
    "About the position on my Coaching Board (played refutation, after 23… Nf6):",
  )
})

test("says how far into a shown line the board has walked", () => {
  expect(
    boardPositionReferent({
      heading: playerVisibleSanLiteral("23… Nf6"),
      kind: "Engine line",
      lineStep: 1,
      played: false,
    }),
  ).toBe(
    "About the position on my Coaching Board (engine line from 23… Nf6, 1 move in):",
  )
  expect(
    boardPositionReferent({
      heading: playerVisibleSanLiteral("23… Nf6"),
      kind: "Engine line",
      lineStep: 3,
      played: false,
    }),
  ).toBe(
    "About the position on my Coaching Board (engine line from 23… Nf6, 3 moves in):",
  )
})

test("a board that cannot name its move points at the board alone", () => {
  expect(
    boardPositionReferent({
      heading: PLAYER_VISIBLE_MOVE_FALLBACK,
      kind: null,
      lineStep: 0,
      played: false,
    }),
  ).toBe("About the position on my Coaching Board:")
  expect(
    boardPositionReferent({
      heading: PLAYER_VISIBLE_MOVE_FALLBACK,
      kind: "Engine line",
      lineStep: 2,
      played: false,
    }),
  ).toBe("About the position on my Coaching Board (engine line, 2 moves in):")
})

test("carries SAN the Player reads, never raw UCI", () => {
  const referent = boardPositionReferent({
    heading: playerVisibleSanFromLegalUci(AFTER_E4_FEN, "e7e5"),
    kind: "Alternative branch",
    lineStep: 0,
    played: true,
  })
  expect(referent).toContain("e5")
  expect(containsRawUci(referent)).toBe(false)
})
