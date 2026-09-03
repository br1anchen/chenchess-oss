import { FirebaseError } from "firebase/app"
import { useCallback, useRef, useState } from "react"

import type {
  AlternativeMoveProgressStage,
  CoachTurnProgressStage,
  CommandRejectionReason,
  ImportProgressStage,
  OperationCompletion,
  OperationId,
  ProviderKind,
  ProviderUnavailableReason,
  IdempotencyKey,
  RejectionRecovery,
  ReviewMomentPreparationProgressStage,
  ReviewSessionProgressStage,
  ReviewSessionCommand,
  ReviewSessionEvent,
} from "@chenchess/coach-engine-sdk"

import {
  createCommandEnvelope,
  streamReviewSessionCommand,
  type FetchAccessToken,
} from "./client"
import { recoveryMessage } from "./model"
import { hostTurnStepDisplayLabel } from "./thread-state"

export type OperationLane =
  | "import"
  | "navigation"
  | "alternative"
  | "coach"
  | "hostTurn"
  | "control"

type ActiveOperationBase = {
  operationId: OperationId
  label: string
}

export type ActiveOperation =
  | (ActiveOperationBase & {
      kind: "passive"
    })
  | (ActiveOperationBase & {
      kind: "alternative"
      key: IdempotencyKey
    })
  | (ActiveOperationBase & {
      kind: "hostTurn"
      key: IdempotencyKey
    })

export type CancellableOperation = Extract<
  ActiveOperation,
  {
    kind: "alternative" | "hostTurn"
  }
>

export type ReviewSessionCommandResult =
  | OperationCompletion
  | {
      kind: "unavailable"
      reason: ProviderUnavailableReason
    }
  | {
      kind: "rejected"
      reason: CommandRejectionReason
      recovery: RejectionRecovery
    }

export type RunReviewSessionCommand = (
  lane: OperationLane,
  command: ReviewSessionCommand,
  label: string,
) => Promise<ReviewSessionCommandResult | null>

export type RunIndependentReviewSessionCommand = (
  command: ReviewSessionCommand,
  label: string,
) => Promise<OperationCompletion | null>

export function useReviewSessionCommands(
  fetchAccessToken: FetchAccessToken,
  onUnknownGameImport?: () => void,
) {
  const latest = useRef<Partial<Record<OperationLane, OperationId>>>({})
  const [active, setActive] = useState<
    Partial<Record<OperationLane, ActiveOperation>>
  >({})
  const [failure, setFailure] = useState<string | null>(null)

  const run = useCallback(
    async (
      lane: OperationLane,
      command: ReviewSessionCommand,
      label: string,
    ): Promise<ReviewSessionCommandResult | null> => {
      const envelope = createCommandEnvelope(command)
      const operation = activeOperation(command, envelope.operationId, label)
      latest.current[lane] = envelope.operationId
      setFailure(null)

      setActive((current) => ({
        ...current,
        [lane]: operation,
      }))
      let completion: OperationCompletion | null = null
      let unavailableReason: ProviderUnavailableReason | null = null
      let rejected: Extract<
        ReviewSessionCommandResult,
        { kind: "rejected" }
      > | null = null
      let streamCompleted = false
      let published: ReviewSessionCommandResult | null = null

      try {
        await streamReviewSessionCommand({
          envelope,
          fetchAccessToken: async (options) => {
            const credential = await fetchAccessToken(options)
            return latest.current[lane] === envelope.operationId
              ? credential
              : null
          },
          onEvent: (event) => {
            if (latest.current[lane] !== envelope.operationId) return
            const next = event.event
            if (next.kind === "progress") {
              const label = progressLabel(next)
              if (label === null) return
              setActive((current) => {
                const operation = current[lane]
                if (operation?.operationId !== envelope.operationId)
                  return current
                return {
                  ...current,
                  [lane]: { ...operation, label },
                }
              })
            } else if (next.kind === "completed") {
              completion = next.result
            } else if (
              next.kind === "unavailable" &&
              command.kind === "startHostTurn"
            ) {
              unavailableReason = next.reason
            } else if (
              next.kind === "rejected" &&
              command.kind === "startHostTurn"
            ) {
              if (next.reason === "unknownGameImport") {
                onUnknownGameImport?.()
              }
              rejected = {
                kind: "rejected",
                reason: next.reason,
                recovery: next.recovery,
              }
            } else {
              const message = eventFailure(next, onUnknownGameImport)
              if (message) setFailure(message)
            }
          },
        })
        streamCompleted = true
      } catch (caught) {
        if (latest.current[lane] === envelope.operationId) {
          setFailure(parseTransportFailureMessage(caught))
        }
      } finally {
        if (latest.current[lane] === envelope.operationId) {
          if (streamCompleted && completion) {
            published = completion
          } else if (streamCompleted && unavailableReason) {
            published = {
              kind: "unavailable",
              reason: unavailableReason,
            }
          } else if (streamCompleted && rejected) {
            published = rejected
          }
          delete latest.current[lane]
          setActive((current) => {
            if (current[lane]?.operationId !== envelope.operationId)
              return current
            const next = { ...current }
            delete next[lane]
            return next
          })
        }
      }

      return published
    },
    [fetchAccessToken, onUnknownGameImport],
  )

  const runIndependent = useCallback<RunIndependentReviewSessionCommand>(
    async (command) => {
      const envelope = createCommandEnvelope(command)
      let completion: OperationCompletion | null = null
      try {
        await streamReviewSessionCommand({
          envelope,
          fetchAccessToken,
          onEvent: ({ event }) => {
            if (event.kind === "completed") {
              completion = event.result
            }
          },
        })
        return completion
      } catch {
        return null
      }
    },
    [fetchAccessToken],
  )

  const invalidate = useCallback(() => {
    latest.current = {}
    setActive({})
  }, [])

  return { active, failure, invalidate, run, runIndependent, setFailure }
}

