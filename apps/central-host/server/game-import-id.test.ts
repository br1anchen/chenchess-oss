import { expect, test } from "vitest"

import { gameImportIdForm, isPlausibleGameImportId } from "./game-import-id"

test("accepts what Coach Engine mints and the shared fixture corpus", () => {
  expect(
    isPlausibleGameImportId(`game-import:${"a".repeat(64)}:${"b".repeat(32)}`),
  ).toBe(true)
  // `generate_review_session_contract.rs` seeds every fixture with this handle.
  expect(isPlausibleGameImportId("game-import:fixture:1")).toBe(true)
})

test("rejects handles no Game Import could have produced", () => {
  for (const candidate of [
    "the review from earlier",
    "game-import",
    "game-import:",
    "game-import:only-two",
    "game-import::1",
    "game-import:a:b:c",
    "review-session:abc:def",
    "",
  ]) {
    expect(isPlausibleGameImportId(candidate)).toBe(false)
  }
})

test("describes a rejected handle without reproducing it", () => {
  const form = gameImportIdForm("review-session:DEADBEEF:not a handle")

  expect(form).toEqual({
    namespace: "other",
    segmentCharsets: ["base64url", "base64url", "other"],
    segmentCount: 3,
    segmentLengths: [14, 8, 12],
    totalLength: 36,
  })
  // A rejected handle is model-authored text; only its form is safe to log, so
  // no field may carry a fragment of the handle itself.
  expect(JSON.stringify(form)).not.toContain("DEADBEEF")
  expect(JSON.stringify(form)).not.toContain("review-session")
})
