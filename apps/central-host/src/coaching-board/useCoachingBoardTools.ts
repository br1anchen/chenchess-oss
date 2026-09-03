import { useEffect, useRef } from "react"
import { toJsonSchema } from "@valibot/to-json-schema"
import * as v from "valibot"
import type {
  GameImportId,
  HostTurnShowLine,
  ReviewedGameSearchRequest,
} from "@chenchess/coach-engine-sdk"

import {
  coachWebBoardToolNames,
  coachWebLobbyToolNames,
  coachWebToolNames,
} from "../../server/board/tool-surface"

import {
  annotateBoardDescription,
  evaluateOpeningContinuationDescription,
  evaluatePlayerLineWebDescription,
  findOpeningLineDescription,
  searchReviewedGamesWebDescription,
  listCriticalMomentsWebDescription,
  listRecentProfileGamesDescription,
  listPlayedOpeningsDescription,
  openOpeningLineDescription,
  openReviewedGameDescription,
  openReviewMomentInPlaceWebDescription,
  readCoachingBoardDescription,
  setBoardPositionDescription,
  showLineDescription,
  stepLineDescription,
  refusedLobbyResult,
  stageGameImportDescription,
  unavailableLobbyResult,
} from "./coachingBoardConstraints"
import {
  evaluateInputSchema,
  listInputSchema,
  openInputSchema as openMomentInPlaceSchema,
  openingContinuationInputSchema,
  parseEvaluatePlayerLineInput,
  parseListCriticalMomentsInput,
  parseOpeningContinuationInput,
  parseOpenReviewMomentInPlaceInput,
  unavailableBoardCoachResult,
} from "./coachingBoardCoachTools"
import type { BoardAnnotationRequest } from "./boardAnnotation"
import type { CoachingBoardStepTarget } from "./coachingBoardLinePlayback"
import {
  driveRefusal,
  type CoachingBoardPositionTarget,
  type CoachingBoardToolResult,
} from "./coachingBoardDrive"
import {
  annotateBoardSchema,
  gameImportIdSchema,
  parseAnnotateBoard,
  parseSetPosition,
  parseShowLine,
  parseStepLine,
  setPositionSchema,
  showLineSchema,
  stepLineSchema,
  type CoachingBoardToolTarget,
} from "./coachingBoardToolInput"
import type {
  CoachingBoardOrientation,
  CoachingBoardSnapshot,
} from "./coachingBoardSnapshot"
import { parseOpeningLineRef, type OpeningLineRef } from "./openingLineRef"
import type { GameImportFields } from "./stagedGameImport"

export type CoachingBoardToolSurface = "board" | "lobby"

export type CoachingBoardToolHost = {
  annotateBoard: (request: {
    requests: readonly BoardAnnotationRequest[]
    revision: number
  }) => CoachingBoardToolResult
  evaluateOpeningContinuation: (input: {
    continuation: (
      | { kind: "san"; san: string }
      | { kind: "uci"; uci: string }
    )[]
    openingLineRef: string
  }) => object | Promise<object>
  evaluatePlayerLine: (input: {
    gameImportId: string
    moment:
      | { kind: "critical"; reviewMomentId: string }
      | { kind: "ply"; ply: number }
    moves: ({ kind: "san"; san: string } | { kind: "uci"; uci: string })[]
    opponentReplies: "engineBest" | "supplied"
  }) => object | Promise<object>
  findOpeningLine: (query: string) => object | Promise<object>
  listCriticalMoments: (gameImportId: string) => object | Promise<object>
  listPlayedOpenings: () => object | Promise<object>
  listRecentProfileGames: () => object | Promise<object>
  openOpeningLine: (openingLineRef: OpeningLineRef) => object
  /** Navigation to a Game the Player already reviewed; the same consent
   * class as an Opening Line. */
  openReviewedGame: (gameImportId: GameImportId) => object
  openReviewMomentInPlace: (input: {
    gameImportId: string
    moment:
      | { kind: "critical"; reviewMomentId: string }
      | { kind: "ply"; ply: number }
      | {
          afterReviewMomentId?: string
          classification?: "improvementOpportunity"
          kind: "next"
        }
  }) => object | Promise<object>
  readSnapshot: () => CoachingBoardSnapshot | null
  searchReviewedGames: (
    request: ReviewedGameSearchRequest,
  ) => object | Promise<object>
  setBoardPosition: (
    target: CoachingBoardPositionTarget,
  ) => CoachingBoardToolResult
  showLine: (line: HostTurnShowLine) => CoachingBoardToolResult
  stepLine: (target: CoachingBoardStepTarget) => CoachingBoardToolResult
  /** The drive never refuses a turn; a surface with no board does. */
  turnBoard: (orientation: CoachingBoardOrientation) => CoachingBoardToolResult
  stageGameImport: (fields: GameImportFields) => object
}

