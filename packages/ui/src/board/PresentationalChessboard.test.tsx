// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react"
import { parseBoardSquare } from "../contracts"
import { afterEach, describe, expect, test } from "vitest"

import { PresentationalChessboard } from "./PresentationalChessboard"

afterEach(cleanup)

const pieces = (["white", "black"] as const).flatMap((color, colorIndex) =>
  (["king", "queen", "bishop", "knight", "rook", "pawn"] as const).map(
    (role, roleIndex) => ({
      color,
      role,
      square: parseBoardSquare(
        `${String.fromCharCode(97 + roleIndex)}${colorIndex + 1}`,
      ),
    }),
  ),
)

describe("presentational Coach App chessboard", () => {
  test.each(["white", "black"] as const)(
    "renders one sprite across all pieces in %s orientation",
    (orientation) => {
      const { container } = render(
        <PresentationalChessboard
          arrows={[
            {
              from: "e2",
              label: "Engine",
              to: "e4",
              tone: "engine",
            },
            {
              from: "g1",
              label: "Elo 1246 player",
              to: "f3",
              tone: "peer",
            },
          ]}
          board={{
            announcement: "White to move. e4 was the last move.",
            checkSquare: "a2",
            disabled: false,
            fen: "fixture",
            id: `board-${orientation}`,
            lastMove: { from: "e2", to: "e4" },
            legalDestinations: ["e3"],
            orientation,
            pieces,
            promotion: null,
            selectedSquare: "e2",
          }}
        />,
      )

      const board = screen.getByRole("img", {
        name: /Chessboard\. White to move/,
      })
      expect(board.getAttribute("data-orientation")).toBe(orientation)
      expect(container.querySelectorAll("[data-square]")).toHaveLength(64)
      expect(container.querySelectorAll(".coach-board-piece")).toHaveLength(12)
      expect(
        new Set(
          [
            ...container.querySelectorAll<HTMLElement>(".coach-board-piece"),
          ].map((piece) => piece.style.backgroundImage),
        ).size,
      ).toBe(1)
      expect(
        container.querySelectorAll('[data-last-move="true"]'),
      ).toHaveLength(2)
      expect(
        container
          .querySelector('[data-check="true"]')
          ?.getAttribute("data-square"),
      ).toBe("a2")
      expect(container.querySelectorAll("[data-arrow-label]")).toHaveLength(2)
      expect(board.textContent).toContain("Engine, e2 to e4")
      expect(board.textContent).toContain("Elo 1246 player, g1 to f3")
      expect(board.textContent).not.toMatch(/stockfish|maia|best reply/i)
      expect(container.querySelector("button")).toBeNull()
      expect(container.querySelector("[draggable]")).toBeNull()
      expect(container.textContent).not.toContain("e3")
    },
  )
})
