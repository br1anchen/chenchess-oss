// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, test } from "vitest"
import type { ReactNode } from "react"

import { ChenTheme } from "../theme/ChenTheme"
import {
  BrandedReviewWorkspace,
  ReviewFocusCard,
} from "./BrandedReviewWorkspace"

function renderTheme(ui: ReactNode) {
  return render(<ChenTheme>{ui}</ChenTheme>)
}

afterEach(cleanup)

describe("BrandedReviewWorkspace", () => {
  test("uses a two-column board and coaching layout when conversation is absent", () => {
    renderTheme(
      <BrandedReviewWorkspace
        board={<section aria-label="Shared board" />}
        coaching={<section aria-label="Shared coaching" />}
        title="Game Review"
      />,
    )

    const workspace = screen.getByRole("main")
    expect(workspace.dataset.hasConversation).toBeUndefined()
    expect(screen.queryByLabelText("Shared conversation")).toBeNull()
    expect(screen.getByText("ChenChess")).toBeTruthy()
  })

  test("composes board, coaching, and conversation in the shared shell", () => {
    renderTheme(
      <BrandedReviewWorkspace
        board={<section aria-label="Shared board" />}
        coaching={<section aria-label="Shared coaching" />}
        conversation={<section aria-label="Shared conversation" />}
        title="Review Session"
      />,
    )

    const workspace = screen.getByRole("main")
    const coaching = screen.getByLabelText("Coaching review")
    const board = screen.getByLabelText("Shared board")

    expect(workspace.dataset.hasConversation).toBe("true")
    expect(
      coaching.compareDocumentPosition(board) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy()
    expect(screen.getByLabelText("Shared conversation")).toBeTruthy()
    // The workspace names the product in words. This snapshot ships no mark
    // artwork, so there is nothing here to assert about icons.
    expect(screen.getByText("ChenChess")).toBeTruthy()
  })

  test("renders the branded review focus treatment", () => {
    renderTheme(
      <ReviewFocusCard
        description="Compare the candidate moves."
        moveLabel="5… Nf6"
        title="Positive Highlight"
        tone="positive"
      />,
    )

    expect(
      screen.getByRole("heading", { name: "Positive Highlight" }),
    ).toBeTruthy()
    expect(screen.getByText("5… Nf6")).toBeTruthy()
    expect(screen.getByText("Compare the candidate moves.")).toBeTruthy()
    expect(document.querySelector(".chen-review-companion")).toBeNull()
  })
})
