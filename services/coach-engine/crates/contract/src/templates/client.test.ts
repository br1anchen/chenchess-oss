import { expect, test, vi } from "vitest"

import {
  CoachEngineClient,
  CoachEngineDailyCoachingHttpError,
  decodeArtifactRetentionPreference,
  decodeDailyCoachingSetupState,
} from "./client.js"
import { decodeDailyCoachingDigestDetail } from "./decoder.js"
import { commands, events } from "./fixtures.js"
import type { ReviewSessionCommandEnvelope } from "./ReviewSessionCommandEnvelope.js"

test("requests a current credential for every command without storing auth state", async () => {
  const credentials = ["firebase-id-token-1", "firebase-id-token-2"]
  const credential = vi.fn(async () => credentials.shift() ?? "")
  const fetchImplementation = vi.fn<typeof fetch>()
  for (let call = 0; call < 2; call += 1) {
    const command = commands[0] as ReviewSessionCommandEnvelope
    const stream = [events[0], events[2]].map((event, sequence) => ({
      ...event,
      requestId: command.requestId,
      operationId: command.operationId,
      sequence,
    }))
    fetchImplementation.mockResolvedValueOnce(ndjsonResponse(stream))
  }
  const client = new CoachEngineClient({
    credential,
    fetch: fetchImplementation,
  })

  await client.stream(
    commands[0] as ReviewSessionCommandEnvelope,
    () => undefined,
  )
  await client.stream(
    commands[0] as ReviewSessionCommandEnvelope,
    () => undefined,
  )

  expect(credential).toHaveBeenCalledTimes(2)
  expect(fetchImplementation.mock.calls[0]?.[1]?.headers).toEqual(
    expect.objectContaining({
      Authorization: "Bearer firebase-id-token-1",
    }),
  )
  expect(fetchImplementation.mock.calls[1]?.[1]?.headers).toEqual(
    expect.objectContaining({
      Authorization: "Bearer firebase-id-token-2",
    }),
  )
})

test("reads and updates the authoritative artifact retention preference", async () => {
  const credential = vi
    .fn<() => Promise<string>>()
    .mockResolvedValueOnce("firebase-id-token-1")
    .mockResolvedValueOnce("firebase-id-token-2")
  const fetchImplementation = vi
    .fn<typeof fetch>()
    .mockResolvedValueOnce(
      Response.json({
        available: true,
        deletedReviewSnapshots: 0,
        disclosureRequired: true,
        enabled: true,
      }),
    )
    .mockResolvedValueOnce(
      Response.json({
        available: true,
        deletedReviewSnapshots: 1,
        disclosureRequired: false,
        enabled: false,
      }),
    )
  const client = new CoachEngineClient({
    baseUrl: "https://coach.example.test/",
    credential,
    fetch: fetchImplementation,
  })

  await expect(client.artifactRetentionPreference()).resolves.toMatchObject({
    disclosureRequired: true,
    enabled: true,
  })
  await expect(
    client.setArtifactRetentionPreference(false),
  ).resolves.toMatchObject({
    deletedReviewSnapshots: 1,
    disclosureRequired: false,
    enabled: false,
  })

  expect(fetchImplementation.mock.calls).toEqual([
    [
      "https://coach.example.test/api/v1/review-artifacts/preference",
      {
        headers: { Authorization: "Bearer firebase-id-token-1" },
        method: "GET",
      },
    ],
    [
      "https://coach.example.test/api/v1/review-artifacts/preference",
      {
        body: JSON.stringify({ enabled: false }),
        headers: {
          Authorization: "Bearer firebase-id-token-2",
          "Content-Type": "application/json",
        },
        method: "PUT",
      },
    ],
  ])
})

test("posts review feedback reason codes and accepts an empty success body", async () => {
  const credential = vi.fn(async () => "firebase-id-token-1")
  const fetchImplementation = vi
    .fn<typeof fetch>()
    .mockResolvedValueOnce(new Response(null, { status: 204 }))
  const client = new CoachEngineClient({
    baseUrl: "https://coach.example.test/",
    credential,
    fetch: fetchImplementation,
  })

  await expect(
    client.recordReviewFeedback(["explanationNotHelpful"]),
  ).resolves.toBeUndefined()

  expect(fetchImplementation.mock.calls).toEqual([
    [
      "https://coach.example.test/api/v1/review-artifacts/feedback",
      {
        body: JSON.stringify({ reasonCodes: ["explanationNotHelpful"] }),
        headers: {
          Authorization: "Bearer firebase-id-token-1",
          "Content-Type": "application/json",
        },
        method: "POST",
      },
    ],
  ])
})

