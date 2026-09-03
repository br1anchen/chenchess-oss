import { describe, expect, test } from "bun:test";
import { resolve } from "node:path";

import {
  checkLichessApiConformance,
  compareDecodedStruct,
  readPublishedApi,
  readRustStructs,
} from "./lichess-api-conformance";

const ROOT = resolve(import.meta.dir, "../..");

describe("the Coach Engine's Lichess client", () => {
  test("sends only paths and query parameters Lichess publishes", () => {
    const errors = checkLichessApiConformance(ROOT).filter(
      (finding) => finding.severity === "error",
    );

    expect(errors).toEqual([]);
  });

  test("reads GameJson through the published game export operation", () => {
    const published = readPublishedApi();

    expect(published.paths.has("/api/games/user/{}")).toBe(true);
    const query = published.queryParameters.get("/api/games/user/{}");
    for (const parameter of [
      "since",
      "until",
      "max",
      "moves",
      "perfType",
      "sort",
    ]) {
      expect(query?.has(parameter)).toBe(true);
    }
  });
});

describe("a decoded struct compared against its published schema", () => {
  const gameJson = readPublishedApi().schemas.get("GameJson");

  test("fails when a required field is not published", () => {
    // The exact shape that shipped: `turns` is not a GameJson property, so every Lichess Game
    // failed to decode and the digest published Chess.com Games alone.
    const [struct] = [
      ...readRustStructs(
        "struct LichessWindowGame {\n    id: String,\n    turns: u32,\n}",
      ).values(),
    ];

    const findings = compareDecodedStruct(struct!, "GameJson", gameJson!);

    expect(findings).toEqual([
      {
        severity: "error",
        kind: "requiredFieldIsNotPublished",
        detail:
          'LichessWindowGame requires "turns", which Lichess does not publish on GameJson; every response will fail to decode',
      },
    ]);
  });

  test("only warns when an unpublished field is optional", () => {
    const [struct] = [
      ...readRustStructs(
        "struct LichessWindowGame {\n    id: String,\n    turns: Option<u32>,\n}",
      ).values(),
    ];

    const findings = compareDecodedStruct(struct!, "GameJson", gameJson!);

    expect(findings.map((finding) => finding.severity)).toEqual(["warning"]);
  });

  test("accepts a struct whose fields are all published", () => {
    const [struct] = [
      ...readRustStructs(
        "struct LichessWindowGame {\n    id: String,\n    last_move_at: Option<u64>,\n    moves: Option<String>,\n}",
      ).values(),
    ];

    const findings = compareDecodedStruct(struct!, "GameJson", gameJson!);

    expect(findings).toEqual([]);
  });
});