/** The message for a command that never reached the Coach Engine over a
 * dropped connection. Nothing was lost: the token retry already ran. */
export const CONNECTION_DROPPED =
  "Your connection dropped. Nothing was lost — try again."

/**
 * The catch around a command stream sees transport failures only — every
 * engine outcome arrives as a typed event — so a vendor error string here is
 * an implementation detail, never Player-facing prose.
 */
function parseTransportFailureMessage(caught: unknown): string {
  if (caught instanceof FirebaseError) {
    return caught.code === "auth/network-request-failed"
      ? CONNECTION_DROPPED
      : "Your session could not be authorized. Reload the page to sign in again."
  }
  // fetch rejects with a TypeError when the network itself fails.
  if (caught instanceof TypeError) return CONNECTION_DROPPED
  return caught instanceof Error
    ? caught.message
    : "Something went wrong. Try again."
}

function eventFailure(
  event: Exclude<ReviewSessionEvent, { kind: "completed" | "progress" }>,
  onUnknownGameImport?: () => void,
): string | null {
  if (event.kind === "unavailable") return unavailableMessage(event)
  if (event.kind === "reviewMomentUnavailable") {
    return `${unavailableReasonMessage(event.reason)} That moment is still unprepared — open it again to retry.`
  }
  if (event.kind === "rejected") {
    // Only an address the Coach Engine does not know is worth forgetting. A
    // lost Review Session costs transient state alone: the review is still
    // durable at its Game Import ID and the next command rebuilds over it, so
    // forgetting the ID here would throw away the Player's place for a reason
    // that no longer exists.
    if (event.reason === "unknownGameImport") onUnknownGameImport?.()
    return recoveryMessage(event.recovery)
  }
  if (event.kind === "conflict") {
    return "A newer result replaced this one."
  }
  if (event.kind === "cancelled") {
    return "Cancelled. Nothing was added."
  }
  return null
}

function activeOperation(
  command: ReviewSessionCommand,
  operationId: OperationId,
  label: string,
): ActiveOperation {
  if (command.kind === "exploreAlternativeMove") {
    return {
      kind: "alternative",
      operationId,
      label,
      key: command.idempotencyKey,
    }
  }
  if (command.kind === "startHostTurn") {
    return {
      kind: "hostTurn",
      operationId,
      label,
      key: command.idempotencyKey,
    }
  }
  return { kind: "passive", operationId, label }
}

function progressLabel(
  event: Extract<ReviewSessionEvent, { kind: "progress" }>,
): string | null {
  switch (event.stage.kind) {
    case "import":
      return importProgressLabels[event.stage.stage]
    case "reviewSession":
      return reviewSessionProgressLabels[event.stage.stage]
    case "reviewMomentPreparation":
      return reviewMomentPreparationProgressLabels[event.stage.stage]
    case "alternativeMove":
      return alternativeProgressLabels[event.stage.stage]
    case "alternativeMoveAllowance":
      return null
    case "coachTurn":
      return coachProgressLabels[event.stage.stage]
    case "hostTurn":
      return hostTurnStepDisplayLabel(event.stage.label)
    default: {
      const _exhaustive: never = event.stage
      return _exhaustive
    }
  }
}

