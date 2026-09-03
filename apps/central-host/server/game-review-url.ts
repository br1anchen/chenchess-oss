import type { GameImportId } from "@chenchess/coach-engine-sdk"

/**
 * Where a Player opens this Game Review on the web.
 *
 * The Game Import ID is the review's whole address, so the same handle names it
 * in a tool call, in a resource URI, and in this URL. It is an identifier and
 * not a capability: opening it still requires the Player's own sign-in.
 */
export function authenticatedGameReviewUrl(
  origin: URL,
  gameImportId: GameImportId,
) {
  return new URL(
    `/app/game-reviews/${encodeURIComponent(gameImportId)}`,
    origin,
  ).toString()
}
