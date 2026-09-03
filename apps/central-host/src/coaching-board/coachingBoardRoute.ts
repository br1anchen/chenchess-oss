import type { GameImportId } from "@chenchess/coach-engine-sdk"

import { isGameImportId } from "@/game-review/reviewAddress"

import { parseOpeningLineRef, type OpeningLineRef } from "./openingLineRef"

/**
 * Where a Player is on the Coaching Board, written as a URL.
 *
 * Own path only. Review Session and Game Review addresses never rewrite here,
 * and these addresses never enter a live Review Session.
 */
export type CoachingBoardRoute =
  | { kind: "none" }
  | { kind: "invalid" }
  | { kind: "empty" }
  | { kind: "game"; gameImportId: GameImportId }
  | { kind: "opening"; openingLineRef: OpeningLineRef }

const coachingBoardPrefix = "/app/board"

export function parseCoachingBoardRoute(pathname: string): CoachingBoardRoute {
  if (
    pathname === coachingBoardPrefix ||
    pathname === `${coachingBoardPrefix}/`
  ) {
    return { kind: "empty" }
  }
  if (!pathname.startsWith(`${coachingBoardPrefix}/`)) return { kind: "none" }
  const rest = pathname.slice(`${coachingBoardPrefix}/`.length)
  let decoded: string
  try {
    decoded = decodeURIComponent(rest)
  } catch {
    return { kind: "invalid" }
  }
  const segments = decoded.split("/")
  if (segments[0] === "games" && segments.length === 2 && segments[1]) {
    return isGameImportId(segments[1])
      ? { kind: "game", gameImportId: segments[1] }
      : { kind: "invalid" }
  }
  if (segments[0] === "openings" && segments.length === 2 && segments[1]) {
    const openingLineRef = parseOpeningLineRef(segments[1])
    return openingLineRef
      ? { kind: "opening", openingLineRef }
      : { kind: "invalid" }
  }
  return { kind: "invalid" }
}

export function coachingBoardPath() {
  return coachingBoardPrefix
}

export function coachingBoardGamePath(gameImportId: GameImportId) {
  return `${coachingBoardPrefix}/games/${encodeURIComponent(gameImportId)}`
}

export function coachingBoardOpeningPath(openingLineRef: OpeningLineRef) {
  return `${coachingBoardPrefix}/openings/${encodeURIComponent(openingLineRef)}`
}