const stageFieldsSchema = v.strictObject({
  elo: v.optional(v.string(), ""),
  reviewSide: v.optional(v.picklist(["black", "both", "white"]), "white"),
  source: v.string(),
})

const findQuerySchema = v.strictObject({
  query: v.string(),
})

const openLineSchema = v.strictObject({
  openingLineRef: v.string(),
})

const openGameSchema = v.strictObject({
  gameImportId: gameImportIdSchema,
})

export function webToolNamesForSurface(surface: CoachingBoardToolSurface) {
  return surface === "lobby" ? coachWebLobbyToolNames : coachWebBoardToolNames
}

export function authoredWebToolNames() {
  return coachWebToolNames
}

/**
 * Register Coaching Board tools after Beta Access authorizes a Player.
 *
 * Called from every Coaching Board surface. Nothing registers at module load.
 * Teardown uses the AbortSignal registerTool accepts. Signed-out, loading,
 * unverified, and beta-unauthorized surfaces pass no authorizedPlayerId and
 * register nothing.
 */
export function useCoachingBoardTools({
  authorizedPlayerId,
  host,
  surface,
}: {
  authorizedPlayerId: string | null
  host: CoachingBoardToolHost
  surface: CoachingBoardToolSurface
}) {
  const hostRef = useRef(host)
  hostRef.current = host

  useEffect(() => {
    if (!authorizedPlayerId) return
    const modelContext = document.modelContext
    if (!modelContext) return
    const controller = new AbortController()
    for (const tool of toolsForSurface(surface, hostRef)) {
      modelContext.registerTool(tool, { signal: controller.signal })
    }
    return () => controller.abort()
  }, [authorizedPlayerId, surface])
}

/**
 * Where each `set_board_position` target goes.
 *
 * One tool, four destinations, and this is the only place that knows which:
 * an Opening Line or a reviewed Game is navigation to another board, so the
 * surface host opens it and the snapshot arrives from `read_coaching_board`
 * after the page settles; an orientation turns the board without moving it;
 * a position is the drive's.
 */
function boardTargetResult(
  host: CoachingBoardToolHost,
  target: CoachingBoardToolTarget,
): object {
  switch (target.kind) {
    case "game":
      return host.openReviewedGame(target.gameImportId)
    case "openingLine":
      return host.openOpeningLine(target.openingLineRef)
    case "orientation":
      return host.turnBoard(target.orientation)
    case "ply":
    case "alternativeMove":
      return host.setBoardPosition(target)
    default: {
      const _exhaustive: never = target
      return _exhaustive
    }
  }
}

