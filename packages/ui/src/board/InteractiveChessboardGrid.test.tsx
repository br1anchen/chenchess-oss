// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, test } from "vitest"

import { workspaceFixture } from "../fixtures"
import { InteractiveChessboardGrid } from "./InteractiveChessboardGrid"

import type { BoardSquare } from "../contracts"

afterEach(cleanup)

describe("InteractiveChessboardGrid", () => {
  test("renders sixty-four squares and reports a legal destination click", async () => {
    const user = userEvent.setup()
    const squares: BoardSquare[] = []
    const { container } = render(
      <InteractiveChessboardGrid
        destinations={workspaceFixture.board.legalDestinations}
        disabled={false}
        lastMove={workspaceFixture.board.lastMove}
        onSquare={(square) => squares.push(square)}
        orientation={workspaceFixture.board.orientation}
        pieces={workspaceFixture.board.pieces}
        selectedSquare={workspaceFixture.board.selectedSquare}
      />,
    )

    expect(container.querySelectorAll("[data-square]")).toHaveLength(64)
    expect(
      screen
        .getByRole("gridcell", { name: "d4 white pawn" })
        .getAttribute("aria-selected"),
    ).toBe("true")
    await user.click(
      screen.getByRole("gridcell", {
        name: "d5 black queen, legal destination",
      }),
    )
    expect(squares).toEqual(["d5"])
    expect(screen.queryByLabelText(/White evaluation share/)).toBeNull()
  })

  test("flips files and ranks for black orientation and can show an eval bar", () => {
    const { rerender } = render(
      <InteractiveChessboardGrid
        destinations={[]}
        disabled={false}
        lastMove={null}
        onSquare={() => undefined}
        orientation="black"
        pieces={[{ color: "black", role: "king", square: "e8" }]}
        selectedSquare={null}
      />,
    )

    const first = screen.getAllByRole("gridcell")[0]
    expect(first?.getAttribute("data-square")).toBe("h1")
    expect(screen.getByRole("gridcell", { name: "e8 black king" })).toBeTruthy()

    rerender(
      <InteractiveChessboardGrid
        destinations={[]}
        disabled
        evaluationPercent={61.4}
        lastMove={null}
        onSquare={() => undefined}
        orientation="white"
        pieces={[]}
        selectedSquare={null}
      />,
    )
    expect(screen.getByLabelText("White evaluation share 61%")).toBeTruthy()
    expect(screen.getByRole("gridcell", { name: "a8 empty" })).toHaveProperty(
      "disabled",
      true,
    )
  })
})
