import type {
  AlternativeMoveResult,
  BranchParent,
  CriticalMomentId,
  GameImportId,
  IdempotencyKey,
  MoveInput,
  OperationCompletion,
  OperationId,
  PositionInspection,
  PositionSnapshot,
  ReviewSessionCommand,
  ReviewSessionEventEnvelope,
} from "@chenchess/coach-engine-sdk"
import {
  fromCriticalMomentId,
  fromGameImportId,
  fromIdempotencyKey,
  mintOperationId,
} from "@chenchess/coach-engine-sdk"
import { canonicalMovesFromFen } from "@chenchess/review-projection"

import {
  evaluatedPlayerLineContent,
  illegalMoveContent,
  interruptedPlayerLineContent,
  playerLineUnavailableContent,
  type EvaluatedPlayerLinePly,
  type PlayerLineStructuredContent,
} from "./player-line-content.js"
import { CoachAppRequestLimiter } from "./request-limiter.js"

export type PlayerLineMoment =
  | { kind: "critical"; reviewMomentId: string }
  | { kind: "ply"; ply: number }

export type PlayerLineInput = {
  gameImportId: string
  moment: PlayerLineMoment
  moves: MoveInput[]
  opponentReplies: "engineBest" | "supplied"
}

export type PlayerLineCommandOutcome =
  | { kind: "completed"; completion: OperationCompletion }
  | { kind: "deadlineReached" }
  | { kind: "explorationExhausted" }
  | { kind: "failed" }
  | { kind: "idempotencyKeyMismatch" }
  | { kind: "illegalMove" }

export type PlayerLineCommandExecute = (
  command: ReviewSessionCommand,
  options?: {
    onEvent?: (event: ReviewSessionEventEnvelope) => void
    operationId?: OperationId
    signal?: AbortSignal
  },
) => Promise<PlayerLineCommandOutcome>

export type PlayerLineKeyMint = {
  idempotency: (purpose: string, values: readonly unknown[]) => IdempotencyKey
  options: (
    stage: string,
    values: readonly unknown[],
  ) => {
    operationId: OperationId
    signal: AbortSignal
  }
  signal: AbortSignal
}

export type PlayerLineObservations = {
  observe: (envelope: ReviewSessionEventEnvelope) => void
  rateLimited: () => boolean
  remainingAllowance: () => number
}

type Step<T> =
  | { kind: "ready"; value: T }
  | { kind: "terminal"; result: PlayerLineStructuredContent }

type PlayerLineRoot = {
  inspection: PositionInspection
  reviewMomentId: CriticalMomentId
}

type EvaluatedMoveStep =
  | {
      kind: "evaluated"
      alternativeMove: AlternativeMoveResult
      ply: EvaluatedPlayerLinePly
    }
  | { kind: "illegal" }
  | {
      kind: "interrupted"
      reason: "deadlineReached" | "explorationExhausted"
    }
  | { kind: "terminal"; result: PlayerLineStructuredContent }

export const playerLineRateLimitCategory = "player-line-analysis"

/**
 * A walked Player line: what a model reads, and the moves it was built from.
 *
 * The structured content is the model-facing contract and carries no branch
 * identity; a board needs the evaluated moves themselves to fold them into
 * its exploration tree.
 */
export type PlayerLineEvaluation = {
  content: PlayerLineStructuredContent
  evaluatedMoves: readonly AlternativeMoveResult[]
}

export async function evaluatePlayerLineFromCommands(
  input: PlayerLineInput,
  {
    execute,
    keys,
    observations,
  }: {
    execute: PlayerLineCommandExecute
    keys: PlayerLineKeyMint
    observations: PlayerLineObservations
  },
): Promise<PlayerLineEvaluation> {
  const gameImportId = fromGameImportId(input.gameImportId)
  const root = await preparePlayerLineRoot(
    execute,
    keys,
    gameImportId,
    input.moment,
  )
  if (root.kind === "terminal") {
    return { content: root.result, evaluatedMoves: [] }
  }
  return evaluatePlayerLineMoves(
    execute,
    keys,
    gameImportId,
    root.value,
    input,
    observations,
  )
}