function toolsForSurface(
  surface: CoachingBoardToolSurface,
  hostRef: { current: CoachingBoardToolHost },
): ModelContextToolDefinition[] {
  switch (surface) {
    case "lobby":
      return [
        {
          annotations: { idempotentHint: true, readOnlyHint: true },
          description: searchReviewedGamesWebDescription,
          execute: (args) => {
            const parsed = parseSearchReviewedGamesRequest(args)
            return parsed
              ? guardedStructuredContent(
                  () => hostRef.current.searchReviewedGames(parsed),
                  unavailableLobbyResult,
                )
              : { structuredContent: refusedLobbyResult("invalidFilters") }
          },
          inputSchema: searchReviewedGamesInputSchema,
          name: "search_reviewed_games",
        },
        {
          annotations: { idempotentHint: true, readOnlyHint: true },
          description: listRecentProfileGamesDescription,
          execute: () =>
            guardedStructuredContent(
              () => hostRef.current.listRecentProfileGames(),
              unavailableLobbyResult,
            ),
          inputSchema: emptyInputSchema,
          name: "list_recent_profile_games",
        },
        {
          annotations: { readOnlyHint: false },
          description: stageGameImportDescription,
          execute: (args) => {
            const parsed = parseStageFields(args)
            return {
              structuredContent: parsed
                ? hostRef.current.stageGameImport(parsed)
                : refusedLobbyResult("invalidFields"),
            }
          },
          inputSchema: stageInputSchema,
          name: "stage_game_import",
        },
        {
          annotations: { idempotentHint: true, readOnlyHint: true },
          description: findOpeningLineDescription,
          execute: (args) => {
            const query = parseFindQuery(args)
            if (query === null) {
              return { structuredContent: refusedLobbyResult("invalidFields") }
            }
            return guardedStructuredContent(
              () => hostRef.current.findOpeningLine(query),
              unavailableLobbyResult,
            )
          },
          inputSchema: findInputSchema,
          name: "find_opening_line",
        },
        {
          annotations: { idempotentHint: true, readOnlyHint: true },
          description: listPlayedOpeningsDescription,
          execute: () =>
            guardedStructuredContent(
              () => hostRef.current.listPlayedOpenings(),
              unavailableLobbyResult,
            ),
          inputSchema: emptyInputSchema,
          name: "list_played_openings",
        },
        {
          annotations: { idempotentHint: true, readOnlyHint: true },
          description: openOpeningLineDescription,
          execute: (args) => {
            const parsed = parseOpenOpeningLineRef(args)
            return {
              structuredContent: parsed
                ? hostRef.current.openOpeningLine(parsed)
                : refusedLobbyResult("invalidFields"),
            }
          },
          inputSchema: openInputSchema,
          name: "open_opening_line",
        },
        {
          annotations: { idempotentHint: true, readOnlyHint: true },
          description: openReviewedGameDescription,
          execute: (args) => {
            const parsed = v.safeParse(openGameSchema, args)
            return {
              structuredContent: parsed.success
                ? hostRef.current.openReviewedGame(parsed.output.gameImportId)
                : refusedLobbyResult("invalidFields"),
            }
          },
          inputSchema: openGameInputSchema,
          name: "open_reviewed_game",
        },
      ]
    case "board":
      return [
        {
          annotations: { idempotentHint: true, readOnlyHint: true },
          description: listCriticalMomentsWebDescription,
          execute: (args) => {
            const parsed = parseListCriticalMomentsInput(args)
            const unavailable = () =>
              unavailableBoardCoachResult(hostRef.current.readSnapshot())
            return parsed
              ? guardedStructuredContent(
                  () =>
                    hostRef.current.listCriticalMoments(parsed.gameImportId),
                  unavailable,
                )
              : { structuredContent: unavailable() }
          },
          inputSchema: listCriticalMomentsInputSchema,
          name: "list_critical_moments",
        },
        {
          annotations: { idempotentHint: true, readOnlyHint: false },
          description: evaluatePlayerLineWebDescription,
          execute: (args) => {
            const parsed = parseEvaluatePlayerLineInput(args)
            const unavailable = () =>
              unavailableBoardCoachResult(hostRef.current.readSnapshot())
            return parsed
              ? guardedStructuredContent(
                  () => hostRef.current.evaluatePlayerLine(parsed),
                  unavailable,
                )
              : { structuredContent: unavailable() }
          },
          inputSchema: evaluatePlayerLineInputSchema,
          name: "evaluate_player_line",
        },
        {
          annotations: { idempotentHint: true, readOnlyHint: false },
          description: openReviewMomentInPlaceWebDescription,
          execute: (args) => {
            const parsed = parseOpenReviewMomentInPlaceInput(args)
            const unavailable = () =>
              unavailableBoardCoachResult(hostRef.current.readSnapshot())
            return parsed
              ? guardedStructuredContent(
                  () => hostRef.current.openReviewMomentInPlace(parsed),
                  unavailable,
                )
              : { structuredContent: unavailable() }
          },
          inputSchema: openReviewMomentInPlaceInputSchema,
          name: "open_review_moment_in_place",
        },
        {
          annotations: { idempotentHint: true, readOnlyHint: true },
          description: readCoachingBoardDescription,
          execute: () => {
            const snapshot = hostRef.current.readSnapshot()
            return snapshot
              ? { structuredContent: snapshot }
              : {
                  content: [
                    {
                      text: "The board is not on a grounded origin yet.",
                      type: "text",
                    },
                  ],
                }
          },
          inputSchema: emptyInputSchema,
          name: "read_coaching_board",
        },
        {
          annotations: { readOnlyHint: false },
          description: showLineDescription,
          execute: (args) => {
            const parsed = parseShowLine(args)
            if (parsed.kind === "refused") {
              return {
                structuredContent: driveRefusal(
                  parsed.reason,
                  hostRef.current.readSnapshot(),
                ),
              }
            }
            return {
              structuredContent: hostRef.current.showLine(parsed.line),
            }
          },
          inputSchema: showLineInputSchema,
          name: "show_line",
        },
        {
          annotations: { readOnlyHint: false },
          description: stepLineDescription,
          execute: (args) => {
            const parsed = parseStepLine(args)
            return {
              structuredContent:
                parsed.kind === "refused"
                  ? driveRefusal(parsed.reason, hostRef.current.readSnapshot())
                  : hostRef.current.stepLine(parsed.target),
            }
          },
          inputSchema: stepLineInputSchema,
          name: "step_line",
        },
        {
          annotations: { readOnlyHint: false },
          description: setBoardPositionDescription,
          execute: (args) => {
            const parsed = parseSetPosition(args)
            if (parsed.kind === "refused") {
              return {
                structuredContent: driveRefusal(
                  parsed.reason,
                  hostRef.current.readSnapshot(),
                ),
              }
            }
            return {
              structuredContent: boardTargetResult(
                hostRef.current,
                parsed.target,
              ),
            }
          },
          inputSchema: setPositionInputSchema,
          name: "set_board_position",
        },
        {
          annotations: { idempotentHint: false, readOnlyHint: false },
          description: annotateBoardDescription,
          execute: (args) => {
            const parsed = parseAnnotateBoard(args)
            if (parsed.kind === "refused") {
              return {
                structuredContent: driveRefusal(
                  parsed.reason,
                  hostRef.current.readSnapshot(),
                ),
              }
            }
            return guardedStructuredContent(
              () => hostRef.current.annotateBoard(parsed.request),
              () => unavailableBoardCoachResult(hostRef.current.readSnapshot()),
            )
          },
          inputSchema: annotateBoardInputSchema,
          name: "annotate_board",
        },
        {
          annotations: { idempotentHint: true, readOnlyHint: false },
          description: evaluateOpeningContinuationDescription,
          execute: (args) => {
            const parsed = parseOpeningContinuationInput(args)
            const unavailable = () =>
              unavailableBoardCoachResult(hostRef.current.readSnapshot())
            return parsed
              ? guardedStructuredContent(
                  () => hostRef.current.evaluateOpeningContinuation(parsed),
                  unavailable,
                )
              : { structuredContent: unavailable() }
          },
          inputSchema: evaluateOpeningContinuationInputSchema,
          name: "evaluate_opening_continuation",
        },
      ]
    default: {
      const _exhaustive: never = surface
      return _exhaustive
    }
  }
}