test("rejects malformed artifact retention state instead of trusting a surface", () => {
  expect(() =>
    decodeArtifactRetentionPreference({
      available: true,
      deletedReviewSnapshots: -1,
      disclosureRequired: false,
      enabled: false,
    }),
  ).toThrow("artifact retention preference response is invalid")
})

test("reads setup state and connects a public profile with the observed timezone", async () => {
  const fetchImplementation = vi
    .fn<typeof fetch>()
    .mockResolvedValueOnce(Response.json({ kind: "notConnected" }))
    .mockResolvedValueOnce(
      Response.json({
        canonicalUrl: "https://lichess.org/@/PlayerOne",
        outcome: "completed",
        provider: "lichess",
        status: "connected",
        username: "PlayerOne",
      }),
    )
  const client = new CoachEngineClient({
    baseUrl: "https://coach.example.test/",
    credential: vi.fn().mockResolvedValue("firebase-token"),
    fetch: fetchImplementation,
  })

  await expect(client.dailyCoachingState()).resolves.toEqual({
    kind: "notConnected",
  })
  await expect(
    client.connectPlayingProfile({
      profileUrl: "https://lichess.org/@/PlayerOne/all/",
      timezone: "Europe/Oslo",
    }),
  ).resolves.toMatchObject({
    canonicalUrl: "https://lichess.org/@/PlayerOne",
    outcome: "completed",
  })

  expect(fetchImplementation.mock.calls[1]).toEqual([
    "https://coach.example.test/api/v1/daily-coaching/connections",
    {
      body: JSON.stringify({
        profileUrl: "https://lichess.org/@/PlayerOne/all/",
        timezone: "Europe/Oslo",
      }),
      headers: {
        Authorization: "Bearer firebase-token",
        "Content-Type": "application/json",
      },
      method: "POST",
    },
  ])
})

test("uses semantic identities for replace, check, and remove mutations", async () => {
  const connected = {
    connections: [
      {
        canonicalUrl: "https://lichess.org/@/PlayerTwo",
        provider: "lichess",
        status: "connected",
        username: "PlayerTwo",
      },
    ],
    enabled: false,
    kind: "connected",
    timezone: "Europe/Oslo",
  }
  const fetchImplementation = vi
    .fn<typeof fetch>()
    .mockResolvedValueOnce(
      Response.json({ outcome: "completed", state: connected }),
    )
    .mockResolvedValueOnce(
      Response.json(
        { outcome: "profileUnavailable", provider: "lichess" },
        { status: 404 },
      ),
    )
    .mockResolvedValueOnce(
      Response.json({ outcome: "completed", state: { kind: "notConnected" } }),
    )
  const client = new CoachEngineClient({
    credential: vi.fn().mockResolvedValue("firebase-token"),
    fetch: fetchImplementation,
  })

  await client.replacePlayingProfile("lichess", {
    expectedUsername: "PlayerOne",
    profileUrl: "https://lichess.org/@/PlayerTwo",
  })
  await expect(
    client.checkPlayingProfile("lichess", {
      expectedUsername: "PlayerTwo",
    }),
  ).resolves.toEqual({ outcome: "profileUnavailable", provider: "lichess" })
  await client.removePlayingProfile("lichess", {
    expectedUsername: "PlayerTwo",
  })

  expect(
    fetchImplementation.mock.calls.map(([url, init]) => [
      url,
      init?.method,
      init?.body,
    ]),
  ).toEqual([
    [
      "/api/v1/daily-coaching/connections/lichess",
      "PUT",
      JSON.stringify({
        expectedUsername: "PlayerOne",
        profileUrl: "https://lichess.org/@/PlayerTwo",
      }),
    ],
    [
      "/api/v1/daily-coaching/connections/lichess/check",
      "POST",
      JSON.stringify({ expectedUsername: "PlayerTwo" }),
    ],
    [
      "/api/v1/daily-coaching/connections/lichess",
      "DELETE",
      JSON.stringify({ expectedUsername: "PlayerTwo" }),
    ],
  ])
})

