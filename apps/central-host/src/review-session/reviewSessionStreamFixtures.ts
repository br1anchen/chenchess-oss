import * as v from "valibot"

import {
  coreContract,
  decodeReviewSessionCommandEnvelope,
  decodeReviewSessionCoreContract,
  decodeReviewSessionEventEnvelope,
  events,
  fromAlternativeMoveId,
  fromBranchRef,
  fromCriticalMomentId,
  fromGameImportId,
  fromPositionRef,
  type AlternativeMoveResult,
  type GameReview,
  type GameReviewCriticalMoment,
  type GameImportId,
  type HostTurnShowLine,
  type OperationKind,
  type PositionInspection,
  type ReviewMomentLearningMaterial,
  type ReviewSessionCommand,
  type ReviewSessionCommandEnvelope,
  type ReviewSessionCoreContract,
  type ReviewSessionEvent,
  type ReviewSessionEventEnvelope,
  type ReviewSessionMoment,
  fromReviewContentDigest,
} from "@chenchess/coach-engine-sdk"

/**
 * The Coach Engine command stream, answered from the generated contract
 * fixtures. One Review Session lives in each responder: the cores it has
 * prepared, the moment the engine last opened, the retention preference it
 * has been told about.
 *
 * This is the App test's own fixture, lifted out so Storybook can serve the
 * same stream through MSW — a `ReviewSessionWorkspace` story needs the ndjson
 * transport, not a hand-written double. The module stays free of vitest: the
 * test wraps `reviewSessionResponder` in `vi.fn` for its call assertions,
 * the story hands it to an MSW handler.
 */
export type ReviewMomentOpenHold = {
  release?: () => void
  cancel?: () => void
}

export type EngineOpenSlot = {
  ply: number | null
}

export type FailOpenSlot = {
  current: number | null
}

export type ReviewSessionFixtureOptions = {
  alternativeScenario?: "success"
  failInspection?: boolean
  holdAlternative?: ReviewMomentOpenHold
  hostTurn?: HostTurnFixture | HostTurnFixture[]
  holdFirstOpen?: ReviewMomentOpenHold
  holdPlayerSelectedOpen?: ReviewMomentOpenHold
  failOpenPly?: FailOpenSlot
  engineOpen?: EngineOpenSlot
  rejectImport?: boolean
  preparedReviewMoments?: ReviewSessionCoreContract[]
  rejectGameReview?: boolean
  playerSelectedLearningMaterial?: ReviewMomentLearningMaterial
  review?: GameReview
  rejectPreference?: boolean
  retentionAvailable?: boolean
  rejectSessionStart?: boolean | "unknownGameImport"
}

/** The Game Import the fixture stream answers for. */
export const FIXTURE_GAME_IMPORT_ID: GameImportId = fromGameImportId(
  "game-import:test:web",
)

type ReviewSessionFixtures = {
  accepted: Extract<ReviewSessionEvent, { kind: "accepted" }>
  core: ReviewSessionCoreContract
  review: GameReview
}

let loaded: ReviewSessionFixtures | undefined
let loading: Promise<ReviewSessionFixtures> | undefined

/**
 * Decodes the generated contract fixtures once per module. Callers await this
 * before building responses; `fixtureCore`, `fixtureGameReview` and
 * `preparedCoreAtPly` read the decoded result synchronously afterwards.
 */
export function loadReviewSessionFixtures(): Promise<ReviewSessionFixtures> {
  loading ??= decodeFixtures().then((decoded) => {
    loaded = decoded
    return decoded
  })
  return loading
}

async function decodeFixtures(): Promise<ReviewSessionFixtures> {
  const core = await decodeReviewSessionCoreContract(
    structuredClone(coreContract),
  )
  const decodedEvents = await Promise.all(
    events.map(decodeReviewSessionEventEnvelope),
  )
  const imported = decodedEvents.find(
    (fixture) =>
      fixture.event.kind === "completed" &&
      fixture.event.result.kind === "gameImported",
  )
  if (
    !imported ||
    imported.event.kind !== "completed" ||
    imported.event.result.kind !== "gameImported"
  ) {
    throw new Error("generated fixtures must contain a Game Review")
  }
  const accepted = decodedEvents[0]?.event
  if (accepted?.kind !== "accepted") {
    throw new Error("generated fixtures must start with an accepted event")
  }
  return { accepted, core, review: imported.event.result.review }
}

