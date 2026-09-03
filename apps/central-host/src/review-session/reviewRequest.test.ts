import { describe, expect, test } from "vitest"

import { extractCompletedPgn } from "./reviewRequest"

describe("the completed game inside a message", () => {
  test("takes a fenced block and cuts it at the result", () => {
    expect(
      extractCompletedPgn(
        "Here is the game:\n```pgn\n1. e4 e5 2. Nf3 Nc6 1-0\n```\nWhat went wrong?",
      ),
    ).toBe("1. e4 e5 2. Nf3 Nc6 1-0")
  })

  test("keeps a Lichess Site header as part of the game", () => {
    const pgn = '[Site "https://lichess.org/Synthet1Demo"]\n\n1. d4 d5 0-1'
    expect(extractCompletedPgn(`Review this:\n${pgn}`)).toBe(pgn)
  })

  test("extracts untagged movetext without the conversation after it", () => {
    expect(
      extractCompletedPgn(
        "Paste:\n1. e4 c5 2. Nf3 d6 1/2-1/2\nWas the Sicilian a mistake?",
      ),
    ).toBe("1. e4 c5 2. Nf3 d6 1/2-1/2")
  })

  test("cuts at the last result when several appear", () => {
    expect(extractCompletedPgn("1. e4 e5 1-0 then 2. d4 d5 0-1")).toBe(
      "1. e4 e5 1-0 then 2. d4 d5 0-1",
    )
  })

  test("a game with no result is not a completed Game", () => {
    expect(extractCompletedPgn("1. e4 e5 2. Nf3 Nc6")).toBeNull()
  })

  test("a message with no game at all is nothing", () => {
    expect(extractCompletedPgn("Can you review my last game?")).toBeNull()
  })
})
