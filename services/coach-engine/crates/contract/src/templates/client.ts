import {
  decodeDailyCoachingDashboardState,
  decodeDailyCoachingDigestDetail,
  decodeImportedGameListPage,
  decodeReviewedGameSearchResult,
  decodeReviewSessionCommandEnvelope,
  decodeReviewSessionEventEnvelope,
} from "./decoder.js"
import type { ReviewSessionCommandEnvelope } from "./ReviewSessionCommandEnvelope.js"
import type { ReviewSessionEvent } from "./ReviewSessionEvent.js"
import type { ReviewSessionEventEnvelope } from "./ReviewSessionEventEnvelope.js"
import type { ConnectPlayingProfileOutcome } from "./ConnectPlayingProfileOutcome.js"
import type { ConnectPlayingProfileRequest } from "./ConnectPlayingProfileRequest.js"
import type { CheckPlayingProfileOutcome } from "./CheckPlayingProfileOutcome.js"
import type { CheckPlayingProfileRequest } from "./CheckPlayingProfileRequest.js"
import type { DailyCoachingDashboardState } from "./DailyCoachingDashboardState.js"
import type { DailyCoachingDigestDetail } from "./DailyCoachingDigestDetail.js"
import type { DailyCoachingMutationOutcome } from "./DailyCoachingMutationOutcome.js"
import type { DailyCoachingProvider } from "./DailyCoachingProvider.js"
import type { DailyCoachingSetupState } from "./DailyCoachingSetupState.js"
import type { ImportedGameListPage } from "./ImportedGameListPage.js"
import type { PlayingProfileConnection } from "./PlayingProfileConnection.js"
import type { ReviewedGameSearchRequest } from "./ReviewedGameSearchRequest.js"
import type { ReviewedGameSearchResult } from "./ReviewedGameSearchResult.js"
import type { RemovePlayingProfileRequest } from "./RemovePlayingProfileRequest.js"
import type { ReplacePlayingProfileRequest } from "./ReplacePlayingProfileRequest.js"
import type { RetryDirective } from "./RetryDirective.js"
import {
  findOpeningLines,
  type FindOpeningLinesRequest,
  type OpeningLineFindResult,
} from "./findOpeningLines.js"
import {
  CoachEngineOpeningAnalysisHttpError,
  parseOpeningAnalysisOutcome,
  resolveOpeningLine,
  type OpeningAnalysisOutcome,
  type OpeningAnalysisRequest,
  type ResolveOpeningLineOutcome,
} from "./openingAnalysis.js"
import {
  parsePlayedOpeningsResult,
  type PlayedOpeningsResult,
} from "./playedOpenings.js"
import {
  parseRecentPlayingProfileGamesOutcome,
  type RecentPlayingProfileGamesOutcome,
} from "./recentPlayingProfileGames.js"

export type CoachCredentialProvider = () => Promise<string>

export type ArtifactRetentionPreference = {
  available: boolean
  deletedReviewSnapshots: number
  disclosureRequired: boolean
  enabled: boolean
}

export type ArtifactRetentionPreferenceOutcome =
  | {
      kind: "artifactRetentionPreferenceRead"
      preference: ArtifactRetentionPreference
    }
  | {
      kind: "artifactRetentionPreferenceUpdated"
      preference: ArtifactRetentionPreference
    }

export type ReviewFeedbackReason =
  | "explanationHelpful"
  | "explanationIncorrect"
  | "explanationNotHelpful"
  | "explanationUnclear"
  | "shouldSelect"

export type CoachEngineClientOptions = {
  baseUrl?: string
  credential: CoachCredentialProvider
  fetch?: typeof globalThis.fetch
}

export class CoachEngineDailyCoachingHttpError extends Error {
  constructor(
    readonly status: number,
    surface: string,
  ) {
    super(`Coach Engine Daily Coaching ${surface} failed with HTTP ${status}`)
    this.name = "CoachEngineDailyCoachingHttpError"
  }
}