const importProgressLabels = {
  validatingSource: "Checking the link…",
  waitingForLichess: "Fetching the game from Lichess…",
  waitingForChessCom: "Fetching the game from Chess.com…",
  fetchingGame: "Fetching the game…",
  validatingGame: "Checking the game…",
  runningGameReview: "Reviewing the game…",
  buildingSnapshot: "Saving the review…",
} satisfies Record<ImportProgressStage, string>

const reviewSessionProgressLabels = {
  resolvingMoment: "Opening the moment…",
  buildingPosition: "Setting up the position…",
  preparingEvidence: "Checking the engine lines…",
} satisfies Record<ReviewSessionProgressStage, string>

const reviewMomentPreparationProgressLabels = {
  waitingForCapacity: "Queued…",
  preparingAuthoringContext: "Gathering what the coach needs…",
  committingAuthoringContext: "Finishing up…",
} satisfies Record<ReviewMomentPreparationProgressStage, string>

/** Shared with the Coaching Board, so both boards name the same wait the same
 * way rather than keeping two copies of this prose. */
export const alternativeProgressLabels = {
  validatingMove: "Checking the move…",
  waitingForStockfish: "Waiting for the engine…",
  evaluatingMove: "The engine is evaluating…",
  committingMove: "Saving the line…",
} satisfies Record<AlternativeMoveProgressStage, string>

const coachProgressLabels = {
  queued: "Queued…",
  inspectingPosition: "Reading the position…",
  projectingIntent: "Checking what players at your rating usually do…",
  analyzingRefutation: "Checking the strongest reply…",
  generatingResponse: "Coach is writing…",
  repairingResponse: "Coach is checking its work…",
  validatingResponse: "Checking the answer…",
} satisfies Record<CoachTurnProgressStage, string>

/** One phrase for every interactive-coaching unavailable path. */
export const INTERACTIVE_COACHING_UNAVAILABLE =
  "The coach can’t answer right now. Your review is safe, and you can still try moves against the engine."

function unavailableMessage(
  event: Extract<ReviewSessionEvent, { kind: "unavailable" }>,
): string {
  return unavailableReasonMessage(event.reason)
}

export function unavailableReasonMessage(
  reason: ProviderUnavailableReason,
): string {
  switch (reason.kind) {
    case "languageLayer":
    case "queueDeadline":
      return INTERACTIVE_COACHING_UNAVAILABLE
    case "rateLimited":
      return `That was too quick. Try again in ${reason.retryAfterSeconds} seconds.`
    case "timeout":
      return timeoutMessage(reason.provider)
    case "admissionLimit":
      return "The coach is busy right now. You can still try moves against the engine — retry in a moment."
    case "coachEngineTransport":
      return "The review is temporarily unavailable. Nothing was lost — try again."
    case "maiaTransport":
      return "The most common choices at your rating are unavailable. Nothing changed."
    case "lichessTransport":
    case "chessComTransport":
    case "stockfishProcess":
    case "persistence":
      return `${providerLabel(reason.kind)} is unavailable. Nothing changed.`
    default: {
      const _exhaustive: never = reason
      return _exhaustive
    }
  }
}

function timeoutMessage(provider: ProviderKind): string {
  switch (provider) {
    case "languageLayer":
      return INTERACTIVE_COACHING_UNAVAILABLE
    case "maia":
      return "Looking up the most common choices at your rating took too long. Nothing changed."
    case "lichess":
    case "chessCom":
    case "stockfish":
      return `${providerLabel(provider)} took too long. Nothing changed.`
    default: {
      const _exhaustive: never = provider
      return _exhaustive
    }
  }
}

type PlayerVisibleProviderLabel =
  | Exclude<ProviderKind, "maia" | "languageLayer">
  | "stockfishProcess"
  | "lichessTransport"
  | "chessComTransport"
  | "persistence"

function providerLabel(provider: PlayerVisibleProviderLabel): string {
  return providerLabels[provider]
}

const providerLabels = {
  stockfish: "Stockfish",
  lichess: "Lichess",
  chessCom: "Chess.com",
  stockfishProcess: "Stockfish",
  lichessTransport: "Lichess",
  chessComTransport: "Chess.com",
  persistence: "Your saved review",
} satisfies Record<PlayerVisibleProviderLabel, string>
