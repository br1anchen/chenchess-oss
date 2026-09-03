export type EvaluationPointPresentation = {
  label: string
  ply: number
  value: number
}

export type EngineEvaluationPresentation =
  | {
      kind: "centipawns"
      perspective: "black" | "white"
      value: number
    }
  | {
      distancePlies: number
      kind: "mate"
      outcome: "loss" | "win"
      perspective: "black" | "white"
    }

export type ReviewMomentMarkerPresentation = {
  glyph: string
  label: string
  moveLabel: string
  ply: number
  summary?: string
  tone: "improvement" | "positive" | "selected"
  /** Frozen automatic set only. Nominated extras stay in the list, not the x/N. */
  countsInTotal?: boolean
}

export function reviewMomentCountLabel(
  moments: readonly ReviewMomentMarkerPresentation[],
  activePly: number,
) {
  const totalMoments = moments.filter(
    (moment) => moment.countsInTotal !== false,
  )
  const totalCount = totalMoments.length
  const countedIndex = totalMoments.findIndex(
    (moment) => moment.ply === activePly,
  )
  return countedIndex >= 0
    ? `${countedIndex + 1}/${totalCount}`
    : `+/${totalCount}`
}

export type ReviewContextNavigationProps = {
  activePly: number
  disabled: boolean
  moments: readonly ReviewMomentMarkerPresentation[]
  onSelect: (ply: number) => void
}

/**
 * White's share of an evaluation bar, clamped so the losing side keeps a
 * sliver. The canonical version: the Review Session board, the landing board
 * and the chat-app widget all read it, and a mate has to reach the end of the
 * bar rather than the middle.
 */
export function whiteEvaluationShare(
  evaluation: EngineEvaluationPresentation | null,
): number {
  if (!evaluation) return 50
  if (evaluation.kind === "mate") {
    const perspectiveWins = evaluation.outcome === "win"
    const whiteWins =
      evaluation.perspective === "white" ? perspectiveWins : !perspectiveWins
    return whiteWins ? 96 : 4
  }
  const whiteValue =
    evaluation.perspective === "white" ? evaluation.value : -evaluation.value
  return Math.max(4, Math.min(96, 50 + whiteValue / 12))
}

export function evaluationPointPresentation(
  ply: number,
  evaluation: EngineEvaluationPresentation,
): EvaluationPointPresentation {
  if (evaluation.kind === "centipawns") {
    const value =
      evaluation.perspective === "white" ? evaluation.value : -evaluation.value
    return {
      label: formatEvaluation(evaluation),
      ply,
      value: Math.max(-600, Math.min(600, value)),
    }
  }
  const perspectiveValue = evaluation.outcome === "win" ? 600 : -600
  return {
    label: formatEvaluation(evaluation),
    ply,
    value:
      evaluation.perspective === "white" ? perspectiveValue : -perspectiveValue,
  }
}

export function formatEvaluation(
  evaluation: EngineEvaluationPresentation | null,
) {
  if (!evaluation) return "—"
  if (evaluation.kind === "mate") {
    const sign = evaluation.outcome === "win" ? "" : "−"
    return `${sign}M${Math.ceil(evaluation.distancePlies / 2)}`
  }
  const value = evaluation.value / 100
  return `${value >= 0 ? "+" : "−"}${Math.abs(value).toFixed(2)}`
}

export function evaluationAt(
  points: readonly EvaluationPointPresentation[],
  ply: number,
) {
  return (
    points.find((point) => point.ply === ply) ??
    [...points].reverse().find((point) => point.ply <= ply)
  )
}