export class CoachEngineClient {
  readonly #commandEndpoint: string
  readonly #credential: CoachCredentialProvider
  readonly #dailyCoachingEndpoint: string
  readonly #fetch: typeof globalThis.fetch
  readonly #feedbackEndpoint: string
  readonly #importedGamesEndpoint: string
  readonly #baseUrl: string
  readonly #retentionEndpoint: string
  readonly #reviewedGamesEndpoint: string

  constructor({
    baseUrl = "",
    credential,
    fetch: fetchImplementation = globalThis.fetch.bind(globalThis),
  }: CoachEngineClientOptions) {
    const normalizedBaseUrl = baseUrl.replace(/\/$/, "")
    this.#baseUrl = normalizedBaseUrl
    this.#commandEndpoint = `${normalizedBaseUrl}/api/v1/review-session/commands`
    this.#dailyCoachingEndpoint = `${normalizedBaseUrl}/api/v1/daily-coaching`
    this.#credential = credential
    this.#fetch = fetchImplementation
    this.#feedbackEndpoint = `${normalizedBaseUrl}/api/v1/review-artifacts/feedback`
    this.#importedGamesEndpoint = `${normalizedBaseUrl}/api/v1/imported-games`
    this.#retentionEndpoint = `${normalizedBaseUrl}/api/v1/review-artifacts/preference`
    this.#reviewedGamesEndpoint = `${normalizedBaseUrl}/api/v1/reviewed-games/search`
  }

  async artifactRetentionPreference(): Promise<ArtifactRetentionPreference> {
    return this.#requestRetentionPreference({ method: "GET" })
  }

