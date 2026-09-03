// @vitest-environment jsdom

import { act, cleanup, renderHook } from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import {
  commands,
  decodeReviewSessionCommandEnvelope,
  decodeReviewSessionEventEnvelope,
  events,
  fromGameImportId,
} from "@chenchess/coach-engine-sdk"
import type {
  GameReview,
  ProviderUnavailableReason,
  ReviewSessionCommand,
  ReviewSessionCommandEnvelope,
  ReviewSessionEventEnvelope,
} from "@chenchess/coach-engine-sdk"

import { FirebaseError } from "firebase/app"

import { containsRawUci } from "@chenchess/review-projection"
import {
  CONNECTION_DROPPED,
  INTERACTIVE_COACHING_UNAVAILABLE,
  useReviewSessionCommands,
} from "./useReviewSessionCommands"

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

test("a superseded command cannot publish after its replacement has already finished", async () => {
  let closeFirst: CloseStream | null = null
  let requestCount = 0
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockImplementation(async (_input, init) => {
      requestCount += 1
      const command = await postedCommand(init)
      const response = await commandEvents(command)
      if (requestCount === 2) return ndjsonResponse(response)

      const stream = new ReadableStream<Uint8Array>({
        start(controller) {
          const encoder = new TextEncoder()
          for (const event of response) {
            controller.enqueue(encoder.encode(`${JSON.stringify(event)}\n`))
          }
          closeFirst = () => controller.close()
        },
      })
      return new Response(stream)
    }),
  )
  const { result } = renderHook(() =>
    useReviewSessionCommands(async () => "player-jwt"),
  )
  const command = await fixtureCommand()
  let firstResult: Awaited<ReturnType<typeof result.current.run>> | undefined
  let secondResult: Awaited<ReturnType<typeof result.current.run>> | undefined

  const first = result.current.run("navigation", command, "First")
  await vi.waitFor(() => expect(closeFirst).not.toBeNull())
  await act(async () => {
    secondResult = await result.current.run("navigation", command, "Second")
  })
  closeFirst!()
  await act(async () => {
    firstResult = await first
  })

  expect(secondResult?.kind).toBe("gameImported")
  expect(firstResult).toBeNull()
})

test("invocation order wins when an older authentication refresh finishes last", async () => {
  let resolveFirstToken: ResolveToken | null = null
  let tokenRequestCount = 0
  const fetchAccessToken = vi.fn(async () => {
    tokenRequestCount += 1
    if (tokenRequestCount === 2) return "player-jwt"
    return new Promise<string | null>((resolve) => {
      resolveFirstToken = resolve
    })
  })
  const fetchMock = vi
    .fn<typeof fetch>()
    .mockImplementation(async (_input, init) => {
      const command = await postedCommand(init)
      return ndjsonResponse(await commandEvents(command))
    })
  vi.stubGlobal("fetch", fetchMock)
  const { result } = renderHook(() =>
    useReviewSessionCommands(fetchAccessToken),
  )
  const command = await fixtureCommand()

  const first = result.current.run("navigation", command, "First")
  await vi.waitFor(() => expect(resolveFirstToken).not.toBeNull())
  let secondResult: Awaited<ReturnType<typeof result.current.run>> | undefined
  await act(async () => {
    secondResult = await result.current.run("navigation", command, "Second")
  })
  resolveFirstToken!("player-jwt")
  let firstResult: Awaited<ReturnType<typeof result.current.run>> | undefined
  await act(async () => {
    firstResult = await first
  })

  expect(secondResult?.kind).toBe("gameImported")
  expect(firstResult).toBeNull()
  expect(fetchMock).toHaveBeenCalledTimes(1)
})

