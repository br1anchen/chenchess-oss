import { afterAll, beforeAll, describe, expect, test } from "bun:test";

import { run, SmokeFailure, validateReview } from "./local-smoke";
import * as v from "valibot";

import {
  jsonObjectSchema,
  parseJsonObject,
  type JsonObject,
  type JsonValue,
} from "@chenchess/coach-engine-sdk";
const momentId = `review-moment:${"a".repeat(64)}:1`;

function importedResult(): JsonObject {
  return {
    kind: "gameImported",
    review: {
      summary: "Smoke review",
      playerProfile: { elo: 1450 },
      criticalMoments: [
        {
          criticalMomentId: momentId,
          ply: 1,
          objective: {
            bestMoveUci: "e2e4",
            bestEvaluation: { kind: "centipawns", value: 50 },
          },
          human: { mostLikelyMoveUci: "e2e4" },
        },
      ],
      positionViews: [],
      evaluationTimeline: [
        { ply: 1, evaluation: { kind: "centipawns", value: 50 } },
      ],
      practiceSelection: { eligibleLessons: [] },
    },
  };
}

let server: ReturnType<typeof Bun.serve>;

beforeAll(() => {
  server = Bun.serve({
    port: 0,
    fetch: async (request) => {
      const url = new URL(request.url);
      if (request.method === "GET" && url.pathname === "/health") {
        return Response.json({ ok: true });
      }
      if (
        request.method !== "POST" ||
        url.pathname !== "/api/v1/review-session/commands"
      ) {
        return new Response("not found", { status: 404 });
      }
      if (request.headers.get("Authorization") !== "Bearer smoke-jwt") {
        return new Response("unauthorized", { status: 401 });
      }
      const body = await request.json();
      if (parseSmokeImportRating(body) !== 1450) {
        return new Response("bad request", { status: 400 });
      }
      const events = [
        { event: { kind: "accepted", operation: "gameImport" } },
        { event: { kind: "completed", result: importedResult() } },
      ];
      return new Response(
        `${events.map((event) => JSON.stringify(event)).join("\n")}\n`,
        {
          headers: { "Content-Type": "application/x-ndjson" },
        },
      );
    },
  });
});

afterAll(() => server.stop(true));

describe("local smoke", () => {
  test("checks the authenticated Game Review smoke contract", async () => {
    const result = await run(`http://127.0.0.1:${server.port}`, "smoke-jwt");
    expect(result.review.summary).toBe("Smoke review");
  });

  test("rejects an incomplete pipeline response", () => {
    const result = parseJsonBag(importedResult());
    const review = parseJsonBag(result.review);
    delete review.evaluationTimeline;
    result.review = review;
    expect(() => validateReview(result)).toThrow(
      new SmokeFailure(
        "Game Review did not contain the real-game evaluation timeline",
      ),
    );
  });

  test("rejects a missing Game Review summary", () => {
    const result = parseJsonBag(importedResult());
    const review = parseJsonBag(result.review);
    review.summary = "";
    result.review = review;
    expect(() => validateReview(result)).toThrow(/summary/);
  });
});

function parseJsonBag(value: unknown) {
  const bag: { [key: string]: JsonValue } = {};
  const parsed = parseJsonObject(value);
  for (const key of Object.keys(parsed)) {
    const item = parsed[key];
    if (item === undefined) continue;
    bag[key] = item;
  }
  return bag;
}

function parseSmokeImportRating(body: unknown): number | undefined {
  if (!v.is(jsonObjectSchema, body)) return undefined;
  if (!v.is(jsonObjectSchema, body.command)) return undefined;
  if (!v.is(jsonObjectSchema, body.command.eloProfile)) return undefined;
  const rating = body.command.eloProfile.rating;
  return v.is(v.number(), rating) ? rating : undefined;
}