  async recordReviewFeedback(
    reasonCodes: readonly ReviewFeedbackReason[],
  ): Promise<void> {
    const response = await this.#fetch(this.#feedbackEndpoint, {
      body: JSON.stringify({ reasonCodes }),
      headers: {
        Authorization: `Bearer ${await this.#currentCredential()}`,
        "Content-Type": "application/json",
      },
      method: "POST",
    })
    if (!response.ok) {
      throw new Error(
        `Coach Engine review feedback failed with HTTP ${response.status}`,
      )
    }
  }

  async dailyCoachingState(): Promise<DailyCoachingSetupState> {
    const response = await this.#fetch(this.#dailyCoachingEndpoint, {
      headers: {
        Authorization: `Bearer ${await this.#currentCredential()}`,
      },
      method: "GET",
    })
    if (!response.ok) {
      throw new Error(
        `Coach Engine Daily Coaching state failed with HTTP ${response.status}`,
      )
    }
    const value: unknown = await response.json()
    return decodeDailyCoachingSetupState(value)
  }

  async dailyCoachingDashboard(): Promise<DailyCoachingDashboardState> {
    const value = await this.#dailyCoachingRead(
      `${this.#dailyCoachingEndpoint}/dashboard`,
      "dashboard",
    )
    return decodeDailyCoachingDashboardState(value)
  }

  async recentPlayingProfileGames(): Promise<RecentPlayingProfileGamesOutcome> {
    const response = await this.#fetch(
      `${this.#dailyCoachingEndpoint}/recent-profile-games`,
      {
        headers: {
          Authorization: `Bearer ${await this.#currentCredential()}`,
        },
        method: "GET",
      },
    )
    // The route answers 200 for found/noPlayingProfile and 503 with a typed
    // unavailable body; any other status has no outcome body to parse.
    if (response.status !== 200 && response.status !== 503) {
      throw new CoachEngineDailyCoachingHttpError(
        response.status,
        "recent profile Games",
      )
    }
    let value: unknown
    try {
      value = await response.json()
    } catch {
      throw new CoachEngineDailyCoachingHttpError(
        response.status,
        "recent profile Games",
      )
    }
    // Both statuses carry the same typed outcome; 503 is how the engine
    // reports `unavailable`, not a transport failure with a different body.
    return parseRecentPlayingProfileGamesOutcome(value)
  }

  async dailyCoachingDigest(
    digestId: string,
  ): Promise<DailyCoachingDigestDetail> {
    const value = await this.#dailyCoachingRead(
      `${this.#dailyCoachingEndpoint}/digests/${encodeURIComponent(digestId)}`,
      "digest",
    )
    return decodeDailyCoachingDigestDetail(value)
  }

  async importedGames(cursor?: string): Promise<ImportedGameListPage> {
    const endpoint = cursor
      ? `${this.#importedGamesEndpoint}?cursor=${encodeURIComponent(cursor)}`
      : this.#importedGamesEndpoint
    const response = await this.#fetch(endpoint, {
      headers: {
        Authorization: `Bearer ${await this.#currentCredential()}`,
      },
      method: "GET",
    })
    if (!response.ok) {
      throw new CoachEngineDailyCoachingHttpError(
        response.status,
        "Imported Games",
      )
    }
    return decodeImportedGameListPage(await response.json())
  }

  async findOpeningLines(
    request: FindOpeningLinesRequest,
  ): Promise<OpeningLineFindResult> {
    return findOpeningLines(request, {
      baseUrl: this.#baseUrl,
      fetch: this.#fetch,
    })
  }

  async playedOpenings(): Promise<PlayedOpeningsResult> {
    const response = await this.#fetch(
      `${this.#baseUrl}/api/v1/openings/played`,
      {
        headers: {
          Authorization: `Bearer ${await this.#currentCredential()}`,
        },
        method: "GET",
      },
    )
    if (!response.ok) {
      throw new CoachEngineDailyCoachingHttpError(
        response.status,
        "played openings",
      )
    }
    return parsePlayedOpeningsResult(await response.json())
  }

  async resolveOpeningLine(
    openingLineRef: string,
  ): Promise<ResolveOpeningLineOutcome> {
    return resolveOpeningLine(openingLineRef, {
      baseUrl: this.#baseUrl,
      fetch: this.#fetch,
    })
  }

  async analyzeOpeningLine(
    request: OpeningAnalysisRequest,
  ): Promise<OpeningAnalysisOutcome> {
    const response = await this.#fetch(
      `${this.#baseUrl}/api/v1/opening-lines/analysis`,
      {
        body: JSON.stringify(request),
        headers: {
          Authorization: `Bearer ${await this.#currentCredential()}`,
          "Content-Type": "application/json",
        },
        method: "POST",
      },
    )
    // The route answers 200 for typed outcomes and 503 with a typed
    // unavailable body; any other status has no outcome body to parse.
    if (response.status !== 200 && response.status !== 503) {
      throw new CoachEngineOpeningAnalysisHttpError(response.status)
    }
    let value: unknown
    try {
      value = await response.json()
    } catch {
      throw new CoachEngineOpeningAnalysisHttpError(response.status)
    }
    return parseOpeningAnalysisOutcome(value)
  }

  async searchReviewedGames(
    request: ReviewedGameSearchRequest,
  ): Promise<ReviewedGameSearchResult> {
    const response = await this.#fetch(this.#reviewedGamesEndpoint, {
      body: JSON.stringify(request),
      headers: {
        Authorization: `Bearer ${await this.#currentCredential()}`,
        "Content-Type": "application/json",
      },
      method: "POST",
    })
    if (!response.ok) {
      throw new CoachEngineDailyCoachingHttpError(
        response.status,
        "reviewed-Game search",
      )
    }
    return decodeReviewedGameSearchResult(await response.json())
  }

  async connectPlayingProfile(
    request: ConnectPlayingProfileRequest,
  ): Promise<ConnectPlayingProfileOutcome> {
    const response = await this.#dailyCoachingRequest({
      body: request,
      endpoint: `${this.#dailyCoachingEndpoint}/connections`,
      method: "POST",
    })
    return decodeConnectPlayingProfileOutcome(response)
  }

  async replacePlayingProfile(
    provider: DailyCoachingProvider,
    request: ReplacePlayingProfileRequest,
  ): Promise<DailyCoachingMutationOutcome> {
    const response = await this.#dailyCoachingRequest({
      body: request,
      endpoint: `${this.#dailyCoachingEndpoint}/connections/${providerPath(provider)}`,
      method: "PUT",
    })
    return decodeDailyCoachingMutationOutcome(response)
  }

  async checkPlayingProfile(
    provider: DailyCoachingProvider,
    request: CheckPlayingProfileRequest,
  ): Promise<CheckPlayingProfileOutcome> {
    const response = await this.#dailyCoachingRequest({
      body: request,
      endpoint: `${this.#dailyCoachingEndpoint}/connections/${providerPath(provider)}/check`,
      method: "POST",
    })
    return decodeCheckPlayingProfileOutcome(response)
  }

  async removePlayingProfile(
    provider: DailyCoachingProvider,
    request: RemovePlayingProfileRequest,
  ): Promise<DailyCoachingMutationOutcome> {
    const response = await this.#dailyCoachingRequest({
      body: request,
      endpoint: `${this.#dailyCoachingEndpoint}/connections/${providerPath(provider)}`,
      method: "DELETE",
    })
    return decodeDailyCoachingMutationOutcome(response)
  }

  async setDailyCoachingEnabled(
    enabled: boolean,
  ): Promise<DailyCoachingMutationOutcome> {
    const response = await this.#dailyCoachingRequest({
      body: { enabled },
      endpoint: `${this.#dailyCoachingEndpoint}/enabled`,
      method: "PUT",
    })
    return decodeDailyCoachingMutationOutcome(response)
  }

  async setDigestEmailEnabled(
    enabled: boolean,
  ): Promise<DailyCoachingMutationOutcome> {
    const response = await this.#dailyCoachingRequest({
      body: { enabled },
      endpoint: `${this.#dailyCoachingEndpoint}/email`,
      method: "PUT",
    })
    return decodeDailyCoachingMutationOutcome(response)
  }

  async setArtifactRetentionPreference(
    enabled: boolean,
  ): Promise<ArtifactRetentionPreference> {
    return this.#requestRetentionPreference({ enabled, method: "PUT" })
  }

  async stream(
    envelope: ReviewSessionCommandEnvelope,
    onEvent: (event: ReviewSessionEventEnvelope) => void | Promise<void>,
  ): Promise<void> {
    const command = await decodeReviewSessionCommandEnvelope(envelope)
    const response = await this.#fetch(this.#commandEndpoint, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${await this.#currentCredential()}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(command),
    })
    if (!response.ok) {
      throw new Error(
        `Coach Engine command failed with HTTP ${response.status}`,
      )
    }
    if (!response.body) {
      throw new Error("Coach Engine command returned no event stream")
    }

    let sequence = -1
    let terminalReceived = false
    await readNdjson(response.body, async (value) => {
      const event = await decodeReviewSessionEventEnvelope(value)
      if (
        event.requestId !== command.requestId ||
        event.operationId !== command.operationId
      ) {
        throw new Error(
          "Coach Engine event identity does not match its command",
        )
      }
      if (event.sequence !== sequence + 1) {
        throw new Error("Coach Engine events arrived out of sequence")
      }
      if (terminalReceived) {
        throw new Error("Coach Engine emitted data after its terminal outcome")
      }
      sequence = event.sequence
      terminalReceived = isTerminal(event.event)
      await onEvent(event)
    })
    if (sequence < 0) {
      throw new Error("Coach Engine returned an empty event stream")
    }
    if (!terminalReceived) {
      throw new Error("Coach Engine ended before its terminal outcome")
    }
  }

  async #currentCredential(): Promise<string> {
    const credential = await this.#credential()
    if (credential.trim().length === 0) {
      throw new Error("Coach Engine credential provider returned no credential")
    }
    return credential
  }

  async #requestRetentionPreference(
    request:
      | { method: "GET" }
      | {
          enabled: boolean
          method: "PUT"
        },
  ): Promise<ArtifactRetentionPreference> {
    const headers: Record<string, string> = {
      Authorization: `Bearer ${await this.#currentCredential()}`,
    }
    const init: RequestInit = { headers, method: request.method }
    if (request.method === "PUT") {
      headers["Content-Type"] = "application/json"
      init.body = JSON.stringify({ enabled: request.enabled })
    }
    const response = await this.#fetch(this.#retentionEndpoint, {
      ...init,
    })
    if (!response.ok) {
      throw new Error(
        `Coach Engine artifact retention preference failed with HTTP ${response.status}`,
      )
    }
    return decodeArtifactRetentionPreference(await response.json())
  }

  async #dailyCoachingRequest(request: {
    body: unknown
    endpoint: string
    method: "DELETE" | "POST" | "PUT"
  }): Promise<unknown> {
    const response = await this.#fetch(request.endpoint, {
      body: JSON.stringify(request.body),
      headers: {
        Authorization: `Bearer ${await this.#currentCredential()}`,
        "Content-Type": "application/json",
      },
      method: request.method,
    })
    try {
      return await response.json()
    } catch {
      throw new CoachEngineDailyCoachingHttpError(response.status, "mutation")
    }
  }

  async #dailyCoachingRead(
    endpoint: string,
    surface: string,
  ): Promise<unknown> {
    const response = await this.#fetch(endpoint, {
      headers: {
        Authorization: `Bearer ${await this.#currentCredential()}`,
      },
      method: "GET",
    })
    if (!response.ok) {
      throw new CoachEngineDailyCoachingHttpError(response.status, surface)
    }
    return response.json()
  }
}

