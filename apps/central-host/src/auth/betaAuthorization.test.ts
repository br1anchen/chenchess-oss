import { afterEach, expect, test, vi } from "vitest"

import { checkBetaAuthorization } from "./betaAuthorization"

afterEach(() => {
  vi.unstubAllGlobals()
})

test("admits only the exact Player returned by Coach Engine", async () => {
  const fetchAccessToken = vi.fn(
    async (): Promise<string | null> => "firebase-token",
  )
  const fetchMock = vi
    .fn<typeof fetch>()
    .mockResolvedValue(Response.json({ playerId: "firebase-player-a" }))
  vi.stubGlobal("fetch", fetchMock)

  await expect(
    checkBetaAuthorization(fetchAccessToken, "firebase-player-a"),
  ).resolves.toEqual({ kind: "granted", playerId: "firebase-player-a" })
  // Write paths mint a fresh token so Coach Engine can judge email_verified.
  // This read does not: GET /authorization only asks whether Beta Access exists.
  expect(fetchAccessToken).toHaveBeenCalledWith({ forceRefreshToken: false })
  expect(fetchMock).toHaveBeenCalledWith("/api/v1/beta-access/authorization", {
    headers: { Authorization: "Bearer firebase-token" },
    method: "GET",
  })

  fetchMock.mockResolvedValueOnce(
    Response.json({ playerId: "firebase-player-b" }),
  )
  await expect(
    checkBetaAuthorization(fetchAccessToken, "firebase-player-a"),
  ).resolves.toEqual({ kind: "unavailable" })
})

test("keeps denial, authentication, and availability outcomes distinct", async () => {
  const fetchAccessToken = vi.fn(
    async (): Promise<string | null> => "firebase-token",
  )
  const fetchMock = vi.fn<typeof fetch>()
  vi.stubGlobal("fetch", fetchMock)

  for (const [status, expected] of [
    [401, "authenticationRequired"],
    [403, "required"],
    [503, "unavailable"],
  ] as const) {
    fetchMock.mockResolvedValueOnce(new Response(null, { status }))
    await expect(
      checkBetaAuthorization(fetchAccessToken, "firebase-player-a"),
    ).resolves.toEqual({ kind: expected })
  }

  fetchAccessToken.mockResolvedValueOnce(null)
  await expect(
    checkBetaAuthorization(fetchAccessToken, "firebase-player-a"),
  ).resolves.toEqual({ kind: "authenticationRequired" })
})
