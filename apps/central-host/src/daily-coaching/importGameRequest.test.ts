import { expect, test } from "vitest"

import {
  parseImportGameRequest,
  preselectedReviewSide,
} from "./importGameRequest"

const pgn = `[Event "Rated blitz game"]
[WhiteElo "1500"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 1-0`

test("imports a bare Lichess URL at the selected side", () => {
  expect(
    parseImportGameRequest({
      elo: "",
      reviewSide: "black",
      source: " https://lichess.org/Synthet1 ",
    }),
  ).toEqual({
    eloProfile: { kind: "fromImportedMetadata" },
    kind: "ready",
    reviewSide: "black",
    source: { kind: "lichessUrl", url: "https://lichess.org/Synthet1" },
  })
})

test("keeps a side-qualified Lichess URL and its matching selection", () => {
  expect(
    parseImportGameRequest({
      elo: "1800",
      reviewSide: "white",
      source: "https://lichess.org/Synthet1/white",
    }),
  ).toMatchObject({
    eloProfile: { kind: "playerProvided", rating: 1800 },
    kind: "ready",
    reviewSide: "white",
    source: { kind: "lichessUrl", url: "https://lichess.org/Synthet1/white" },
  })
})

test("lets the control override a side-qualified Lichess URL", () => {
  // `resolve_lichess_review_side` takes the selected side over the qualifier, so
  // refusing this here would reject a Game the Engine imports.
  expect(
    parseImportGameRequest({
      elo: "",
      reviewSide: "white",
      source: "https://lichess.org/Synthet1/black",
    }),
  ).toMatchObject({
    kind: "ready",
    reviewSide: "white",
    source: { kind: "lichessUrl", url: "https://lichess.org/Synthet1/black" },
  })
})

test("reads the preselected side out of a side-qualified Lichess URL", () => {
  expect(preselectedReviewSide("  https://lichess.org/Synthet1/black ")).toBe(
    "black",
  )
  expect(preselectedReviewSide("https://lichess.org/Synthet1")).toBeNull()
  expect(
    preselectedReviewSide("https://www.chess.com/game/live/100000000001"),
  ).toBeNull()
})

test("refuses Both sides for a Lichess URL", () => {
  expect(
    parseImportGameRequest({
      elo: "1500",
      reviewSide: "both",
      source: "https://lichess.org/Synthet1",
    }),
  ).toEqual({
    field: "reviewSide",
    kind: "invalid",
    message: "A Lichess game is reviewed as White or as Black.",
  })
})

test("answers an http Lichess URL as a Lichess URL, not an unknown host", () => {
  expect(
    parseImportGameRequest({
      elo: "",
      reviewSide: "white",
      source: "http://lichess.org/Synthet1",
    }),
  ).toMatchObject({
    kind: "invalid",
    message: expect.stringContaining("https://lichess.org/"),
  })
})

test("imports a Chess.com game URL", () => {
  expect(
    parseImportGameRequest({
      elo: "",
      reviewSide: "white",
      source: "https://www.chess.com/game/live/100000000001",
    }),
  ).toMatchObject({
    kind: "ready",
    source: {
      kind: "chessComUrl",
      url: "https://www.chess.com/game/live/100000000001",
    },
  })
})

test("refuses Both sides for a provider URL", () => {
  expect(
    parseImportGameRequest({
      elo: "1500",
      reviewSide: "both",
      source: "https://www.chess.com/game/live/100000000001",
    }),
  ).toEqual({
    field: "reviewSide",
    kind: "invalid",
    message: "A Chess.com game is reviewed as White or as Black.",
  })
})

test("imports a pasted PGN cut at its result", () => {
  const request = parseImportGameRequest({
    elo: "",
    reviewSide: "white",
    source: `${pgn}\n\nthanks!`,
  })

  expect(request).toMatchObject({ kind: "ready", reviewSide: "white" })
  expect(request.kind === "ready" && request.source).toEqual({
    kind: "pastedPgn",
    pgn,
  })
})

test("accepts a PGN flattened onto one line", () => {
  const flattened = pgn.replace(/\s+/g, " ")

  expect(
    parseImportGameRequest({ elo: "", reviewSide: "white", source: flattened }),
  ).toMatchObject({
    kind: "ready",
    source: { kind: "pastedPgn", pgn: flattened },
  })
})

test("accepts Both sides for a pasted PGN only with an Elo", () => {
  expect(
    parseImportGameRequest({ elo: "", reviewSide: "both", source: pgn }),
  ).toEqual({
    field: "elo",
    kind: "invalid",
    message: "Reviewing both sides needs an Elo to coach at.",
  })
  expect(
    parseImportGameRequest({ elo: "1500", reviewSide: "both", source: pgn }),
  ).toMatchObject({ kind: "ready", reviewSide: "both" })
})

test("refuses a PGN with no result", () => {
  expect(
    parseImportGameRequest({
      elo: "",
      reviewSide: "white",
      source: "1. e4 e5 2. Nf3 Nc6",
    }),
  ).toEqual({
    field: "source",
    kind: "invalid",
    message:
      "Paste one completed game URL, or the game's full PGN including its result.",
  })
})

test("refuses a URL from an unsupported host", () => {
  expect(
    parseImportGameRequest({
      elo: "",
      reviewSide: "white",
      source: "https://chess24.com/game/1",
    }),
  ).toEqual({
    field: "source",
    kind: "invalid",
    message:
      "Only Chess.com and Lichess game URLs can be imported. Paste the game's PGN instead.",
  })
})

test("refuses an Elo outside the coaching range", () => {
  expect(
    parseImportGameRequest({
      elo: "42",
      reviewSide: "white",
      source: "https://lichess.org/Synthet1",
    }),
  ).toEqual({
    field: "elo",
    kind: "invalid",
    message: "Elo must be a whole number between 100 and 3500.",
  })
})

test("provider-invalid and unsupported-host refusals attach to the source field", () => {
  expect(
    parseImportGameRequest({
      elo: "",
      reviewSide: "white",
      source: "https://www.chess.com/member/somebody",
    }),
  ).toMatchObject({ field: "source", kind: "invalid" })
  expect(
    parseImportGameRequest({
      elo: "",
      reviewSide: "white",
      source: "https://lichess.org/@/somebody",
    }),
  ).toMatchObject({ field: "source", kind: "invalid" })
  expect(
    parseImportGameRequest({
      elo: "",
      reviewSide: "white",
      source: "https://chess24.com/game/1",
    }),
  ).toMatchObject({ field: "source", kind: "invalid" })
})

test("refuses an empty source", () => {
  expect(
    parseImportGameRequest({ elo: "", reviewSide: "white", source: "  " }),
  ).toEqual({
    field: "source",
    kind: "invalid",
    message: "Paste a Chess.com or Lichess game URL, or a full PGN.",
  })
})
