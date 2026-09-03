import type {
  CriticalMomentId,
  GameImportId,
  MoveSequencePresentationKind,
} from "@chenchess/coach-engine-sdk"

import {
  type AddressedGameReview,
  decodeAddressSegments,
  isCriticalMomentId,
  isGameImportId,
  isSequenceKind,
} from "./reviewAddress"

/**
 * Where a Player is inside their own reviews, written as a URL.
 *
 * The Game Review is canonical and everything else hangs off it, mirroring the
 * resource addresses the widget reads: a review, a moment of that review, and
 * one canonical continuation of that moment. There is no route for a Review
 * Session because there is no Review Session — a Player's address for their
 * study is the Game Import they own.
 *
 * The address is an identifier, never a bearer capability. Every route resolves
 * behind the same sign-in and beta-access gate, and the Game Import's owner
 * subtree is the Engine's authorization boundary, so pasting one of these into a
 * transcript hands over nothing.
 */
export type GameReviewRoute =
  | { kind: "none" }
  | { kind: "invalid" }
  | AddressedGameReview

const gameReviewRoutePrefix = "/app/game-reviews/"

export function parseGameReviewRoute(pathname: string): GameReviewRoute {
  if (!pathname.startsWith(gameReviewRoutePrefix)) return { kind: "none" }
  const segments = decodeAddressSegments(
    pathname.slice(gameReviewRoutePrefix.length),
  )
  if (!segments) return { kind: "invalid" }
  const [gameImportId, moments, reviewMomentId, sequences, sequenceKind] =
    segments
  if (gameImportId === undefined || !isGameImportId(gameImportId)) {
    return { kind: "invalid" }
  }
  if (segments.length === 1) {
    return { gameImportId, kind: "gameReview" }
  }
  if (
    moments !== "moments" ||
    reviewMomentId === undefined ||
    !isCriticalMomentId(reviewMomentId)
  ) {
    return { kind: "invalid" }
  }
  const addressed = {
    gameImportId,
    reviewMomentId,
  }
  if (segments.length === 3) return { ...addressed, kind: "reviewMoment" }
  return segments.length === 5 &&
    sequences === "sequences" &&
    isSequenceKind(sequenceKind)
    ? { ...addressed, kind: "moveSequence", sequenceKind }
    : { kind: "invalid" }
}

export function gameReviewPath(gameImportId: GameImportId) {
  return `${gameReviewRoutePrefix}${encodeURIComponent(gameImportId)}`
}

export function reviewMomentPath(
  gameImportId: GameImportId,
  reviewMomentId: CriticalMomentId,
) {
  return `${gameReviewPath(gameImportId)}/moments/${encodeURIComponent(reviewMomentId)}`
}

export function moveSequencePath(
  gameImportId: GameImportId,
  reviewMomentId: CriticalMomentId,
  sequenceKind: MoveSequencePresentationKind,
) {
  return `${reviewMomentPath(gameImportId, reviewMomentId)}/sequences/${encodeURIComponent(sequenceKind)}`
}

/** The path any addressed route is written to, and the one the browser shows. */
export function addressedGameReviewPath(route: AddressedGameReview) {
  switch (route.kind) {
    case "gameReview":
      return gameReviewPath(route.gameImportId)
    case "reviewMoment":
      return reviewMomentPath(route.gameImportId, route.reviewMomentId)
    case "moveSequence":
      return moveSequencePath(
        route.gameImportId,
        route.reviewMomentId,
        route.sequenceKind,
      )
  }
}

export function replaceGameReviewPath(
  gameImportId: GameImportId,
  history: Pick<History, "replaceState"> = window.history,
) {
  try {
    history.replaceState(null, "", gameReviewPath(gameImportId))
  } catch {
    // A host-controlled browser may deny history updates; the review remains usable.
  }
}

/**
 * Which ply the board is standing on, read from the address.
 *
 * The ply is a query parameter rather than a path segment because it is not a
 * different resource: `AddressedGameReview` names what the widget reads, and
 * the same three handles are written into resource URIs and the sign-in query.
 * Where the board is standing inside that resource is presentation, and putting
 * it in the path would push presentation into an address the Coach Engine
 * answers.
 *
 * It lives in the address at all so that a reload restores the board and a
 * copied link shows the recipient what the sender was looking at. Remembering
 * it per Player instead would make one URL mean two things.
 */
export const VIEWED_PLY_PARAMETER = "ply"

export function parseViewedPly(search: string): number | null {
  const raw = new URLSearchParams(search).get(VIEWED_PLY_PARAMETER)
  if (raw === null) return null
  const ply = Number(raw)
  // A ply is a positive whole number. Anything else is a hand-edited address,
  // and falling back to the moment's own ply is better than an empty board.
  return Number.isSafeInteger(ply) && ply > 0 ? ply : null
}

/**
 * Writes the viewed ply into the address without growing the history stack.
 *
 * Walking a line is not navigation the Back button should replay one step at a
 * time, so this replaces rather than pushes.
 */
export function replaceViewedPly(
  ply: number | null,
  location: Pick<Location, "pathname" | "search"> = window.location,
  history: Pick<History, "replaceState"> = window.history,
) {
  const parameters = new URLSearchParams(location.search)
  if (ply === null) parameters.delete(VIEWED_PLY_PARAMETER)
  else parameters.set(VIEWED_PLY_PARAMETER, String(ply))
  const query = parameters.toString()
  try {
    history.replaceState(
      null,
      "",
      query ? `${location.pathname}?${query}` : location.pathname,
    )
  } catch {
    // A host-controlled browser may deny history updates; the board still moves.
  }
}
