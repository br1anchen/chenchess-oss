import { expect, test } from "vitest"

import { projectedRestingPoint, resistedOffset } from "./swipeMomentum"

test("a throw lands where it was aimed, not where the finger left", () => {
  // A slow drag barely travels past the release point…
  expect(projectedRestingPoint(-40)).toBeCloseTo(-19.96, 1)
  // …while a flick carries several hundred pixels, which is what lets a swipe
  // past the commit threshold be a throw rather than a long drag.
  expect(projectedRestingPoint(-800)).toBeCloseTo(-399.2, 1)
  expect(projectedRestingPoint(0)).toBe(0)
  // Direction is the velocity's, so a rightward throw projects rightward.
  expect(projectedRestingPoint(800)).toBeCloseTo(399.2, 1)
})

test("the row tracks one-to-one inside its travel", () => {
  expect(resistedOffset(0, 96, 600)).toBe(0)
  expect(resistedOffset(-50, 96, 600)).toBe(-50)
  expect(resistedOffset(-96, 96, 600)).toBe(-96)
})

test("past the reveal the row resists instead of stopping dead", () => {
  const resisted = resistedOffset(-200, 96, 600)
  // It keeps moving — a hard clamp would read as a frozen interface…
  expect(resisted).toBeLessThan(-96)
  // …but by much less than the finger did, and never past the row itself.
  expect(resisted).toBeGreaterThan(-200)
  expect(resisted).toBeGreaterThan(-600)
  // Further always means further, so the row never doubles back under the
  // pointer.
  expect(resistedOffset(-400, 96, 600)).toBeLessThan(resisted)
})

test("the closed edge resists harder, because nothing is revealed that way", () => {
  const reveal = 96
  const rowWidth = 600
  // The same overshoot moves the row far less at the closed edge than at the
  // open one: there is no action to the right, so the nudge stays a nudge.
  const rightward = resistedOffset(104, reveal, rowWidth)
  const leftward = -reveal - resistedOffset(-reveal - 104, reveal, rowWidth)
  expect(rightward).toBeLessThan(Math.abs(leftward))
  expect(rightward).toBeLessThan(reveal)
})
