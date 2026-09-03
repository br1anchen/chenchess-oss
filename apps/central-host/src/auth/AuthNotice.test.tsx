// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, expect, test } from "vitest"

import { AuthNotice } from "./AuthNotice"

afterEach(() => {
  cleanup()
})

test("maps failure to alert and success to status", () => {
  const { rerender } = render(
    <AuthNotice message="Could not sign in." status="error" />,
  )
  expect(screen.getByRole("alert").textContent).toContain("Could not sign in.")

  rerender(<AuthNotice message="Request received." status="success" />)
  expect(screen.getByRole("status").textContent).toContain("Request received.")
})