export function decodeDailyCoachingSetupState(
  value: unknown,
): DailyCoachingSetupState {
  if (!isRecord(value) || typeof value.kind !== "string") {
    throw invalidDailyCoachingResponse()
  }
  if (value.kind === "notConnected" && Object.keys(value).length === 1) {
    return { kind: "notConnected" }
  }
  if (
    value.kind === "connected" &&
    typeof value.enabled === "boolean" &&
    typeof value.timezone === "string" &&
    value.timezone.trim().length > 0 &&
    Array.isArray(value.connections) &&
    value.connections.length > 0
  ) {
    return {
      connections: value.connections.map(decodePlayingProfileConnection),
      enabled: value.enabled,
      kind: "connected",
      timezone: value.timezone,
    }
  }
  throw invalidDailyCoachingResponse()
}

export function decodeConnectPlayingProfileOutcome(
  value: unknown,
): ConnectPlayingProfileOutcome {
  if (!isRecord(value) || typeof value.outcome !== "string") {
    throw invalidDailyCoachingResponse()
  }
  if (
    value.outcome === "completed" &&
    isDailyCoachingProvider(value.provider) &&
    typeof value.username === "string" &&
    typeof value.canonicalUrl === "string" &&
    value.status === "connected"
  ) {
    return {
      canonicalUrl: value.canonicalUrl,
      outcome: "completed",
      provider: value.provider,
      status: "connected",
      username: value.username,
    }
  }
  if (value.outcome === "rejected" && isConnectRejectionReason(value.reason)) {
    return { outcome: "rejected", reason: value.reason }
  }
  if (
    value.outcome === "unavailable" &&
    isDailyCoachingUnavailableReason(value.reason)
  ) {
    return {
      outcome: "unavailable",
      reason: value.reason,
      retry: decodeRetryDirective(value.retry),
    }
  }
  throw invalidDailyCoachingResponse()
}

