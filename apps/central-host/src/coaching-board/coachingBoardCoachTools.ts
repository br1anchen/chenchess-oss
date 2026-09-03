import * as v from "valibot"
import {
  fromGameImportId,
  mintIdempotencyKey,
  mintOperationId,
  type GameImportId,
  type ReviewSessionCommand,
  type ReviewSessionEvent,
  type ReviewSessionEventEnvelope,
} from "@chenchess/coach-engine-sdk"

import {
  createCommandEnvelope,
  streamReviewSessionCommand,
  type FetchAccessToken,
} from "@/review-session/client"

import {
  evaluatePlayerLineFromCommands,
  playerLineObservations,
  playerLineRateLimitCategory,
  type PlayerLineCommandExecute,
  type PlayerLineCommandOutcome,
  type PlayerLineInput,
  type PlayerLineKeyMint,
} from "../../server/board/player-line-evaluate"
import {
  rateLimitedPlayerLineContent,
  type PlayerLineStructuredContent,
} from "../../server/board/player-line-content"
import { CoachAppRequestLimiter } from "../../server/board/request-limiter"
import { criticalMomentRouting } from "../../server/board/review-moment-routing"
import {
  openAddressedReviewMomentCommand,
  type ReviewMomentReference,
} from "../../server/board/review-moment-command"

import { boardConstraints } from "./coachingBoardConstraints"
import type {
  CoachingBoardExplorationBranch,
  CoachingBoardSnapshot,
} from "./coachingBoardSnapshot"
import { webFingerprint } from "./webFingerprint"

const boardPlayerLineLimiter = new CoachAppRequestLimiter(120)

export type CoachingBoardCoachToolFacts = {
  constraints: CoachingBoardSnapshot["constraints"]
  snapshot: CoachingBoardSnapshot | null
}

export function wrapBoardCoachResult<T extends object>(
  facts: T,
  snapshot: CoachingBoardSnapshot | null,
): T & CoachingBoardCoachToolFacts {
  return {
    ...facts,
    constraints: snapshot?.constraints ?? boardConstraints(),
    snapshot,
  }
}

export function unavailableBoardCoachResult(
  snapshot: CoachingBoardSnapshot | null,
) {
  return wrapBoardCoachResult({ kind: "unavailable" as const }, snapshot)
}

export async function listCriticalMomentsOnBoard({
  execute,
  gameImportId,
  snapshot,
}: {
  execute: CoachingBoardCommandExecute
  gameImportId: string
  snapshot: CoachingBoardSnapshot | null
}) {
  const event = await execute({
    gameImportId: fromGameImportId(gameImportId),
    kind: "readGameReviewSnapshot",
  })
  if (
    event?.kind !== "completed" ||
    event.result.kind !== "gameReviewSnapshotRead"
  ) {
    return unavailableBoardCoachResult(snapshot)
  }
  const read = event.result
  return wrapBoardCoachResult(
    {
      gameImportId: read.gameImportId,
      kind: read.kind,
      opening: read.importedGame.game.opening,
      reviewMoments: criticalMomentRouting(read.reviewMoments),
      summary: {
        criticalMomentCount: read.reviewMoments.length,
        text: read.review.summary,
      },
    },
    snapshot,
  )
}

export async function openReviewMomentInPlaceOnBoard({
  execute,
  gameImportId,
  moment,
  onOpened,
  snapshot,
}: {
  execute: CoachingBoardCommandExecute
  gameImportId: string
  moment: ReviewMomentReference
  onOpened: (detail: {
    gameImportId: GameImportId
    ply: number
    reviewMomentId: string
  }) => CoachingBoardSnapshot | null
  snapshot: CoachingBoardSnapshot | null
}) {
  const event = await execute(
    openAddressedReviewMomentCommand(gameImportId, moment),
  )
  if (
    event?.kind !== "completed" ||
    event.result.kind !== "addressedReviewMomentOpened"
  ) {
    return unavailableBoardCoachResult(snapshot)
  }
  const { detail } = event.result
  const nextSnapshot = onOpened({
    gameImportId: detail.gameImportId,
    ply: detail.ply,
    reviewMomentId: detail.reviewMomentId,
  })
  return wrapBoardCoachResult(
    {
      comment: detail.comment,
      continuation: detail.continuation,
      decisionLearningOutcome: detail.decisionLearningOutcome,
      explanation: detail.explanation,
      explanationRef: detail.explanationRef,
      gameImportId: detail.gameImportId,
      kind: "addressedReviewMomentOpened" as const,
      objectiveLines: detail.objectiveLines,
      ply: detail.ply,
      reviewMomentId: detail.reviewMomentId,
    },
    nextSnapshot ?? snapshot,
  )
}

