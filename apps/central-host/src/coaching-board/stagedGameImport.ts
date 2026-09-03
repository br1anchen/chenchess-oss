import type { ReviewSide } from "@chenchess/coach-engine-sdk"

export type GameImportFields = {
  elo: string
  reviewSide: ReviewSide
  source: string
}

export const emptyGameImportFields: GameImportFields = {
  elo: "",
  reviewSide: "white",
  source: "",
}

/**
 * Apply an agent-staged Game import without clobbering what the Player typed.
 *
 * The lobby treats the agent's result as a proposal. If the Player has edited
 * any field since the last committed player edit, the stage is refused and
 * the current fields stay.
 */
export type StagedGameImportResult = {
  fields: GameImportFields
  kind: "applied" | "kept"
}

export function applyStagedGameImport(
  current: GameImportFields,
  staged: GameImportFields,
  playerEdited: boolean,
): StagedGameImportResult {
  if (playerEdited) return { fields: current, kind: "kept" }
  return { fields: staged, kind: "applied" }
}

export function gameImportFieldsEdited(
  current: GameImportFields,
  baseline: GameImportFields,
) {
  return (
    current.source !== baseline.source ||
    current.reviewSide !== baseline.reviewSide ||
    current.elo !== baseline.elo
  )
}