export function decodeDailyCoachingMutationOutcome(
  value: unknown,
): DailyCoachingMutationOutcome {
  if (!isRecord(value) || typeof value.outcome !== "string") {
    throw invalidDailyCoachingResponse()
  }
  if (value.outcome === "completed") {
    return {
      outcome: "completed",
      state: decodeDailyCoachingSetupState(value.state),
    }
  }
  if (
    value.outcome === "rejected" &&
    isDailyCoachingMutationRejectionReason(value.reason)
  ) {
    return { outcome: "rejected", reason: value.reason }
  }
  if (
    value.outcome === "unavailable" &&
    isDailyCoachingUnavailableReason(value.reason)
  ) {
    return {
      outcome: "unavailable",
      reason: value.reason,
      retry: decodeRetryDirective(value.retry),
    }
  }
  throw invalidDailyCoachingResponse()
}

export function decodeCheckPlayingProfileOutcome(
  value: unknown,
): CheckPlayingProfileOutcome {
  if (!isRecord(value) || typeof value.outcome !== "string") {
    throw invalidDailyCoachingResponse()
  }
  if (
    (value.outcome === "reachable" || value.outcome === "profileUnavailable") &&
    isDailyCoachingProvider(value.provider)
  ) {
    return { outcome: value.outcome, provider: value.provider }
  }
  if (
    value.outcome === "providerUnavailable" &&
    isDailyCoachingProvider(value.provider)
  ) {
    return {
      outcome: "providerUnavailable",
      provider: value.provider,
      retry: decodeRetryDirective(value.retry),
    }
  }
  if (
    value.outcome === "rejected" &&
    isDailyCoachingMutationRejectionReason(value.reason)
  ) {
    return { outcome: "rejected", reason: value.reason }
  }
  if (
    value.outcome === "unavailable" &&
    isDailyCoachingUnavailableReason(value.reason)
  ) {
    return {
      outcome: "unavailable",
      reason: value.reason,
      retry: decodeRetryDirective(value.retry),
    }
  }
  throw invalidDailyCoachingResponse()
}

