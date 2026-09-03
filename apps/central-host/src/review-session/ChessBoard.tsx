import {
  fromSquare,
  type PositionSnapshot,
  type Square,
} from "@chenchess/coach-engine-sdk"
import {
  InteractiveChessboardGrid,
  parseBoardMove,
  parseBoardSquare,
  type BoardArrow,
  type BoardMove,
  type BoardOrientation,
  type BoardPiece,
  type BoardSquareMark,
} from "@chenchess/ui"

type ChessBoardProps = {
  position: Pick<PositionSnapshot, "occupied">
  orientation: BoardOrientation
  arrows?: readonly BoardArrow[]
  evaluationPercent?: number
  selectedSquare: Square | null
  destinations: Square[]
  fill?: boolean
  lastMove: string | null
  marks?: readonly BoardSquareMark[]
  disabled: boolean
  onSquare: (square: Square) => void
}

export function ChessBoard({
  position,
  orientation,
  arrows,
  evaluationPercent,
  selectedSquare,
  destinations,
  fill = false,
  lastMove,
  marks,
  disabled,
  onSquare,
}: ChessBoardProps) {
  return (
    <InteractiveChessboardGrid
      arrows={arrows}
      destinations={destinations.map(parseBoardSquare)}
      disabled={disabled}
      fill={fill}
      evaluationPercent={evaluationPercent}
      lastMove={boardMove(lastMove)}
      marks={marks}
      onSquare={(square) => onSquare(fromSquare(square))}
      orientation={orientation}
      pieces={position.occupied.map(
        ({ piece, square }): BoardPiece => ({
          ...piece,
          square: parseBoardSquare(square),
        }),
      )}
      selectedSquare={
        selectedSquare === null ? null : parseBoardSquare(selectedSquare)
      }
    />
  )
}

function boardMove(move: string | null): BoardMove | null {
  if (!move || move.length < 4) return null
  return parseBoardMove({ from: move.slice(0, 2), to: move.slice(2, 4) })
}
