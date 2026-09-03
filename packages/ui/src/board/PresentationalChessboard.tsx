import {
  parseBoardFile,
  parseBoardRank,
  parseBoardSquare,
  type BoardArrow,
  type BoardMove,
  type BoardOrientation,
  type BoardPresentation,
  type BoardSquare,
} from "../contracts"
import type { CSSProperties } from "react"
import { memo, useId } from "react"

import { chessPieceSpriteBackground } from "../chessPieceSprite"

import { BoardArrowLayer, describeBoardArrows } from "./BoardArrowLayer"

const files = ["a", "b", "c", "d", "e", "f", "g", "h"] as const
const ranks = ["1", "2", "3", "4", "5", "6", "7", "8"] as const

export const PresentationalChessboard = memo(function PresentationalChessboard({
  arrows,
  board,
  transition,
}: {
  arrows: readonly BoardArrow[]
  board: BoardPresentation
  transition?: BoardTransition
}) {
  const descriptionId = useId()
  const squares = orientedSquares(board.orientation)
  const pieces = piecesBySquare(board)
  const lastMove = new Set(
    board.lastMove ? [board.lastMove.from, board.lastMove.to] : [],
  )

  return (
    <div
      aria-describedby={descriptionId}
      aria-label={`Chessboard. ${board.announcement}`}
      className="coach-presentational-board"
      data-board-id={board.id}
      data-orientation={board.orientation}
      role="img"
    >
      <div aria-hidden="true" className="coach-board-grid">
        {squares.map((square, index) => {
          const squarePieces = pieces.get(square) ?? []
          return (
            <span
              className="coach-board-square"
              data-check={board.checkSquare === square ? "true" : undefined}
              data-last-move={lastMove.has(square) ? "true" : undefined}
              data-square={square}
              key={square}
            >
              {squarePieces.map((piece) => (
                <span
                  className="coach-board-piece"
                  data-moving={
                    transition &&
                    !transition.reducedMotion &&
                    piece.square === transition.move.to
                      ? "true"
                      : undefined
                  }
                  data-piece-color={piece.color}
                  data-piece-id={piece.pieceId}
                  data-piece-role={piece.role}
                  key={`${piece.pieceId}:${
                    transition?.move.to === piece.square
                      ? transition.epoch
                      : "static"
                  }`}
                  style={pieceStyle(piece, board.orientation, transition)}
                />
              ))}
              {coordinateLabel(index, square)}
            </span>
          )
        })}
      </div>
      <BoardArrowLayer arrows={arrows} orientation={board.orientation} />
      <span className="coach-sr-only" id={descriptionId}>
        {describeBoardArrows(arrows)}
      </span>
    </div>
  )
})

export type BoardTransition = {
  epoch: number
  move: BoardMove
  reducedMotion: boolean
}

type RenderPiece = {
  color: BoardPresentation["pieces"][number]["color"]
  pieceId: string
  role: BoardPresentation["pieces"][number]["role"]
  square: BoardSquare
}

function piecesBySquare(board: BoardPresentation) {
  const pieces = board.pieces.map(({ color, role, square }) => ({
    color,
    pieceId: `${color}:${role}:${square}`,
    role,
    square,
  }))
  const grouped = new Map<BoardSquare, RenderPiece[]>()
  for (const piece of pieces) {
    const squarePieces = grouped.get(piece.square) ?? []
    squarePieces.push(piece)
    grouped.set(piece.square, squarePieces)
  }
  return grouped
}

function pieceStyle(
  piece: RenderPiece,
  orientation: BoardOrientation,
  transition: BoardTransition | undefined,
) {
  const style: TransitionStyle = {
    ...chessPieceSpriteBackground(),
  }
  if (
    !transition ||
    transition.reducedMotion ||
    transition.move.to !== piece.square
  ) {
    return style
  }
  const offset = motionOffset(
    transition.move.from,
    transition.move.to,
    orientation,
  )
  style["--coach-piece-motion-x"] = `${offset.x}%`
  style["--coach-piece-motion-y"] = `${offset.y}%`
  return style
}

type TransitionStyle = CSSProperties & {
  "--coach-piece-motion-x"?: string
  "--coach-piece-motion-y"?: string
}

function motionOffset(
  from: BoardSquare,
  to: BoardSquare,
  orientation: BoardOrientation,
) {
  const start = squareGridPosition(from, orientation)
  const end = squareGridPosition(to, orientation)
  return {
    x: (start.column - end.column) * 100,
    y: (start.row - end.row) * 100,
  }
}

function squareGridPosition(
  square: BoardSquare,
  orientation: BoardOrientation,
) {
  const file = files.indexOf(parseBoardFile(square[0]))
  const rank = ranks.indexOf(parseBoardRank(square[1]))
  return orientation === "white"
    ? { column: file, row: 7 - rank }
    : { column: 7 - file, row: rank }
}

function orientedSquares(orientation: BoardOrientation) {
  const orientedFiles = orientation === "white" ? files : [...files].reverse()
  const orientedRanks = orientation === "white" ? [...ranks].reverse() : ranks
  return orientedRanks.flatMap((rank) =>
    orientedFiles.map((file) => parseBoardSquare(`${file}${rank}`)),
  )
}

// Ranks label the left column and files label the bottom row. The corner square
// carries both, so they are separate elements pinned to opposite corners
// instead of one span that would read as a single run-together word.
function coordinateLabel(index: number, square: BoardSquare) {
  const row = Math.floor(index / 8)
  const column = index % 8
  if (row !== 7 && column !== 0) return null
  return (
    <>
      {column === 0 ? (
        <span className="coach-board-coordinate" data-axis="rank">
          {square[1]}
        </span>
      ) : null}
      {row === 7 ? (
        <span className="coach-board-coordinate" data-axis="file">
          {square[0]}
        </span>
      ) : null}
    </>
  )
}
