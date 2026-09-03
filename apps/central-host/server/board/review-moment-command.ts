import {
  fromCriticalMomentId,
  fromGameImportId,
  type ReviewSessionCommand,
} from "@chenchess/coach-engine-sdk"

export type ReviewMomentReference =
  | { kind: "critical"; reviewMomentId: string }
  | { kind: "ply"; ply: number }
  | {
      afterReviewMomentId?: string
      classification?: "improvementOpportunity"
      kind: "next"
    }

/** The one open command, for the two tools that differ only in what mounts. */
export function openAddressedReviewMomentCommand(
  gameImportId: string,
  moment: ReviewMomentReference,
): Extract<ReviewSessionCommand, { kind: "openAddressedReviewMoment" }> {
  return {
    gameImportId: fromGameImportId(gameImportId),
    kind: "openAddressedReviewMoment",
    reference: reviewMomentReferenceCommand(moment),
  }
}

/**
 * Narrows a validated moment reference to the Coach Engine command shape.
 *
 * The two identifier-carrying kinds need their handles branded, and a `next`
 * with no anchor has to omit the field rather than send it undefined — the
 * command contract rejects unknown and null-valued fields. Written as one total
 * switch so a fourth kind added to the schema fails to compile here instead of
 * reaching the Engine as an unnarrowed passthrough.
 */
export function reviewMomentReferenceCommand(
  moment: ReviewMomentReference,
): Extract<
  ReviewSessionCommand,
  { kind: "openAddressedReviewMoment" }
>["reference"] {
  switch (moment.kind) {
    case "critical":
      return {
        kind: moment.kind,
        reviewMomentId: fromCriticalMomentId(moment.reviewMomentId),
      }
    case "ply":
      return { kind: moment.kind, ply: moment.ply }
    case "next": {
      const reference: Extract<
        ReviewSessionCommand,
        { kind: "openAddressedReviewMoment" }
      >["reference"] & { kind: "next" } = { kind: moment.kind }
      if (moment.afterReviewMomentId) {
        reference.afterReviewMomentId = fromCriticalMomentId(
          moment.afterReviewMomentId,
        )
      }
      if (moment.classification) {
        reference.classification = moment.classification
      }
      return reference
    }
    default: {
      const exhaustive: never = moment
      return exhaustive
    }
  }
}
