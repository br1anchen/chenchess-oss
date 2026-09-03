import { describe, expect, test } from "vitest"

import { parseChessComInput } from "./chess-com"

describe("Chess.com input parsing", () => {
  test("accepts shared computer, Daily, and live PvP Game URL forms", () => {
    expect(
      parseChessComInput("https://www.chess.com/game/computer/1403674481"),
    ).toEqual({
      kind: "ready",
      url: "https://www.chess.com/game/computer/1403674481",
    })
    expect(
      parseChessComInput("https://www.chess.com/game/daily/100000000002"),
    ).toEqual({
      kind: "ready",
      url: "https://www.chess.com/game/daily/100000000002",
    })
    expect(
      parseChessComInput("https://www.chess.com/game/live/100000000001"),
    ).toEqual({
      kind: "ready",
      url: "https://www.chess.com/game/live/100000000001",
    })
  })

  test.each([
    "http://www.chess.com/game/computer/1403674481",
    "https://chess.com/game/computer/1403674481",
    "https://www.chess.com/game/correspondence/100000000002",
    "https://www.chess.com/game/computer/1403674481/",
    "https://www.chess.com/game/daily/100000000002/",
    "https://www.chess.com/game/daily/100000000002?move=1",
    "https://www.chess.com/game/computer/1403674481?move=1",
    "https://www.chess.com/game/live/100000000001/",
    "https://www.chess.com/game/live/100000000001?move=1",
  ])("rejects unsupported URL %s", (url) => {
    expect(parseChessComInput(url).kind).toBe("invalid")
  })
})