test("invalidation prevents a pending authentication refresh from reviving a command", async () => {
  let resolveToken: ResolveToken | null = null
  const fetchAccessToken = vi.fn(
    () =>
      new Promise<string | null>((resolve) => {
        resolveToken = resolve
      }),
  )
  const fetchMock = vi.fn<typeof fetch>()
  vi.stubGlobal("fetch", fetchMock)
  const { result } = renderHook(() =>
    useReviewSessionCommands(fetchAccessToken),
  )
  const command = await fixtureCommand()

  const pending = result.current.run("navigation", command, "Pending")
  await vi.waitFor(() => expect(resolveToken).not.toBeNull())
  act(() => result.current.invalidate())
  resolveToken!("player-jwt")
  let completion: Awaited<ReturnType<typeof result.current.run>> | undefined
  await act(async () => {
    completion = await pending
  })

  expect(completion).toBeNull()
  expect(fetchMock).not.toHaveBeenCalled()
  expect(result.current.active).toEqual({})
})

test("independent commands can complete concurrently without superseding each other", async () => {
  const fetchMock = vi
    .fn<typeof fetch>()
    .mockImplementation(async (_input, init) => {
      const command = await postedCommand(init)
      return ndjsonResponse(await commandEvents(command))
    })
  vi.stubGlobal("fetch", fetchMock)
  const { result } = renderHook(() =>
    useReviewSessionCommands(async () => "player-jwt"),
  )
  const command = await fixtureCommand()

  let completions: Awaited<ReturnType<typeof result.current.runIndependent>>[] =
    []
  await act(async () => {
    completions = await Promise.all([
      result.current.runIndependent(command, "First feedback"),
      result.current.runIndependent(command, "Second feedback"),
    ])
  })

  expect(completions.map((completion) => completion?.kind)).toEqual([
    "gameImported",
    "gameImported",
  ])
  expect(fetchMock).toHaveBeenCalledTimes(2)
  expect(result.current.active).toEqual({})
})

test("independent command failures do not replace shared command state", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockImplementation(async (_input, init) => {
      const command = await postedCommand(init)
      const rejected = (
        await Promise.all(events.map(decodeReviewSessionEventEnvelope))
      ).find(
        ({ event }) =>
          event.kind === "rejected" && event.reason === "unknownSession",
      )
      if (!rejected) throw new Error("generated fixture needs unknownSession")
      return ndjsonResponse([
        {
          ...rejected,
          operationId: command.operationId,
          requestId: command.requestId,
        },
      ])
    }),
  )
  const onUnknownGameImport = vi.fn()
  const { result } = renderHook(() =>
    useReviewSessionCommands(async () => "player-jwt", onUnknownGameImport),
  )
  const command = await fixtureCommand()
  act(() => result.current.setFailure("Keep the foreground failure"))

  await act(() => result.current.runIndependent(command, "Feedback"))

  expect(result.current.failure).toBe("Keep the foreground failure")
  expect(onUnknownGameImport).not.toHaveBeenCalled()
  expect(result.current.active).toEqual({})
})

test("HostTurn unknownGameImport rejection forgets the Game Import", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockImplementation(async (_input, init) => {
      const command = await postedCommand(init)
      return ndjsonResponse([
        {
          event: {
            kind: "rejected",
            operation: "hostTurn",
            reason: "unknownGameImport",
            recovery: { kind: "startNewReviewSession" },
          },
          operationId: command.operationId,
          requestId: command.requestId,
          sequence: 0,
        },
      ])
    }),
  )
  const onUnknownGameImport = vi.fn()
  const { result } = renderHook(() =>
    useReviewSessionCommands(async () => "player-jwt", onUnknownGameImport),
  )
  const command = await fixtureHostTurnCommand()

  let published: Awaited<ReturnType<typeof result.current.run>> | undefined
  await act(async () => {
    published = await result.current.run("hostTurn", command, "Writing…")
  })

  expect(published).toMatchObject({
    kind: "rejected",
    reason: "unknownGameImport",
  })
  expect(onUnknownGameImport).toHaveBeenCalledOnce()
})

