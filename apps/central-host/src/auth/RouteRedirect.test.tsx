// @vitest-environment jsdom

import { cleanup, render, waitFor } from "@testing-library/react"
import { StrictMode } from "react"
import { afterEach, expect, test, vi } from "vitest"

import { RouteRedirect } from "./RouteRedirect"

afterEach(cleanup)

test("navigates once when Strict Mode replays the redirect effect", async () => {
  const navigate = vi.fn()

  render(
    <StrictMode>
      <RouteRedirect
        description="Continue."
        href="/dashboard/"
        navigate={navigate}
        title="Opening dashboard"
      />
    </StrictMode>,
  )

  await waitFor(() => expect(navigate).toHaveBeenCalledOnce())
  expect(navigate).toHaveBeenCalledWith("/dashboard/")
})
