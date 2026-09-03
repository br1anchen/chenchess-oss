import { FirebaseError } from "firebase/app"
import { afterEach, expect, test, vi } from "vitest"

import {
  commands,
  decodeReviewSessionCommandEnvelope,
  decodeReviewSessionEventEnvelope,
  events,
} from "@chenchess/coach-engine-sdk"
import type { ReviewSessionEventEnvelope } from "@chenchess/coach-engine-sdk"

import { createCommandEnvelope, streamReviewSessionCommand } from "./client"

afterEach(() => vi.unstubAllGlobals())

test("decodes each NDJSON event before exposing it to the caller", async () => {
  const envelope = createCommandEnvelope(await fixtureCommand())
  const stream = [0, 1, 2].map((index) => ({
    ...events[index],
    requestId: envelope.requestId,
    operationId: envelope.operationId,
  }))
  const fetchMock = vi
    .fn<typeof fetch>()
    .mockResolvedValue(ndjsonResponse(stream))
  vi.stubGlobal("fetch", fetchMock)
  const received: ReviewSessionEventEnvelope[] = []

  await streamReviewSessionCommand({
    envelope,
    fetchAccessToken: async () => "player-jwt",
    onEvent: (event) => {
      received.push(event)
    },
  })

  expect(received.map((event) => event.event.kind)).toEqual([
    "accepted",
    "progress",
    "completed",
  ])
  expect(fetchMock.mock.calls[0]?.[1]?.headers).toEqual(
    expect.objectContaining({ Authorization: "Bearer player-jwt" }),
  )
})

test("decodes game-import timing represented by the uint64 contract format", async () => {
  const envelope = createCommandEnvelope(await fixtureCommand())
  const completed = {
    ...events[2],
    requestId: envelope.requestId,
    operationId: envelope.operationId,
    event: {
      ...events[2]!.event,
      result: {
        ...events[2]!.event.result,
        timing: {
          runtimeStartupMilliseconds: 8_066,
          totalPipelineMilliseconds: 4_920,
          engineAnalysis: {
            provider: "Stockfish",
            callCount: 84,
            totalMilliseconds: 13_972,
            medianMilliseconds: 100,
            maximumMilliseconds: 979,
          },
          humanMoveModel: {
            provider: "Maia",
            callCount: 84,
            totalMilliseconds: 4_124,
            medianMilliseconds: 44,
            maximumMilliseconds: 109,
          },
        },
      },
    },
  }
  const stream = [events[0], events[1], completed].map((event) => ({
    ...event,
    requestId: envelope.requestId,
    operationId: envelope.operationId,
  }))
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockResolvedValue(ndjsonResponse(stream)),
  )
  const received: ReviewSessionEventEnvelope[] = []

  await streamReviewSessionCommand({
    envelope,
    fetchAccessToken: async () => "player-jwt",
    onEvent: (event) => {
      received.push(event)
    },
  })

  const terminal = received[2]?.event
  expect(terminal?.kind).toBe("completed")
  if (terminal?.kind !== "completed") throw new Error("missing completion")
  expect(terminal.result.kind).toBe("gameImported")
  if (terminal.result.kind !== "gameImported") {
    throw new Error("missing game import completion")
  }
  expect(terminal.result.timing?.totalPipelineMilliseconds).toBe(4_920)
  expect(terminal.result.timing?.humanMoveModel.provider).toBe("Maia")
})

test("decodes HostTurn command and terminal events through the existing stream", async () => {
  const hostTurn = commands.find(
    (envelope) => envelope.command.kind === "startHostTurn",
  )
  if (!hostTurn)
    throw new Error("generated fixtures must include StartHostTurn")
  const decoded = await decodeReviewSessionCommandEnvelope(hostTurn)
  expect(decoded.command.kind).toBe("startHostTurn")
  if (decoded.command.kind !== "startHostTurn") {
    throw new Error("StartHostTurn fixture lost its kind")
  }
  expect(decoded.command.priorTurns).toHaveLength(1)
  expect(decoded.command.priorTurns.length).toBeLessThanOrEqual(4)

  const completed = events.find(
    (envelope) =>
      envelope.event.kind === "completed" &&
      envelope.event.result?.kind === "hostTurnCompleted",
  )
  const refused = events.find(
    (envelope) =>
      envelope.event.kind === "completed" &&
      envelope.event.result?.kind === "hostTurnRefused",
  )
  const unavailable = events.find(
    (envelope) =>
      envelope.event.kind === "unavailable" &&
      envelope.event.operation === "hostTurn",
  )
  const step = events.find(
    (envelope) =>
      envelope.event.kind === "progress" &&
      envelope.event.stage?.kind === "hostTurn",
  )
  if (!completed || !refused || !unavailable || !step) {
    throw new Error("generated fixtures must include HostTurn terminals")
  }
  await expect(decodeReviewSessionEventEnvelope(completed)).resolves.toEqual(
    completed,
  )
  await expect(decodeReviewSessionEventEnvelope(refused)).resolves.toEqual(
    refused,
  )
  await expect(decodeReviewSessionEventEnvelope(unavailable)).resolves.toEqual(
    unavailable,
  )
  await expect(decodeReviewSessionEventEnvelope(step)).resolves.toEqual(step)

  const envelope = createCommandEnvelope(decoded.command)
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockResolvedValue(
      ndjsonResponse([
        {
          ...completed,
          requestId: envelope.requestId,
          operationId: envelope.operationId,
          sequence: 0,
        },
      ]),
    ),
  )
  const received: ReviewSessionEventEnvelope[] = []
  await streamReviewSessionCommand({
    envelope,
    fetchAccessToken: async () => "player-jwt",
    onEvent: (event) => {
      received.push(event)
    },
  })
  expect(received[0]?.event).toEqual(completed.event)
})

