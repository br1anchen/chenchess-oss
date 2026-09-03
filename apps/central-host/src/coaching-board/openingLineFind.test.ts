import { expect, test, vi } from "vitest"

import { openingLineCatalog } from "./openingLineCatalog"
import {
  boundedPlayedHint,
  findOpeningLines,
  openingLineLookupFromRows,
  selectOpeningLineMatches,
  OPENING_LINE_PLAYED_HINT_LIMIT,
} from "./openingLineFind"
import { openingLineRefFromPath, parseOpeningLineRef } from "./openingLineRef"

const frenchA = "1. e4 e6"
const frenchB = "1. e4 e6 2. d4 d5"

test("ECO or name alone never addresses an Opening Line", () => {
  expect(parseOpeningLineRef("C00")).toBeUndefined()
  expect(parseOpeningLineRef("French Defense")).toBeUndefined()
  expect(parseOpeningLineRef("french-defense")).toBeUndefined()
  expect(parseOpeningLineRef("C00-french-defense")).toBeUndefined()
})

test("the address constructor pins byte-for-byte to the engine's twin", () => {
  // Mirrored by the engine's opening_line_reference parity pins
  // (opening_line_reference_matches_the_central_host_constructor); a change
  // on either side must move both pins.
  expect(
    openingLineRefFromPath(
      "B90",
      "Sicilian Defense: Najdorf Variation",
      "1. e4 c5 2. Nf3 d6 3. d4 cxd4 4. Nxd4 Nf6 5. Nc3 a6",
    ),
  ).toBe("B90-sicilian-defense-najdorf-variation-a203")
  expect(openingLineRefFromPath("C00", "French Defense", "1. e4 e6")).toBe(
    "C00-french-defense-1564",
  )
  expect(openingLineRefFromPath("A00", "Amar Opening", "1. Nh3")).toBe(
    "A00-amar-opening-b2ca",
  )
})

test("two catalog rows sharing ECO and name receive distinct addresses", () => {
  const first = openingLineRefFromPath("C00", "French Defense", frenchA)
  const second = openingLineRefFromPath("C00", "French Defense", frenchB)
  expect(first).not.toBe(second)
  expect(parseOpeningLineRef(first)).toBe(first)
  expect(parseOpeningLineRef(second)).toBe(second)
})

test("lookup treats an ECO-shaped query as a prefix and anything else as a name substring", () => {
  const byEco = selectOpeningLineMatches(openingLineCatalog, "C41")
  expect(byEco.matches.map((match) => match.name)).toEqual(["Philidor Defense"])

  const byName = selectOpeningLineMatches(openingLineCatalog, "Najdorf")
  expect(byName.matches.map((match) => match.name)).toEqual([
    "Sicilian Defense: Najdorf Variation",
  ])

  const both = selectOpeningLineMatches(openingLineCatalog, "C41 Philidor")
  expect(both.matches).toEqual([])
  expect(both.truncation).toEqual({ kind: "complete", totalMatchCount: 0 })
})

test("at most ten results use the existing truncation marker shape", () => {
  const sample = openingLineCatalog[0]
  if (!sample) throw new Error("v1 catalog has at least one Opening Line")
  const rows = Array.from({ length: 12 }, (_, index) => ({
    eco: "C00",
    ideas: sample.ideas,
    name: `French Defense fixture ${index.toString().padStart(2, "0")}`,
    path: `1. e4 e6 ${index}`,
    ref: openingLineRefFromPath(
      "C00",
      `French Defense fixture ${index.toString().padStart(2, "0")}`,
      `1. e4 e6 ${index}`,
    ),
  }))
  const found = selectOpeningLineMatches(rows, "French")
  expect(found.matches).toHaveLength(10)
  expect(found.truncation).toEqual({
    kind: "truncated",
    totalMatchCount: 12,
  })
})

test("ties break toward shorter names", () => {
  const found = selectOpeningLineMatches(openingLineCatalog, "Defense")
  expect(found.matches.map((match) => match.name)).toEqual([
    "French Defense",
    "Philidor Defense",
    "Sicilian Defense: Najdorf Variation",
  ])
})

test("played matches rank first, then shorter names, matching the engine order", () => {
  const found = selectOpeningLineMatches(openingLineCatalog, "Defense", [
    { eco: "C41", name: "Philidor Defense" },
  ])
  expect(found.matches.map((match) => [match.name, match.played])).toEqual([
    ["Philidor Defense", true],
    ["French Defense", false],
    ["Sicilian Defense: Najdorf Variation", false],
  ])
})

test("a played opening never surfaces for a query it does not match", () => {
  const found = selectOpeningLineMatches(openingLineCatalog, "Italian", [
    { eco: "C41", name: "Philidor Defense" },
  ])
  expect(found.matches.map((match) => match.name)).toEqual(["Italian Game"])
})

test("the played hint rides bounded and deduplicated", () => {
  const played = Array.from({ length: 120 }, (_, index) => ({
    eco: "C00",
    name: `Opening ${index % 60}`,
  }))
  const bounded = boundedPlayedHint(played)
  expect(bounded).toHaveLength(OPENING_LINE_PLAYED_HINT_LIMIT)
  expect(new Set(bounded.map((entry) => entry.name)).size).toBe(
    OPENING_LINE_PLAYED_HINT_LIMIT,
  )
})

test("the Choose an opening control and the agent tool use the same lookup", async () => {
  const lookup = vi.fn(openingLineLookupFromRows(openingLineCatalog))
  const fromControl = await findOpeningLines("Najdorf", [], lookup)
  const fromTool = await findOpeningLines("Najdorf", [], lookup)
  expect(lookup).toHaveBeenCalledTimes(2)
  expect(fromControl).toEqual(fromTool)
  expect(fromControl.matches[0]?.ref).toBe(
    openingLineRefFromPath(
      "B90",
      "Sicilian Defense: Najdorf Variation",
      "1. e4 c5 2. Nf3 d6 3. d4 cxd4 4. Nxd4 Nf6 5. Nc3 a6",
    ),
  )
})