test("HostTurn progress uses D9 product language", async () => {
  let closeStream: CloseStream | null = null
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockImplementation(async (_input, init) => {
      const command = await postedCommand(init)
      const encoder = new TextEncoder()
      const stream = new ReadableStream<Uint8Array>({
        start(controller) {
          controller.enqueue(
            encoder.encode(
              `${JSON.stringify({
                event: {
                  kind: "progress",
                  stage: { kind: "hostTurn", label: "writing" },
                },
                operationId: command.operationId,
                requestId: command.requestId,
                sequence: 0,
              })}\n`,
            ),
          )
          closeStream = () => controller.close()
        },
      })
      return new Response(stream)
    }),
  )
  const { result } = renderHook(() =>
    useReviewSessionCommands(async () => "player-jwt"),
  )
  const command = await fixtureCommand()

  const pending = result.current.run("navigation", command, "Starting…")
  await vi.waitFor(() =>
    expect(result.current.active.navigation?.label).toBe("Writing…"),
  )
  expect(result.current.active.navigation?.label).not.toMatch(
    /read_moment|list_moments|evaluate_line|learning_material|capability/i,
  )
  closeStream!()
  await act(async () => {
    await pending
  })
})

test("Coach Turn projection progress uses safe-render wording", async () => {
  let closeStream: CloseStream | null = null
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockImplementation(async (_input, init) => {
      const command = await postedCommand(init)
      const encoder = new TextEncoder()
      const stream = new ReadableStream<Uint8Array>({
        start(controller) {
          controller.enqueue(
            encoder.encode(
              `${JSON.stringify({
                event: {
                  kind: "progress",
                  stage: { kind: "coachTurn", stage: "projectingIntent" },
                },
                operationId: command.operationId,
                requestId: command.requestId,
                sequence: 0,
              })}\n`,
            ),
          )
          closeStream = () => controller.close()
        },
      })
      return new Response(stream)
    }),
  )
  const { result } = renderHook(() =>
    useReviewSessionCommands(async () => "player-jwt"),
  )
  const command = await fixtureCommand()

  const pending = result.current.run("coach", command, "Starting…")
  await vi.waitFor(() =>
    expect(result.current.active.coach?.label).toBe(
      "Checking what players at your rating usually do…",
    ),
  )
  expect(result.current.active.coach?.label).not.toMatch(
    /human-likely|human likely|human model|move model|\bmaia\b/i,
  )
  closeStream!()
  await act(async () => {
    await pending
  })
})

test("Human Move Model unavailability stays in the coach register", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockImplementation(async (_input, init) => {
      const command = await postedCommand(init)
      return ndjsonResponse([
        {
          event: {
            kind: "unavailable",
            operation: "coachTurn",
            reason: { kind: "maiaTransport" },
            retry: { kind: "retryAllowed" },
          },
          operationId: command.operationId,
          requestId: command.requestId,
          sequence: 0,
        },
      ])
    }),
  )
  const { result } = renderHook(() =>
    useReviewSessionCommands(async () => "player-jwt"),
  )
  const command = await fixtureCommand()

  await act(async () => {
    await result.current.run("coach", command, "Starting…")
  })

  expect(result.current.failure).toBe(
    "The most common choices at your rating are unavailable. Nothing changed.",
  )
  expect(result.current.failure).not.toMatch(
    /human-likely|human likely|human model|move model|\bmaia\b/i,
  )
})

test("Human Move Model timeout stays in the coach register", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockImplementation(async (_input, init) => {
      const command = await postedCommand(init)
      return ndjsonResponse([
        {
          event: {
            kind: "unavailable",
            operation: "coachTurn",
            reason: { kind: "timeout", provider: "maia" },
            retry: { kind: "retryAllowed" },
          },
          operationId: command.operationId,
          requestId: command.requestId,
          sequence: 0,
        },
      ])
    }),
  )
  const { result } = renderHook(() =>
    useReviewSessionCommands(async () => "player-jwt"),
  )
  const command = await fixtureCommand()

  await act(async () => {
    await result.current.run("coach", command, "Starting…")
  })

  expect(result.current.failure).toBe(
    "Looking up the most common choices at your rating took too long. Nothing changed.",
  )
  expect(result.current.failure).not.toMatch(
    /human-likely|human likely|human model|move model|\bmaia\b/i,
  )
})

