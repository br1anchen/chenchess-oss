import { readJsonObject } from "@chenchess/coach-engine-sdk"

export type CoachToolTarget = "app" | "model" | "web"

/**
 * The one declaration of every Coach tool and who may call it.
 *
 * Registration reads this map through `coachToolMeta`, the `CoachToolName`
 * union is its keys, and the model/app name lists below derive from it, so a
 * tool's visibility is written exactly once. Map order is registration order;
 * the deployed conformance gate pins the model-visible projection of it
 * against a live host with an independently written list.
 *
 * `["model", "app"]` is the Language Layer surface: Digest reads addressed by
 * an optional permanent Digest ID, Game Review tools addressed by the
 * permanent Game Import ID, plus the one profile connection the Player asks
 * for by name. No tool relies on session state or an expiring continuation
 * handle; Player Line evaluation performs bounded, idempotent computation at
 * one Review Moment.
 *
 * `["app"]` tools each take a handle no model-visible tool returns — a Review
 * Session reference, an authoring handoff's ledger, or a mounted frame — so
 * the app is the only caller that can name their arguments.
 */
export const coachToolNames = [
  "get_coaching_digest",
  "search_reviewed_games",
  "connect_playing_profile",
  "review_game",
  "list_critical_moments",
  "open_review_moment",
  "evaluate_player_line",
  "open_review_moment_in_place",
  "record_learning_path_exposure",
  "update_learning_path_vote",
  "publish_review_moment_comment",
  "inspect_position",
  "explore_alternative_move",
  "cancel_operation",
  "render_move_sequence",
  "report_app_performance",
  "read_game_review_snapshot",
  "read_move_sequence_snapshot",
  "read_coaching_board",
  "show_line",
  "step_line",
  "set_board_position",
  "annotate_board",
  "evaluate_opening_continuation",
  "list_recent_profile_games",
  "stage_game_import",
  "find_opening_line",
  "list_played_openings",
  "open_opening_line",
  "open_reviewed_game",
  "read_session_status",
] as const

export type CoachToolName = (typeof coachToolNames)[number]

export const coachToolSurface = {
  get_coaching_digest: ["model", "app"],
  search_reviewed_games: ["model", "app", "web"],
  connect_playing_profile: ["model", "app"],
  review_game: ["model", "app"],
  list_critical_moments: ["model", "app", "web"],
  open_review_moment: ["model", "app"],
  evaluate_player_line: ["model", "app", "web"],
  // The mounted selector switching moments inside its own frame (#326). Kept
  // off the model surface deliberately: the model asking for a moment wants a
  // card, and this is the one open that mounts nothing. The web board is
  // already showing the review, so in-place open is the switch that belongs
  // there.
  open_review_moment_in_place: ["app", "web"],
  record_learning_path_exposure: ["app"],
  update_learning_path_vote: ["app"],
  publish_review_moment_comment: ["app"],
  inspect_position: ["app"],
  explore_alternative_move: ["app"],
  cancel_operation: ["app"],
  render_move_sequence: ["model", "app"],
  report_app_performance: ["app"],
  read_game_review_snapshot: ["app"],
  read_move_sequence_snapshot: ["app"],
  read_coaching_board: ["web"],
  show_line: ["web"],
  // Walking a line already shown. Web-only for the same reason the
  // marks are: it moves a board, and no MCP surface has one.
  step_line: ["web"],
  set_board_position: ["web"],
  // Verify-then-draw, the sibling of evaluate-then-show (ADR 0059).
  // Web-only because the marks live on the page that renders them:
  // no MCP surface has a board to point at.
  annotate_board: ["web"],
  // The opening board's evaluate-then-show gate. Web-only because it is keyed
  // by an Opening Line, which no MCP surface addresses: the model list keeps
  // evaluate_player_line, keyed by a Game Import and a Review Moment.
  evaluate_opening_continuation: ["web"],
  list_recent_profile_games: ["web"],
  stage_game_import: ["web"],
  find_opening_line: ["web"],
  list_played_openings: ["web"],
  open_opening_line: ["web"],
  // Navigation to a Game the Player already reviewed: the same consent class
  // as an Opening Line — it creates nothing and discloses nothing.
  open_reviewed_game: ["web"],
  read_session_status: ["web"],
} as const satisfies Record<CoachToolName, readonly CoachToolTarget[]>

export type CoachWebToolKind = "board" | "lobby" | "session"

/** The tools coachToolSurface marks web-visible. Derived — never authored. */
type CoachWebToolName = {
  [K in CoachToolName]: "web" extends (typeof coachToolSurface)[K][number]
    ? K
    : never
}[CoachToolName]

export const coachWebToolKind = {
  search_reviewed_games: "lobby",
  list_critical_moments: "board",
  evaluate_player_line: "board",
  open_review_moment_in_place: "board",
  read_coaching_board: "board",
  show_line: "board",
  step_line: "board",
  set_board_position: "board",
  annotate_board: "board",
  evaluate_opening_continuation: "board",
  list_recent_profile_games: "lobby",
  stage_game_import: "lobby",
  find_opening_line: "lobby",
  list_played_openings: "lobby",
  open_opening_line: "lobby",
  open_reviewed_game: "lobby",
  read_session_status: "session",
} as const satisfies Record<CoachWebToolName, CoachWebToolKind>