function decoded(): ReviewSessionFixtures {
  if (!loaded) {
    throw new Error("await loadReviewSessionFixtures() before reading fixtures")
  }
  return loaded
}

/**
 * The Coach Engine responder, shaped like `fetch`. Fixture decoding is
 * deferred to the first request so a caller can build the responder at module
 * scope — an MSW handler list is evaluated before any story loader runs.
 */
export function reviewSessionResponder(
  options: ReviewSessionFixtureOptions = {},
): (input: RequestInfo | URL, init?: RequestInit) => Promise<Response> {
  let session:
    | ((input: RequestInfo | URL, init?: RequestInit) => Promise<Response>)
    | undefined
  return async (input, init) => {
    if (!session) {
      await loadReviewSessionFixtures()
      session = reviewSessionSession(options)
    }
    return session(input, init)
  }
}

type FixtureRuntime = {
  alternative: AlternativeMoveResult | undefined
  firstOpenHolds: number
  hostTurnQueue: HostTurnFixture[]
  openGeneration: number
  retentionEnabled: boolean
  retentionResolved: boolean
  serverCore: ReviewSessionCoreContract
  sessionCores: Map<GameImportId, ReviewSessionCoreContract[]>
}

type OpenReviewMomentCommand = Extract<
  ReviewSessionCommand,
  { kind: "openReviewMoment" }
>

type InspectPositionCommand = Extract<
  ReviewSessionCommand,
  { kind: "inspectPosition" }
>

type ExploreAlternativeMoveCommand = Extract<
  ReviewSessionCommand,
  { kind: "exploreAlternativeMove" }
>

type StartReviewSessionCommand = Extract<
  ReviewSessionCommand,
  { kind: "startReviewSession" }
>

type PipelineSelection = Extract<
  OpenReviewMomentCommand["selection"],
  { kind: "pipelineCriticalMoment" }
>

type PlayerSelectedSelection = Extract<
  OpenReviewMomentCommand["selection"],
  { kind: "playerSelectedMoment" }
>

function claimOpenGeneration(runtime: FixtureRuntime): number {
  runtime.openGeneration += 1
  return runtime.openGeneration
}

function recordEngineOpen(
  options: ReviewSessionFixtureOptions,
  runtime: FixtureRuntime,
  opened: ReviewSessionCoreContract,
  generation: number,
) {
  if (generation !== runtime.openGeneration) return
  runtime.serverCore = opened
  if (options.engineOpen) options.engineOpen.ply = opened.reviewMoment.ply
}

function reviewSessionSession(options: ReviewSessionFixtureOptions) {
  const review = structuredClone(options.review ?? fixtureGameReview())
  const runtime: FixtureRuntime = {
    alternative: undefined,
    firstOpenHolds: 0,
    hostTurnQueue: Array.isArray(options.hostTurn) ? [...options.hostTurn] : [],
    openGeneration: 0,
    retentionEnabled: true,
    retentionResolved: false,
    serverCore: fixtureCore(),
    sessionCores: new Map(),
  }

  return async (input: RequestInfo | URL, init?: RequestInit) => {
    const preference = retentionPreferenceResponse(
      input,
      init,
      options,
      runtime,
    )
    if (preference) return preference
    const command = await decodeReviewSessionCommandEnvelope(
      JSON.parse(String(init?.body)) as unknown,
    )
    bindSessionCore(command, runtime)
    return respondToReviewSessionCommand(command, options, runtime, review)
  }
}

function retentionPreferenceResponse(
  input: RequestInfo | URL,
  init: RequestInit | undefined,
  options: ReviewSessionFixtureOptions,
  runtime: FixtureRuntime,
) {
  if (!String(input).endsWith("/api/v1/review-artifacts/preference")) {
    return null
  }
  if (options.rejectPreference) {
    return new Response("unavailable", { status: 500 })
  }
  if (init?.method === "PUT") {
    runtime.retentionEnabled = parseEnabledPreference(init.body).enabled
    runtime.retentionResolved = true
  }
  return Response.json({
    available: options.retentionAvailable ?? false,
    deletedReviewSnapshots: 0,
    enabled: options.retentionAvailable ? runtime.retentionEnabled : false,
    disclosureRequired:
      (options.retentionAvailable ?? false) && !runtime.retentionResolved,
  })
}

