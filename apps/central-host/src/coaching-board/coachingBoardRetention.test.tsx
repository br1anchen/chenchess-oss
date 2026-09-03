// @vitest-environment jsdom

import { cleanup, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, expect, test, vi } from "vitest"

import { ChenTheme } from "@chenchess/ui/theme"

import { TestFirebaseAuthProvider } from "@/auth/FirebaseAuthProvider"
import { parseEnabledPreference } from "@/review-session/reviewSessionStreamFixtures"

import { CoachingBoardMount } from "./CoachingBoardMount"
import {
  betaAuthorizedResponder,
  verifiedIdentity,
} from "./coachingBoardMountFixtures"

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

function lobbyFetch() {
  return vi.fn<typeof fetch>().mockImplementation(betaAuthorizedResponder())
}

function renderLobby() {
  render(
    <ChenTheme>
      <TestFirebaseAuthProvider
        value={{
          fetchAccessToken: vi.fn().mockResolvedValue("firebase-token"),
          identity: verifiedIdentity(),
        }}
      >
        <CoachingBoardMount navigate={vi.fn()} route={{ kind: "empty" }} />
      </TestFirebaseAuthProvider>
    </ChenTheme>,
  )
}

test("the lobby settles the retention preference before it keeps a Game", async () => {
  const fetchMock = lobbyFetch()
  vi.stubGlobal("fetch", fetchMock)
  const user = userEvent.setup()
  renderLobby()

  await user.type(
    await screen.findByRole("textbox", { name: /Game URL or PGN/ }),
    "https://lichess.org/Synthet1Demo/black",
  )
  await user.click(screen.getByRole("button", { name: "Import" }))

  await waitFor(() => {
    expect(commandIndex(fetchMock)).toBeGreaterThan(-1)
  })

  // The mount also reads the preference, so only the write counts as settled.
  const settled = fetchMock.mock.calls.findIndex(
    ([input, init]) =>
      String(input).endsWith("/api/v1/review-artifacts/preference") &&
      init?.method === "PUT",
  )
  expect(settled).toBeGreaterThan(-1)
  expect(settled).toBeLessThan(commandIndex(fetchMock))
})

test("the disclosure names what keeping a Game costs, before it is kept", async () => {
  vi.stubGlobal("fetch", lobbyFetch())
  renderLobby()

  expect(
    await screen.findByRole("heading", { name: "Before this Game is kept" }),
  ).toBeTruthy()
})

test("the first Game kept settles the preference as enabled", async () => {
  const fetchMock = lobbyFetch()
  vi.stubGlobal("fetch", fetchMock)
  const user = userEvent.setup()
  renderLobby()

  await user.type(
    await screen.findByRole("textbox", { name: /Game URL or PGN/ }),
    "https://lichess.org/Synthet1Demo/black",
  )
  await user.click(screen.getByRole("button", { name: "Import" }))

  await waitFor(() => {
    expect(preferenceWrites(fetchMock)).toEqual([{ enabled: true }])
  })
})

function preferenceWrites(fetchMock: ReturnType<typeof lobbyFetch>) {
  return fetchMock.mock.calls
    .filter(
      ([input, init]) =>
        String(input).endsWith("/api/v1/review-artifacts/preference") &&
        init?.method === "PUT",
    )
    .map(([, init]) => parseEnabledPreference(init?.body))
}

function commandIndex(fetchMock: ReturnType<typeof lobbyFetch>) {
  return fetchMock.mock.calls.findIndex(([input]) =>
    String(input).endsWith("/api/v1/review-session/commands"),
  )
}
