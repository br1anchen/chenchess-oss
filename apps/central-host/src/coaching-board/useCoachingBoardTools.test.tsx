// @vitest-environment jsdom

import { cleanup, render } from "@testing-library/react"
import { afterEach, expect, test } from "vitest"
import {
  fromAlternativeMoveId,
  fromGameImportId,
} from "@chenchess/coach-engine-sdk"

import {
  coachAppOnlyToolNames,
  coachWebSessionToolNames,
  coachWebToolNames,
} from "../../server/board/tool-surface"

import {
  evaluatePlayerLineDescription,
  listCriticalMomentsDescription,
  openReviewMomentInPlaceDescription,
} from "../../server/board/conversation-policy"
import { sharedGroundingSentences } from "@chenchess/shared-assets"

import { boardConstraints, lobbyConstraints } from "./coachingBoardConstraints"
import {
  coachingBoardSnapshot,
  type CoachingBoardSnapshot,
} from "./coachingBoardSnapshot"
import {
  clearModelContextPolyfill,
  installModelContextPolyfill,
} from "./modelContextPolyfill"
import { driveRefusal } from "./coachingBoardDrive"
import {
  authoredWebToolNames,
  useCoachingBoardTools,
  webToolNamesForSurface,
} from "./useCoachingBoardTools"

afterEach(() => {
  cleanup()
  clearModelContextPolyfill()
})

function Probe({
  authorizedPlayerId,
  snapshot = null,
  surface,
}: {
  authorizedPlayerId: string | null
  snapshot?: CoachingBoardSnapshot | null
  surface: "board" | "lobby"
}) {
  useCoachingBoardTools({
    authorizedPlayerId,
    host: {
      evaluateOpeningContinuation: (input) => ({
        constraints: boardConstraints(),
        continuation: input.continuation,
        kind: "openingContinuationEvaluated",
        openingLineRef: input.openingLineRef,
        snapshot,
      }),
      evaluatePlayerLine: () => ({
        constraints: boardConstraints(),
        kind: "unavailable",
        snapshot,
      }),
      findOpeningLine: (query) => ({
        constraints: lobbyConstraints(),
        kind: "lobby",
        query,
      }),
      listCriticalMoments: () => ({
        constraints: boardConstraints(),
        kind: "unavailable",
        snapshot,
      }),
      listPlayedOpenings: () => ({
        constraints: lobbyConstraints(),
        kind: "lobby",
        openings: [
          {
            eco: "C41",
            lastPlayedAtUnixMilliseconds: 1_000,
            name: "Philidor Defense",
            playCount: 3,
          },
        ],
      }),
      listRecentProfileGames: () => ({
        constraints: lobbyConstraints(),
        games: [
          {
            provider: "lichess",
            reviewSide: "black",
            source: "https://lichess.org/abcdefgh",
          },
        ],
        kind: "lobby",
        outcome: "found",
      }),
      openOpeningLine: (openingLineRef) => ({
        constraints: lobbyConstraints(),
        kind: "lobby",
        openingLineRef,
      }),
      openReviewedGame: (gameImportId) => ({
        constraints: lobbyConstraints(),
        gameImportId,
        kind: "lobby",
        outcome: "opened",
      }),
      openReviewMomentInPlace: () => ({
        constraints: boardConstraints(),
        kind: "unavailable",
        snapshot,
      }),
      readSnapshot: () => snapshot,
      searchReviewedGames: (request) => ({
        constraints: lobbyConstraints(),
        kind: "lobby",
        request,
      }),
      annotateBoard: () => driveRefusal("staleRevision", snapshot),
      setBoardPosition: () => driveRefusal("unreachablePosition", snapshot),
      showLine: () => driveRefusal("noRenderOption", snapshot),
      stepLine: () => driveRefusal("noLineShown", snapshot),
      turnBoard: () => driveRefusal("unreachablePosition", snapshot),
      stageGameImport: (fields) => ({
        constraints: lobbyConstraints(),
        fields,
        kind: "lobby",
      }),
    },
    surface,
  })
  return null
}

