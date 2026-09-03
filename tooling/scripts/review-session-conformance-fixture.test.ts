import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

import { expect, test } from "bun:test";

import { canonicalSanitizedConformancePgn } from "./review-session-conformance-fixture";

const fixtureRoot = resolve(
  import.meta.dir,
  "../../packages/shared-assets/fixtures/Synthet1",
);

test("accepts only the canonical sanitized conformance Game", async () => {
  const [canonical, raw] = await Promise.all([
    readFile(resolve(fixtureRoot, "lichess-export.pgn"), "utf8"),
    readFile(resolve(fixtureRoot, "lichess-export.raw.pgn"), "utf8"),
  ]);

  expect(canonicalSanitizedConformancePgn(raw)).toBe(
    canonicalSanitizedConformancePgn(canonical),
  );
  expect(() =>
    canonicalSanitizedConformancePgn('[White "not canonical"]\n\n1. e4 *'),
  ).toThrow("does not match canonical Synthet1");
});