/**
 * A stream or network failure inside a tool execute must land as a typed
 * unavailable result, not a raw rejection the page's model context surfaces
 * as an opaque protocol error.
 */
async function guardedStructuredContent(
  run: () => object | Promise<object>,
  unavailable: () => object,
) {
  try {
    return { structuredContent: await run() }
  } catch {
    return { structuredContent: unavailable() }
  }
}

function parseStageFields(args: unknown): GameImportFields | null {
  const parsed = v.safeParse(stageFieldsSchema, args)
  return parsed.success ? parsed.output : null
}

function parseFindQuery(args: unknown): string | null {
  const parsed = v.safeParse(findQuerySchema, args)
  return parsed.success ? parsed.output.query : null
}

const searchRequestSchema = v.pipe(
  v.strictObject({
    openingEcoPrefix: v.optional(
      v.pipe(v.string(), v.minLength(1), v.maxLength(16)),
    ),
    openingName: v.optional(
      v.pipe(v.string(), v.minLength(1), v.maxLength(160)),
    ),
    opponentName: v.optional(
      v.pipe(v.string(), v.minLength(1), v.maxLength(160)),
    ),
    opponentRatingMax: v.optional(
      v.pipe(v.number(), v.integer(), v.minValue(100), v.maxValue(3500)),
    ),
    opponentRatingMin: v.optional(
      v.pipe(v.number(), v.integer(), v.minValue(100), v.maxValue(3500)),
    ),
    outcome: v.optional(v.picklist(["win", "loss", "draw"])),
    playedFrom: v.optional(v.pipe(v.string(), v.isoDate())),
    playedTo: v.optional(v.pipe(v.string(), v.isoDate())),
    provider: v.optional(v.picklist(["lichess", "chessCom", "pastedPgn"])),
    reviewSide: v.optional(v.picklist(["white", "black", "both"])),
    timeControlClass: v.optional(
      v.picklist([
        "classical",
        "correspondence",
        "rapid",
        "blitz",
        "bullet",
        "ultraBullet",
      ]),
    ),
  }),
  v.check(
    ({ playedFrom, playedTo }) =>
      playedFrom === undefined ||
      playedTo === undefined ||
      playedFrom <= playedTo,
  ),
  v.check(
    ({ opponentRatingMin, opponentRatingMax }) =>
      opponentRatingMin === undefined ||
      opponentRatingMax === undefined ||
      opponentRatingMin <= opponentRatingMax,
  ),
)