function boardSnapshot() {
  return coachingBoardSnapshot({
    activeBranchId: null,
    branches: [],
    constraints: boardConstraints(),
    currentPosition: {
      fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
      sideToMove: "white",
    },
    linePlayback: null,
    mainLine: {
      continuesWith: null,
      evaluation: null,
      lastPly: 0,
      reachedBy: null,
    },
    marks: [],
    orientation: "white",
    origin: {
      gameImportId: fromGameImportId("game-import:board:execute"),
      kind: "reviewMoment",
      ply: 1,
      reviewMomentId: null,
      reviewSide: "white",
    },
    pendingMove: null,
    playerChangedAtRevision: null,
    revision: 1,
    revisionChangedBy: null,
    shownLine: null,
    study: null,
    viewedPly: 1,
  })
}

function structured(result: { structuredContent?: object }) {
  return result.structuredContent
}

test("registers nothing without an authorized Player", () => {
  const tools = installModelContextPolyfill()
  render(<Probe authorizedPlayerId={null} surface="lobby" />)
  expect([...tools.keys()]).toEqual([])
})

test("registers lobby tools in authored map order after Beta Access", () => {
  const tools = installModelContextPolyfill()
  render(<Probe authorizedPlayerId="player:board" surface="lobby" />)
  expect([...tools.keys()]).toEqual(webToolNamesForSurface("lobby"))
  expect(webToolNamesForSurface("lobby")).toEqual([
    "search_reviewed_games",
    "list_recent_profile_games",
    "stage_game_import",
    "find_opening_line",
    "list_played_openings",
    "open_opening_line",
    "open_reviewed_game",
  ])
})

test("registers only the board web projection on a board surface", () => {
  const tools = installModelContextPolyfill()
  render(<Probe authorizedPlayerId="player:board" surface="board" />)
  expect([...tools.keys()]).toEqual([
    "list_critical_moments",
    "evaluate_player_line",
    "open_review_moment_in_place",
    "read_coaching_board",
    "show_line",
    "step_line",
    "set_board_position",
    "annotate_board",
    "evaluate_opening_continuation",
  ])
  expect(tools.has("read_session_status")).toBe(false)
})

test("how to name a mark rides on results, not on any description", () => {
  const namingRule = "never by a colour"
  expect(
    boardConstraints().sentences.filter((sentence) =>
      sentence.includes(namingRule),
    ),
  ).toHaveLength(1)

  const tools = installModelContextPolyfill()
  render(<Probe authorizedPlayerId="player:board" surface="board" />)
  render(<Probe authorizedPlayerId="player:board" surface="lobby" />)

  // A description is read once, before anything is drawn; a result is read
  // fresh on every call, with the marks it governs in front of it. Paying for
  // this rule on every registered description would buy nothing.
  for (const tool of tools.values()) {
    expect(tool.description).not.toContain(namingRule)
  }
  // The lobby has no board and draws nothing.
  expect(lobbyConstraints().sentences.join(" ")).not.toContain(namingRule)
})

test("retracts tools when the surface unmounts", () => {
  const tools = installModelContextPolyfill()
  const view = render(
    <Probe authorizedPlayerId="player:board" surface="board" />,
  )
  expect(tools.has("read_coaching_board")).toBe(true)
  view.unmount()
  expect(tools.has("read_coaching_board")).toBe(false)
})

test("retracts tools when the authorized Player clears", () => {
  const tools = installModelContextPolyfill()
  const view = render(
    <Probe authorizedPlayerId="player:board" surface="lobby" />,
  )
  expect(tools.size).toBe(webToolNamesForSurface("lobby").length)
  view.rerender(<Probe authorizedPlayerId={null} surface="lobby" />)
  expect(tools.size).toBe(0)
})

