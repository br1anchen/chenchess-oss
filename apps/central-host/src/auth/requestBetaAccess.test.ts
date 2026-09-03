import { afterEach, expect, test, vi } from "vitest"

import {
  betaAccessRequestAcceptedMessage,
  betaAccessRequestSessionMessage,
  betaAccessRequestUnavailableMessage,
  requestBetaAccess,
} from "./requestBetaAccess"

afterEach(() => {
  vi.unstubAllGlobals()
})

test("mints a fresh token before submitting an empty request", async () => {
  const fetchAccessToken = vi.fn(async () => "fresh-firebase-token")
  const fetchMock = vi
    .fn<typeof fetch>()
    .mockResolvedValue(
      Response.json(
        { message: betaAccessRequestAcceptedMessage },
        { status: 202 },
      ),
    )
  vi.stubGlobal("fetch", fetchMock)

  await expect(requestBetaAccess({ fetchAccessToken })).resolves.toEqual({
    kind: "accepted",
    message: betaAccessRequestAcceptedMessage,
  })
  expect(fetchAccessToken).toHaveBeenCalledWith({ forceRefreshToken: true })
  expect(fetchMock).toHaveBeenCalledWith("/api/v1/beta-access/requests", {
    headers: {
      Accept: "application/json",
      Authorization: "Bearer fresh-firebase-token",
    },
    method: "POST",
  })
  expect(fetchMock.mock.calls[0]?.[1]).not.toHaveProperty("body")
})

test("treats a missing or rejected token as a session problem", async () => {
  await expect(
    requestBetaAccess({ fetchAccessToken: async () => null }),
  ).resolves.toEqual({
    kind: "session",
    message: betaAccessRequestSessionMessage,
  })

  await expect(
    requestBetaAccess({
      fetchAccessToken: async () => {
        throw new Error("token refresh failed")
      },
    }),
  ).resolves.toEqual({
    kind: "session",
    message: betaAccessRequestSessionMessage,
  })
})

test("does not present an auth-token rejection as an outage", async () => {
  const fetchAccessToken = vi.fn(async () => "stale-firebase-token")
  vi.stubGlobal(
    "fetch",
    vi
      .fn<typeof fetch>()
      .mockResolvedValue(
        Response.json({ error: "Invalid Auth Token" }, { status: 401 }),
      ),
  )

  await expect(requestBetaAccess({ fetchAccessToken })).resolves.toEqual({
    kind: "session",
    message: betaAccessRequestSessionMessage,
  })
})

test("surfaces Coach Engine's 403 message for an inadmissible identity", async () => {
  const fetchAccessToken = vi.fn(async () => "fresh-firebase-token")
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockResolvedValue(
      Response.json(
        {
          message:
            "Confirm your email address, then request Beta Access again.",
        },
        { status: 403 },
      ),
    ),
  )

  await expect(requestBetaAccess({ fetchAccessToken })).resolves.toEqual({
    kind: "session",
    message: "Confirm your email address, then request Beta Access again.",
  })
})

test("does not present an origin rejection as an identity problem", async () => {
  const fetchAccessToken = vi.fn(async () => "fresh-firebase-token")
  vi.stubGlobal(
    "fetch",
    vi
      .fn<typeof fetch>()
      .mockResolvedValue(
        Response.json({ error: "origin_rejected" }, { status: 403 }),
      ),
  )

  await expect(requestBetaAccess({ fetchAccessToken })).resolves.toEqual({
    kind: "unavailable",
    message: betaAccessRequestUnavailableMessage,
  })
})

test("keeps the outage copy for a genuine Coach Engine unavailable result", async () => {
  const fetchAccessToken = vi.fn(async () => "fresh-firebase-token")
  vi.stubGlobal(
    "fetch",
    vi
      .fn<typeof fetch>()
      .mockResolvedValue(
        Response.json(
          { message: betaAccessRequestUnavailableMessage },
          { status: 503 },
        ),
      ),
  )

  await expect(requestBetaAccess({ fetchAccessToken })).resolves.toEqual({
    kind: "unavailable",
    message: betaAccessRequestUnavailableMessage,
  })
})

test("keeps the outage copy when the Central Host proxy cannot reach Coach Engine", async () => {
  const fetchAccessToken = vi.fn(async () => "fresh-firebase-token")
  vi.stubGlobal(
    "fetch",
    vi
      .fn<typeof fetch>()
      .mockResolvedValue(
        new Response("Coach Engine unavailable\n", { status: 502 }),
      ),
  )

  await expect(requestBetaAccess({ fetchAccessToken })).resolves.toEqual({
    kind: "unavailable",
    message: betaAccessRequestUnavailableMessage,
  })
})
