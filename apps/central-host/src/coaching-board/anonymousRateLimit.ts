/**
 * Anonymous Coaching Board rate limit (v1 lock).
 *
 * The signed lock required a conservative number and did not pick one.
 * Ten lobby import-form openings per rolling hour, per client, is the v1
 * cap. A Sign-in refusal for the durable Game import does not spend this
 * allowance. The unused opening-analysis allowance is retired until an
 * anonymous analysis route exists.
 */
export const ANONYMOUS_GAME_STAGING_PER_HOUR = 10
export const ANONYMOUS_RATE_WINDOW_MS = 60 * 60 * 1000

export type AnonymousBoardAllowance = "gameStaging"

export type AnonymousAttemptStore = {
  read(kind: AnonymousBoardAllowance): readonly number[]
  write(kind: AnonymousBoardAllowance, stamps: readonly number[]): void
}

const storageKey = {
  gameStaging: "chenchess.board.anon.gameStaging",
} as const

type AnonymousAttemptSeed = {
  gameStaging?: readonly number[]
}

export function memoryAnonymousAttemptStore(
  seed: AnonymousAttemptSeed = {},
): AnonymousAttemptStore {
  let gameStaging = [...(seed.gameStaging ?? [])]
  return {
    read: () => gameStaging,
    write: (_kind, next) => {
      gameStaging = [...next]
    },
  }
}

export function localAnonymousAttemptStore(
  storage: Pick<Storage, "getItem" | "setItem">,
): AnonymousAttemptStore {
  return {
    read: (kind) => parseStamps(storage.getItem(storageKey[kind])),
    write: (kind, stamps) => {
      storage.setItem(storageKey[kind], JSON.stringify(stamps))
    },
  }
}

/**
 * Spend one anonymous game-staging allowance. Returns whether the request
 * may proceed.
 */
export function consumeAnonymousAllowance(
  store: AnonymousAttemptStore,
  now = Date.now(),
): boolean {
  const recent = store
    .read("gameStaging")
    .filter((stamp) => now - stamp < ANONYMOUS_RATE_WINDOW_MS)
  if (recent.length >= ANONYMOUS_GAME_STAGING_PER_HOUR) return false
  store.write("gameStaging", [...recent, now])
  return true
}

function parseStamps(raw: string | null): number[] {
  if (raw === null) return []
  try {
    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed.filter((stamp): stamp is number => Number.isFinite(stamp))
  } catch {
    return []
  }
}
