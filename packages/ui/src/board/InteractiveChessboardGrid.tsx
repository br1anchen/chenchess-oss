import * as stylex from "@stylexjs/stylex"
import type { KeyboardEvent, ReactNode } from "react"
import { useId, useMemo } from "react"

import { chessPieceSpriteBackground } from "../chessPieceSprite"
import {
  parseBoardSquare,
  type BoardArrow,
  type BoardMove,
  type BoardOrientation,
  type BoardPiece,
  type BoardSquare,
  type BoardSquareMark,
} from "../contracts"

import { BoardArrowLayer, describeBoardArrows } from "./BoardArrowLayer"
import { BoardMarkLayer, describeBoardMarks } from "./BoardMarkLayer"
import { boardStyles } from "./InteractiveChessboardGrid.styles"

/** Keeps the structural class hooks alongside the compiled StyleX classes;
 * WatercolorBoard.css skins the squares and pieces through them. */
function craft(
  hooks: ReadonlyArray<string | false>,
  ...styles: ReadonlyArray<object | false | null | undefined>
) {
  // SAFETY: every argument is compiled StyleX from
  // InteractiveChessboardGrid.styles.ts; the published prop types cannot
  // express the authored style objects.
  const applied = stylex.props(...(styles as never[]))
  return {
    ...applied,
    className: [...hooks, applied.className].filter(Boolean).join(" "),
  }
}

export type InteractiveChessboardGridProps = {
  /** Move arrows drawn over the position (engine/Maia comparison). */
  arrows?: readonly BoardArrow[]
  boardFooter?: ReactNode
  destinations: readonly BoardSquare[]
  disabled: boolean
  evaluationPercent?: number
  /** Size the grid to its parent leftover box instead of the 820px cap. */
  fill?: boolean
  lastMove: BoardMove | null
  /** Squares the coach singled out about this position (ADR 0059). */
  marks?: readonly BoardSquareMark[]
  onSquare: (square: BoardSquare) => void
  orientation: BoardOrientation
  pieces: readonly BoardPiece[]
  selectedSquare: BoardSquare | null
}

const files = ["a", "b", "c", "d", "e", "f", "g", "h"] as const
const ranks = ["8", "7", "6", "5", "4", "3", "2", "1"] as const