/**
 * The game board's evaluate-then-show gate.
 *
 * Every evaluated move becomes a branch of this board's exploration tree,
 * whoever asked for it, and the returned facts carry the tree that resulted
 * rather than the one that went in. `applyBranches` decides what the board
 * then does: an agent's evaluation folds and stays put, the Player's own move
 * folds and follows.
 */
export async function evaluatePlayerLineOnBoard({
  applyBranches,
  execute,
  input,
  playerId,
  signal,
  snapshot,
}: {
  applyBranches: (
    minted: readonly CoachingBoardExplorationBranch[],
  ) => CoachingBoardSnapshot
  execute: CoachingBoardCommandExecute
  input: PlayerLineInput
  playerId: string
  signal?: AbortSignal
  snapshot: CoachingBoardSnapshot | null
}): Promise<PlayerLineStructuredContent & CoachingBoardCoachToolFacts> {
  return boardPlayerLineLimiter.runExclusive(
    playerId,
    playerLineRateLimitCategory,
    async () => {
      const retryAfterSeconds = boardPlayerLineLimiter.retryAfter(
        playerId,
        playerLineRateLimitCategory,
      )
      if (retryAfterSeconds !== undefined) {
        return wrapBoardCoachResult(
          rateLimitedPlayerLineContent(retryAfterSeconds),
          snapshot,
        )
      }
      const { content, evaluatedMoves } = await evaluatePlayerLineFromCommands(
        input,
        {
          execute: playerLineExecuteFor(execute),
          keys: webPlayerLineKeys(playerId, signal),
          observations: playerLineObservations(
            boardPlayerLineLimiter,
            playerId,
          ),
        },
      )
      return wrapBoardCoachResult(
        content,
        evaluatedMoves.length > 0 ? applyBranches(evaluatedMoves) : snapshot,
      )
    },
  )
}

export type CoachingBoardCommandExecute = (
  command: ReviewSessionCommand,
  options?: {
    onEvent?: (event: ReviewSessionEventEnvelope) => void
    operationId?: ReturnType<typeof mintOperationId>
    signal?: AbortSignal
  },
) => Promise<ReviewSessionEvent | null>

export function coachingBoardCommandExecute(
  fetchAccessToken: FetchAccessToken,
): CoachingBoardCommandExecute {
  return async (command, options) => {
    const minted = createCommandEnvelope(command)
    const envelope = options?.operationId
      ? { ...minted, operationId: options.operationId }
      : minted
    let terminal: ReviewSessionEvent | null = null
    await streamReviewSessionCommand({
      envelope,
      fetchAccessToken,
      onEvent: (event) => {
        options?.onEvent?.(event)
        if (
          event.event.kind !== "accepted" &&
          event.event.kind !== "progress"
        ) {
          terminal = event.event
        }
      },
    })
    return terminal
  }
}

/** Shared with the Player's own drag path, which sends the same commands
 * through the same channel and reads the same outcomes. */
export function playerLineExecuteFor(
  execute: CoachingBoardCommandExecute,
): PlayerLineCommandExecute {
  return async (command, options) =>
    outcomeFromEvent(await execute(command, options))
}

function outcomeFromEvent(
  event: ReviewSessionEvent | null,
): PlayerLineCommandOutcome {
  if (!event) return { kind: "failed" }
  if (event.kind === "completed") {
    return { kind: "completed", completion: event.result }
  }
  if (event.kind === "rejected" && event.reason === "illegalMove") {
    return { kind: "illegalMove" }
  }
  if (event.kind === "rejected" && event.reason === "alternativeMoveLimit") {
    return { kind: "explorationExhausted" }
  }
  if (
    event.kind === "unavailable" &&
    event.reason.kind === "timeout" &&
    event.reason.provider === "stockfish"
  ) {
    return { kind: "deadlineReached" }
  }
  if (event.kind === "conflict" && event.reason === "idempotencyKeyMismatch") {
    return { kind: "idempotencyKeyMismatch" }
  }
  return { kind: "failed" }
}