test("registered names equal the web projection of the authored map, in map order", () => {
  expect(authoredWebToolNames()).toEqual(coachWebToolNames)
  expect(coachWebToolNames).toEqual([
    "search_reviewed_games",
    "list_critical_moments",
    "evaluate_player_line",
    "open_review_moment_in_place",
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
  ])
  expect(coachWebSessionToolNames).toEqual(["read_session_status"])
  expect(
    [
      ...webToolNamesForSurface("board"),
      ...webToolNamesForSurface("lobby"),
      ...coachWebSessionToolNames,
    ].sort(),
  ).toEqual([...coachWebToolNames].sort())
  // Each surface preserves the authored map's relative order.
  for (const surface of ["board", "lobby"] as const) {
    const names = webToolNamesForSurface(surface)
    const mapPositions = names.map((name) => coachWebToolNames.indexOf(name))
    expect(mapPositions).toEqual([...mapPositions].sort((a, b) => a - b))
  }
  expect(coachAppOnlyToolNames).not.toContain("read_coaching_board")
  expect(coachAppOnlyToolNames).not.toContain("show_line")
  expect(coachAppOnlyToolNames).not.toContain("set_board_position")
  expect(coachAppOnlyToolNames).not.toContain("list_recent_profile_games")
  expect(coachAppOnlyToolNames).not.toContain("stage_game_import")
  expect(coachAppOnlyToolNames).not.toContain("read_session_status")
})

test("lobby tool execute returns kind lobby plus constraints, not a snapshot", async () => {
  const tools = installModelContextPolyfill()
  render(<Probe authorizedPlayerId="player:board" surface="lobby" />)
  const listed = await tools.get("list_recent_profile_games")?.execute({})
  const staged = await tools.get("stage_game_import")?.execute({})
  const found = await tools.get("find_opening_line")?.execute({})
  const opened = await tools.get("open_opening_line")?.execute({})
  const played = await tools.get("list_played_openings")?.execute({})
  expect(structured(played ?? {})).toMatchObject({
    kind: "lobby",
    openings: [{ eco: "C41", playCount: 3 }],
  })
  const searched = await tools
    .get("search_reviewed_games")
    ?.execute({ openingName: "Najdorf", reviewSide: "white" })
  expect(structured(searched ?? {})).toMatchObject({
    request: { openingName: "Najdorf", reviewSide: "white" },
  })
  for (const result of [listed, staged, found, opened, searched]) {
    const body = structured(result ?? {})
    expect(body).toMatchObject({
      constraints: { kind: "constraints" },
      kind: "lobby",
    })
    expect(body).not.toHaveProperty("origin")
    expect(body).not.toHaveProperty("exploration")
    expect(body).not.toHaveProperty("kind", "coachingBoard")
  }
})

test("malformed lobby filters and fields come back as typed refusals", async () => {
  const tools = installModelContextPolyfill()
  render(<Probe authorizedPlayerId="player:board" surface="lobby" />)
  const refusals = [
    await tools.get("search_reviewed_games")?.execute({ outcome: "stalemate" }),
    await tools
      .get("search_reviewed_games")
      ?.execute({ playedFrom: "2026-08-20", playedTo: "2026-08-01" }),
    await tools
      .get("search_reviewed_games")
      ?.execute({ opponentRatingMax: 900, opponentRatingMin: 1800 }),
  ]
  for (const refusal of refusals) {
    expect(structured(refusal ?? {})).toMatchObject({
      kind: "lobby",
      outcome: "refused",
      reason: { kind: "invalidFilters" },
    })
  }
  const malformedStage = await tools
    .get("stage_game_import")
    ?.execute({ reviewSide: "sideways", source: "https://lichess.org/x" })
  expect(structured(malformedStage ?? {})).toMatchObject({
    outcome: "refused",
    reason: { kind: "invalidFields" },
  })
})

test("a required field the model omits is refused, not defaulted", async () => {
  const tools = installModelContextPolyfill()
  render(<Probe authorizedPlayerId="player:board" surface="lobby" />)
  // The advertised JSON schema names these fields `required`. Serving a
  // default for an omitted one tells the model it asked a complete question
  // when it did not.
  const omitted = [
    { name: "stage_game_import", request: { reviewSide: "white" } },
    { name: "find_opening_line", request: {} },
    { name: "open_opening_line", request: {} },
    { name: "open_reviewed_game", request: {} },
  ]
  for (const { name, request } of omitted) {
    const refused = await tools.get(name)?.execute(request)
    expect(structured(refused ?? {})).toMatchObject({
      outcome: "refused",
      reason: { kind: "invalidFields" },
    })
  }
})

