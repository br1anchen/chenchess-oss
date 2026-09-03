// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, expect, test, vi } from "vitest"

import { TrailingActionRow } from "./TrailingActionRow"

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

/** Answers the pointer-precision query the row asks, and nothing else. */
function pointerIsPrecise(precise: boolean) {
  vi.stubGlobal(
    "matchMedia",
    vi.fn().mockImplementation((media: string) => ({
      matches: precise,
      media,
      addEventListener: vi.fn<() => void>(),
      removeEventListener: vi.fn<() => void>(),
    })),
  )
}

test("reaches the revealed action without the gesture", async () => {
  const onAction = vi.fn()
  const player = userEvent.setup()
  render(
    <TrailingActionRow
      action={{
        accessibleLabel: "Delete the black Game against Ada",
        label: "Delete",
        onAction,
      }}
    >
      <p>vs. Ada</p>
    </TrailingActionRow>,
  )

  await player.click(
    screen.getByRole("button", { name: "Delete the black Game against Ada" }),
  )

  expect(onAction).toHaveBeenCalledTimes(1)
})

test("a busy action refuses a second press while the first is running", async () => {
  const onAction = vi.fn()
  const player = userEvent.setup()
  render(
    <TrailingActionRow
      action={{
        accessibleLabel: "Delete the black Game against Ada",
        busy: true,
        label: "Delete",
        onAction,
      }}
    >
      <p>vs. Ada</p>
    </TrailingActionRow>,
  )

  await player.click(
    screen.getByRole("button", { name: "Delete the black Game against Ada" }),
  )

  expect(onAction).not.toHaveBeenCalled()
})

test("a precise pointer is shown the action on the row, which never moves", () => {
  pointerIsPrecise(true)
  render(
    <TrailingActionRow
      action={{
        accessibleLabel: "Delete the black Game against Ada",
        label: "Delete",
        onAction: vi.fn(),
      }}
    >
      <p>vs. Ada</p>
    </TrailingActionRow>,
  )

  const control = screen.getByRole("button", {
    name: "Delete the black Game against Ada",
  })
  /* The row the content sits on carries no transform, so nothing has to be
     dragged aside to reach the control. */
  const surface = screen.getByText("vs. Ada").parentElement
  expect(control).toBeTruthy()
  expect(surface?.style.transform).toBeFalsy()
})