export function InteractiveChessboardGrid({
  arrows,
  boardFooter,
  destinations,
  disabled,
  evaluationPercent,
  fill = false,
  lastMove,
  marks,
  onSquare,
  orientation,
  pieces,
  selectedSquare,
}: InteractiveChessboardGridProps) {
  const arrowDescriptionId = useId()
  const shownArrows = arrows ?? []
  const shownMarks = marks ?? []
  const described = [
    shownArrows.length > 0 ? describeBoardArrows(shownArrows) : "",
    shownMarks.length > 0 ? describeBoardMarks(shownMarks) : "",
  ]
    .filter(Boolean)
    .join(" ")
  const squares = useMemo(() => boardSquares(orientation), [orientation])
  const piecesBySquare = useMemo(
    () => new Map(pieces.map((piece) => [piece.square, piece])),
    [pieces],
  )
  const lastMoveSquares = lastMove ? [lastMove.from, lastMove.to] : []
  const destinationSet = new Set(destinations)
  const board = (
    <div
      aria-describedby={described ? arrowDescriptionId : undefined}
      aria-label="Chess position"
      {...craft(
        ["chen-workspace-board"],
        boardStyles.board,
        fill && boardStyles.fillBoard,
      )}
      onKeyDown={(event) => moveBoardFocus(event, squares)}
      role="grid"
      tabIndex={0}
    >
      {squares.map((square, index) => {
        const piece = piecesBySquare.get(square)
        const isLight = (index + Math.floor(index / 8)) % 2 === 0
        const isDestination = destinationSet.has(square)
        const isSelected = selectedSquare === square
        const isLastMove = lastMoveSquares.includes(square)
        return (
          <button
            aria-label={`${square}${
              piece ? ` ${piece.color} ${piece.role}` : " empty"
            }${isDestination ? ", legal destination" : ""}`}
            aria-selected={isSelected}
            {...craft(
              [
                "chen-workspace-square",
                isLight
                  ? "chen-workspace-square-light"
                  : "chen-workspace-square-dark",
                isSelected && "chen-workspace-square-selected",
                isDestination && "chen-workspace-square-destination",
                isLastMove && "chen-workspace-square-last",
              ],
              boardStyles.square,
              isLastMove && boardStyles.squareLast,
              isSelected && boardStyles.squareSelected,
              isDestination && boardStyles.squareDestination,
            )}
            data-square={square}
            disabled={disabled}
            key={square}
            onClick={() => onSquare(square)}
            role="gridcell"
            type="button"
          >
            {piece ? (
              <span
                aria-hidden="true"
                {...craft(["chen-workspace-piece"], boardStyles.piece)}
                data-piece-color={piece.color}
                data-piece-role={piece.role}
                style={chessPieceSpriteBackground()}
              />
            ) : null}
            {isDestination ? (
              <span
                aria-hidden="true"
                {...craft(
                  ["chen-workspace-destination"],
                  boardStyles.destination,
                )}
              />
            ) : null}
            {index % 8 === 0 ? (
              <span
                {...craft(
                  ["chen-workspace-rank-label"],
                  boardStyles.coordinate,
                  boardStyles.rankCoordinate,
                )}
              >
                {square[1]}
              </span>
            ) : null}
            {index >= 56 ? (
              <span
                {...craft(
                  ["chen-workspace-file-label"],
                  boardStyles.coordinate,
                  boardStyles.fileCoordinate,
                )}
              >
                {square[0]}
              </span>
            ) : null}
          </button>
        )
      })}
      {shownMarks.length > 0 ? (
        <BoardMarkLayer marks={shownMarks} orientation={orientation} />
      ) : null}
      {shownArrows.length > 0 ? (
        <BoardArrowLayer arrows={shownArrows} orientation={orientation} />
      ) : null}
      {described ? (
        <span className="coach-sr-only" hidden id={arrowDescriptionId}>
          {described}
        </span>
      ) : null}
    </div>
  )
  const stagedBoard = boardFooter ? (
    <div
      {...craft(
        ["chen-workspace-board-stage"],
        boardStyles.boardCell,
        fill && boardStyles.fillCell,
      )}
    >
      {board}
      {boardFooter}
    </div>
  ) : (
    board
  )
  if (evaluationPercent === undefined) {
    return stagedBoard
  }
  return (
    <div
      {...craft(
        ["chen-workspace-board-row"],
        boardStyles.row,
        fill && boardStyles.fillRow,
      )}
    >
      <div
        aria-label={`White evaluation share ${Math.round(evaluationPercent)}%`}
        {...craft(["chen-workspace-eval-bar"], boardStyles.evalBar)}
      >
        <span
          {...stylex.props(boardStyles.evalFill)}
          style={{ height: `${evaluationPercent}%` }}
        />
      </div>
      {stagedBoard}
    </div>
  )
}

function boardSquares(orientation: BoardOrientation): BoardSquare[] {
  const black = orientation === "black"
  const orderedFiles = black ? [...files].reverse() : files
  const orderedRanks = black ? [...ranks].reverse() : ranks
  return orderedRanks.flatMap((rank) =>
    orderedFiles.map((file) => parseBoardSquare(`${file}${rank}`)),
  )
}

function moveBoardFocus(
  event: KeyboardEvent<HTMLDivElement>,
  squares: BoardSquare[],
) {
  const delta =
    event.key === "ArrowRight"
      ? 1
      : event.key === "ArrowLeft"
        ? -1
        : event.key === "ArrowDown"
          ? 8
          : event.key === "ArrowUp"
            ? -8
            : 0
  if (delta === 0) return
  const target = event.target
  if (!(target instanceof HTMLElement)) return
  const square = target.dataset.square
  if (!square) return
  const currentIndex = squares.findIndex((candidate) => candidate === square)
  if (currentIndex < 0) return
  const next = squares[currentIndex + delta]
  if (!next) return
  event.preventDefault()
  event.currentTarget
    .querySelector<HTMLElement>(`[data-square="${next}"]`)
    ?.focus()
}
