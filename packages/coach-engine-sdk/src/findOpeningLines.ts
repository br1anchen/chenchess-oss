import { validateContractDefinition } from "./contract-runtime.js"
import schemaDocument from "./review-session.schema.json"
import type { FindOpeningLinesRequest } from "./FindOpeningLinesRequest.js"
import type { OpeningLineFindResult } from "./OpeningLineFindResult.js"

export type { FindOpeningLinesRequest } from "./FindOpeningLinesRequest.js"
export type { OpeningLineFindMatch } from "./OpeningLineFindMatch.js"
export type { OpeningLineFindResult } from "./OpeningLineFindResult.js"
export type { OpeningLineFindTruncation } from "./OpeningLineFindTruncation.js"
export type { PlayedOpening } from "./PlayedOpening.js"

export function parseOpeningLineFindResult(
  value: unknown,
): OpeningLineFindResult {
  validateContractDefinition(schemaDocument, "OpeningLineFindResult", value)
  return value as OpeningLineFindResult
}

export class CoachEngineOpeningLinesHttpError extends Error {
  constructor(readonly status: number) {
    super(`Coach Engine Opening Line find failed with HTTP ${status}`)
    this.name = "CoachEngineOpeningLinesHttpError"
  }
}

export async function findOpeningLines(
  request: FindOpeningLinesRequest,
  options: {
    baseUrl?: string
    fetch?: typeof globalThis.fetch
  } = {},
): Promise<OpeningLineFindResult> {
  const fetchImplementation = options.fetch ?? globalThis.fetch.bind(globalThis)
  const baseUrl = (options.baseUrl ?? "").replace(/\/$/, "")
  const response = await fetchImplementation(
    `${baseUrl}/api/v1/opening-lines/find`,
    {
      body: JSON.stringify(request),
      headers: { "Content-Type": "application/json" },
      method: "POST",
    },
  )
  let value: unknown
  try {
    value = await response.json()
  } catch {
    throw new CoachEngineOpeningLinesHttpError(response.status)
  }
  if (!response.ok) {
    throw new CoachEngineOpeningLinesHttpError(response.status)
  }
  return parseOpeningLineFindResult(value)
}