test("updates digest email only through the authenticated dashboard surface", async () => {
  const state = {
    connections: [
      {
        canonicalUrl: "https://lichess.org/@/PlayerOne",
        provider: "lichess",
        status: "connected",
        username: "PlayerOne",
      },
    ],
    enabled: true,
    kind: "connected",
    timezone: "Europe/Oslo",
  }
  const fetchImplementation = vi
    .fn<typeof fetch>()
    .mockResolvedValue(Response.json({ outcome: "completed", state }))
  const client = new CoachEngineClient({
    credential: vi.fn().mockResolvedValue("firebase-token"),
    fetch: fetchImplementation,
  })

  await expect(client.setDigestEmailEnabled(true)).resolves.toEqual({
    outcome: "completed",
    state,
  })
  expect(fetchImplementation).toHaveBeenCalledWith(
    "/api/v1/daily-coaching/email",
    {
      body: JSON.stringify({ enabled: true }),
      headers: {
        Authorization: "Bearer firebase-token",
        "Content-Type": "application/json",
      },
      method: "PUT",
    },
  )
})

test.each(["noVerifiedAccountEmail", "digestEmailUnavailable"] as const)(
  "decodes the %s digest email rejection",
  async (reason) => {
    const client = new CoachEngineClient({
      credential: vi.fn().mockResolvedValue("firebase-token"),
      fetch: vi
        .fn<typeof fetch>()
        .mockResolvedValue(Response.json({ outcome: "rejected", reason })),
    })

    await expect(client.setDigestEmailEnabled(true)).resolves.toEqual({
      outcome: "rejected",
      reason,
    })
  },
)

test("reads a Daily Coaching dashboard and its frozen digest", async () => {
  const dashboard = {
    archive: [
      {
        coverageDate: "2026-08-09",
        digestId: "daily-2026-08-09",
        gameCount: 1,
        learningPathCount: 0,
        publishedAt: "2026-08-10T05:15:00Z",
      },
    ],
    connections: [
      {
        canonicalUrl: "https://lichess.org/@/PlayerOne",
        provider: "lichess",
        status: "connected",
        username: "PlayerOne",
      },
    ],
    enabled: true,
    hostConnections: [],
    kind: "connected",
    lead: { digestId: "daily-2026-08-09", kind: "digest" },
    timezone: "Europe/Oslo",
  }
  const digest = {
    ...dashboard.archive[0],
    games: [
      {
        endedAt: "2026-08-09T20:10:00Z",
        gameImportId: "game-import:daily-1",
        learningPathCount: 0,
        opening: null,
        opponentName: null,
        outcome: "win",
        provider: "lichess",
        reviewSide: "black",
        timeControlClass: "rapid",
        timeControlRaw: "600+0",
      },
    ],
    priorities: [],
    timezone: "Europe/Oslo",
  }
  const fetchImplementation = vi
    .fn<typeof fetch>()
    .mockResolvedValueOnce(Response.json(dashboard))
    .mockResolvedValueOnce(Response.json(digest))
  const client = new CoachEngineClient({
    credential: vi.fn().mockResolvedValue("firebase-token"),
    fetch: fetchImplementation,
  })

  await expect(client.dailyCoachingDashboard()).resolves.toEqual(dashboard)
  await expect(client.dailyCoachingDigest("daily-2026-08-09")).resolves.toEqual(
    digest,
  )

  expect(fetchImplementation.mock.calls.map(([url]) => url)).toEqual([
    "/api/v1/daily-coaching/dashboard",
    "/api/v1/daily-coaching/digests/daily-2026-08-09",
  ])
})

test("reads recent Playing Profile Games as a typed outcome without importing", async () => {
  const found = {
    games: [
      {
        endedAtUnixMilliseconds: 2000,
        provider: "lichess",
        reviewSide: "white",
        source: "https://lichess.org/abcdefgh",
      },
    ],
    outcome: "found",
  }
  const fetchImplementation = vi
    .fn<typeof fetch>()
    .mockResolvedValueOnce(Response.json(found))
    .mockResolvedValueOnce(Response.json({ outcome: "noPlayingProfile" }))
    .mockResolvedValueOnce(
      Response.json(
        {
          outcome: "unavailable",
          reason: "providerUnreachable",
          retry: { kind: "retryAllowed" },
        },
        { status: 503 },
      ),
    )
  const client = new CoachEngineClient({
    baseUrl: "https://coach.example.test/",
    credential: vi.fn().mockResolvedValue("firebase-token"),
    fetch: fetchImplementation,
  })

  await expect(client.recentPlayingProfileGames()).resolves.toEqual(found)
  await expect(client.recentPlayingProfileGames()).resolves.toEqual({
    outcome: "noPlayingProfile",
  })
  await expect(client.recentPlayingProfileGames()).resolves.toEqual({
    outcome: "unavailable",
    reason: "providerUnreachable",
    retry: { kind: "retryAllowed" },
  })
  expect(
    fetchImplementation.mock.calls.map(([url, init]) => [url, init?.method]),
  ).toEqual([
    [
      "https://coach.example.test/api/v1/daily-coaching/recent-profile-games",
      "GET",
    ],
    [
      "https://coach.example.test/api/v1/daily-coaching/recent-profile-games",
      "GET",
    ],
    [
      "https://coach.example.test/api/v1/daily-coaching/recent-profile-games",
      "GET",
    ],
  ])
})

