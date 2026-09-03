import { describe, expect, test } from "vitest"

import { parseLichessInput } from "./lichess"

describe("Lichess input parsing", () => {
  test("distinguishes bare, side-qualified, and invalid Game URLs", () => {
    expect(parseLichessInput("https://lichess.org/Synthet1")).toEqual({
      kind: "bare",
      url: "https://lichess.org/Synthet1",
    })
    expect(parseLichessInput("https://lichess.org/Synthet1/black")).toEqual({
      kind: "qualified",
      side: "black",
      url: "https://lichess.org/Synthet1/black",
    })
    expect(parseLichessInput("https://example.com/Synthet1").kind).toBe(
      "invalid",
    )
  })
})
