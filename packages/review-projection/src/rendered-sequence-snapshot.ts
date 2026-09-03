import {
  decodeMoveSequenceSnapshot,
  type MoveSequenceSnapshot,
} from "@chenchess/coach-engine-sdk"

/**
 * One evaluated Player Line played out by the same app that renders canonical
 * Move Sequences.
 *
 * The UCI path is part of the address check, not another evaluation payload.
 * Everything else is the existing board-and-notation snapshot contract.
 */
export type PlayerLineSequenceSnapshot = Omit<MoveSequenceSnapshot, "kind"> & {
  kind: "playerLine"
  uci: string[]
}

export type RenderedSequenceSnapshot =
  | MoveSequenceSnapshot
  | PlayerLineSequenceSnapshot

export const canonicalUciPattern = /^[a-h][1-8][a-h][1-8][qrbn]?$/

/**
 * Decode both generations of the shared renderer without widening the Coach
 * Engine's generated canonical-line contract.
 */
export function decodeRenderedSequenceSnapshot(
  value: unknown,
): RenderedSequenceSnapshot {
  return hasPlayerLineKind(value)
    ? decodePlayerLineSequenceSnapshot(value)
    : decodeMoveSequenceSnapshot(value)
}

export function decodePlayerLineSequenceSnapshot(
  value: unknown,
): PlayerLineSequenceSnapshot {
  if (!hasPlayerLineKind(value)) {
    throw new Error("Player Line snapshot kind is invalid")
  }
  if (!("uci" in value) || !isUciLine(value.uci)) {
    throw new Error("Player Line snapshot UCI path is invalid")
  }
  const { uci, ...snapshot } = value
  const canonical = decodeMoveSequenceSnapshot({
    ...snapshot,
    kind: "engineBest",
  })
  if (uci.length !== canonical.moves.length) {
    throw new Error("Player Line snapshot address and moves disagree")
  }
  return { ...canonical, kind: "playerLine", uci: [...uci] }
}

function hasPlayerLineKind(value: unknown): value is { kind: "playerLine" } {
  return (
    typeof value === "object" &&
    value !== null &&
    "kind" in value &&
    value.kind === "playerLine"
  )
}

export function isUciLine(value: unknown): value is string[] {
  return (
    Array.isArray(value) &&
    value.length >= 1 &&
    value.length <= 12 &&
    value.every(
      (move): move is string =>
        typeof move === "string" && canonicalUciPattern.test(move),
    )
  )
}