export function playerLineObservations(
  limiter: CoachAppRequestLimiter,
  playerId: string,
): PlayerLineObservations {
  const analyzedOperations = new Set<OperationId>()
  let allowance: number | undefined
  return {
    observe: (envelope: ReviewSessionEventEnvelope) => {
      const event = envelope.event
      if (event.kind !== "progress") return
      if (
        event.stage.kind === "alternativeMove" &&
        event.stage.stage === "evaluatingMove"
      ) {
        if (!analyzedOperations.has(envelope.operationId)) {
          analyzedOperations.add(envelope.operationId)
          limiter.charge(playerId, playerLineRateLimitCategory, 1)
        }
      } else if (event.stage.kind === "alternativeMoveAllowance") {
        allowance = event.stage.remaining
      }
    },
    rateLimited: () =>
      limiter.retryAfter(playerId, playerLineRateLimitCategory) !== undefined,
    remainingAllowance: () => {
      if (allowance === undefined) {
        throw new Error(
          "Coach Engine omitted Alternative Move allowance before its terminal event",
        )
      }
      return allowance
    },
  }
}

async function preparePlayerLineRoot(
  execute: PlayerLineCommandExecute,
  keys: PlayerLineKeyMint,
  gameImportId: GameImportId,
  moment: PlayerLineMoment,
): Promise<Step<PlayerLineRoot>> {
  const resolvedStep = requiredCompletion(
    await execute(
      {
        gameImportId,
        kind: "openAddressedReviewMoment",
        reference:
          moment.kind === "critical"
            ? {
                kind: moment.kind,
                reviewMomentId: fromCriticalMomentId(moment.reviewMomentId),
              }
            : moment,
      },
      keys.options("resolve", [gameImportId, moment]),
    ),
    "addressedReviewMomentOpened",
  )
  if (resolvedStep.kind === "terminal") return resolvedStep
  const { detail } = resolvedStep.value

  const startedStep = requiredCompletion(
    await execute(
      { gameImportId, kind: "startReviewSession" },
      keys.options("start", [gameImportId]),
    ),
    "reviewSessionStarted",
  )
  if (startedStep.kind === "terminal") return startedStep

  const selection =
    moment.kind === "critical"
      ? {
          criticalMomentId: detail.reviewMomentId,
          kind: "pipelineCriticalMoment" as const,
        }
      : { kind: "playerSelectedMoment" as const, ply: detail.ply }
  const openedStep = requiredCompletion(
    await execute(
      {
        gameImportId,
        idempotencyKey: keys.idempotency("moment-open", [
          gameImportId,
          selection,
        ]),
        kind: "openReviewMoment",
        selection,
      },
      keys.options("open", [gameImportId, selection]),
    ),
    "reviewMomentOpened",
  )
  if (openedStep.kind === "terminal") return openedStep

  const inspectedStep = requiredCompletion(
    await execute(
      {
        gameImportId,
        kind: "inspectPosition",
        reviewMomentId: detail.reviewMomentId,
        target: { kind: "reviewedMove" },
      },
      keys.options("inspect", [gameImportId, detail.reviewMomentId]),
    ),
    "positionInspected",
  )
  return inspectedStep.kind === "terminal"
    ? inspectedStep
    : {
        kind: "ready",
        value: {
          inspection: inspectedStep.value.inspection,
          reviewMomentId: detail.reviewMomentId,
        },
      }
}

