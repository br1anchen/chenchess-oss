/**
 * The engine-vs-human comparison arrows for one Critical Moment.
 *
 * The engine's best move and the Maia most-likely move often coincide, so the
 * two sources merge into one arrow per from/to pair, the label naming every
 * source it stands for. Both the widget board and the web Review Session board
 * draw these; the merge lives here so they cannot disagree.
 */
import {
  fromSquare,
  type GameReviewCriticalMoment,
  type ReviewSessionPresentationArrow,
  type Square,
} from "@chenchess/coach-engine-sdk"

export type ComparisonBoardArrow = {
  from: Square
  label: string
  to: Square
  tone: "engine" | "peer"
}

type ComparisonArrowKind = "engineBest" | "maia"

type ComparisonArrowSource = {
  from: Square
  kind: ComparisonArrowKind
  legacyKinds?: ComparisonArrowKind[]
  to: Square
}

export function criticalMomentComparisonArrows(
  moment: GameReviewCriticalMoment,
  elo: number | undefined,
): ComparisonBoardArrow[] {
  return mergeComparisonArrows(
    [
      moveArrowSource(moment.objective.bestMoveUci, "engineBest"),
      moveArrowSource(moment.human.mostLikelyMoveUci, "maia"),
    ].flatMap((source) => (source ? [source] : [])),
    elo,
  )
}

/**
 * One engine move, as the arrow a board draws for it.
 *
 * A board showing a single engine line has nothing to merge against, but the
 * move still has to survive the same shape check every other arrow passes: a
 * uci that is not a move draws nothing rather than a square pair sliced out
 * of it.
 */
export function engineMoveArrow(
  uci: string | undefined,
): ComparisonBoardArrow | undefined {
  const source = moveArrowSource(uci, "engineBest")
  if (!source) return undefined
  return { from: source.from, label: "Engine", to: source.to, tone: "engine" }
}

export function presentationComparisonArrows(
  arrows: readonly ReviewSessionPresentationArrow[],
  elo: number | undefined,
): ComparisonBoardArrow[] {
  return mergeComparisonArrows(
    arrows.flatMap((arrow) => {
      const source = presentationArrowSource(arrow)
      return source ? [source] : []
    }),
    elo,
  )
}

function mergeComparisonArrows(
  sources: readonly ComparisonArrowSource[],
  elo: number | undefined,
): ComparisonBoardArrow[] {
  const groups = new Map<
    string,
    {
      from: Square
      kinds: Set<ComparisonArrowKind>
      to: Square
    }
  >()
  for (const source of sources) {
    const key = `${source.from}:${source.to}`
    const group = groups.get(key) ?? {
      from: source.from,
      kinds: new Set<ComparisonArrowKind>(),
      to: source.to,
    }
    group.kinds.add(source.kind)
    for (const kind of source.legacyKinds ?? []) group.kinds.add(kind)
    groups.set(key, group)
  }
  return [...groups.values()].map(({ from, kinds, to }) => {
    const orderedKinds = (["engineBest", "maia"] as const).filter((kind) =>
      kinds.has(kind),
    )
    return {
      tone: kinds.has("engineBest") ? ("engine" as const) : ("peer" as const),
      from,
      label: orderedKinds
        .map((kind) =>
          kind === "engineBest" ? "Engine" : `Elo ${elo ?? "matched"} player`,
        )
        .join(" + "),
      to,
    }
  })
}

function moveArrowSource(
  uci: string | undefined,
  kind: ComparisonArrowKind,
): ComparisonArrowSource | undefined {
  if (!uci || !/^[a-h][1-8][a-h][1-8][qrbn]?$/.test(uci)) return undefined
  return {
    from: fromSquare(uci.slice(0, 2)),
    kind,
    to: fromSquare(uci.slice(2, 4)),
  }
}

function presentationArrowSource(
  arrow: ReviewSessionPresentationArrow,
): ComparisonArrowSource | undefined {
  if (arrow.kind === "bestReply") return undefined
  const legacyKinds: ComparisonArrowKind[] = []
  const legacyLabel = arrow.label.toLowerCase()
  if (legacyLabel.includes("engine")) legacyKinds.push("engineBest")
  if (legacyLabel.includes("maia")) legacyKinds.push("maia")
  return {
    from: arrow.from,
    kind: arrow.kind === "maia" ? "maia" : "engineBest",
    legacyKinds,
    to: arrow.to,
  }
}
