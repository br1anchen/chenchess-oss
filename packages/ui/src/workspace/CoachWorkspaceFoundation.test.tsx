// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useReducer } from "react"
import { afterEach, describe, expect, test } from "vitest"

import type { WorkspaceAction } from "../contracts"
import { reduceWorkspaceFixture, workspaceFixture } from "../fixtures"
import { ChenMotionProvider } from "../motion"
import { ChenTheme } from "../theme/ChenTheme"
import { CoachWorkspaceFoundation } from "./CoachWorkspaceFoundation"

function Workspace({ disclosure = false }: { disclosure?: boolean }) {
  const [model, dispatch] = useReducer(reduceWorkspaceFixture, {
    ...workspaceFixture,
    retention: {
      ...workspaceFixture.retention,
      disclosureRequired: disclosure,
    },
  })
  return (
    <ChenTheme>
      <ChenMotionProvider>
        <CoachWorkspaceFoundation model={model} onAction={dispatch} />
      </ChenMotionProvider>
    </ChenTheme>
  )
}

afterEach(cleanup)

describe("fixture workspace journey", () => {
  test("requests a board move from a legal destination on the owned grid", async () => {
    const user = userEvent.setup()
    const actions: WorkspaceAction[] = []
    render(
      <ChenTheme>
        <ChenMotionProvider>
          <CoachWorkspaceFoundation
            model={{
              ...workspaceFixture,
              retention: {
                ...workspaceFixture.retention,
                disclosureRequired: false,
              },
            }}
            onAction={(action) => actions.push(action)}
          />
        </ChenMotionProvider>
      </ChenTheme>,
    )

    await user.click(
      screen.getByRole("gridcell", {
        name: "d5 black queen, legal destination",
      }),
    )
    expect(actions).toEqual([
      { type: "boardMoveRequested", move: { from: "d4", to: "d5" } },
    ])
  })

  test("keeps the uncertain hypothesis in comment prose without lifecycle controls", () => {
    render(<Workspace />)

    expect(
      screen.getByText(/Keeping the queen flexible makes the next few moves/),
    ).toBeTruthy()
    for (const control of [
      "Confirm",
      "Correct",
      "Another idea",
      "Discuss",
      "Skip",
    ]) {
      expect(screen.queryByRole("button", { name: control })).toBeNull()
    }
    expect(screen.queryByText(/confidence/i)).toBeNull()
    expect(screen.queryByText(/trace/i)).toBeNull()
  })

  test("navigates moments without rendering separate intent state", async () => {
    const user = userEvent.setup()
    render(<Workspace />)

    await user.click(screen.getByRole("button", { name: /12… c6/ }))
    expect(
      screen.getByText(
        "The most common choices at your rating were unavailable; objective evidence remains visible.",
      ),
    ).toBeTruthy()
    expect(screen.getByText(/does not invent a plan/)).toBeTruthy()
    expect(screen.queryByText("Move Intent unavailable")).toBeNull()
    expect(
      screen.queryByText(/human-likely|human model|move model|maia/i),
    ).toBeNull()
  })

  test("cancellation is immediate and restores focus", async () => {
    const user = userEvent.setup()
    render(<Workspace />)

    await user.click(screen.getByRole("button", { name: "Cancel active work" }))
    const heading = screen.getByRole("heading", { name: "Alternative moves" })
    expect(document.activeElement).toBe(heading)
    expect(
      screen.getByText(
        "Active coaching work cancelled. Focus returned to alternatives.",
      ),
    ).toBeTruthy()
  })

  test("hands an evaluated Alternative Move question to chat without starting work", async () => {
    const user = userEvent.setup()
    render(<Workspace />)

    await user.click(screen.getByRole("button", { name: /Nf6/ }))
    await user.type(
      screen.getByLabelText("Ask about this alternative"),
      "Why is this move practical?",
    )
    await user.click(screen.getByRole("button", { name: "Ask coach" }))

    expect(screen.getByText("Alternative Move handed to chat.")).toBeTruthy()
    expect(screen.queryByText(/Coach Turn requested/)).toBeNull()
  })

  test("retention changes immediately and the disclosure offers withdrawal", async () => {
    const user = userEvent.setup()
    const { rerender } = render(<Workspace />)

    const preference = screen.getByRole("checkbox", {
      name: "Help improve coaching",
    })
    await user.click(preference)
    expect(
      screen.getByRole("checkbox", { name: "Help improve coaching" }),
    ).toHaveProperty("checked", false)

    rerender(<Workspace key="disclosure" disclosure />)
    expect(await screen.findByRole("dialog")).toBeTruthy()
    await user.click(
      screen.getByRole("button", { name: "Turn off and continue" }),
    )
    expect(screen.queryByRole("dialog")).toBeNull()
  })
})