test("finds Opening Lines without a credential or an import", async () => {
  const found = {
    matches: [
      {
        eco: "B90",
        name: "Sicilian Defense: Najdorf Variation",
        path: "1. e4 c5 2. Nf3 d6 3. d4 cxd4 4. Nxd4 Nf6 5. Nc3 a6",
        played: false,
      },
    ],
    truncation: { kind: "complete" as const, totalMatchCount: 1 },
  }
  const truncated = {
    matches: found.matches,
    truncation: { kind: "truncated" as const, totalMatchCount: 28 },
  }
  const fetchImplementation = vi
    .fn<typeof fetch>()
    .mockResolvedValueOnce(Response.json(found))
    .mockResolvedValueOnce(Response.json(truncated))
  const credential = vi.fn().mockResolvedValue("firebase-token")
  const client = new CoachEngineClient({
    baseUrl: "https://coach.example.test/",
    credential,
    fetch: fetchImplementation,
  })

  await expect(client.findOpeningLines({ query: "Najdorf" })).resolves.toEqual(
    found,
  )
  await expect(
    client.findOpeningLines({
      played: [{ eco: "A00", name: "Saragossa Opening" }],
      query: "Defense",
    }),
  ).resolves.toEqual(truncated)
  expect(credential).not.toHaveBeenCalled()
  expect(
    fetchImplementation.mock.calls.map(([url, init]) => [
      url,
      init?.method,
      init?.headers,
      init?.body,
    ]),
  ).toEqual([
    [
      "https://coach.example.test/api/v1/opening-lines/find",
      "POST",
      { "Content-Type": "application/json" },
      JSON.stringify({ query: "Najdorf" }),
    ],
    [
      "https://coach.example.test/api/v1/opening-lines/find",
      "POST",
      { "Content-Type": "application/json" },
      // The client forwards the caller's object verbatim, so the key order
      // is the caller's rather than a rebuilt body's.
      JSON.stringify({
        played: [{ eco: "A00", name: "Saragossa Opening" }],
        query: "Defense",
      }),
    ],
  ])
})

test("preserves a Daily Coaching read HTTP status for boundary recovery", async () => {
  const client = new CoachEngineClient({
    credential: vi.fn().mockResolvedValue("firebase-token"),
    fetch: vi
      .fn<typeof fetch>()
      .mockResolvedValue(
        Response.json({ error: "unavailable" }, { status: 503 }),
      ),
  })

  const error = await client
    .dailyCoachingDashboard()
    .catch((cause: unknown) =>
      cause instanceof CoachEngineDailyCoachingHttpError ? cause : undefined,
    )
  expect(error).toBeInstanceOf(CoachEngineDailyCoachingHttpError)
  expect(error?.status).toBe(503)
})

test("reads the next opaque Imported Games page", async () => {
  const page = { games: [], nextCursor: null }
  const fetchImplementation = vi
    .fn<typeof fetch>()
    .mockResolvedValue(Response.json(page))
  const client = new CoachEngineClient({
    credential: vi.fn().mockResolvedValue("firebase-token"),
    fetch: fetchImplementation,
  })

  await expect(client.importedGames("opaque cursor")).resolves.toEqual(page)
  expect(fetchImplementation.mock.calls[0]?.[0]).toBe(
    "/api/v1/imported-games?cursor=opaque%20cursor",
  )
})