test("rejects a stream that ends without a terminal outcome", async () => {
  const envelope = createCommandEnvelope(await fixtureCommand())
  const stream = [0, 1].map((index) => ({
    ...events[index],
    requestId: envelope.requestId,
    operationId: envelope.operationId,
  }))
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockResolvedValue(ndjsonResponse(stream)),
  )

  await expect(
    streamReviewSessionCommand({
      envelope,
      fetchAccessToken: async () => "player-jwt",
      onEvent: () => undefined,
    }),
  ).rejects.toThrow("ended before its terminal outcome")
})

test("rejects data after a terminal outcome without exposing it", async () => {
  const envelope = createCommandEnvelope(await fixtureCommand())
  const stream = [0, 2, 1].map((index, sequence) => ({
    ...events[index],
    requestId: envelope.requestId,
    operationId: envelope.operationId,
    sequence,
  }))
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockResolvedValue(ndjsonResponse(stream)),
  )
  const received: ReviewSessionEventEnvelope[] = []

  await expect(
    streamReviewSessionCommand({
      envelope,
      fetchAccessToken: async () => "player-jwt",
      onEvent: (event) => {
        received.push(event)
      },
    }),
  ).rejects.toThrow("after its terminal outcome")
  expect(received.map((event) => event.event.kind)).toEqual([
    "accepted",
    "completed",
  ])
})

test("rejects undecodable and out-of-order transport data without exposing it", async () => {
  const envelope = createCommandEnvelope(await fixtureCommand())
  const accepted = {
    ...events[0],
    requestId: envelope.requestId,
    operationId: envelope.operationId,
  }
  const invalid = { ...accepted, sequence: 2 }
  vi.stubGlobal(
    "fetch",
    vi
      .fn<typeof fetch>()
      .mockResolvedValue(ndjsonResponse([accepted, invalid])),
  )
  const received: ReviewSessionEventEnvelope[] = []

  await expect(
    streamReviewSessionCommand({
      envelope,
      fetchAccessToken: async () => "player-jwt",
      onEvent: (event) => {
        received.push(event)
      },
    }),
  ).rejects.toThrow("out of sequence")
  expect(received).toHaveLength(1)
})

test("a transient auth network failure retries the token, not the command", async () => {
  const envelope = createCommandEnvelope(await fixtureCommand())
  const stream = [0, 1, 2].map((index) => ({
    ...events[index],
    requestId: envelope.requestId,
    operationId: envelope.operationId,
  }))
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockResolvedValue(ndjsonResponse(stream)),
  )
  const fetchAccessToken = vi
    .fn<(options: { forceRefreshToken: boolean }) => Promise<string | null>>()
    .mockRejectedValueOnce(
      new FirebaseError(
        "auth/network-request-failed",
        "Firebase: Error (auth/network-request-failed).",
      ),
    )
    .mockResolvedValue("player-jwt")
  const received: ReviewSessionEventEnvelope[] = []

  await streamReviewSessionCommand({
    envelope,
    fetchAccessToken,
    onEvent: (event) => {
      received.push(event)
    },
  })

  expect(received.at(-1)?.event.kind).toBe("completed")
  expect(fetchAccessToken).toHaveBeenCalledTimes(2)
  for (const call of fetchAccessToken.mock.calls) {
    expect(call[0]).toEqual({ forceRefreshToken: false })
  }
})

test("a non-network credential failure aborts without retrying", async () => {
  const envelope = createCommandEnvelope(await fixtureCommand())
  vi.stubGlobal("fetch", vi.fn<typeof fetch>())
  const fetchAccessToken = vi
    .fn<(options: { forceRefreshToken: boolean }) => Promise<string | null>>()
    .mockRejectedValue(
      new FirebaseError(
        "auth/user-token-expired",
        "Firebase: Error (auth/user-token-expired).",
      ),
    )

  await expect(
    streamReviewSessionCommand({
      envelope,
      fetchAccessToken,
      onEvent: () => undefined,
    }),
  ).rejects.toThrow("auth/user-token-expired")
  expect(fetchAccessToken).toHaveBeenCalledTimes(1)
})

async function fixtureCommand() {
  return (await decodeReviewSessionCommandEnvelope(commands[0])).command
}

function ndjsonResponse(values: unknown[]): Response {
  return new Response(
    `${values.map((value) => JSON.stringify(value)).join("\n")}\n`,
    {
      headers: { "Content-Type": "application/x-ndjson" },
    },
  )
}
