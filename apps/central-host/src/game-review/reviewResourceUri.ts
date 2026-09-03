import {
  type AddressedGameReview,
  decodeAddressSegments,
  isCriticalMomentId,
  isGameImportId,
  isSequenceKind,
} from "./reviewAddress"

/**
 * Which review resource a widget just asked for.
 *
 * The widget mount asks its MCP host for these URIs and the web mount asks the
 * Coach Engine, so the web bridge has to read the same addresses the widget
 * writes rather than take an address by side channel. Reading them here is what
 * keeps "only the fetch differs" true: the widget is unchanged and unaware of
 * which surface answered.
 *
 * Only the two resources a widget renders from are addressable. The proof
 * resource is audit-only and the Review Moment detail resource is the host
 * model's grounded read; neither renders, so neither has a fetch here.
 */
export type ReviewResourceAddress = Extract<
  AddressedGameReview,
  { kind: "gameReview" | "moveSequence" }
>

const reviewResourceUriPrefix = "chenchess://game-review/"

export function parseReviewResourceUri(
  uri: string,
): ReviewResourceAddress | undefined {
  if (!uri.startsWith(reviewResourceUriPrefix)) return undefined
  const segments = decodeAddressSegments(
    uri.slice(reviewResourceUriPrefix.length),
  )
  if (!segments) return undefined
  const [gameImportId, moment, reviewMomentId, sequence, sequenceKind] =
    segments
  if (gameImportId === undefined || !isGameImportId(gameImportId)) {
    return undefined
  }
  if (segments.length === 1) {
    return { gameImportId, kind: "gameReview" }
  }
  return segments.length === 5 &&
    moment === "moment" &&
    sequence === "sequence" &&
    reviewMomentId !== undefined &&
    isCriticalMomentId(reviewMomentId) &&
    isSequenceKind(sequenceKind)
    ? {
        gameImportId,
        kind: "moveSequence",
        reviewMomentId,
        sequenceKind,
      }
    : undefined
}