async function evaluatePlayerLineMoves(
  execute: PlayerLineCommandExecute,
  keys: PlayerLineKeyMint,
  gameImportId: GameImportId,
  root: PlayerLineRoot,
  input: PlayerLineInput,
  observations: PlayerLineObservations,
): Promise<PlayerLineEvaluation> {
  let parent: BranchParent = {
    kind: "root",
    positionRef: root.inspection.positionSnapshot.positionRef,
  }
  let position = root.inspection.positionSnapshot
  let movePath: MoveInput[] = []
  const plies: EvaluatedPlayerLinePly[] = []
  const evaluatedMoves: AlternativeMoveResult[] = []

  for (const [playerIndex, playerMove] of input.moves.entries()) {
    const playerPath = [...movePath, playerMove]
    const playerStep = await evaluatePlayerLinePly({
      execute,
      gameImportId,
      index: plies.length,
      keys,
      move: playerMove,
      movePath: playerPath,
      parent,
      position,
      reviewMomentId: root.reviewMomentId,
      source: "player",
      observations,
    })
    if (playerStep.kind === "illegal") {
      return walked(
        evaluatedMoves,
        illegalMoveContent(
          gameImportId,
          root.reviewMomentId,
          plies,
          plies.length,
          playerMove,
          observations.remainingAllowance(),
        ),
      )
    }
    if (playerStep.kind === "interrupted") {
      return walked(
        evaluatedMoves,
        interruptedPlayerLineContent({
          gameImportId,
          kind: playerStep.reason,
          plies,
          remainingAllowance: observations.remainingAllowance(),
          reviewMomentId: root.reviewMomentId,
        }),
      )
    }
    if (playerStep.kind === "terminal") {
      return walked(evaluatedMoves, playerLineTerminalResult(playerStep.result))
    }
    plies.push(playerStep.ply)
    evaluatedMoves.push(playerStep.alternativeMove)
    movePath = playerPath
    parent = { kind: "move", branchRef: playerStep.alternativeMove.branchRef }
    position = playerStep.alternativeMove.resultingPosition
    if (playerIndex < input.moves.length - 1 && observations.rateLimited()) {
      return walked(
        evaluatedMoves,
        interruptedPlayerLineContent({
          gameImportId,
          kind: "rateLimited",
          plies,
          remainingAllowance: observations.remainingAllowance(),
          reviewMomentId: root.reviewMomentId,
        }),
      )
    }

    // The final offered reply stays on its Player ply. Only an intermediate
    // reply needs its own evaluation to reach the next supplied Player move;
    // this preserves the parent contract that one move is a one-ply line.
    if (
      input.opponentReplies !== "engineBest" ||
      playerIndex === input.moves.length - 1 ||
      playerStep.alternativeMove.strongestReply.kind !== "offered"
    ) {
      continue
    }

    const engineMove = {
      kind: "uci",
      uci: playerStep.alternativeMove.strongestReply.uci,
    } satisfies MoveInput
    const enginePath = [...movePath, engineMove]
    const engineStep = await evaluatePlayerLinePly({
      execute,
      gameImportId,
      index: plies.length,
      keys,
      move: engineMove,
      movePath: enginePath,
      parent,
      position,
      reviewMomentId: root.reviewMomentId,
      source: "engine",
      observations,
    })
    if (engineStep.kind === "illegal") {
      throw new Error("Coach Engine offered an illegal strongest reply")
    }
    if (engineStep.kind === "interrupted") {
      return walked(
        evaluatedMoves,
        interruptedPlayerLineContent({
          gameImportId,
          kind: engineStep.reason,
          plies,
          remainingAllowance: observations.remainingAllowance(),
          reviewMomentId: root.reviewMomentId,
        }),
      )
    }
    if (engineStep.kind === "terminal") {
      return walked(evaluatedMoves, playerLineTerminalResult(engineStep.result))
    }
    plies.push(engineStep.ply)
    evaluatedMoves.push(engineStep.alternativeMove)
    movePath = enginePath
    parent = { kind: "move", branchRef: engineStep.alternativeMove.branchRef }
    position = engineStep.alternativeMove.resultingPosition
    if (observations.rateLimited()) {
      return walked(
        evaluatedMoves,
        interruptedPlayerLineContent({
          gameImportId,
          kind: "rateLimited",
          plies,
          remainingAllowance: observations.remainingAllowance(),
          reviewMomentId: root.reviewMomentId,
        }),
      )
    }
  }

  return walked(
    evaluatedMoves,
    evaluatedPlayerLineContent({
      gameImportId,
      plies,
      remainingAllowance: observations.remainingAllowance(),
      reviewMomentId: root.reviewMomentId,
    }),
  )
}