test("a dropped network reads as a plain sentence, never a fetch error", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockRejectedValue(new TypeError("Failed to fetch")),
  )
  const { result } = renderHook(() =>
    useReviewSessionCommands(async () => "player-jwt"),
  )
  const command = await fixtureCommand()

  await act(async () => {
    await result.current.run("navigation", command, "Opening…")
  })

  expect(result.current.failure).toBe(CONNECTION_DROPPED)
})

test("an exhausted auth network retry reads as a dropped connection", async () => {
  vi.stubGlobal("fetch", vi.fn<typeof fetch>())
  const { result } = renderHook(() =>
    useReviewSessionCommands(async () => {
      throw new FirebaseError(
        "auth/network-request-failed",
        "Firebase: Error (auth/network-request-failed).",
      )
    }),
  )
  const command = await fixtureCommand()

  await act(async () => {
    await result.current.run("navigation", command, "Opening…")
  })

  expect(result.current.failure).toBe(CONNECTION_DROPPED)
  expect(result.current.failure).not.toMatch(/firebase/i)
})

test("a non-network auth failure asks for a reload, not a vendor string", async () => {
  vi.stubGlobal("fetch", vi.fn<typeof fetch>())
  const { result } = renderHook(() =>
    useReviewSessionCommands(async () => {
      throw new FirebaseError(
        "auth/user-token-expired",
        "Firebase: Error (auth/user-token-expired).",
      )
    }),
  )
  const command = await fixtureCommand()

  await act(async () => {
    await result.current.run("navigation", command, "Opening…")
  })

  expect(result.current.failure).toBe(
    "Your session could not be authorized. Reload the page to sign in again.",
  )
  expect(result.current.failure).not.toMatch(/firebase/i)
})

test("Language Layer unavailability uses the interactive coaching phrase", async () => {
  expect(await unavailableFailure({ kind: "languageLayer" })).toBe(
    INTERACTIVE_COACHING_UNAVAILABLE,
  )
})

test("Language Layer timeout uses the same interactive coaching phrase", async () => {
  expect(
    await unavailableFailure({
      kind: "timeout",
      provider: "languageLayer",
    }),
  ).toBe(INTERACTIVE_COACHING_UNAVAILABLE)
})

test("queue deadline unavailability uses the same interactive coaching phrase", async () => {
  expect(await unavailableFailure({ kind: "queueDeadline" })).toBe(
    INTERACTIVE_COACHING_UNAVAILABLE,
  )
})

test("interactive coaching copy names no internal vocabulary and no raw UCI", () => {
  expect(INTERACTIVE_COACHING_UNAVAILABLE).toBe(
    "The coach can’t answer right now. Your review is safe, and you can still try moves against the engine.",
  )
  expect(INTERACTIVE_COACHING_UNAVAILABLE).not.toMatch(
    /language layer|queue deadline|coach turn|rust backend|\bllm\b|coach persistence|\bprovider\b/i,
  )
  expect(containsRawUci(INTERACTIVE_COACHING_UNAVAILABLE)).toBe(false)
})

test("persistence unavailability names no internal vocabulary", async () => {
  expect(await unavailableFailure({ kind: "persistence" })).toBe(
    "Your saved review is unavailable. Nothing changed.",
  )
})

test("rate-limit copy names no internal vocabulary", async () => {
  expect(
    await unavailableFailure({ kind: "rateLimited", retryAfterSeconds: 12 }),
  ).toBe("That was too quick. Try again in 12 seconds.")
})

