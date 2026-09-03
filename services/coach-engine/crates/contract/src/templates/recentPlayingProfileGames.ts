import { validateContractDefinition } from "./contract-runtime.js"
import schemaDocument from "./review-session.schema.json"
import type { RecentPlayingProfileGamesOutcome } from "./RecentPlayingProfileGamesOutcome.js"

export type { RecentPlayingProfileGame } from "./RecentPlayingProfileGame.js"
export type { RecentPlayingProfileGamesOutcome } from "./RecentPlayingProfileGamesOutcome.js"

export function parseRecentPlayingProfileGamesOutcome(
  value: unknown,
): RecentPlayingProfileGamesOutcome {
  validateContractDefinition(
    schemaDocument,
    "RecentPlayingProfileGamesOutcome",
    value,
  )
  return value as RecentPlayingProfileGamesOutcome
}
