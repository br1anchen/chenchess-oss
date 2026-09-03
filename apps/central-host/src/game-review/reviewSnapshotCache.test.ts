import {
  fromGameImportId,
  fromReviewContentDigest,
  type GameReviewSnapshot,
} from "@chenchess/coach-engine-sdk"
import { expect, test } from "vitest"

import {
  memoryReviewSnapshotCache,
  PROJECTION_VERSION,
} from "./reviewSnapshotCache"

const OWNER = "uid:owner"
const OTHER = "uid:other"
const REVIEW = fromGameImportId("game-import:one:owner")

function digest(fill: string) {
  return fromReviewContentDigest(`sha256:${fill.repeat(64)}`)
}

function snapshot(gameImportId: string): GameReviewSnapshot {
  // SAFETY: the cache stores whatever the projection produced and never reads
  // into it, so identity of the stored value is the whole contract under test.
  // A marker keeps these cases about keying, purging and versioning rather
  // than about a projection this module does not own.
  return { gameImportId } as unknown as GameReviewSnapshot
}

test("a review written for one Player is unreadable as another", async () => {
  const cache = memoryReviewSnapshotCache()
  await cache.write(OWNER, REVIEW, {
    contentDigest: digest("a"),
    snapshot: snapshot("owned"),
  })

  expect(await cache.read(OWNER, REVIEW)).toEqual({
    contentDigest: digest("a"),
    snapshot: snapshot("owned"),
  })
  expect(await cache.read(OTHER, REVIEW)).toBeUndefined()
})

test("signing out leaves nothing readable behind", async () => {
  const cache = memoryReviewSnapshotCache()
  await cache.write(OWNER, REVIEW, {
    contentDigest: digest("a"),
    snapshot: snapshot("owned"),
  })

  await cache.purge()

  expect(await cache.read(OWNER, REVIEW)).toBeUndefined()
})

test("an entry projected by an older bundle is a miss, not a stale hit", async () => {
  const stale = new Map([
    [
      `${OWNER} ${REVIEW}`,
      {
        contentDigest: digest("a"),
        projectionVersion: PROJECTION_VERSION - 1,
        snapshot: snapshot("stale"),
      },
    ],
  ])

  expect(
    await memoryReviewSnapshotCache(stale).read(OWNER, REVIEW),
  ).toBeUndefined()
})

test("a rewritten review replaces what was cached for that address", async () => {
  const cache = memoryReviewSnapshotCache()
  await cache.write(OWNER, REVIEW, {
    contentDigest: digest("a"),
    snapshot: snapshot("first"),
  })
  await cache.write(OWNER, REVIEW, {
    contentDigest: digest("b"),
    snapshot: snapshot("second"),
  })

  expect(await cache.read(OWNER, REVIEW)).toEqual({
    contentDigest: digest("b"),
    snapshot: snapshot("second"),
  })
})