function decodePlayingProfileConnection(
  value: unknown,
): PlayingProfileConnection {
  if (
    !isRecord(value) ||
    !isDailyCoachingProvider(value.provider) ||
    typeof value.username !== "string" ||
    typeof value.canonicalUrl !== "string" ||
    !isPlayingProfileConnectionStatus(value.status)
  ) {
    throw invalidDailyCoachingResponse()
  }
  return {
    canonicalUrl: value.canonicalUrl,
    provider: value.provider,
    status: value.status,
    username: value.username,
  }
}

function isPlayingProfileConnectionStatus(
  value: unknown,
): value is PlayingProfileConnection["status"] {
  return value === "connected" || value === "profileUnavailable"
}

function decodeRetryDirective(value: unknown): RetryDirective {
  if (!isRecord(value) || typeof value.kind !== "string") {
    throw invalidDailyCoachingResponse()
  }
  if (
    value.kind === "retryAllowed" ||
    value.kind === "startNewOperation" ||
    value.kind === "notRetryable"
  ) {
    return { kind: value.kind }
  }
  if (
    value.kind === "retryAfter" &&
    isCount(value.seconds) &&
    value.seconds > 0
  ) {
    return { kind: "retryAfter", seconds: value.seconds }
  }
  throw invalidDailyCoachingResponse()
}

function providerPath(provider: DailyCoachingProvider): string {
  switch (provider) {
    case "lichess":
      return "lichess"
    case "chessCom":
      return "chessCom"
  }
}

function isDailyCoachingProvider(
  value: unknown,
): value is DailyCoachingProvider {
  return value === "lichess" || value === "chessCom"
}

function isConnectRejectionReason(
  value: unknown,
): value is
  | "profileNotFound"
  | "providerAlreadyConnected"
  | "unparseableProfileUrl"
  | "unsupportedProvider" {
  return (
    value === "profileNotFound" ||
    value === "providerAlreadyConnected" ||
    value === "unparseableProfileUrl" ||
    value === "unsupportedProvider"
  )
}

