import { expect, test } from "vitest"

import {
  ANONYMOUS_GAME_STAGING_PER_HOUR,
  ANONYMOUS_RATE_WINDOW_MS,
  consumeAnonymousAllowance,
  memoryAnonymousAttemptStore,
} from "./anonymousRateLimit"

test("allows ten anonymous game-staging attempts in one hour", () => {
  const store = memoryAnonymousAttemptStore()
  const now = 1_700_000_000_000
  for (let index = 0; index < ANONYMOUS_GAME_STAGING_PER_HOUR; index += 1) {
    expect(consumeAnonymousAllowance(store, now + index)).toBe(true)
  }
  expect(
    consumeAnonymousAllowance(store, now + ANONYMOUS_GAME_STAGING_PER_HOUR),
  ).toBe(false)
})

test("forgets stamps outside the rolling hour", () => {
  const now = 1_700_000_000_000
  const store = memoryAnonymousAttemptStore({
    gameStaging: [now - ANONYMOUS_RATE_WINDOW_MS - 1],
  })
  expect(consumeAnonymousAllowance(store, now)).toBe(true)
})