test("the advertised search schema and its valibot twin cannot diverge", async () => {
  const tools = installModelContextPolyfill()
  render(<Probe authorizedPlayerId="player:board" surface="lobby" />)
  // Sync gate: everything the advertised JSON schema accepts, the valibot
  // twin must accept too, and the schema advertises no extra keys.
  const fullFilters = {
    openingEcoPrefix: "B9",
    openingName: "Najdorf",
    opponentName: "guest",
    opponentRatingMax: 2000,
    opponentRatingMin: 1000,
    outcome: "win",
    playedFrom: "2026-08-01",
    playedTo: "2026-08-20",
    provider: "lichess",
    reviewSide: "white",
    timeControlClass: "blitz",
  }
  const searchedFull = await tools
    .get("search_reviewed_games")
    ?.execute(fullFilters)
  expect(structured(searchedFull ?? {})).toMatchObject({ request: fullFilters })
  // SAFETY: the registered JSON schema is authored above with an object
  // `properties` map; the assertion only reads its keys.
  const advertised = tools.get("search_reviewed_games")?.inputSchema as {
    properties: object
  }
  expect(Object.keys(advertised.properties).sort()).toEqual(
    Object.keys(fullFilters).sort(),
  )
  // Constraint parity, not just key parity: what the advertised schema
  // rejects, the valibot twin must reject too.
  const constraintViolations = [
    { openingEcoPrefix: "" },
    { openingEcoPrefix: "B".repeat(17) },
    { openingName: "" },
    { opponentRatingMin: 99 },
    { opponentRatingMax: 3501 },
    { playedFrom: "not-a-date" },
    { timeControlClass: "hyperbullet" },
  ]
  for (const violation of constraintViolations) {
    const rejected = await tools
      .get("search_reviewed_games")
      ?.execute(violation)
    expect(structured(rejected ?? {})).toMatchObject({
      outcome: "refused",
      reason: { kind: "invalidFilters" },
    })
  }
})

test("open_opening_line refuses a malformed address as invalid fields", async () => {
  const tools = installModelContextPolyfill()
  render(<Probe authorizedPlayerId="player:board" surface="lobby" />)
  const refused = await tools
    .get("open_opening_line")
    ?.execute({ openingLineRef: "not-an-address" })
  expect(structured(refused ?? {})).toMatchObject({
    kind: "lobby",
    outcome: "refused",
    reason: { kind: "invalidFields" },
  })
  const opened = await tools
    .get("open_opening_line")
    ?.execute({ openingLineRef: "B90-sicilian-defense-najdorf-variation-a203" })
  expect(structured(opened ?? {})).toMatchObject({
    openingLineRef: "B90-sicilian-defense-najdorf-variation-a203",
  })
})

test("open_reviewed_game navigates by an exact Game Import id and refuses anything else", async () => {
  const tools = installModelContextPolyfill()
  render(<Probe authorizedPlayerId="player:board" surface="lobby" />)
  const refused = await tools
    .get("open_reviewed_game")
    ?.execute({ gameImportId: "not-a-game-import" })
  expect(structured(refused ?? {})).toMatchObject({
    kind: "lobby",
    outcome: "refused",
    reason: { kind: "invalidFields" },
  })
  const opened = await tools
    .get("open_reviewed_game")
    ?.execute({ gameImportId: "game-import:board:reviewed" })
  expect(structured(opened ?? {})).toMatchObject({
    gameImportId: "game-import:board:reviewed",
    kind: "lobby",
    outcome: "opened",
  })
})

