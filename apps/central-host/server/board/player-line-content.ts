import type {
  CriticalMomentId,
  GameImportId,
  MoveInput,
} from "@chenchess/coach-engine-sdk"

export const playerLineToolName = "evaluate_player_line" as const

export type EvaluatedPlayerLinePly = {
  evaluation: {
    bestMove:
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
    bestMoveUci: string
    comparison:
      | { kind: "centipawns"; value: number }
      | {
          best:
            | { kind: "notForced" }
            | {
                distancePlies: number
                kind: "forced"
                outcome: "loss" | "win"
              }
          kind: "mate"
          selected:
            | { kind: "notForced" }
            | {
                distancePlies: number
                kind: "forced"
                outcome: "loss" | "win"
              }
        }
    selectedMove:
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
  }
  index: number
  move: { san: string; uci: string }
  mover: "black" | "white"
  source: "engine" | "player"
  strongestReply?: { kind: "offered"; uci: string }
}

export type PlayerLineRenderOption = {
  arguments: {
    gameImportId: string
    kind: "playerLine"
    moves: string[]
    reviewMomentId: string
  }
  kind: "playerLine"
  moveCount: number
  name: "render_move_sequence"
  san: string[]
  title: "Player Line"
}

export type PlayerLineEvaluatedResult = {
  gameImportId: string
  kind: "playerLineEvaluated"
  plies: EvaluatedPlayerLinePly[]
  remainingAllowance: number
  renderOptions: PlayerLineRenderOption[]
  reviewMomentId: string
  verdict:
    | { kind: "completed" }
    | { index: number; kind: "illegalMove"; move: MoveInput }
    | {
        index: number
        kind: "deadlineReached" | "explorationExhausted" | "rateLimited"
      }
}

export type PlayerLineStructuredContent =
  | {
      operation: typeof playerLineToolName
      outcome: "completed"
      result: PlayerLineEvaluatedResult
      schemaVersion: 1
    }
  | {
      operation: typeof playerLineToolName
      outcome: "unavailable"
      reason: { kind: "playerLineUnavailable" }
      schemaVersion: 1
    }
  | {
      operation: typeof playerLineToolName
      outcome: "unavailable"
      reason: { kind: "rateLimited"; retryAfterSeconds: number }
      retry: { kind: "retryAfter"; seconds: number }
      schemaVersion: 1
    }

export type PlayerLineCompletedContent = Extract<
  PlayerLineStructuredContent,
  { outcome: "completed" }
>

export function evaluatedPlayerLineContent({
  gameImportId,
  plies,
  remainingAllowance,
  reviewMomentId,
}: {
  gameImportId: GameImportId
  plies: readonly EvaluatedPlayerLinePly[]
  remainingAllowance: number
  reviewMomentId: CriticalMomentId
}): PlayerLineCompletedContent {
  return completedPlayerLineContent({
    gameImportId,
    plies,
    remainingAllowance,
    reviewMomentId,
    verdict: { kind: "completed" },
  })
}

export function illegalMoveContent(
  gameImportId: GameImportId,
  reviewMomentId: CriticalMomentId,
  plies: readonly EvaluatedPlayerLinePly[],
  index: number,
  move: MoveInput,
  remainingAllowance: number,
): PlayerLineCompletedContent {
  return completedPlayerLineContent({
    gameImportId,
    plies,
    remainingAllowance,
    reviewMomentId,
    verdict: { index, kind: "illegalMove", move },
  })
}

export function interruptedPlayerLineContent({
  gameImportId,
  kind,
  plies,
  remainingAllowance,
  reviewMomentId,
}: {
  gameImportId: GameImportId
  kind: "deadlineReached" | "explorationExhausted" | "rateLimited"
  plies: readonly EvaluatedPlayerLinePly[]
  remainingAllowance: number
  reviewMomentId: CriticalMomentId
}): PlayerLineCompletedContent {
  return completedPlayerLineContent({
    gameImportId,
    plies,
    remainingAllowance,
    reviewMomentId,
    verdict: { index: plies.length, kind },
  })
}

