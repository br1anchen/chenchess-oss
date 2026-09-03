import { expect, test } from "vitest"

import {
  applyStagedGameImport,
  emptyGameImportFields,
  gameImportFieldsEdited,
} from "./stagedGameImport"

test("an agent stage fills empty lobby fields", () => {
  const staged = {
    elo: "1246",
    reviewSide: "black" as const,
    source: "https://lichess.org/Synthet1",
  }
  expect(applyStagedGameImport(emptyGameImportFields, staged, false)).toEqual({
    fields: staged,
    kind: "applied",
  })
})

test("a stage never clobbers fields the Player is editing", () => {
  const current = {
    elo: "",
    reviewSide: "white" as const,
    source: "https://lichess.org/player-typed",
  }
  const staged = {
    elo: "1800",
    reviewSide: "black" as const,
    source: "https://lichess.org/agent-staged",
  }
  expect(applyStagedGameImport(current, staged, true)).toEqual({
    fields: current,
    kind: "kept",
  })
  expect(gameImportFieldsEdited(current, emptyGameImportFields)).toBe(true)
})
