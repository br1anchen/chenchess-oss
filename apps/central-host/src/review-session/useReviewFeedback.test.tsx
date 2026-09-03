// @vitest-environment jsdom

import { act, renderHook, waitFor } from "@testing-library/react"
import { StrictMode } from "react"
import { expect, test, vi } from "vitest"

import { useReviewFeedback } from "./useReviewFeedback"

const firstMoment = "game-import:1:moment:1:12"
const secondMoment = "game-import:1:moment:2:26"

function accessToken() {
  return vi.fn(async () => "firebase-id-token-1")
}

test("posts the vote's reason codes and keeps the record on its own moment", async () => {
  const fetchSpy = vi
    .spyOn(globalThis, "fetch")
    .mockResolvedValue(new Response(null, { status: 204 }))
  const { result } = renderHook(() => useReviewFeedback(accessToken()), {
    wrapper: StrictMode,
  })

  await act(async () => {
    await result.current.submit(firstMoment, "thumbsUp")
  })

  await waitFor(() =>
    expect(result.current.stateFor(firstMoment).vote).toBe("thumbsUp"),
  )
  const request = fetchSpy.mock.calls[0]?.[1]
  expect(request?.body).toBe(
    JSON.stringify({ reasonCodes: ["explanationHelpful"] }),
  )
  expect(result.current.stateFor(secondMoment)).toEqual({
    failure: null,
    pending: false,
    vote: null,
  })
  fetchSpy.mockRestore()
})

test("reports a refused write against the moment it was cast on", async () => {
  const fetchSpy = vi
    .spyOn(globalThis, "fetch")
    .mockResolvedValue(new Response(null, { status: 503 }))
  const { result } = renderHook(() => useReviewFeedback(accessToken()), {
    wrapper: StrictMode,
  })

  await act(async () => {
    await result.current.submit(firstMoment, "thumbsDown")
  })

  await waitFor(() =>
    expect(result.current.stateFor(firstMoment).failure).toBe(
      "Coach Engine review feedback failed with HTTP 503",
    ),
  )
  expect(result.current.stateFor(firstMoment).vote).toBeNull()
  expect(result.current.stateFor(secondMoment).failure).toBeNull()
  fetchSpy.mockRestore()
})

test("holds a second vote on the same moment while the first is in flight", async () => {
  let release: (() => void) | null = null
  const fetchSpy = vi.spyOn(globalThis, "fetch").mockImplementation(
    () =>
      new Promise<Response>((resolve) => {
        release = () => resolve(new Response(null, { status: 204 }))
      }),
  )
  const { result } = renderHook(() => useReviewFeedback(accessToken()), {
    wrapper: StrictMode,
  })

  let first: Promise<void> | null = null
  await act(async () => {
    first = result.current.submit(firstMoment, "thumbsUp")
    await Promise.resolve()
  })
  await waitFor(() =>
    expect(result.current.stateFor(firstMoment).pending).toBe(true),
  )
  await act(async () => {
    await result.current.submit(firstMoment, "thumbsDown")
  })
  expect(fetchSpy).toHaveBeenCalledTimes(1)

  await act(async () => {
    release?.()
    await first
  })
  expect(result.current.stateFor(firstMoment).vote).toBe("thumbsUp")
  fetchSpy.mockRestore()
})