test("set_board_position accepts a reviewed Game target as navigation", async () => {
  const tools = installModelContextPolyfill()
  render(<Probe authorizedPlayerId="player:board" surface="board" />)
  const opened = await tools.get("set_board_position")?.execute({
    gameImportId: "game-import:board:reviewed",
    kind: "game",
  })
  expect(structured(opened ?? {})).toMatchObject({
    gameImportId: "game-import:board:reviewed",
    outcome: "opened",
  })
  const refused = await tools.get("set_board_position")?.execute({
    gameImportId: "not-a-game-import",
    kind: "game",
  })
  expect(structured(refused ?? {})).toMatchObject({
    kind: "refused",
    reason: "outsideTargetVocabulary",
  })
})

test("set_board_position accepts an Opening Line target as navigation", async () => {
  const tools = installModelContextPolyfill()
  render(<Probe authorizedPlayerId="player:board" surface="board" />)
  const opened = await tools.get("set_board_position")?.execute({
    kind: "openingLine",
    openingLineRef: "B90-sicilian-defense-najdorf-variation-a203",
  })
  expect(structured(opened ?? {})).toMatchObject({
    openingLineRef: "B90-sicilian-defense-najdorf-variation-a203",
  })
  const refused = await tools.get("set_board_position")?.execute({
    kind: "openingLine",
    openingLineRef: "not-an-address",
  })
  expect(structured(refused ?? {})).toMatchObject({
    kind: "refused",
    reason: "outsideTargetVocabulary",
  })
})

test("board read execute returns the snapshot plus constraints", async () => {
  const tools = installModelContextPolyfill()
  const snapshot = boardSnapshot()
  render(
    <Probe
      authorizedPlayerId="player:board"
      snapshot={snapshot}
      surface="board"
    />,
  )
  const read = await tools.get("read_coaching_board")?.execute({})
  expect(structured(read ?? {})).toEqual(snapshot)
  expect(structured(read ?? {})).toMatchObject({
    constraints: { kind: "constraints" },
    kind: "coachingBoard",
  })
})

test("opening-continuation evaluation passes a bounded continuation through and refuses the rest", async () => {
  const tools = installModelContextPolyfill()
  const snapshot = boardSnapshot()
  render(
    <Probe
      authorizedPlayerId="player:board"
      snapshot={snapshot}
      surface="board"
    />,
  )
  const evaluated = await tools.get("evaluate_opening_continuation")?.execute({
    continuation: [{ kind: "san", san: "f4" }],
    openingLineRef: "B90-sicilian-najdorf-1a2b",
  })
  expect(structured(evaluated ?? {})).toMatchObject({
    continuation: [{ kind: "san", san: "f4" }],
    kind: "openingContinuationEvaluated",
    openingLineRef: "B90-sicilian-najdorf-1a2b",
  })

  // Thirteen plies is past the cap, so the call never reaches the host.
  const overCap = await tools.get("evaluate_opening_continuation")?.execute({
    continuation: Array.from({ length: 13 }, () => ({
      kind: "san" as const,
      san: "f4",
    })),
    openingLineRef: "B90-sicilian-najdorf-1a2b",
  })
  expect(structured(overCap ?? {})).toMatchObject({ kind: "unavailable" })

  const empty = await tools.get("evaluate_opening_continuation")?.execute({
    continuation: [],
    openingLineRef: "B90-sicilian-najdorf-1a2b",
  })
  expect(structured(empty ?? {})).toMatchObject({ kind: "unavailable" })
})

test("show-line refuses anything outside the closed union and leaves the board", async () => {
  const tools = installModelContextPolyfill()
  const snapshot = boardSnapshot()
  render(
    <Probe
      authorizedPlayerId="player:board"
      snapshot={snapshot}
      surface="board"
    />,
  )
  const invented = await tools.get("show_line")?.execute({
    kind: "inventedLine",
  })
  expect(structured(invented ?? {})).toMatchObject({
    kind: "refused",
    reason: "outsideClosedLineUnion",
    snapshot: { revision: 1, shownLine: null },
  })
  const missing = await tools.get("show_line")?.execute({
    alternativeMoveId: fromAlternativeMoveId("alternative-move:board:missing"),
    kind: "alternativeMove",
  })
  expect(structured(missing ?? {})).toMatchObject({
    kind: "refused",
    reason: "noRenderOption",
    snapshot: { revision: 1 },
  })
})