const coachToolNamesByTarget = (target: CoachToolTarget) =>
  coachToolNames.filter((name) =>
    coachToolSurface[name].some((candidate) => candidate === target),
  )

/** Model-visible tool names, in registration order. Derived — never edit. */
export const contractedCoachModelToolNames = coachToolNamesByTarget("model")

/**
 * App-only tool names, in registration order. Derived from the app target —
 * never from the absence of the model target, or a web-only tool would land
 * in the MCP app list.
 */
export const coachAppOnlyToolNames = coachToolNames.filter((name) => {
  const targets: readonly CoachToolTarget[] = coachToolSurface[name]
  return targets.includes("app") && !targets.includes("model")
})

/** Web-visible tool names, in authored map order. Derived — never edit. */
export const coachWebToolNames = coachToolNamesByTarget("web")

export const coachWebBoardToolNames = coachWebToolNames.filter(
  (name) => webToolKind(name) === "board",
)

export const coachWebLobbyToolNames = coachWebToolNames.filter(
  (name) => webToolKind(name) === "lobby",
)

export const coachWebSessionToolNames = coachWebToolNames.filter(
  (name) => webToolKind(name) === "session",
)

function webToolKind(name: CoachToolName): CoachWebToolKind | null {
  // SAFETY: the `in` check just established the name is one of the map's
  // keys.
  return name in coachWebToolKind
    ? coachWebToolKind[name as keyof typeof coachWebToolKind]
    : null
}

type ListedCoachTool = {
  _meta?: unknown
  description?: string
  inputSchema: unknown
  name: string
  outputSchema?: unknown
}

export type CoachCallerKind = "app" | "model" | "server-compound"
export type CoachModelContextCallerKind = Exclude<
  CoachCallerKind,
  "server-compound"
>

export type CoachModelContextResultMeasurement = {
  callerKind: CoachModelContextCallerKind
  contentTextBytes: number
  structuredContentBytes: number
}

export const coachToolVisibility = parseCoachToolVisibility

export function parseCoachToolVisibility(tool: {
  _meta?: unknown
}): CoachToolTarget[] {
  const meta = readJsonObject(tool._meta)
  const ui = readJsonObject(meta?.ui)
  if (!Array.isArray(ui?.visibility)) return []
  return ui.visibility.filter(
    (target): target is CoachToolTarget =>
      target === "app" || target === "model" || target === "web",
  )
}

export function modelVisibleCoachTools<T extends ListedCoachTool>(
  tools: T[],
): T[] {
  return tools.filter((tool) =>
    parseCoachToolVisibility(tool).includes("model"),
  )
}

/**
 * Model-context cost of the advertised tool surface.
 *
 * Description and schema byte counts are telemetry, not contracts. Read them
 * to spot large catalog growth; do not pin them in tests. Tool names and count
 * define the model-visible contract.
 */
export function measureModelToolSurface(tools: ListedCoachTool[]) {
  const modelTools = modelVisibleCoachTools(tools)
  const inputSchemaBytes = modelTools.reduce(
    (total, tool) => total + parseByteLength(tool.inputSchema),
    0,
  )
  const outputSchemaBytes = modelTools.reduce(
    (total, tool) => total + parseByteLength(tool.outputSchema),
    0,
  )
  return {
    descriptionBytes: modelTools.reduce(
      (total, tool) => total + utf8ByteLength(tool.description ?? ""),
      0,
    ),
    inputSchemaBytes,
    names: modelTools.map(({ name }) => name),
    outputSchemaBytes,
    schemaBytes: inputSchemaBytes + outputSchemaBytes,
    toolCount: modelTools.length,
  }
}

/**
 * Model-context cost of one conversation over the advertised surface.
 *
 * Byte counts are telemetry, not contracts. The model total charges the static
 * instructions and definitions once, then only results returned to the model.
 * App-only results stay visible by caller without inflating that total.
 */
export function measureModelContextBudget({
  instructionsBytes,
  results,
  toolSurface,
}: {
  instructionsBytes: number
  results: readonly CoachModelContextResultMeasurement[]
  toolSurface: Pick<
    ReturnType<typeof measureModelToolSurface>,
    "descriptionBytes" | "schemaBytes"
  >
}) {
  const resultBytesByCaller = {
    app: 0,
    model: 0,
  }
  const resultCountByCaller = {
    app: 0,
    model: 0,
  }
  for (const result of results) {
    resultBytesByCaller[result.callerKind] +=
      result.contentTextBytes + result.structuredContentBytes
    resultCountByCaller[result.callerKind] += 1
  }
  const definitionBytes = toolSurface.descriptionBytes + toolSurface.schemaBytes
  const resultBytes = resultBytesByCaller.model
  return {
    definitionBytes,
    instructionsBytes,
    resultBytes,
    resultBytesByCaller,
    resultCount: resultCountByCaller.model,
    resultCountByCaller,
    totalBytes: instructionsBytes + definitionBytes + resultBytes,
  }
}

function parseByteLength(value: unknown) {
  return utf8ByteLength(JSON.stringify(value) ?? "")
}

function utf8ByteLength(value: string) {
  return new TextEncoder().encode(value).length
}
