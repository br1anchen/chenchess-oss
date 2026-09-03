import type { ReviewSessionPresentation } from "@chenchess/coach-engine-sdk"

/**
 * Version-skew insurance for a live Coach App card.
 *
 * The full presentation travels beside this in private redraw metadata, but it
 * is versioned. The Coach App resource is privately cached for five minutes, so
 * just after a deploy a cached older widget can receive a newer server's
 * presentation and fail to decode it. This digest is the version-stable subset
 * that still renders the chronological Critical Moment picker and its summary —
 * no boards, arrows, evaluations, evidence, or authoring context — carrying its
 * own independent version so an older reader can always decode it.
 *
 * It rides in `_meta`, never in `structuredContent`: the Language Layer must
 * not pay context bytes for graphical state.
 *
 * This does NOT survive a card being reopened, and was never able to. Measured
 * on ChatGPT 2026-08-01: a reopened card gets no persisted state at all — no
 * `toolOutput`, no `toolResponseMetadata`, no `window.openai` — and the MCP
 * Apps SDK exposes no host-backed widget-state store. Only the original tool
 * arguments arrive, via the host's `toolinput` notification. Restoring a
 * reopened card therefore always costs one Coach Engine round trip; putting
 * this digest in `structuredContent` instead would not change that, because
 * `toolOutput` does not survive either.
 */
export type ReviewSessionDigest = {
  digestVersion: 1
  eloRating: number
  maxPly: number
  moments: ReviewSessionDigestMoment[]
  orientation: "black" | "white"
  reviewSide: "black" | "both" | "white"
  selectedMomentId: string | null
  gameImportId: string
  sessionLabel: string
  source: string
  summaryText: string
}

export type ReviewSessionDigestMoment = {
  glyph: string
  kind: string
  momentId: string
  moveLabel: string
  ply: number
  summary: string
  title: string
  tone: string
}

export function projectReviewSessionDigest(
  presentation: ReviewSessionPresentation,
  summaryText: string,
): ReviewSessionDigest {
  return {
    digestVersion: 1,
    eloRating: presentation.eloRating,
    maxPly: presentation.maxPly,
    moments: presentation.moments.map((moment) => ({
      glyph: moment.glyph,
      kind: moment.kind,
      momentId: moment.momentId,
      moveLabel: moment.moveLabel,
      ply: moment.ply,
      summary: moment.summary,
      title: moment.title,
      tone: moment.tone,
    })),
    orientation: presentation.orientation,
    reviewSide: presentation.reviewSide,
    selectedMomentId: presentation.selectedMomentId,
    gameImportId: presentation.gameImportId,
    sessionLabel:
      presentation.opening.kind === "present"
        ? presentation.opening.name
        : "Review Session",
    source: presentation.source,
    summaryText,
  }
}
