import {
  fromSquare,
  type ReviewSessionPresentationPiece,
} from "@chenchess/coach-engine-sdk"

export function presentationPiecesFromFen(
  fen: string,
): ReviewSessionPresentationPiece[] {
  const placement = fen.split(" ", 1)[0] ?? ""
  const ranks = placement.split("/")
  if (ranks.length !== 8) {
    throw new Error("Presentation FEN must have eight ranks")
  }
  const pieces: ReviewSessionPresentationPiece[] = []
  for (const [rankIndex, symbols] of ranks.entries()) {
    let fileIndex = 0
    for (const symbol of symbols) {
      if (symbol >= "1" && symbol <= "8") {
        fileIndex += Number(symbol)
        continue
      }
      const role = fenRole(symbol)
      if (!role || fileIndex > 7) {
        throw new Error("Presentation FEN contains an invalid piece")
      }
      const square = fromSquare(`${"abcdefgh"[fileIndex]}${8 - rankIndex}`)
      const color = symbol === symbol.toLowerCase() ? "black" : "white"
      pieces.push({
        piece: { color, role },
        pieceId: `${color}:${role}:${square}`,
        square,
      })
      fileIndex += 1
    }
    if (fileIndex !== 8) {
      throw new Error("Presentation FEN rank must have eight files")
    }
  }
  return pieces
}

function fenRole(symbol: string) {
  switch (symbol.toLowerCase()) {
    case "b":
      return "bishop" as const
    case "k":
      return "king" as const
    case "n":
      return "knight" as const
    case "p":
      return "pawn" as const
    case "q":
      return "queen" as const
    case "r":
      return "rook" as const
    default:
      return undefined
  }
}