test("set-position refuses an unreachable target and leaves the board", async () => {
  const tools = installModelContextPolyfill()
  const snapshot = boardSnapshot()
  render(
    <Probe
      authorizedPlayerId="player:board"
      snapshot={snapshot}
      surface="board"
    />,
  )
  const fen = await tools.get("set_board_position")?.execute({
    fen: "8/8/8/8/8/8/8/8 w - - 0 1",
    kind: "fen",
  })
  expect(structured(fen ?? {})).toMatchObject({
    kind: "refused",
    reason: "outsideTargetVocabulary",
    snapshot: { revision: 1, viewedPly: 1 },
  })
})

test("board coach-tool descriptions carry the shared grounding source", () => {
  const tools = installModelContextPolyfill()
  render(<Probe authorizedPlayerId="player:board" surface="board" />)
  const listed = tools.get("list_critical_moments")
  const evaluated = tools.get("evaluate_player_line")
  const opened = tools.get("open_review_moment_in_place")
  expect(listed?.description).toContain(listCriticalMomentsDescription)
  expect(evaluated?.description).toContain(evaluatePlayerLineDescription)
  expect(opened?.description).toContain(openReviewMomentInPlaceDescription)
  for (const sentence of sharedGroundingSentences) {
    expect(listed?.description).toContain(sentence)
    expect(evaluated?.description).toContain(sentence)
    expect(opened?.description).toContain(sentence)
  }
})

test("board coach-tool execute results carry constraints and the snapshot envelope", async () => {
  const tools = installModelContextPolyfill()
  const snapshot = boardSnapshot()
  render(
    <Probe
      authorizedPlayerId="player:board"
      snapshot={snapshot}
      surface="board"
    />,
  )
  const listed = await tools.get("list_critical_moments")?.execute({
    gameImportId: "game-import:board:execute",
  })
  const opened = await tools.get("open_review_moment_in_place")?.execute({
    gameImportId: "game-import:board:execute",
    moment: { kind: "ply", ply: 1 },
  })
  const evaluated = await tools.get("evaluate_player_line")?.execute({
    gameImportId: "game-import:board:execute",
    moment: { kind: "ply", ply: 1 },
    moves: [{ kind: "uci", uci: "e2e4" }],
    opponentReplies: "supplied",
  })
  for (const result of [listed, opened, evaluated]) {
    expect(structured(result ?? {})).toMatchObject({
      constraints: { kind: "constraints" },
      snapshot: { kind: "coachingBoard", revision: 1 },
    })
  }
})

test("lobby list then stage stays on the import surface", async () => {
  const tools = installModelContextPolyfill()
  render(<Probe authorizedPlayerId="player:board" surface="lobby" />)
  const listed = structured(
    (await tools.get("list_recent_profile_games")?.execute({})) ?? {},
  )
  expect(listed).toMatchObject({
    games: [{ source: "https://lichess.org/abcdefgh", reviewSide: "black" }],
    kind: "lobby",
    outcome: "found",
  })
  const staged = structured(
    (await tools.get("stage_game_import")?.execute({
      reviewSide: "black",
      source: "https://lichess.org/abcdefgh",
    })) ?? {},
  )
  expect(staged).toMatchObject({
    fields: { source: "https://lichess.org/abcdefgh" },
    kind: "lobby",
  })
  expect(tools.has("show_line")).toBe(false)
  expect(tools.has("read_coaching_board")).toBe(false)
})

test("lobby does not register board-drive tools", () => {
  const tools = installModelContextPolyfill()
  render(<Probe authorizedPlayerId="player:board" surface="lobby" />)
  expect(tools.has("show_line")).toBe(false)
  expect(tools.has("set_board_position")).toBe(false)
  expect(tools.has("read_coaching_board")).toBe(false)
  expect(tools.has("list_critical_moments")).toBe(false)
  expect(tools.has("evaluate_player_line")).toBe(false)
  expect(tools.has("open_review_moment_in_place")).toBe(false)
  expect(tools.has("read_session_status")).toBe(false)
})