function bindSessionCore(
  command: ReviewSessionCommandEnvelope,
  runtime: FixtureRuntime,
) {
  if (!("gameImportId" in command.command)) return
  const cores = runtime.sessionCores.get(command.command.gameImportId)
  const reviewMomentId =
    "reviewMomentId" in command.command
      ? command.command.reviewMomentId
      : undefined
  runtime.serverCore =
    cores?.find((core) => core.reviewMoment.momentId === reviewMomentId) ??
    cores?.[0] ??
    runtime.serverCore
}

function respondToReviewSessionCommand(
  command: ReviewSessionCommandEnvelope,
  options: ReviewSessionFixtureOptions,
  runtime: FixtureRuntime,
  review: GameReview,
) {
  const rejected = rejectedCommandEvents(command, options)
  if (rejected) return ndjsonResponse(rejected)
  const handled = handleKnownCommand(command, options, runtime, review)
  if (handled) return handled
  return ndjsonResponse(conflictEvents(command))
}

function rejectedCommandEvents(
  command: ReviewSessionCommandEnvelope,
  options: ReviewSessionFixtureOptions,
) {
  if (
    options.rejectSessionStart &&
    command.command.kind === "startReviewSession"
  ) {
    return [
      makeEvent(command, 0, {
        kind: "rejected",
        operation: "reviewSessionStart",
        reason:
          options.rejectSessionStart === "unknownGameImport"
            ? "unknownGameImport"
            : "unknownSession",
        recovery: { kind: "startNewReviewSession" },
      }),
    ]
  }
  if (options.rejectGameReview && command.command.kind === "openGameReview") {
    return [
      makeEvent(command, 0, {
        kind: "rejected",
        operation: "gameReviewOpen",
        reason: "unknownGameImport",
        recovery: { kind: "correctInput" },
      }),
    ]
  }
  if (options.rejectImport && command.command.kind === "importGame") {
    return [
      makeEvent(command, 0, {
        kind: "rejected",
        operation: "gameImport",
        reason: "invalidLichessUrl",
        recovery: { kind: "correctInput" },
      }),
    ]
  }
  return null
}

function handleKnownCommand(
  command: ReviewSessionCommandEnvelope,
  options: ReviewSessionFixtureOptions,
  runtime: FixtureRuntime,
  review: GameReview,
) {
  if (command.command.kind === "openGameReview") {
    return ndjsonResponse(
      completedEvents(command, "gameReviewOpen", {
        gameImportId: command.command.gameImportId,
        kind: "gameReviewOpened",
        review,
      }),
    )
  }
  if (command.command.kind === "importGame") {
    return ndjsonResponse(importGameEvents(command, review))
  }
  if (command.command.kind === "readGameReviewSnapshot") {
    return ndjsonResponse(
      readGameReviewSnapshotEvents(command, command.command, review),
    )
  }
  if (command.command.kind === "startReviewSession") {
    return ndjsonResponse(
      startReviewSessionEvents(
        command,
        command.command,
        options,
        runtime,
        review,
      ),
    )
  }
  if (command.command.kind === "openReviewMoment") {
    return openReviewMomentResponse(
      command,
      command.command,
      options,
      runtime,
      review,
    )
  }
  if (command.command.kind === "inspectPosition") {
    return ndjsonResponse(
      inspectPositionEvents(command, command.command, options, runtime),
    )
  }
  if (command.command.kind === "exploreAlternativeMove") {
    return exploreAlternativeMoveResponse(
      command,
      command.command,
      options,
      runtime,
    )
  }
  if (command.command.kind === "startHostTurn") {
    return startHostTurnResponse(command, options, runtime)
  }
  if (command.command.kind === "cancelOperation") {
    return ndjsonResponse(cancelOperationEvents(command, options))
  }
  return null
}

function importGameEvents(
  command: ReviewSessionCommandEnvelope,
  review: GameReview,
) {
  return [
    makeEvent(command, 0, acceptedEvent("gameImport")),
    makeEvent(command, 1, {
      kind: "progress",
      stage: { kind: "import", stage: "runningGameReview" },
    }),
    makeEvent(command, 2, {
      kind: "completed",
      result: {
        kind: "gameImported",
        gameImportId: FIXTURE_GAME_IMPORT_ID,
        review,
      },
    }),
  ]
}

