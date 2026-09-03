import type {
  HostTurnPriorTurn,
  HostTurnRefusalReason,
  HostTurnShowLine,
  HostTurnStepLabel,
  ProviderUnavailableReason,
  RejectionRecovery,
  ReviewSessionLimits,
} from "@chenchess/coach-engine-sdk"
import { sharedLimits } from "@chenchess/shared-assets"

/**
 * D3: last completed HostTurns the web composer may resend as prose.
 *
 * Gated to `ReviewSessionLimits::V1` via `@chenchess/shared-assets` limits.
 */
export const HOST_TURN_MAX_PRIOR_TURNS: ReviewSessionLimits["maxHostTurnPriorTurns"] =
  sharedLimits.hostTurnMaxPriorTurns

/** D9: Player-facing HostTurn progress. Never a capability name. */
export const hostTurnStepLabels = {
  lookingAtAnotherMoment: "Looking at another moment…",
  checkingThatLine: "Checking that line…",
  writing: "Writing…",
} as const satisfies Record<HostTurnStepLabel, string>

export type HostTurnStepDisplayLabel =
  (typeof hostTurnStepLabels)[HostTurnStepLabel]

export type HostTurnEffects = {
  /** Ply (half-move index) to open. Not a CriticalMomentId. */
  focusMoment?: number | null
  showLine?: HostTurnShowLine | null
}

export type ThreadItem =
  | {
      kind: "playerMessage"
      id: string
      text: string
    }
  | {
      kind: "coachAnswer"
      id: string
      answer: string
      effects: HostTurnEffects
    }
  | {
      kind: "unavailable"
      id: string
      reason: ProviderUnavailableReason
    }
  | {
      kind: "refusal"
      id: string
      reason: HostTurnRefusalReason
    }
  | {
      kind: "rejected"
      id: string
      recovery: RejectionRecovery
    }

/** Stockfish exploration notes sit beside HostTurn items. Not a #433 kind. */
export type WorkspaceThreadItem =
  | ThreadItem
  | {
      kind: "systemNote"
      id: string
      text: string
    }

type DistributiveOmit<T, K extends PropertyKey> = T extends unknown
  ? Omit<T, K>
  : never

/** Thread item before the workspace assigns an id. */
export type WorkspaceThreadDraft = DistributiveOmit<WorkspaceThreadItem, "id">

export const hostTurnRefusalText = {
  notAboutThisReview:
    "I can only talk about this reviewed game and the moments on the board. Ask about a move, a moment, or a line from this review.",
  notAboutChess: "I can only help with this chess review.",
  unsafeRequest: "I cannot help with that request.",
} as const satisfies Record<HostTurnRefusalReason, string>

/**
 * Matches `ReviewSessionLimits::V1.max_player_message_bytes` /
 * `gate_player_message`. The step schema caps a new answer at 2000
 * characters; UTF-8 can still exceed the byte gate.
 */
export const HOST_TURN_MAX_PLAYER_MESSAGE_BYTES = 4096

export function priorHostTurns(
  items: readonly WorkspaceThreadItem[],
): HostTurnPriorTurn[] {
  const pairs: HostTurnPriorTurn[] = []
  const hostTurnItems = items.filter((item) => item.kind !== "systemNote")
  for (let index = 0; index < hostTurnItems.length; index += 1) {
    const item = hostTurnItems[index]
    const next = hostTurnItems[index + 1]
    if (item?.kind === "playerMessage" && next?.kind === "coachAnswer") {
      if (priorTurnFitsPlayerMessageGate(item.text, next.answer)) {
        pairs.push({ message: item.text, answer: next.answer })
      }
      index += 1
    }
  }
  return pairs.slice(-HOST_TURN_MAX_PRIOR_TURNS)
}

function priorTurnFitsPlayerMessageGate(
  message: string,
  answer: string,
): boolean {
  return playerMessageFitsGate(message) && playerMessageFitsGate(answer)
}

function playerMessageFitsGate(text: string): boolean {
  if (text.trim() === "") return false
  if (utf8ByteLength(text) > HOST_TURN_MAX_PLAYER_MESSAGE_BYTES) return false
  for (const character of text) {
    if (disallowedControlCharacter(character)) return false
  }
  return true
}

function utf8ByteLength(text: string): number {
  return new TextEncoder().encode(text).length
}

function disallowedControlCharacter(character: string): boolean {
  if (character === "\n" || character === "\r" || character === "\t") {
    return false
  }
  const code = character.codePointAt(0)
  return code !== undefined && (code <= 0x1f || (code >= 0x7f && code <= 0x9f))
}

export function shownLineLabel(showLine: HostTurnShowLine): string {
  switch (showLine.kind) {
    case "engineBest":
      return "Engine line"
    case "playedMoveRefutation":
      return "Played refutation"
    case "alternativeMove":
      return "Alternative branch"
    default: {
      const _exhaustive: never = showLine
      return _exhaustive
    }
  }
}

export type HostTurnProgress = {
  label: HostTurnStepDisplayLabel
}

export type ComposerState =
  | {
      kind: "idle"
      draft: string
    }
  | {
      kind: "hostTurn"
      draft: string
      progress: HostTurnProgress
    }

export function hostTurnStepDisplayLabel(
  label: HostTurnStepLabel,
): HostTurnStepDisplayLabel {
  return hostTurnStepLabels[label]
}
