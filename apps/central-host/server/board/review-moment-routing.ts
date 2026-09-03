import type { ReviewSessionMoment } from "@chenchess/coach-engine-sdk"

/**
 * The listed review as a routing table: which moments exist, and where.
 *
 * One projection for both the structured result and the text summary, because
 * the model reads them as one answer — two independently written copies of the
 * same list are two chances to disagree about which moments a review has.
 */
export function criticalMomentRouting(
  reviewMoments: readonly ReviewSessionMoment[],
) {
  return reviewMoments.map(({ reviewMoment }) => ({
    momentId: reviewMoment.momentId,
    ply: reviewMoment.ply,
  }))
}
