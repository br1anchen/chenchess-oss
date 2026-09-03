import {
  parseIsCriticalMomentId,
  parseIsGameImportId,
  type CriticalMomentId,
  type GameImportId,
  type MoveSequencePresentationKind,
} from "@chenchess/coach-engine-sdk"

/**
 * What a Player's study is addressed by, wherever the address is written down.
 *
 * The same three handles name a review, a moment of it, and one canonical
 * continuation of that moment — in a web route, in a resource URI, and in the
 * query that carries an address across sign-in. They are one type because they
 * are one address: a second declaration of it is a second thing to keep
 * agreeing with the first.
 */
export type AddressedGameReview =
  | { gameImportId: GameImportId; kind: "gameReview" }
  | {
      gameImportId: GameImportId
      kind: "reviewMoment"
      reviewMomentId: CriticalMomentId
    }
  | {
      gameImportId: GameImportId
      kind: "moveSequence"
      reviewMomentId: CriticalMomentId
      sequenceKind: MoveSequencePresentationKind
    }

/**
 * A handle as the Coach Engine mints them.
 *
 * Every address here arrives from somewhere the Player or a host controls — the
 * address bar, a sign-in query, a resource read across a module boundary — so a
 * segment the Engine could not have issued is refused before it is interpolated
 * into a path or a command rather than after.
 */
const enginePattern = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/

export function isEngineHandle(value: string) {
  return enginePattern.test(value)
}

export function isGameImportId(value: string): value is GameImportId {
  return parseIsGameImportId(value)
}

export function isCriticalMomentId(value: string): value is CriticalMomentId {
  return parseIsCriticalMomentId(value)
}

export function isSequenceKind(
  value: string | null | undefined,
): value is MoveSequencePresentationKind {
  return value === "engineBest" || value === "playedMoveRefutation"
}

/**
 * A slash-separated address, decoded, with every handle checked.
 *
 * Both the route and the resource URI are `{gameImportId}` followed by literal
 * and handle segments in alternation, so the literals — which each caller
 * compares for itself — are the odd positions and the handles are the even
 * ones.
 */
export function decodeAddressSegments(path: string): string[] | undefined {
  if (!path) return undefined
  try {
    const segments = path.split("/").map(decodeURIComponent)
    return segments.every((segment, index) => {
      if (index % 2 === 1) return true
      if (index === 0) return isGameImportId(segment)
      if (index === 2) return isCriticalMomentId(segment)
      return isEngineHandle(segment)
    })
      ? segments
      : undefined
  } catch {
    return undefined
  }
}