test("private allowance progress does not replace the visible operation label", async () => {
  let closeStream: CloseStream | null = null
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockImplementation(async (_input, init) => {
      const command = await postedCommand(init)
      const encoder = new TextEncoder()
      const stream = new ReadableStream<Uint8Array>({
        start(controller) {
          for (const [sequence, stage] of [
            { kind: "alternativeMove", stage: "evaluatingMove" },
            { kind: "alternativeMoveAllowance", remaining: 23 },
          ].entries()) {
            controller.enqueue(
              encoder.encode(
                `${JSON.stringify({
                  event: { kind: "progress", stage },
                  operationId: command.operationId,
                  requestId: command.requestId,
                  sequence,
                })}\n`,
              ),
            )
          }
          closeStream = () => controller.close()
        },
      })
      return new Response(stream)
    }),
  )
  const { result } = renderHook(() =>
    useReviewSessionCommands(async () => "player-jwt"),
  )
  const command = await fixtureCommand()

  const pending = result.current.run("alternative", command, "Starting…")
  await vi.waitFor(() =>
    expect(result.current.active.alternative?.label).toBe(
      "The engine is evaluating…",
    ),
  )
  closeStream!()
  await act(async () => {
    await pending
  })
})

type CloseStream = () => void
type ResolveToken = (token: string | null) => void

async function unavailableFailure(reason: ProviderUnavailableReason) {
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockImplementation(async (_input, init) => {
      const command = await postedCommand(init)
      return ndjsonResponse([
        {
          event: {
            kind: "unavailable",
            operation: "coachTurn",
            reason,
            retry: { kind: "retryAllowed" },
          },
          operationId: command.operationId,
          requestId: command.requestId,
          sequence: 0,
        },
      ])
    }),
  )
  const { result } = renderHook(() =>
    useReviewSessionCommands(async () => "player-jwt"),
  )
  const command = await fixtureCommand()

  await act(async () => {
    await result.current.run("coach", command, "Starting…")
  })

  expect(result.current.failure).not.toMatch(
    /language layer|queue deadline|coach turn|rust backend|\bllm\b|coach persistence|\bprovider\b/i,
  )
  expect(containsRawUci(result.current.failure ?? "")).toBe(false)
  return result.current.failure
}

async function fixtureCommand(): Promise<ReviewSessionCommand> {
  return (await decodeReviewSessionCommandEnvelope(commands[0])).command
}

async function fixtureHostTurnCommand(): Promise<ReviewSessionCommand> {
  const hostTurn = commands.find(
    (envelope) => envelope.command.kind === "startHostTurn",
  )
  if (!hostTurn)
    throw new Error("generated fixtures must include StartHostTurn")
  return (await decodeReviewSessionCommandEnvelope(hostTurn)).command
}

async function postedCommand(
  init: RequestInit | undefined,
): Promise<ReviewSessionCommandEnvelope> {
  return decodeReviewSessionCommandEnvelope(
    JSON.parse(String(init?.body)) as unknown,
  )
}

async function commandEvents(
  command: ReviewSessionCommandEnvelope,
): Promise<ReviewSessionEventEnvelope[]> {
  const first = await decodeReviewSessionEventEnvelope({
    ...events[0],
    requestId: command.requestId,
    operationId: command.operationId,
  })
  return [
    first,
    {
      requestId: command.requestId,
      operationId: command.operationId,
      sequence: 1,
      event: {
        kind: "completed",
        result: {
          kind: "gameImported",
          gameImportId: fromGameImportId("game-import:test:commands"),
          review: await fixtureGameReview(),
        },
      },
    },
  ]
}

async function fixtureGameReview(): Promise<GameReview> {
  for (const raw of events) {
    const fixture = await decodeReviewSessionEventEnvelope(raw)
    if (
      fixture.event.kind === "completed" &&
      fixture.event.result.kind === "gameImported"
    ) {
      return fixture.event.result.review
    }
  }
  throw new Error("generated fixtures must contain a Game Review")
}

function ndjsonResponse(events: ReviewSessionEventEnvelope[]): Response {
  return new Response(
    `${events.map((event) => JSON.stringify(event)).join("\n")}\n`,
  )
}
