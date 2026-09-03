import type { GameImportId } from "@chenchess/coach-engine-sdk"

/**
 * Rejects a Game Import handle Coach Engine could not have minted.
 *
 * The model supplies `gameImportId` as free text — `handle` in the tool schemas
 * is `boundedText(256)` — so a fabricated or stale-namespace string reaches the
 * Engine, which rejects it before its first read. The typed answer for "this
 * Player owns no such review" is the one worth giving, and giving it here costs
 * no round trip.
 *
 * The check is deliberately structural rather than a copy of the Engine's
 * `game_import_review_key`. The Engine additionally requires the two segments to
 * be 64 and 32 lower-hex characters, and matching that here would mean
 * regenerating the shared contract fixture corpus, whose `game-import:fixture:1`
 * predates the production format and is asserted across the host tests. So a
 * handle with the right skeleton but wrong-length hex still reaches the Engine;
 * `gameImportIdForm` exists to describe the rejects we do catch, so a surviving
 * case arrives with a logged form instead of an anonymous category.
 */
export function isPlausibleGameImportId(
  gameImportId: string,
): gameImportId is GameImportId {
  const segments = gameImportId.split(":")
  return (
    segments.length === 3 &&
    segments[0] === "game-import" &&
    segments[1]!.length > 0 &&
    segments[2]!.length > 0
  )
}

/**
 * Describes a rejected handle without reproducing it.
 *
 * A rejected handle is whatever the model made up, so it may carry Player-typed
 * text and never belongs in a log line — unlike an ID the Engine minted, which
 * call metrics record by design. Segment count, prefix, lengths, and charset are
 * enough to tell a truncated handle from a fabricated one or from a different ID
 * namespace, which is exactly what was missing when a malformed handle first
 * surfaced as an anonymous persistence category.
 */
export function gameImportIdForm(gameImportId: string) {
  const segments = gameImportId.split(":")
  return {
    namespace: segments[0] === "game-import" ? "game-import" : "other",
    segmentCharsets: segments.map(charsetOf),
    segmentCount: segments.length,
    segmentLengths: segments.map((segment) => segment.length),
    totalLength: gameImportId.length,
  }
}

function charsetOf(segment: string) {
  if (segment.length === 0) return "empty"
  if (/^[0-9a-f]+$/.test(segment)) return "lowerHex"
  if (/^[0-9A-Za-z_-]+$/.test(segment)) return "base64url"
  return "other"
}
