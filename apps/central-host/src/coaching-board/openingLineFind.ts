import {
  findOpeningLines as findOpeningLinesFromEngine,
  type OpeningLineFindTruncation,
} from "@chenchess/coach-engine-sdk"

import {
  type OpeningCatalogRow,
  type PlayedOpening,
} from "./openingLineCatalog"
import { openingLineRefFromPath, type OpeningLineRef } from "./openingLineRef"

export const OPENING_LINE_FIND_LIMIT = 10

export type OpeningFindMatch = {
  eco: string
  name: string
  path: string
  played: boolean
  ref: OpeningLineRef
}

export type OpeningLineFindResult = {
  matches: OpeningFindMatch[]
  truncation: OpeningLineFindTruncation
}

export type OpeningLineLookup = (
  query: string,
  played: readonly PlayedOpening[],
) => Promise<OpeningLineFindResult>

export async function findOpeningLines(
  query: string,
  played: readonly PlayedOpening[] = [],
  lookup: OpeningLineLookup = readOpeningLinesFromCatalog,
): Promise<OpeningLineFindResult> {
  return lookup(query, played)
}

/**
 * The find request body is capped server-side (4096 bytes), and a Player's
 * imported-game history is unbounded, so the played hint is deduplicated and
 * bounded before it rides along.
 */
export const OPENING_LINE_PLAYED_HINT_LIMIT = 50

export function boundedPlayedHint(
  played: readonly PlayedOpening[],
): PlayedOpening[] {
  const seen = new Set<string>()
  const bounded: PlayedOpening[] = []
  for (const opening of played) {
    const key = `${opening.eco.toUpperCase()}:${opening.name.toLowerCase()}`
    if (seen.has(key)) continue
    seen.add(key)
    bounded.push(opening)
    if (bounded.length >= OPENING_LINE_PLAYED_HINT_LIMIT) break
  }
  return bounded
}

export async function readOpeningLinesFromCatalog(
  query: string,
  played: readonly PlayedOpening[] = [],
): Promise<OpeningLineFindResult> {
  const found = await findOpeningLinesFromEngine({
    played: boundedPlayedHint(played),
    query,
  })
  return {
    matches: found.matches.map((match) => ({
      eco: match.eco,
      name: match.name,
      path: match.path,
      played: match.played,
      ref: openingLineRefFromPath(match.eco, match.name, match.path),
    })),
    truncation: found.truncation,
  }
}

export function openingLineLookupFromRows(
  rows: readonly OpeningCatalogRow[],
): OpeningLineLookup {
  return async (query, played) => selectOpeningLineMatches(rows, query, played)
}

export function selectOpeningLineMatches(
  rows: readonly OpeningCatalogRow[],
  query: string,
  played: readonly PlayedOpening[] = [],
): OpeningLineFindResult {
  const needle = query.trim()
  if (!needle) {
    return {
      matches: [],
      truncation: { kind: "complete", totalMatchCount: 0 },
    }
  }
  const playedKeys = new Set(
    played.map((opening) => playedKey(opening.eco, opening.name)),
  )
  const matches = rows
    .filter((row) => rowMatches(row, needle))
    .map((row) => ({
      eco: row.eco,
      name: row.name,
      path: row.path,
      played: playedKeys.has(playedKey(row.eco, row.name)),
      ref: row.ref,
    }))
    .sort((left, right) => {
      if (left.played !== right.played) return left.played ? -1 : 1
      if (left.name.length !== right.name.length) {
        return left.name.length - right.name.length
      }
      const eco = left.eco.localeCompare(right.eco)
      if (eco !== 0) return eco
      return left.path.localeCompare(right.path)
    })
  const totalMatchCount = matches.length
  return {
    matches: matches.slice(0, OPENING_LINE_FIND_LIMIT),
    truncation:
      totalMatchCount > OPENING_LINE_FIND_LIMIT
        ? { kind: "truncated", totalMatchCount }
        : { kind: "complete", totalMatchCount },
  }
}

function rowMatches(row: OpeningCatalogRow, needle: string) {
  if (isEcoPrefixQuery(needle)) {
    return row.eco.toUpperCase().startsWith(needle.toUpperCase())
  }
  return row.name.toLowerCase().includes(needle.toLowerCase())
}

function isEcoPrefixQuery(query: string) {
  return /^[A-Ea-e][0-9]{0,2}$/.test(query)
}

function playedKey(eco: string, name: string) {
  return `${eco.toUpperCase()}:${name.toLowerCase()}`
}
