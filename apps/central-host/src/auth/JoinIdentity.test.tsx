// @vitest-environment jsdom

import {
  cleanup,
  render as renderView,
  screen,
  waitFor,
} from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactElement } from "react"
import { afterEach, beforeEach, expect, test, vi } from "vitest"

import {
  TestFirebaseAuthProvider,
  type FirebaseIdentity,
  type ProviderLinkState,
} from "./FirebaseAuthProvider"
import { JoinIdentity } from "./JoinIdentity"
import { coachingBoardDestination } from "./verifiedIdentityDestination"

type FirebaseAuthDouble = {
  fetchAccessToken: ReturnType<typeof vi.fn>
  identity: FirebaseIdentity
  providerLink: ProviderLinkState
  signOut: ReturnType<typeof vi.fn>
}

const firebaseAuth = vi.hoisted(
  (): FirebaseAuthDouble => ({
    fetchAccessToken: vi.fn().mockResolvedValue("firebase-token"),
    identity: { kind: "signedOut" },
    providerLink: { kind: "none" },
    signOut: vi.fn().mockResolvedValue(undefined),
  }),
)

beforeEach(() => {
  firebaseAuth.fetchAccessToken.mockReset().mockResolvedValue("firebase-token")
  firebaseAuth.identity = { kind: "signedOut" }
  firebaseAuth.providerLink = { kind: "none" }
  firebaseAuth.signOut.mockReset().mockResolvedValue(undefined)
})

function render(ui: ReactElement) {
  return renderView(ui, {
    wrapper: ({ children }) => (
      <TestFirebaseAuthProvider value={firebaseAuth}>
        {children}
      </TestFirebaseAuthProvider>
    ),
  })
}

afterEach(() => {
  cleanup()
  // The Beta Access grant cache is tab-scoped global state; without this a
  // granted test leaves the next one skipping the checking gate.
  sessionStorage.clear()
  vi.unstubAllGlobals()
})

test("sends a signed-out visitor to login without losing an invitation", async () => {
  const navigate = vi.fn()

  render(
    <JoinIdentity
      initialInvitationCode="0123456789abcdef0123456789abcdef"
      navigate={navigate}
      verifiedDestination={coachingBoardDestination}
    />,
  )

  await waitFor(() =>
    expect(navigate).toHaveBeenCalledWith(
      "/login/#invite=0123456789abcdef0123456789abcdef",
    ),
  )
})

test("sends an already-authorized Player directly to the requested product", async () => {
  firebaseAuth.identity = verifiedIdentity()
  vi.stubGlobal(
    "fetch",
    vi
      .fn<typeof fetch>()
      .mockResolvedValue(Response.json({ playerId: "firebase-player" })),
  )
  const navigate = vi.fn()

  render(
    <JoinIdentity
      initialInvitationCode={null}
      navigate={navigate}
      verifiedDestination={coachingBoardDestination}
    />,
  )

  await waitFor(() =>
    expect(navigate).toHaveBeenCalledWith(coachingBoardDestination.href),
  )
  expect(screen.queryByLabelText("Invitation code")).toBeNull()
})

test("keeps sign-out available while admission is checking access", async () => {
  firebaseAuth.identity = verifiedIdentity()
  vi.stubGlobal(
    "fetch",
    vi
      .fn<typeof fetch>()
      .mockImplementation(() => new Promise<Response>(() => undefined)),
  )
  const user = userEvent.setup()

  render(
    <JoinIdentity
      initialInvitationCode={null}
      navigate={vi.fn()}
      verifiedDestination={coachingBoardDestination}
    />,
  )

  expect(
    await screen.findByRole("heading", { name: "Checking your access" }),
  ).toBeTruthy()
  await user.click(screen.getByRole("button", { name: "Log out" }))
  expect(firebaseAuth.signOut).toHaveBeenCalledOnce()
})

test("lets a verified Player request access, redeem an invite, or sign out", async () => {
  firebaseAuth.identity = verifiedIdentity()
  const fetchMock = vi.fn<typeof fetch>().mockImplementation(async (input) => {
    switch (String(input)) {
      case "/api/v1/beta-access/authorization":
        return new Response(null, { status: 403 })
      case "/api/v1/beta-access/requests":
        return Response.json(
          { message: "Thanks. Your beta access request has been received." },
          { status: 202 },
        )
      case "/api/v1/beta-access/invitations/redeem":
        return Response.json({ outcome: "granted" })
      default:
        return new Response(null, { status: 404 })
    }
  })
  vi.stubGlobal("fetch", fetchMock)
  const navigate = vi.fn()
  const user = userEvent.setup()

  render(
    <JoinIdentity
      initialInvitationCode="0123456789abcdef0123456789abcdef"
      navigate={navigate}
      verifiedDestination={coachingBoardDestination}
    />,
  )

  expect(
    await screen.findByRole("heading", {
      name: "Ask for an invite",
    }),
  ).toBeTruthy()
  expect(screen.getByDisplayValue("player@example.test")).toHaveProperty(
    "readOnly",
    true,
  )

  await user.click(screen.getByRole("button", { name: "Ask for an invite" }))
  expect(
    await screen.findByText(
      "Thanks. Your beta access request has been received.",
    ),
  ).toBeTruthy()
  expect(firebaseAuth.fetchAccessToken).toHaveBeenCalledWith({
    forceRefreshToken: true,
  })
  expect(fetchMock).toHaveBeenCalledWith(
    "/api/v1/beta-access/requests",
    expect.objectContaining({
      headers: expect.objectContaining({
        Authorization: "Bearer firebase-token",
      }),
      method: "POST",
    }),
  )
  const requestCall = fetchMock.mock.calls.find(
    ([input]) => String(input) === "/api/v1/beta-access/requests",
  )
  expect(requestCall?.[1]).not.toHaveProperty("body")

  await user.click(screen.getByRole("button", { name: "Log out" }))
  expect(firebaseAuth.signOut).toHaveBeenCalledOnce()

  // Redemption is last: arriving replaces this page, so nothing on it survives.
  await user.click(screen.getByRole("button", { name: "Redeem invitation" }))
  await waitFor(() =>
    expect(navigate).toHaveBeenCalledWith(coachingBoardDestination.href),
  )
})

function verifiedIdentity(): Extract<FirebaseIdentity, { kind: "signedIn" }> {
  return {
    email: "player@example.test",
    emailVerified: true,
    kind: "signedIn",
    playerId: "firebase-player",
  }
}