export function playerLineUnavailableContent(): PlayerLineStructuredContent {
  return {
    operation: playerLineToolName,
    outcome: "unavailable",
    reason: { kind: "playerLineUnavailable" },
    schemaVersion: 1,
  }
}

export function rateLimitedPlayerLineContent(
  retryAfterSeconds: number,
): PlayerLineStructuredContent {
  return {
    operation: playerLineToolName,
    outcome: "unavailable",
    reason: { kind: "rateLimited", retryAfterSeconds },
    retry: { kind: "retryAfter", seconds: retryAfterSeconds },
    schemaVersion: 1,
  }
}

export function playerLineRenderOptions(
  gameImportId: GameImportId,
  reviewMomentId: CriticalMomentId,
  plies: readonly EvaluatedPlayerLinePly[],
): PlayerLineRenderOption[] {
  if (plies.length === 0) return []
  return [
    {
      arguments: {
        gameImportId,
        kind: "playerLine",
        moves: plies.map(({ move }) => move.uci),
        reviewMomentId,
      },
      kind: "playerLine",
      moveCount: plies.length,
      name: "render_move_sequence",
      san: plies.map(({ move }) => move.san),
      title: "Player Line",
    },
  ]
}

export function playerLineExplanation(
  plies: readonly EvaluatedPlayerLinePly[],
  options: readonly PlayerLineRenderOption[],
): string {
  if (plies.length === 0) return ""
  const lines = plies.map(
    (ply) =>
      `${ply.index + 1}. ${ply.move.san} [${ply.move.uci}] ${ply.mover} ${ply.source} — selected ${evaluationText(ply.evaluation.selectedMove)}; best ${ply.evaluation.bestMoveUci} ${evaluationText(ply.evaluation.bestMove)}; loss ${lossText(ply.evaluation.comparison)}${ply.strongestReply ? `; strongest reply ${ply.strongestReply.uci}` : ""}`,
  )
  const render = options[0]
    ? `\nTo show this line, call render_move_sequence with ${JSON.stringify(options[0].arguments)}.`
    : ""
  return `\nPlayer Line:\n${lines.join("\n")}${render}`
}

function completedPlayerLineContent({
  gameImportId,
  plies,
  remainingAllowance,
  reviewMomentId,
  verdict,
}: {
  gameImportId: GameImportId
  plies: readonly EvaluatedPlayerLinePly[]
  remainingAllowance: number
  reviewMomentId: CriticalMomentId
  verdict: PlayerLineEvaluatedResult["verdict"]
}): PlayerLineCompletedContent {
  return {
    operation: playerLineToolName,
    outcome: "completed",
    result: {
      gameImportId,
      kind: "playerLineEvaluated",
      plies: [...plies],
      remainingAllowance,
      renderOptions: playerLineRenderOptions(
        gameImportId,
        reviewMomentId,
        plies,
      ),
      reviewMomentId,
      verdict,
    },
    schemaVersion: 1,
  }
}

function evaluationText(
  evaluation: EvaluatedPlayerLinePly["evaluation"]["selectedMove"],
): string {
  return evaluation.kind === "centipawns"
    ? `${evaluation.value}cp for ${evaluation.perspective}`
    : `mate ${evaluation.outcome} in ${evaluation.distancePlies} plies for ${evaluation.perspective}`
}

function lossText(
  comparison: EvaluatedPlayerLinePly["evaluation"]["comparison"],
): string {
  if (comparison.kind === "centipawns") return `${comparison.value}cp`
  return `best ${mateText(comparison.best)}, selected ${mateText(comparison.selected)}`
}

function mateText(
  comparison: Extract<
    EvaluatedPlayerLinePly["evaluation"]["comparison"],
    { kind: "mate" }
  >["best"],
): string {
  return comparison.kind === "forced"
    ? `forced ${comparison.outcome} in ${comparison.distancePlies} plies`
    : "no forced mate"
}