function readGameReviewSnapshotEvents(
  command: ReviewSessionCommandEnvelope,
  readCommand: Extract<
    ReviewSessionCommand,
    { kind: "readGameReviewSnapshot" }
  >,
  review: GameReview,
) {
  const baseCore = fixtureCore()
  return completedEvents(command, "gameReviewOpen", {
    contentDigest: fromReviewContentDigest(
      "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    ),
    kind: "gameReviewSnapshotRead",
    gameImportId: readCommand.gameImportId,
    importedGame: baseCore.importedGame,
    review,
    reviewMoments: [],
  })
}

function startReviewSessionEvents(
  command: ReviewSessionCommandEnvelope,
  startedCommand: StartReviewSessionCommand,
  options: ReviewSessionFixtureOptions,
  runtime: FixtureRuntime,
  review: GameReview,
) {
  const baseCore = fixtureCore()
  const cores = structuredClone(options.preparedReviewMoments ?? [baseCore])
  for (const core of cores) {
    if (core.reviewMoment.selection.kind === "pipelineCriticalMoment") {
      core.reviewMoment.selection = {
        kind: "pipelineCriticalMoment",
        criticalMomentId: core.reviewMoment.momentId,
      }
    }
  }
  const reviewMoments = cores.map((core) => preparedMoment(core))
  runtime.serverCore = cores[0] ?? baseCore
  const gameImportId = startedCommand.gameImportId
  runtime.sessionCores.set(gameImportId, cores)
  return completedEvents(command, "reviewSessionStart", {
    kind: "reviewSessionStarted",
    gameImportId,
    sessionRevision: 1,
    review,
    importedGame: baseCore.importedGame,
    reviewMoments,
  })
}

function openReviewMomentResponse(
  command: ReviewSessionCommandEnvelope,
  openedCommand: OpenReviewMomentCommand,
  options: ReviewSessionFixtureOptions,
  runtime: FixtureRuntime,
  review: GameReview,
) {
  if (openedCommand.selection.kind === "pipelineCriticalMoment") {
    return openPipelineCriticalMoment(
      command,
      openedCommand,
      openedCommand.selection,
      options,
      runtime,
      review,
    )
  }
  if (openedCommand.selection.kind !== "playerSelectedMoment") {
    throw new Error("test only opens Player-selected moments")
  }
  return openPlayerSelectedMoment(
    command,
    openedCommand,
    openedCommand.selection,
    options,
    runtime,
  )
}

function pipelineOpenedResult(
  gameImportId: GameImportId,
  opened: ReviewSessionCoreContract,
  facts: GameReviewCriticalMoment | undefined,
  hosted: GameReviewCriticalMoment["comment"],
) {
  return {
    kind: "reviewMomentOpened" as const,
    criticalMoment: facts
      ? {
          ...structuredClone(facts),
          criticalMomentId: opened.reviewMoment.momentId,
          ply: opened.reviewMoment.ply,
        }
      : openedCriticalMoment(opened),
    decisionExplanationRef: null,
    reviewMoment: opened,
    revisionDelta: {
      changedMomentIds: [opened.reviewMoment.momentId],
      fullRefreshRequired: false,
      priorRevision: 1,
      resultingRevision: 2,
    },
    gameImportId,
    sessionRevision: 2,
    comment: hosted ?? {
      text: "After e4, occupy the center.",
    },
    commentPublished: Boolean(hosted),
    authoringContext: null,
  }
}

function openPipelineCriticalMoment(
  command: ReviewSessionCommandEnvelope,
  openedCommand: OpenReviewMomentCommand,
  selection: PipelineSelection,
  options: ReviewSessionFixtureOptions,
  runtime: FixtureRuntime,
  review: GameReview,
) {
  const criticalMomentId = selection.criticalMomentId
  const current = runtime.sessionCores.get(openedCommand.gameImportId) ?? []
  const opened =
    current.find((core) => core.reviewMoment.momentId === criticalMomentId) ??
    current[0] ??
    runtime.serverCore
  const facts = review.criticalMoments.find(
    (moment) => moment.criticalMomentId === opened.reviewMoment.momentId,
  )
  const hosted = facts?.comment
  const result = pipelineOpenedResult(
    openedCommand.gameImportId,
    opened,
    facts,
    hosted,
  )
  if (
    options.failOpenPly?.current != null &&
    opened.reviewMoment.ply === options.failOpenPly.current
  ) {
    claimOpenGeneration(runtime)
    return ndjsonResponse([
      makeEvent(command, 0, {
        kind: "rejected",
        operation: "reviewMomentOpen",
        reason: "unknownMoment",
        recovery: { kind: "correctInput" },
      }),
    ])
  }
  if (options.holdFirstOpen && runtime.firstOpenHolds === 0) {
    runtime.firstOpenHolds += 1
    const generation = claimOpenGeneration(runtime)
    return heldCompletionResponse(
      command,
      "reviewMomentOpen",
      result,
      (finish) => {
        options.holdFirstOpen!.release = () => {
          recordEngineOpen(options, runtime, opened, generation)
          finish()
        }
      },
    )
  }
  recordEngineOpen(options, runtime, opened, claimOpenGeneration(runtime))
  return ndjsonResponse(completedEvents(command, "reviewMomentOpen", result))
}

function openPlayerSelectedMoment(
  command: ReviewSessionCommandEnvelope,
  openedCommand: OpenReviewMomentCommand,
  selection: PlayerSelectedSelection,
  options: ReviewSessionFixtureOptions,
  runtime: FixtureRuntime,
) {
  const opened = preparedCoreAtPly(selection.ply)
  opened.reviewMoment.selection = selection
  const current = runtime.sessionCores.get(openedCommand.gameImportId) ?? []
  runtime.sessionCores.set(openedCommand.gameImportId, [
    ...current.filter(
      (core) => core.reviewMoment.momentId !== opened.reviewMoment.momentId,
    ),
    opened,
  ])
  const result = {
    kind: "reviewMomentOpened" as const,
    criticalMoment: openedCriticalMoment(
      opened,
      options.playerSelectedLearningMaterial,
    ),
    decisionExplanationRef: null,
    reviewMoment: opened,
    revisionDelta: {
      changedMomentIds: [opened.reviewMoment.momentId],
      fullRefreshRequired: false,
      priorRevision: 1,
      resultingRevision: 2,
    },
    gameImportId: openedCommand.gameImportId,
    sessionRevision: 2,
    comment: {
      text: "Neutral: Nf3. Intent analysis does not apply to Nf3 because it is outside your Review Side. Verified observation: White played Nf3 at ply 3.",
    },
    commentPublished: false,
    authoringContext: null,
  }
  if (options.holdPlayerSelectedOpen) {
    const generation = claimOpenGeneration(runtime)
    return heldCompletionResponse(
      command,
      "reviewMomentOpen",
      result,
      (finish) => {
        options.holdPlayerSelectedOpen!.release = () => {
          recordEngineOpen(options, runtime, opened, generation)
          finish()
        }
      },
    )
  }
  recordEngineOpen(options, runtime, opened, claimOpenGeneration(runtime))
  return ndjsonResponse(completedEvents(command, "reviewMomentOpen", result))
}

function inspectPositionEvents(
  command: ReviewSessionCommandEnvelope,
  inspectedCommand: InspectPositionCommand,
  options: ReviewSessionFixtureOptions,
  runtime: FixtureRuntime,
) {
  const inspectedCore = runtime.sessionCores
    .get(inspectedCommand.gameImportId)
    ?.find(
      (core) => core.reviewMoment.momentId === inspectedCommand.reviewMomentId,
    )
  if (
    options.engineOpen &&
    options.engineOpen.ply != null &&
    inspectedCore &&
    inspectedCore.reviewMoment.ply !== options.engineOpen.ply
  ) {
    return [
      makeEvent(command, 0, {
        kind: "rejected",
        operation: "positionInspection",
        reason: "unknownTarget",
        recovery: { kind: "correctInput" },
      }),
    ]
  }
  if (options.failInspection) {
    return [
      makeEvent(command, 0, {
        kind: "rejected",
        operation: "positionInspection",
        reason: "unknownTarget",
        recovery: { kind: "correctInput" },
      }),
    ]
  }
  const inspection =
    inspectedCommand.target.kind === "alternativeMove"
      ? alternativeInspection(
          runtime.serverCore,
          runtime.alternative ??
            hostedAlternativeFor(
              runtime.serverCore,
              inspectedCommand.target.alternativeMoveId,
            ),
        )
      : inspectionFor(runtime.serverCore)
  return completedEvents(command, "positionInspection", {
    kind: "positionInspected",
    inspection,
  })
}

function exploreAlternativeMoveResponse(
  command: ReviewSessionCommandEnvelope,
  exploredCommand: ExploreAlternativeMoveCommand,
  options: ReviewSessionFixtureOptions,
  runtime: FixtureRuntime,
) {
  if (options.alternativeScenario !== "success" && !options.holdAlternative) {
    return null
  }
  runtime.alternative = alternativeFor(
    runtime.serverCore,
    exploredCommand.moveInput.kind === "uci"
      ? exploredCommand.moveInput.uci
      : "e2e4",
  )
  const evaluated = {
    alternativeMove: runtime.alternative,
    kind: "alternativeMoveEvaluated" as const,
  }
  if (options.holdAlternative) {
    return heldCompletionResponse(
      command,
      "alternativeMoveEvaluation",
      evaluated,
      (finish) => {
        options.holdAlternative!.release = finish
      },
    )
  }
  return ndjsonResponse(
    completedEvents(command, "alternativeMoveEvaluation", evaluated),
  )
}

function startHostTurnResponse(
  command: ReviewSessionCommandEnvelope,
  options: ReviewSessionFixtureOptions,
  runtime: FixtureRuntime,
) {
  const hostTurn = Array.isArray(options.hostTurn)
    ? runtime.hostTurnQueue.shift()
    : options.hostTurn
  if (hostTurn?.kind === "held") {
    return heldHostTurnResponse(command, hostTurn.hold)
  }
  return ndjsonResponse(hostTurnEvents(command, hostTurn))
}

function cancelOperationEvents(
  command: ReviewSessionCommandEnvelope,
  options: ReviewSessionFixtureOptions,
) {
  const hostTurn = Array.isArray(options.hostTurn)
    ? undefined
    : options.hostTurn
  if (hostTurn?.kind === "held") {
    hostTurn.hold.cancel?.()
  }
  return [
    makeEvent(command, 0, acceptedEvent("cancellation")),
    makeEvent(command, 1, {
      kind: "cancelled",
      operation: "cancellation",
    }),
  ]
}

function conflictEvents(command: ReviewSessionCommandEnvelope) {
  return [
    makeEvent(command, 0, acceptedEvent("alternativeMoveEvaluation")),
    makeEvent(command, 1, {
      kind: "conflict",
      operation: "alternativeMoveEvaluation",
      reason: "idempotencyKeyMismatch",
    }),
  ]
}

function ndjsonResponse(eventList: ReviewSessionEventEnvelope[]) {
  return new Response(
    `${eventList.map((event) => JSON.stringify(event)).join("\n")}\n`,
    {
      headers: { "Content-Type": "application/x-ndjson" },
    },
  )
}

function completedEvents(
  command: ReviewSessionCommandEnvelope,
  operation: OperationKind,
  result: Extract<ReviewSessionEvent, { kind: "completed" }>["result"],
): ReviewSessionEventEnvelope[] {
  return [
    makeEvent(command, 0, acceptedEvent(operation)),
    makeEvent(command, 1, { kind: "completed", result }),
  ]
}

function heldHostTurnResponse(
  command: ReviewSessionCommandEnvelope,
  hold: ReviewMomentOpenHold,
) {
  const encoder = new TextEncoder()
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      const events: ReviewSessionEventEnvelope[] = [
        makeEvent(command, 0, acceptedEvent("hostTurn")),
        makeEvent(command, 1, {
          kind: "progress",
          stage: { kind: "hostTurn", label: "lookingAtAnotherMoment" },
        }),
        makeEvent(command, 2, {
          kind: "progress",
          stage: { kind: "hostTurn", label: "checkingThatLine" },
        }),
        makeEvent(command, 3, {
          kind: "progress",
          stage: { kind: "hostTurn", label: "writing" },
        }),
      ]
      for (const event of events) {
        controller.enqueue(encoder.encode(`${JSON.stringify(event)}\n`))
      }
      const finish = (event: ReviewSessionEvent) => {
        controller.enqueue(
          encoder.encode(`${JSON.stringify(makeEvent(command, 4, event))}\n`),
        )
        controller.close()
      }
      hold.release = () =>
        finish({
          kind: "completed",
          result: {
            kind: "hostTurnCompleted",
            answer: "The knight was hanging.",
            focusMoment: null,
            showLine: null,
          },
        })
      hold.cancel = () =>
        finish({
          kind: "cancelled",
          operation: "hostTurn",
        })
    },
  })
  return new Response(stream, {
    headers: { "Content-Type": "application/x-ndjson" },
  })
}