function parseSearchReviewedGamesRequest(
  args: unknown,
): ReviewedGameSearchRequest | null {
  const parsed = v.safeParse(searchRequestSchema, args)
  return parsed.success ? parsed.output : null
}

function parseOpenOpeningLineRef(args: unknown): OpeningLineRef | null {
  const parsed = v.safeParse(openLineSchema, args)
  return parsed.success
    ? (parseOpeningLineRef(parsed.output.openingLineRef) ?? null)
    : null
}

/**
 * What each tool advertises, derived from the schema that enforces it.
 *
 * Authoring these by hand meant the model could be told a field was required
 * and then have its absence honoured. `errorMode: "ignore"` drops the
 * cross-field checks a JSON Schema cannot express — those stay described in
 * the tool's prose, as they always were.
 */
const advertise = (schema: Parameters<typeof toJsonSchema>[0]) =>
  toJsonSchema(schema, { errorMode: "ignore" })

const emptyInputSchema = advertise(v.strictObject({}))
const stageInputSchema = advertise(stageFieldsSchema)
const findInputSchema = advertise(findQuerySchema)
const openInputSchema = advertise(openLineSchema)
const openGameInputSchema = advertise(openGameSchema)
const searchReviewedGamesInputSchema = advertise(searchRequestSchema)
const showLineInputSchema = advertise(showLineSchema)
const stepLineInputSchema = advertise(stepLineSchema)
const setPositionInputSchema = advertise(setPositionSchema)
const listCriticalMomentsInputSchema = advertise(listInputSchema)
const openReviewMomentInPlaceInputSchema = advertise(openMomentInPlaceSchema)
const evaluatePlayerLineInputSchema = advertise(evaluateInputSchema)
const evaluateOpeningContinuationInputSchema = advertise(
  openingContinuationInputSchema,
)
const annotateBoardInputSchema = advertise(annotateBoardSchema)
