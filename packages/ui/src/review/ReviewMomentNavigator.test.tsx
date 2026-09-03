// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, expect, test, vi } from "vitest"

import { ChenTheme } from "../theme/ChenTheme"
import { ReviewMomentNavigator } from "./ReviewMomentNavigator"

afterEach(cleanup)

test("keeps move identity, count, navigation, and discussion in one control", async () => {
  const user = userEvent.setup()
  const onDiscuss = vi.fn()
  const onSelect = vi.fn()
  render(
    <ChenTheme>
      <ReviewMomentNavigator
        activePly={24}
        disabled={false}
        moments={[
          {
            glyph: "?!",
            label: "Missed tactic",
            moveLabel: "12. Qd2",
            ply: 24,
            summary: "The queen move allowed a fork.",
            tone: "improvement",
          },
          {
            glyph: "!",
            label: "Good defense",
            moveLabel: "18. Kh1",
            ply: 35,
            tone: "positive",
          },
        ]}
        onDiscuss={onDiscuss}
        onSelect={onSelect}
      />
    </ChenTheme>,
  )

  const region = screen.getByRole("region", {
    name: "Critical moment navigation",
  })
  expect(region.getAttribute("data-has-discuss")).toBe("true")
  expect(screen.getByText("12. Qd2")).toBeTruthy()
  expect(screen.getByText("Missed tactic")).toBeTruthy()
  expect(screen.getByText("The queen move allowed a fork.")).toBeTruthy()
  expect(
    screen.getByRole("heading", { name: "Critical moments 1/2" }),
  ).toBeTruthy()

  await user.click(screen.getByRole("button", { name: "Next critical moment" }))
  expect(onSelect).toHaveBeenCalledWith(35)
  const discuss = screen.getByRole("button", { name: "Discuss in chat" })
  expect(discuss.textContent).toContain("Discuss in chat")
  expect(discuss.getAttribute("data-variant")).toBe("primary")
  await user.click(discuss)
  expect(onDiscuss).toHaveBeenCalledOnce()
})