/**
 * The player-scoped layout the engine's idempotency and operation keys carry:
 * player, purpose, values. Hashed rather than sent raw, so replaying the same
 * line dedupes on the engine instead of spending Alternative Move allowance
 * twice.
 */
function webRequestFingerprint(
  playerId: string,
  purpose: string,
  values: readonly unknown[],
): string {
  return webFingerprint(`${playerId} ${purpose} ${JSON.stringify(values)}`)
}

export function webPlayerLineKeys(
  playerId: string,
  signal: AbortSignal = new AbortController().signal,
): PlayerLineKeyMint {
  return {
    idempotency: (purpose, values) =>
      mintIdempotencyKey(
        "web",
        `${purpose}:${webRequestFingerprint(playerId, purpose, values)}`,
      ),
    options: (stage, values) => ({
      operationId: mintOperationId(
        "web",
        `${stage}:${webRequestFingerprint(playerId, stage, values)}`,
      ),
      signal,
    }),
    signal,
  }
}

const gameImportIdSchema = v.pipe(v.string(), v.minLength(1))

export const listInputSchema = v.object({
  gameImportId: gameImportIdSchema,
})

const momentSchema = v.variant("kind", [
  v.strictObject({
    kind: v.literal("critical"),
    reviewMomentId: v.pipe(v.string(), v.minLength(1)),
  }),
  v.strictObject({
    kind: v.literal("ply"),
    ply: v.pipe(v.number(), v.integer(), v.minValue(1)),
  }),
  v.strictObject({
    afterReviewMomentId: v.optional(v.pipe(v.string(), v.minLength(1))),
    classification: v.optional(v.literal("improvementOpportunity")),
    kind: v.literal("next"),
  }),
])

export const openInputSchema = v.object({
  gameImportId: gameImportIdSchema,
  moment: momentSchema,
})

const moveInputSchema = v.union([
  v.strictObject({ kind: v.literal("san"), san: v.string() }),
  v.strictObject({ kind: v.literal("uci"), uci: v.string() }),
])

export const evaluateInputSchema = v.pipe(
  v.object({
    gameImportId: gameImportIdSchema,
    moment: v.variant("kind", [
      v.strictObject({
        kind: v.literal("critical"),
        reviewMomentId: v.pipe(v.string(), v.minLength(1)),
      }),
      v.strictObject({
        kind: v.literal("ply"),
        ply: v.pipe(v.number(), v.integer(), v.minValue(1)),
      }),
    ]),
    moves: v.pipe(v.array(moveInputSchema), v.minLength(1), v.maxLength(12)),
    opponentReplies: v.picklist(["engineBest", "supplied"]),
  }),
  v.check(
    ({ moves, opponentReplies }) =>
      opponentReplies !== "engineBest" || moves.length * 2 - 1 <= 12,
  ),
)

export const openingContinuationInputSchema = v.object({
  continuation: v.pipe(
    v.array(moveInputSchema),
    v.minLength(1),
    v.maxLength(12),
  ),
  openingLineRef: v.pipe(v.string(), v.minLength(1)),
})

export function parseOpeningContinuationInput(args: unknown) {
  const parsed = v.safeParse(openingContinuationInputSchema, args)
  return parsed.success ? parsed.output : null
}

export function parseListCriticalMomentsInput(args: unknown) {
  const parsed = v.safeParse(listInputSchema, args)
  return parsed.success ? parsed.output : null
}

export function parseOpenReviewMomentInPlaceInput(args: unknown) {
  const parsed = v.safeParse(openInputSchema, args)
  return parsed.success ? parsed.output : null
}

export function parseEvaluatePlayerLineInput(
  args: unknown,
): PlayerLineInput | null {
  const parsed = v.safeParse(evaluateInputSchema, args)
  if (!parsed.success) return null
  return {
    gameImportId: parsed.output.gameImportId,
    moment: parsed.output.moment,
    moves: parsed.output.moves,
    opponentReplies: parsed.output.opponentReplies,
  }
}
