import { expect, test } from "vitest"

import { digestCoachVoice } from "./digestCoachVoice"

const clearance = {
  purpose: "reinforcement",
  title: "Clearance",
} as const
const desperado = {
  purpose: "improvement",
  title: "Desperado",
} as const

test("leads with what landed and sets the improvement as homework", () => {
  const voice = digestCoachVoice(3, [clearance, desperado])

  expect(voice?.summary).toBe(
    "Nice work in 3 games yesterday — your clearance is landing. Here is what to keep and what to sharpen.",
  )
  expect(voice?.homework).toBe(
    "Today's homework: desperado. Take the lesson, then the drill, and look for it in your next game.",
  )
})

test("counts a single Game in words", () => {
  expect(digestCoachVoice(1, [clearance])?.summary).toContain(
    "Nice work in one game yesterday",
  )
})

test("sets the reinforcement as homework when nothing needs improving", () => {
  expect(digestCoachVoice(2, [clearance])?.homework).toBe(
    "Today's homework: keep clearance sharp — one drill before you play.",
  )
})

test("names the improvement when nothing was reinforced", () => {
  const voice = digestCoachVoice(2, [desperado])

  expect(voice?.summary).toBe(
    "2 games yesterday, and one idea is worth your attention before the next.",
  )
  expect(voice?.homework).toContain("desperado")
})

/* The homework is the priority, so a digest without one has no coaching to
 * put in the Player's mouth — including the published-digest case where Games
 * were reviewed but no priority came out of them. */
test("stays silent when the digest carries no priorities", () => {
  expect(digestCoachVoice(4, [])).toBeNull()
  expect(digestCoachVoice(undefined, [])).toBeNull()
})
