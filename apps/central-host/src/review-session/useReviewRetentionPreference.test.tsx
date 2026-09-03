// @vitest-environment jsdom

import { cleanup, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, expect, test, vi } from "vitest"

import { useReviewRetentionPreference } from "./useReviewRetentionPreference"

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

function RetentionProbe({
  reportsInitialRead,
  token,
}: {
  reportsInitialRead: boolean
  token: string | null
}) {
  const retention = useReviewRetentionPreference(async () => token, {
    reportsInitialRead,
  })
  return (
    <>
      <p>failure: {retention.failure ?? "none"}</p>
      <button
        onClick={() => void retention.resolveBeforeReview()}
        type="button"
      >
        Keep this Game
      </button>
    </>
  )
}

/** A rejecting engine read the Player never asked for. */
function stubFailingRead() {
  const fetch = vi.fn(() => Promise.reject(new Error("engine unavailable")))
  vi.stubGlobal("fetch", fetch)
  return fetch
}

test("a lobby's failed mount read stays silent, and the Player's own read speaks", async () => {
  const fetch = stubFailingRead()
  const user = userEvent.setup()

  render(<RetentionProbe reportsInitialRead={false} token="player-token" />)
  await waitFor(() => {
    expect(fetch).toHaveBeenCalled()
  })
  expect(screen.getByText("failure: none")).toBeTruthy()

  // Asking for the same failing read is what proves the mount read had already
  // settled: a message here means the silence above was chosen, not early.
  await user.click(screen.getByRole("button", { name: "Keep this Game" }))
  await waitFor(() => {
    expect(screen.getByText("failure: engine unavailable")).toBeTruthy()
  })
})

test("the Player-initiated resolve reports what the mount read swallowed", async () => {
  stubFailingRead()

  render(<RetentionProbe reportsInitialRead={false} token="player-token" />)
  await userEvent.click(screen.getByRole("button", { name: "Keep this Game" }))

  await waitFor(() => {
    expect(screen.getByText("failure: engine unavailable")).toBeTruthy()
  })
})

test("a surface the Player opened still reports its own failed read", async () => {
  stubFailingRead()

  render(<RetentionProbe reportsInitialRead token="player-token" />)

  await waitFor(() => {
    expect(screen.getByText("failure: engine unavailable")).toBeTruthy()
  })
})
