// @vitest-environment jsdom

import { cleanup, render as renderView, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactElement } from "react"
import { afterEach, describe, expect, test, vi } from "vitest"

import { ChenTheme } from "@chenchess/ui/theme"

import { AccountSettings } from "./AccountSettings"
import type { useReviewRetentionPreference } from "./useReviewRetentionPreference"

function render(ui: ReactElement) {
  return renderView(ui, { wrapper: ChenTheme })
}

type Retention = ReturnType<typeof useReviewRetentionPreference>

function retention(overrides: Partial<Retention> = {}): Retention {
  return {
    available: true,
    deletedReviewSnapshots: 0,
    disclosureRequired: false,
    enabled: true,
    failure: null,
    resolveBeforeReview: vi.fn(async () => true),
    resolving: false,
    updateEnabled: vi.fn(async () => true),
    ...overrides,
  }
}

describe("AccountSettings hosted Language Layer claim", () => {
  afterEach(() => {
    cleanup()
  })

  test("sits beside Help improve coaching and points at the privacy page", () => {
    render(
      <AccountSettings
        fetchAccessToken={async () => "token"}
        onDeleted={async () => undefined}
        reauthenticate={async () => undefined}
        retention={retention()}
      />,
    )

    expect(
      screen.getByRole("checkbox", { name: "Help improve coaching" }),
    ).toBeTruthy()
    expect(
      screen.getByRole("heading", { name: "Hosted coaching notes" }),
    ).toBeTruthy()
    expect(document.querySelector("[data-hosted-notes]")).toBeTruthy()
    expect(
      screen.getByText(
        /not used for training, and are kept only if an automated safety check flags a request, and only briefly/,
      ),
    ).toBeTruthy()
    expect(
      screen
        .getByRole("link", { name: "Read the privacy page" })
        .getAttribute("href"),
    ).toBe("/privacy/")
  })

  test("keeps Help improve coaching beside hosted notes while first-run disclosure is still required", () => {
    render(
      <AccountSettings
        fetchAccessToken={async () => "token"}
        onDeleted={async () => undefined}
        reauthenticate={async () => undefined}
        retention={retention({ disclosureRequired: true })}
      />,
    )

    expect(
      screen.getByRole("checkbox", { name: "Help improve coaching" }),
    ).toBeTruthy()
    expect(
      screen.getByRole("heading", { name: "Hosted coaching notes" }),
    ).toBeTruthy()
  })

  test("withdraws Help improve coaching from Account Settings before first-run disclosure is acknowledged", async () => {
    const user = userEvent.setup()
    const updateEnabled = vi.fn(async () => true)
    render(
      <AccountSettings
        fetchAccessToken={async () => "token"}
        onDeleted={async () => undefined}
        reauthenticate={async () => undefined}
        retention={retention({
          disclosureRequired: true,
          enabled: true,
          updateEnabled,
        })}
      />,
    )

    const preference = screen.getByRole("checkbox", {
      name: "Help improve coaching",
    })
    expect(preference).toHaveProperty("checked", true)
    // The watercolor checkbox hides its input behind the painted mark, so the
    // Player toggles it through the label — as the pointer does.
    await user.click(screen.getByText("Help improve coaching"))
    expect(updateEnabled).toHaveBeenCalledWith(false)
  })

  test("hides Help improve coaching when the Quality Capture Preference is unavailable", () => {
    render(
      <AccountSettings
        fetchAccessToken={async () => "token"}
        onDeleted={async () => undefined}
        reauthenticate={async () => undefined}
        retention={retention({ available: false, disclosureRequired: true })}
      />,
    )

    expect(
      screen.queryByRole("checkbox", { name: "Help improve coaching" }),
    ).toBeNull()
  })

  test("surfaces a failed Quality Capture Preference write", () => {
    render(
      <AccountSettings
        fetchAccessToken={async () => "token"}
        onDeleted={async () => undefined}
        reauthenticate={async () => undefined}
        retention={retention({
          failure: "Review retention preference is unavailable.",
        })}
      />,
    )

    expect(screen.getByRole("alert").textContent).toContain(
      "Review retention preference is unavailable.",
    )
  })

  test("keeps account deletion when a preference read fails", () => {
    render(
      <AccountSettings
        fetchAccessToken={async () => "token"}
        onDeleted={async () => undefined}
        reauthenticate={async () => undefined}
        retention={retention({
          available: false,
          failure: "Review retention preference is unavailable.",
        })}
      />,
    )

    expect(
      screen.queryByRole("checkbox", { name: "Help improve coaching" }),
    ).toBeNull()
    expect(screen.getByRole("alert").textContent).toContain(
      "Review retention preference is unavailable.",
    )
    expect(screen.getByRole("heading", { name: "Delete account" })).toBeTruthy()
  })
})
