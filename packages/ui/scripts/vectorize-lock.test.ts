import { describe, expect, test } from "vitest"

import {
  parseVectorizeLock,
  vectorizeLockFromSourceDigests,
  type VectorizeLockSources,
} from "./vectorize-lock"

const digest = `sha256:${"a".repeat(64)}` as const

function fixtureSources(): VectorizeLockSources {
  // SAFETY: every VectorizeSourceName is assigned before return.
  const sources = {} as unknown as VectorizeLockSources
  for (const color of ["white", "black"] as const) {
    for (const role of [
      "king",
      "queen",
      "bishop",
      "knight",
      "rook",
      "pawn",
    ] as const) {
      sources[`${color}-${role}.webp`] = digest
    }
  }
  return sources
}

describe("parseVectorizeLock", () => {
  test("accepts a lock written from source digests", () => {
    const lock = vectorizeLockFromSourceDigests(fixtureSources())
    expect(parseVectorizeLock(lock).sources["white-king.webp"]).toBe(digest)
  })

  test("rejects a lock that drops a source WebP", () => {
    const lock = vectorizeLockFromSourceDigests(fixtureSources())
    expect(() => parseVectorizeLock({ ...lock, sources: {} })).toThrow(
      "vectorize.lock.json sources is missing white-king.webp",
    )
  })
})
