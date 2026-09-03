// @vitest-environment jsdom

import { cleanup, render, screen, waitFor } from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import type { FirebaseIdentity } from "./FirebaseAuthProvider"
import { useBetaAuthorization } from "./useBetaAuthorization"

const grantedAccessKey = "chenchess.beta-access.granted"
const playerId = "player:cached"

function signedInIdentity(): Extract<FirebaseIdentity, { kind: "signedIn" }> {
  return {
    kind: "signedIn",
    playerId,
    email: "cached@example.com",
    emailVerified: true,
  }
}

/**
 * An authorization request the test settles by hand. `checkBetaAuthorization`
 * runs untouched: the seam is the network, the way the product has it.
 */
function heldAuthorization() {
  let settle!: (response: Response) => void
  const response = new Promise<Response>((resolve) => {
    settle = resolve
  })
  return {
    fetch: () => response,
    answer: (status: number) =>
      settle(
        new Response(JSON.stringify({ playerId }), {
          headers: { "content-type": "application/json" },
          status,
        }),
      ),
  }
}

function Probe() {
  const { authorization } = useBetaAuthorization(
    async () => "token",
    signedInIdentity(),
  )
  return <output>{authorization.kind}</output>
}

afterEach(() => {
  cleanup()
  sessionStorage.clear()
  vi.unstubAllGlobals()
})

test("a cached grant bridges the check, and a revocation lands and clears it", async () => {
  sessionStorage.setItem(grantedAccessKey, playerId)
  const authorization = heldAuthorization()
  vi.stubGlobal("fetch", authorization.fetch)

  render(<Probe />)
  // The client-writable flag only skips the checking gate; the live check is
  // already in flight and its verdict wins.
  expect(screen.getByRole("status").textContent).toBe("granted")

  authorization.answer(403)
  await waitFor(() =>
    expect(screen.getByRole("status").textContent).toBe("required"),
  )
  expect(sessionStorage.getItem(grantedAccessKey)).toBeNull()
})

test("without a cached grant the check gates the render", async () => {
  const authorization = heldAuthorization()
  vi.stubGlobal("fetch", authorization.fetch)

  render(<Probe />)
  expect(screen.getByRole("status").textContent).toBe("checking")

  authorization.answer(200)
  await waitFor(() =>
    expect(screen.getByRole("status").textContent).toBe("granted"),
  )
  expect(sessionStorage.getItem(grantedAccessKey)).toBe(playerId)
})

test("a transient outage keeps the stored grant for the next load", async () => {
  sessionStorage.setItem(grantedAccessKey, playerId)
  vi.stubGlobal("fetch", () => Promise.reject(new Error("offline")))

  render(<Probe />)
  await waitFor(() =>
    expect(screen.getByRole("status").textContent).toBe("unavailable"),
  )
  expect(sessionStorage.getItem(grantedAccessKey)).toBe(playerId)
})

test("a cached grant for another Player does not bridge this identity", () => {
  sessionStorage.setItem(grantedAccessKey, "player:someone-else")
  vi.stubGlobal("fetch", heldAuthorization().fetch)

  render(<Probe />)
  expect(screen.getByRole("status").textContent).toBe("checking")
})
