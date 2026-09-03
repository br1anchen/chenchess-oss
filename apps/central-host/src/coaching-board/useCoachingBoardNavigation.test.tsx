// @vitest-environment jsdom

import { act, renderHook } from "@testing-library/react"
import { afterEach, expect, test } from "vitest"

import { useCoachingBoardNavigation } from "./useCoachingBoardNavigation"

afterEach(() => {
  window.history.replaceState(null, "", "/app/board")
})

test("an address on the board's own path is pushed and rendered in place, not loaded", () => {
  window.history.replaceState(null, "", "/app/board")
  const left: string[] = []
  const { result } = renderHook(() =>
    useCoachingBoardNavigation((href) => left.push(href)),
  )
  expect(result.current.pathname).toBe("/app/board")

  act(() => {
    result.current.navigate("/app/board/games/game-import%3Atest%3Aopened")
  })
  expect(result.current.pathname).toBe(
    "/app/board/games/game-import%3Atest%3Aopened",
  )
  expect(window.location.pathname).toBe(
    "/app/board/games/game-import%3Atest%3Aopened",
  )
  expect(left).toEqual([])
})

test("every other address still leaves the document", () => {
  const left: string[] = []
  const { result } = renderHook(() =>
    useCoachingBoardNavigation((href) => left.push(href)),
  )
  act(() => {
    result.current.navigate("/login?next=%2Fapp%2Fboard")
    result.current.navigate("/dashboard/")
    result.current.navigate("https://example.test/app/board")
    result.current.navigate("/app/board/not/an/address")
  })
  expect(left).toEqual([
    "/login?next=%2Fapp%2Fboard",
    "/dashboard/",
    "https://example.test/app/board",
    "/app/board/not/an/address",
  ])
  expect(result.current.pathname).toBe("/app/board")
})

test("the back button reads the way the push did", () => {
  const { result } = renderHook(() =>
    useCoachingBoardNavigation(() => undefined),
  )
  act(() => {
    result.current.navigate("/app/board/openings/C50-italian-game-fa87")
  })
  expect(result.current.pathname).toBe(
    "/app/board/openings/C50-italian-game-fa87",
  )
  act(() => {
    window.history.replaceState(null, "", "/app/board")
    window.dispatchEvent(new PopStateEvent("popstate"))
  })
  expect(result.current.pathname).toBe("/app/board")
})
