import type { CanonicalGameMove } from "@chenchess/coach-engine-sdk"

import { moveLabel } from "@/review-session/model"

import {
  openingLineCatalog,
  type OpeningCatalogRow,
} from "./openingLineCatalog"
import type { OpeningLineRef } from "./openingLineRef"
import { openingLineMoves } from "./openingMoves"

export type OpeningNextMove = {
  label: string
  onCurrentLine: boolean
  openingLineRef: OpeningLineRef
  ply: number
  san: string
}

/**
 * Catalog next moves from the Position at `viewedPly`.
 *
 * `viewedPly` is the ply about to be played, matching `browseBoardAtPly`.
 * Rows that share the already-played prefix contribute their move at that
 * ply. The current line's continuation ranks first when several lines share
 * a SAN.
 */
export function openingNextMoves(
  currentRef: OpeningLineRef,
  currentMoves: readonly CanonicalGameMove[],
  viewedPly: number,
  catalog: readonly OpeningCatalogRow[] = openingLineCatalog,
): OpeningNextMove[] {
  const prefix = currentMoves
    .filter((move) => move.ply < viewedPly)
    .map((move) => move.san)
  const bySan = new Map<string, OpeningNextMove>()
  for (const row of catalog) {
    const moves = openingLineMoves(row.path)
    if (!prefixMatches(moves, prefix)) continue
    const next = moves.find((move) => move.ply === viewedPly)
    if (!next) continue
    const candidate: OpeningNextMove = {
      label: moveLabel(next),
      onCurrentLine: row.ref === currentRef,
      openingLineRef: row.ref,
      ply: next.ply,
      san: next.san,
    }
    const existing = bySan.get(next.san)
    if (!existing || candidate.onCurrentLine) bySan.set(next.san, candidate)
  }
  return [...bySan.values()].sort((left, right) => {
    if (left.onCurrentLine !== right.onCurrentLine) {
      return left.onCurrentLine ? -1 : 1
    }
    return left.label.localeCompare(right.label)
  })
}

function prefixMatches(
  moves: readonly CanonicalGameMove[],
  prefix: readonly string[],
) {
  return prefix.every((san, index) => moves[index]?.san === san)
}
