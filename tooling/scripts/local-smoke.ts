import * as v from "valibot";
import { readFile } from "node:fs/promises";

import { GameReview, jsonObjectSchema } from "@chenchess/coach-engine-sdk";
export const DEFAULT_PGN = `[Event "Local smoke test"]
[White "Smoke"]
[Black "Test"]
[Result "0-1"]

1. f3 e5 2. g4 Qh4# 0-1`;

const REQUEST_TIMEOUT_MS = 180_000;

export class SmokeFailure extends Error {
  constructor(parseMessage: string, options?: ErrorOptions) {
    super(parseMessage, options);
    this.name = "SmokeFailure";
  }
}

export type ImportedGameResult = { kind: "gameImported"; review: GameReview };

export async function run(
  apiUrl: string,
  authToken: string,
  pgn = DEFAULT_PGN,
): Promise<ImportedGameResult> {
  const baseUrl = apiUrl.replace(/\/+$/, "");
  await requestJson(`${baseUrl}/health`);
  const events = await parseRequestNdjson(
    `${baseUrl}/api/v1/review-session/commands`,
    authToken,
    {
      requestId: "request:local-smoke:import",
      operationId: "operation:local-smoke:import",
      surface: "web",
      command: {
        kind: "importGame",
        source: { kind: "pastedPgn", pgn },
        reviewSide: { kind: "selected", reviewSide: "both" },
        eloProfile: { kind: "playerProvided", rating: 1450 },
      },
    },
  );
  const terminal = events.at(-1);
  const event = v.is(jsonObjectSchema, terminal) ? terminal.event : undefined;
  if (!v.is(jsonObjectSchema, event) || event.kind !== "completed") {
    throw new SmokeFailure("Review Session import did not complete");
  }
  const result = event.result;
  parseValidateReview(result);
  return result;
}

export const validateReview = parseValidateReview;

export function parseValidateReview(
  result: unknown,
): asserts result is ImportedGameResult {
  if (!v.is(jsonObjectSchema, result) || result.kind !== "gameImported") {
    throw new SmokeFailure("terminal event did not contain a Game import");
  }
  const review = result.review;
  if (!v.is(jsonObjectSchema, review)) {
    throw new SmokeFailure("Game import did not contain a Game Review");
  }
  const moments = review.criticalMoments;
  if (!Array.isArray(moments) || moments.length === 0) {
    throw new SmokeFailure("Game Review did not contain a Critical Moment");
  }
  for (const moment of moments) {
    const objective = v.is(jsonObjectSchema, moment)
      ? moment.objective
      : undefined;
    const human = v.is(jsonObjectSchema, moment) ? moment.human : undefined;
    if (
      !v.is(jsonObjectSchema, objective) ||
      !parseNonEmptyString(objective.bestMoveUci)
    ) {
      throw new SmokeFailure(
        "Critical Moment did not contain Engine Analysis evidence",
      );
    }
    if (!v.is(jsonObjectSchema, objective.bestEvaluation)) {
      throw new SmokeFailure(
        "Critical Moment did not contain an objective evaluation",
      );
    }
    if (
      !v.is(jsonObjectSchema, human) ||
      !parseNonEmptyString(human.mostLikelyMoveUci)
    ) {
      throw new SmokeFailure(
        "Critical Moment did not contain Human Move Model evidence",
      );
    }
  }

  if (!parseNonEmptyString(review.summary)) {
    throw new SmokeFailure("Game Review did not contain a summary");
  }
  const timeline = review.evaluationTimeline;
  if (!Array.isArray(timeline) || timeline.length === 0) {
    throw new SmokeFailure(
      "Game Review did not contain the real-game evaluation timeline",
    );
  }
}

async function requestJson(url: string) {
  let response: Response;
  try {
    response = await fetch(url, {
      signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
    });
  } catch (error) {
    throw new SmokeFailure(`request to ${url} failed: ${parseMessage(error)}`, {
      cause: error,
    });
  }
  if (!response.ok) {
    throw new SmokeFailure(`request to ${url} failed: HTTP ${response.status}`);
  }
  try {
    return await response.json();
  } catch (error) {
    throw new SmokeFailure(`request to ${url} failed: ${parseMessage(error)}`, {
      cause: error,
    });
  }
}

async function parseRequestNdjson(
  url: string,
  authToken: string,
  payload: unknown,
): Promise<unknown[]> {
  let response: Response;
  try {
    response = await fetch(url, {
      method: "POST",
      headers: {
        Accept: "application/x-ndjson",
        Authorization: `Bearer ${authToken}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(payload),
      signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
    });
  } catch (error) {
    throw new SmokeFailure(`request to ${url} failed: ${parseMessage(error)}`, {
      cause: error,
    });
  }
  if (!response.ok) {
    throw new SmokeFailure(`request to ${url} failed: HTTP ${response.status}`);
  }
  try {
    return (await response.text())
      .split("\n")
      .filter((line) => line.trim().length > 0)
      .map((line) => JSON.parse(line) as unknown);
  } catch (error) {
    throw new SmokeFailure(`request to ${url} failed: ${parseMessage(error)}`, {
      cause: error,
    });
  }
}

function parseNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function parseMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

type CliOptions = { apiUrl: string; authToken?: string; pgnFile?: string };

function parseArgs(arguments_: string[]): CliOptions {
  const options: CliOptions = {
    apiUrl: process.env.API_URL ?? "http://127.0.0.1:8787",
  };
  for (let index = 0; index < arguments_.length; index += 1) {
    const argument = arguments_[index];
    const value = arguments_[index + 1];
    if (argument === "--api-url" && value) {
      options.apiUrl = value;
      index += 1;
    } else if (argument === "--auth-token" && value) {
      options.authToken = value;
      index += 1;
    } else if (argument === "--pgn-file" && value) {
      options.pgnFile = value;
      index += 1;
    } else {
      throw new SmokeFailure(`unknown or incomplete argument: ${argument}`);
    }
  }
  options.authToken ??= process.env.AUTH_TOKEN;
  return options;
}

async function main(): Promise<void> {
  const options = parseArgs(process.argv.slice(2));
  if (!options.authToken) {
    throw new SmokeFailure("--auth-token or AUTH_TOKEN is required");
  }
  const pgn = options.pgnFile
    ? await readFile(options.pgnFile, "utf8")
    : DEFAULT_PGN;
  const result = await run(options.apiUrl, options.authToken, pgn);
  console.log(
    `local smoke test passed: ${result.review.criticalMoments.length} Critical Moments; ` +
      `summary: ${result.review.summary}`,
  );
}

if (import.meta.main) {
  main().catch((error: unknown) => {
    console.error(`local smoke test failed: ${parseMessage(error)}`);
    process.exitCode = 1;
  });
}