function heldCompletionResponse(
  command: ReviewSessionCommandEnvelope,
  operation: OperationKind,
  result: Extract<ReviewSessionEvent, { kind: "completed" }>["result"],
  setFinish: (finish: () => void) => void,
) {
  const encoder = new TextEncoder()
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(
        encoder.encode(
          `${JSON.stringify(makeEvent(command, 0, acceptedEvent(operation)))}\n`,
        ),
      )
      setFinish(() => {
        controller.enqueue(
          encoder.encode(
            `${JSON.stringify(makeEvent(command, 1, { kind: "completed", result }))}\n`,
          ),
        )
        controller.close()
      })
    },
  })
  return new Response(stream, {
    headers: { "Content-Type": "application/x-ndjson" },
  })
}
export type HostTurnFixture =
  | {
      kind: "answer"
      answer?: string
      focusMoment?: number | null
      showLine?: HostTurnShowLine | null
    }
  | {
      kind: "refused"
      reason: "notAboutThisReview" | "notAboutChess" | "unsafeRequest"
    }
  | { kind: "unavailable" }
  | { kind: "rejected" }
  | { kind: "steps" }
  | { kind: "held"; hold: ReviewMomentOpenHold }

function hostTurnEvents(
  command: ReviewSessionCommandEnvelope,
  fixture: HostTurnFixture | undefined,
): ReviewSessionEventEnvelope[] {
  const outcome = fixture ?? { kind: "answer" }
  if (outcome.kind === "unavailable") {
    return [
      makeEvent(command, 0, acceptedEvent("hostTurn")),
      makeEvent(command, 1, {
        kind: "unavailable",
        operation: "hostTurn",
        reason: { kind: "languageLayer" },
        retry: { kind: "notRetryable" },
      }),
    ]
  }
  if (outcome.kind === "rejected") {
    return [
      makeEvent(command, 0, {
        kind: "rejected",
        operation: "hostTurn",
        reason: "invalidCommand",
        recovery: { kind: "correctInput" },
      }),
    ]
  }
  if (outcome.kind === "refused") {
    return completedEvents(command, "hostTurn", {
      kind: "hostTurnRefused",
      reason: outcome.reason,
    })
  }
  if (outcome.kind === "held") {
    throw new Error(
      "held HostTurn fixtures stream through heldHostTurnResponse",
    )
  }
  if (outcome.kind === "steps") {
    return [
      makeEvent(command, 0, acceptedEvent("hostTurn")),
      makeEvent(command, 1, {
        kind: "progress",
        stage: { kind: "hostTurn", label: "lookingAtAnotherMoment" },
      }),
      makeEvent(command, 2, {
        kind: "progress",
        stage: { kind: "hostTurn", label: "checkingThatLine" },
      }),
      makeEvent(command, 3, {
        kind: "progress",
        stage: { kind: "hostTurn", label: "writing" },
      }),
      makeEvent(command, 4, {
        kind: "completed",
        result: {
          kind: "hostTurnCompleted",
          answer: "The knight was hanging.",
          focusMoment: null,
          showLine: null,
        },
      }),
    ]
  }
  return completedEvents(command, "hostTurn", {
    kind: "hostTurnCompleted",
    answer: outcome.answer ?? "The knight was hanging.",
    focusMoment: outcome.focusMoment ?? null,
    showLine: outcome.showLine ?? null,
  })
}

