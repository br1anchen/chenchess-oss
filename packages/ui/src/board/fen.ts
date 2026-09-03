import type { BoardPiece } from "../contracts"

const boardFiles = ["a", "b", "c", "d", "e", "f", "g", "h"] as const
const boardRanks = ["8", "7", "6", "5", "4", "3", "2", "1"] as const
function roleFromFenSymbol(symbol: string): BoardPiece["role"] | undefined {
  switch (symbol.toLowerCase()) {
    case "b":
      return "bishop"
    case "k":
      return "king"
    case "n":
      return "knight"
    case "p":
      return "pawn"
    case "q":
      return "queen"
    case "r":
      return "rook"
    default:
      return undefined
  }
}

export function piecesFromFen(fen: string): readonly BoardPiece[] {
  const placement = fen.split(" ", 1)[0] ?? ""
  const ranks = placement.split("/")
  if (ranks.length !== 8) {
    throw new Error(`FEN must have eight ranks: ${fen}`)
  }

  const pieces: BoardPiece[] = []
  for (const [rankIndex, symbols] of ranks.entries()) {
    let fileIndex = 0
    for (const symbol of symbols) {
      if (symbol >= "1" && symbol <= "8") {
        fileIndex += Number(symbol)
        continue
      }
      const file = boardFiles[fileIndex]
      const rank = boardRanks[rankIndex]
      const role = roleFromFenSymbol(symbol)
      if (!file || !rank || !role) {
        throw new Error(`Invalid FEN symbol "${symbol}": ${fen}`)
      }
      pieces.push({
        color: symbol === symbol.toLowerCase() ? "black" : "white",
        role,
        square: `${file}${rank}`,
      })
      fileIndex += 1
    }
    if (fileIndex !== 8) {
      throw new Error(`FEN rank must have eight files: ${fen}`)
    }
  }
  return pieces
}
