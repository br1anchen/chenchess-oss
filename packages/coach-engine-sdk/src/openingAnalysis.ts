import { validateContractDefinition } from "./contract-runtime.js"
import schemaDocument from "./review-session.schema.json"
import type { OpeningAnalysisOutcome } from "./OpeningAnalysisOutcome.js"
import type { ResolveOpeningLineOutcome } from "./ResolveOpeningLineOutcome.js"

export type { OpeningAnalysisOutcome } from "./OpeningAnalysisOutcome.js"
export type { OpeningAnalysisRequest } from "./OpeningAnalysisRequest.js"
export type { OpeningAnalyzedPly } from "./OpeningAnalyzedPly.js"
export type { OpeningAnalyzedRoot } from "./OpeningAnalyzedRoot.js"
export type { OpeningContinuationVerdict } from "./OpeningContinuationVerdict.js"
export type { OpeningLineIdentity } from "./OpeningLineIdentity.js"
export type { ResolveOpeningLineOutcome } from "./ResolveOpeningLineOutcome.js"

export function parseOpeningAnalysisOutcome(
  value: unknown,
): OpeningAnalysisOutcome {
  validateContractDefinition(schemaDocument, "OpeningAnalysisOutcome", value)
  return value as OpeningAnalysisOutcome
}

export function parseResolveOpeningLineOutcome(
  value: unknown,
): ResolveOpeningLineOutcome {
  validateContractDefinition(schemaDocument, "ResolveOpeningLineOutcome", value)
  return value as ResolveOpeningLineOutcome
}

export class CoachEngineOpeningAnalysisHttpError extends Error {
  constructor(readonly status: number) {
    super(`Coach Engine opening analysis failed with HTTP ${status}`)
    this.name = "CoachEngineOpeningAnalysisHttpError"
  }
}

export async function resolveOpeningLine(
  openingLineRef: string,
  options: {
    baseUrl?: string
    fetch?: typeof globalThis.fetch
  } = {},
): Promise<ResolveOpeningLineOutcome> {
  const fetchImplementation = options.fetch ?? globalThis.fetch.bind(globalThis)
  const baseUrl = (options.baseUrl ?? "").replace(/\/$/, "")
  const response = await fetchImplementation(
    `${baseUrl}/api/v1/opening-lines/resolve`,
    {
      body: JSON.stringify({ openingLineRef }),
      headers: { "Content-Type": "application/json" },
      method: "POST",
    },
  )
  let value: unknown
  try {
    value = await response.json()
  } catch {
    throw new CoachEngineOpeningAnalysisHttpError(response.status)
  }
  if (!response.ok) {
    throw new CoachEngineOpeningAnalysisHttpError(response.status)
  }
  return parseResolveOpeningLineOutcome(value)
}