function walked(
  evaluatedMoves: readonly AlternativeMoveResult[],
  content: PlayerLineStructuredContent,
): PlayerLineEvaluation {
  return { content, evaluatedMoves }
}

async function evaluatePlayerLinePly({
  execute,
  gameImportId,
  index,
  keys,
  move,
  movePath,
  parent,
  position,
  reviewMomentId,
  source,
  observations,
}: {
  execute: PlayerLineCommandExecute
  gameImportId: GameImportId
  index: number
  keys: PlayerLineKeyMint
  move: MoveInput
  movePath: readonly MoveInput[]
  parent: BranchParent
  position: PositionSnapshot
  reviewMomentId: CriticalMomentId
  source: EvaluatedPlayerLinePly["source"]
  observations: PlayerLineObservations
}): Promise<EvaluatedMoveStep> {
  const command = {
    gameImportId,
    idempotencyKey: keys.idempotency("move-path", [
      gameImportId,
      reviewMomentId,
      movePath,
    ]),
    kind: "exploreAlternativeMove" as const,
    moveInput: move,
    parent,
    reviewMomentId,
    sourcePositionRef: position.positionRef,
  }
  let explored = await execute(command, {
    ...keys.options("explore", [gameImportId, reviewMomentId, movePath]),
    onEvent: observations.observe,
  })
  if (explored.kind === "idempotencyKeyMismatch") {
    const retry = freshExplorationIdentity()
    explored = await execute(
      { ...command, idempotencyKey: retry.idempotencyKey },
      {
        onEvent: observations.observe,
        operationId: retry.operationId,
        signal: keys.signal,
      },
    )
  }
  if (explored.kind === "illegalMove") {
    return { kind: "illegal" }
  }
  if (explored.kind === "explorationExhausted") {
    return { kind: "interrupted", reason: "explorationExhausted" }
  }
  if (explored.kind === "deadlineReached") {
    return { kind: "interrupted", reason: "deadlineReached" }
  }
  const completion =
    explored.kind === "completed" ? explored.completion : undefined
  if (!completion || completion.kind !== "alternativeMoveEvaluated") {
    return { kind: "terminal", result: playerLineUnavailableContent() }
  }

  const { alternativeMove } = completion
  const canonicalMove = canonicalMovesFromFen(position.fen, [
    { uci: alternativeMove.moveUci },
  ])?.[0]
  if (!canonicalMove) {
    throw new Error(
      "Coach Engine evaluated a move that is not legal from its source Position",
    )
  }
  return {
    alternativeMove,
    kind: "evaluated",
    ply: {
      evaluation: alternativeMove.evaluation,
      index,
      move: canonicalMove,
      mover: position.sideToMove,
      source,
      strongestReply:
        alternativeMove.strongestReply.kind === "offered"
          ? alternativeMove.strongestReply
          : undefined,
    },
  }
}

function playerLineTerminalResult(
  result: PlayerLineStructuredContent,
): PlayerLineStructuredContent {
  return result.outcome === "unavailable"
    ? result
    : playerLineUnavailableContent()
}

function requiredCompletion<K extends OperationCompletion["kind"]>(
  outcome: PlayerLineCommandOutcome,
  kind: K,
): Step<Extract<OperationCompletion, { kind: K }>> {
  if (outcome.kind === "completed" && outcome.completion.kind === kind) {
    return {
      kind: "ready",
      // SAFETY: the kind discriminant was checked on the previous line.
      value: outcome.completion as Extract<OperationCompletion, { kind: K }>,
    }
  }
  return { kind: "terminal", result: playerLineUnavailableContent() }
}

function freshExplorationIdentity() {
  return {
    idempotencyKey: fromIdempotencyKey(
      `idempotency:coach-app:${crypto.randomUUID()}`,
    ),
    operationId: mintOperationId("coach-app", crypto.randomUUID()),
  }
}