function inspectionFor(core: ReviewSessionCoreContract): PositionInspection {
  return {
    positionSnapshot: structuredClone(core.positionSnapshot),
    textBoard: "Fixture board",
    sideToMove: core.positionSnapshot.sideToMove,
    evaluation: { kind: "centipawns", value: 20, perspective: "white" },
    context: structuredClone(core.coachTurnContext),
    evidencePacket: structuredClone(core.evidencePacket),
  }
}

function hostedAlternativeFor(
  core: ReviewSessionCoreContract,
  alternativeMoveId: AlternativeMoveResult["alternativeMoveId"],
): AlternativeMoveResult {
  return {
    ...alternativeFor(core, "e2e4"),
    alternativeMoveId,
  }
}

function alternativeFor(
  core: ReviewSessionCoreContract,
  moveUci: string,
): AlternativeMoveResult {
  const evaluation = {
    kind: "centipawns" as const,
    perspective: "white" as const,
    value: 22,
  }
  return {
    alternativeMoveId: fromAlternativeMoveId("alternative-move:web:e4"),
    branchRef: fromBranchRef("branch:web:e4"),
    evaluation: {
      bestMove: evaluation,
      bestMoveUci: "e2e4",
      comparison: { kind: "centipawns", value: 0 },
      selectedMove: evaluation,
    },
    moveUci,
    parent: {
      kind: "root",
      positionRef: core.positionSnapshot.positionRef,
    },
    resultingPosition: {
      ...structuredClone(core.positionSnapshot),
      fen: "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
      positionRef: fromPositionRef(
        "sha256:9999999999999999999999999999999999999999999999999999999999999999",
      ),
      sideToMove: "black",
    },
    sourcePositionRef: core.positionSnapshot.positionRef,
    strongestReply: { kind: "offered", uci: "e7e5" },
  }
}

