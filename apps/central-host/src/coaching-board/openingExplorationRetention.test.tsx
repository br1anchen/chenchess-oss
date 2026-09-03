// @vitest-environment jsdom

import { cleanup, render } from "@testing-library/react"
import { afterEach, expect, test } from "vitest"

import {
  clearOpeningExplorationRetention,
  OPENING_EXPLORATION_RETENTION_LIMIT,
  recallOpeningExploration,
  retainOpeningExploration,
} from "./openingExplorationRetention"
import { openingLineRefFromPath } from "./openingLineRef"
import { useOpeningExplorationBoundary } from "./useOpeningExplorationBoundary"

afterEach(() => {
  cleanup()
  clearOpeningExplorationRetention()
})

function ref(index: number) {
  return openingLineRefFromPath(
    "A00",
    `Fixture Opening ${index}`,
    `1. Nh3 line${index}`,
  )
}

function exploration(viewedPly: number) {
  return { activeBranchId: null, branches: [], viewedPly }
}

const player = "player:one"

test("exploration is retained per opening line, bounded at five, oldest evicted", () => {
  for (
    let index = 0;
    index < OPENING_EXPLORATION_RETENTION_LIMIT + 1;
    index++
  ) {
    retainOpeningExploration(player, ref(index), exploration(index))
  }
  expect(recallOpeningExploration(player, ref(0))).toBeUndefined()
  for (let index = 1; index <= OPENING_EXPLORATION_RETENTION_LIMIT; index++) {
    expect(recallOpeningExploration(player, ref(index))).toEqual(
      exploration(index),
    )
  }
})

test("a recall refreshes recency, so the recalled line survives the next eviction", () => {
  for (let index = 0; index < OPENING_EXPLORATION_RETENTION_LIMIT; index++) {
    retainOpeningExploration(player, ref(index), exploration(index))
  }
  recallOpeningExploration(player, ref(0))
  retainOpeningExploration(player, ref(9), exploration(9))
  expect(recallOpeningExploration(player, ref(0))).toEqual(exploration(0))
  expect(recallOpeningExploration(player, ref(1))).toBeUndefined()
})

test("retaining the same line twice keeps one slot", () => {
  retainOpeningExploration(player, ref(0), exploration(1))
  retainOpeningExploration(player, ref(0), exploration(2))
  expect(recallOpeningExploration(player, ref(0))).toEqual(exploration(2))
})

function Boundary({ playerId }: { playerId: string | null }) {
  useOpeningExplorationBoundary(playerId)
  return null
}

test("a Player change clears retained exploration; a rerender for the same Player keeps it", () => {
  retainOpeningExploration(player, ref(1), exploration(4))
  const view = render(<Boundary playerId={player} />)
  expect(recallOpeningExploration(player, ref(1))).toEqual(exploration(4))

  view.rerender(<Boundary playerId={player} />)
  expect(recallOpeningExploration(player, ref(1))).toEqual(exploration(4))

  view.rerender(<Boundary playerId="player:two" />)
  expect(recallOpeningExploration(player, ref(1))).toBeUndefined()

  retainOpeningExploration(player, ref(2), exploration(7))
  view.rerender(<Boundary playerId={null} />)
  expect(recallOpeningExploration(player, ref(2))).toBeUndefined()
})

test("another Player recalls nothing, whatever the effect ordering", () => {
  retainOpeningExploration(player, ref(1), exploration(4))
  // A board surface recalls during render, before the boundary effect that
  // clears the previous identity has run.
  expect(recallOpeningExploration("player:two", ref(1))).toBeUndefined()
  expect(recallOpeningExploration(null, ref(1))).toBeUndefined()
  expect(recallOpeningExploration(player, ref(1))).toEqual(exploration(4))
})

test("retaining for a new Player drops the previous identity's lines", () => {
  retainOpeningExploration(player, ref(1), exploration(4))
  retainOpeningExploration("player:two", ref(2), exploration(5))
  expect(recallOpeningExploration("player:two", ref(1))).toBeUndefined()
  expect(recallOpeningExploration("player:two", ref(2))).toEqual(exploration(5))
})