function isDailyCoachingMutationRejectionReason(
  value: unknown,
): value is
  | "digestEmailUnavailable"
  | "noVerifiedAccountEmail"
  | "noPlayingProfile"
  | "profileNotFound"
  | "providerMismatch"
  | "stalePlayingProfile"
  | "unparseableProfileUrl"
  | "unsupportedProvider" {
  return (
    value === "digestEmailUnavailable" ||
    value === "noVerifiedAccountEmail" ||
    value === "noPlayingProfile" ||
    value === "profileNotFound" ||
    value === "providerMismatch" ||
    value === "stalePlayingProfile" ||
    value === "unparseableProfileUrl" ||
    value === "unsupportedProvider"
  )
}

function isDailyCoachingUnavailableReason(
  value: unknown,
): value is "persistence" | "providerUnreachable" {
  return value === "persistence" || value === "providerUnreachable"
}

function invalidDailyCoachingResponse(): Error {
  return new Error("Coach Engine Daily Coaching response is invalid")
}

export {
  CoachEngineOpeningLinesHttpError,
  findOpeningLines,
  parseOpeningLineFindResult,
  type FindOpeningLinesRequest,
  type OpeningLineFindMatch,
  type OpeningLineFindResult,
  type OpeningLineFindTruncation,
  type PlayedOpening,
} from "./findOpeningLines.js"
export {
  CoachEngineOpeningAnalysisHttpError,
  parseOpeningAnalysisOutcome,
  parseResolveOpeningLineOutcome,
  resolveOpeningLine,
  type OpeningAnalysisOutcome,
  type OpeningAnalysisRequest,
  type OpeningAnalyzedPly,
  type OpeningLineIdentity,
  type ResolveOpeningLineOutcome,
} from "./openingAnalysis.js"
export {
  parsePlayedOpeningsResult,
  type PlayedOpeningAggregate,
  type PlayedOpeningsResult,
} from "./playedOpenings.js"
export {
  parseRecentPlayingProfileGamesOutcome,
  type RecentPlayingProfileGame,
  type RecentPlayingProfileGamesOutcome,
} from "./recentPlayingProfileGames.js"

export function decodeArtifactRetentionPreference(
  value: unknown,
): ArtifactRetentionPreference {
  if (
    !isRecord(value) ||
    typeof value.available !== "boolean" ||
    typeof value.enabled !== "boolean" ||
    typeof value.disclosureRequired !== "boolean" ||
    !isCount(value.deletedReviewSnapshots)
  ) {
    throw new Error(
      "Coach Engine artifact retention preference response is invalid",
    )
  }
  return {
    available: value.available,
    deletedReviewSnapshots: value.deletedReviewSnapshots,
    disclosureRequired: value.disclosureRequired,
    enabled: value.enabled,
  }
}

function isCount(value: unknown): value is number {
  return Number.isSafeInteger(value) && Number(value) >= 0
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null
}

function isTerminal(event: ReviewSessionEvent): boolean {
  return event.kind !== "accepted" && event.kind !== "progress"
}

async function readNdjson(
  stream: ReadableStream<Uint8Array>,
  consume: (value: unknown) => void | Promise<void>,
): Promise<void> {
  const reader = stream.getReader()
  const decoder = new TextDecoder()
  let buffered = ""

  try {
    for (;;) {
      const { done, value } = await reader.read()
      buffered += decoder.decode(value, { stream: !done })
      const lines = buffered.split("\n")
      buffered = lines.pop() ?? ""
      for (const line of lines) {
        if (line.trim().length > 0) await consume(JSON.parse(line) as unknown)
      }
      if (done) break
    }
    if (buffered.trim().length > 0) {
      await consume(JSON.parse(buffered) as unknown)
    }
  } finally {
    reader.releaseLock()
  }
}