function alternativeInspection(
  core: ReviewSessionCoreContract,
  alternative: AlternativeMoveResult,
): PositionInspection {
  return {
    context: {
      coachTurnId: core.coachTurnContext.coachTurnId,
      reviewedMove: structuredClone(core.coachTurnContext.reviewedMove),
      selectedPositionRef: alternative.resultingPosition.positionRef,
      target: {
        branchRef: alternative.branchRef,
        kind: "alternativeMove",
        uci: alternative.moveUci,
      },
      requiredEvidenceRefs: [...core.coachTurnContext.requiredEvidenceRefs],
    },
    evaluation: alternative.evaluation.selectedMove,
    evidencePacket: structuredClone(core.evidencePacket),
    positionSnapshot: structuredClone(alternative.resultingPosition),
    sideToMove: alternative.resultingPosition.sideToMove,
    textBoard: "Fixture Alternative Move board",
  }
}

/** The Game Review the fixture stream opens. */
export function fixtureGameReview(): GameReview {
  return structuredClone(decoded().review)
}

/** The Game Review entry an open ships beside the moment it opened. */
function openedCriticalMoment(
  core: ReviewSessionCoreContract,
  learningMaterial?: ReviewMomentLearningMaterial,
): GameReviewCriticalMoment {
  const base = fixtureGameReview().criticalMoments[0]
  if (!base) throw new Error("fixture requires a Game Review Critical Moment")
  const moment = structuredClone(base)
  return {
    ...moment,
    criticalMomentId: core.reviewMoment.momentId,
    learningMaterial: structuredClone(
      learningMaterial ?? moment.learningMaterial,
    ),
    ply: core.reviewMoment.ply,
    provenance: "playerSelected",
  }
}