test("posts strict reviewed-Game filters to the bounded search endpoint", async () => {
  const result = {
    coverage: { reviewedGameCount: 0 },
    games: [],
    truncation: { kind: "complete", totalMatchCount: 0 },
  }
  const fetchImplementation = vi
    .fn<typeof fetch>()
    .mockResolvedValue(Response.json(result))
  const client = new CoachEngineClient({
    credential: vi.fn().mockResolvedValue("firebase-token"),
    fetch: fetchImplementation,
  })

  await expect(
    client.searchReviewedGames({
      openingName: "French",
      outcome: "win",
    }),
  ).resolves.toEqual(result)
  expect(fetchImplementation).toHaveBeenCalledWith(
    "/api/v1/reviewed-games/search",
    {
      body: JSON.stringify({ openingName: "French", outcome: "win" }),
      headers: {
        Authorization: "Bearer firebase-token",
        "Content-Type": "application/json",
      },
      method: "POST",
    },
  )
})

test("rejects a digest whose aggregate counts contradict its Games", () => {
  expect(() =>
    decodeDailyCoachingDigestDetail({
      coverageDate: "2026-08-09",
      digestId: "daily-2026-08-09",
      gameCount: 1,
      games: [],
      learningPathCount: 0,
      priorities: [],
      publishedAt: "2026-08-10T05:15:00Z",
      timezone: "Europe/Oslo",
    }),
  ).toThrow(/gameCount Games/)
})

test.each([
  [
    "enum array",
    { outcome: ["win"] },
    /\$\.games\[0\]\.outcome: is not an allowed value/,
  ],
  [
    "invalid semantic ID",
    { gameImportId: "not a semantic id" },
    /\$\.games\[0\]\.gameImportId: does not fully match the required pattern/,
  ],
  [
    "unknown field",
    { internalFailure: "must not cross the boundary" },
    /\$\.games\[0\]\.internalFailure: is not allowed/,
  ],
])("rejects a digest Game with a malformed %s", (_case, mutation, message) => {
  const digest = dailyCoachingDigestFixture()
  expect(() =>
    decodeDailyCoachingDigestDetail({
      ...digest,
      games: [{ ...digest.games[0], ...mutation }],
    }),
  ).toThrow(message)
})

test("accepts exact supporting Game references and rejects contradictory ones", () => {
  const digest = dailyCoachingDigestFixture()
  const priority = {
    purpose: "improvement",
    resources: [
      {
        canonicalUrl: "https://lichess.org/practice/tactical-awareness",
        kind: "practiceModule",
        resourceId: "resource:daily:tactical-awareness",
        role: "learn",
        title: "Tactical awareness",
      },
    ],
    supportingGameCount: 1,
    supportingGameImportIds: [digest.games[0]?.gameImportId],
    title: "Tactical awareness",
  }

  expect(() =>
    decodeDailyCoachingDigestDetail({ ...digest, priorities: [priority] }),
  ).not.toThrow()
  expect(() =>
    decodeDailyCoachingDigestDetail({
      ...digest,
      priorities: [{ ...priority, supportingGameImportIds: [] }],
    }),
  ).toThrow(/supportingGameImportIds/)
  expect(() =>
    decodeDailyCoachingDigestDetail({
      ...digest,
      priorities: [
        {
          ...priority,
          supportingGameImportIds: ["game-import:another-digest"],
        },
      ],
    }),
  ).toThrow(/supportingGameImportIds/)
})

test("rejects connected Daily Coaching state without a connection", () => {
  expect(() =>
    decodeDailyCoachingSetupState({
      connections: [],
      enabled: true,
      kind: "connected",
      timezone: "Europe/Oslo",
    }),
  ).toThrow("Daily Coaching response is invalid")
})

function dailyCoachingDigestFixture() {
  return {
    coverageDate: "2026-08-09",
    digestId: "daily-2026-08-09",
    gameCount: 1,
    games: [
      {
        endedAt: "2026-08-09T20:10:00Z",
        gameImportId: "game-import:daily-1",
        learningPathCount: 0,
        opening: null,
        opponentName: null,
        outcome: "win",
        provider: "lichess",
        reviewSide: "black",
        timeControlClass: "rapid",
        timeControlRaw: "600+0",
      },
    ],
    learningPathCount: 0,
    priorities: [],
    publishedAt: "2026-08-10T05:15:00Z",
    timezone: "Europe/Oslo",
  }
}

function ndjsonResponse(values: unknown[]): Response {
  return new Response(
    `${values.map((value) => JSON.stringify(value)).join("\n")}\n`,
    {
      headers: { "Content-Type": "application/x-ndjson" },
    },
  )
}
