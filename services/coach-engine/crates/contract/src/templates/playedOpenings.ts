import { validateContractDefinition } from "./contract-runtime.js"
import schemaDocument from "./review-session.schema.json"
import type { PlayedOpeningsResult } from "./PlayedOpeningsResult.js"

export type { PlayedOpeningAggregate } from "./PlayedOpeningAggregate.js"
export type { PlayedOpeningsResult } from "./PlayedOpeningsResult.js"

export function parsePlayedOpeningsResult(
  value: unknown,
): PlayedOpeningsResult {
  validateContractDefinition(schemaDocument, "PlayedOpeningsResult", value)
  return value as PlayedOpeningsResult
}