function preparedMoment(
  core: ReviewSessionCoreContract,
  learningMaterial?: ReviewMomentLearningMaterial,
): ReviewSessionMoment {
  const fixtureMaterial =
    fixtureGameReview().criticalMoments[0]?.learningMaterial
  if (!fixtureMaterial)
    throw new Error("fixture requires Review Moment material")
  const facts = fixtureGameReview().criticalMoments.find(
    (moment) => moment.criticalMomentId === core.reviewMoment.momentId,
  )
  return {
    authoring: { core, kind: "prepared" },
    classificationKind: facts?.classification.kind ?? null,
    learningMaterial: structuredClone(learningMaterial ?? fixtureMaterial),
    positionSnapshot: core.positionSnapshot,
    reviewMoment: core.reviewMoment,
  }
}

/** A Player-selected core at one ply of the fixture game. */
export function preparedCoreAtPly(ply: number): ReviewSessionCoreContract {
  const core = fixtureCore()
  const move = core.importedGame.game.moves.find(
    (candidate) => candidate.ply === ply,
  )
  if (!move) throw new Error(`fixture must contain ply ${ply}`)
  const momentId = fromCriticalMomentId(
    `review-moment:${core.reviewMoment.gameRef}:${ply}`,
  )
  core.reviewMoment = {
    ...core.reviewMoment,
    momentId,
    ply,
    precedingMove: move,
  }
  core.coachTurnContext = {
    ...core.coachTurnContext,
    reviewedMove: {
      ...core.coachTurnContext.reviewedMove,
      criticalMomentId: momentId,
      ply,
      side: move.side,
      playedMoveUci: move.uci,
    },
  }
  return core
}

/** The prepared Review Session core the fixture stream starts from. */
export function fixtureCore(): ReviewSessionCoreContract {
  return structuredClone(decoded().core)
}

function acceptedEvent(operation: OperationKind): ReviewSessionEvent {
  return { ...decoded().accepted, operation }
}

function makeEvent(
  command: ReviewSessionCommandEnvelope,
  sequence: number,
  event: ReviewSessionEvent,
): ReviewSessionEventEnvelope {
  return {
    requestId: command.requestId,
    operationId: command.operationId,
    sequence,
    event,
  }
}

/** The retention preference body both the responder and its callers read. */
export function parseEnabledPreference(body: unknown): { enabled: boolean } {
  return v.parse(
    v.object({ enabled: v.boolean() }),
    JSON.parse(String(body)) as unknown,
  )
}
