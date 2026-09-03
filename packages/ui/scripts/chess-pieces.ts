export const PIECE_COLORS = ["white", "black"] as const
export const PIECE_ROLES = [
  "king",
  "queen",
  "bishop",
  "knight",
  "rook",
  "pawn",
] as const

export type PieceColor = (typeof PIECE_COLORS)[number]
export type PieceRole = (typeof PIECE_ROLES)[number]

export const PIECE_VIEW_BOX = "0 0 100 120" as const
export const PIECE_CELL_WIDTH = 100
export const PIECE_CELL_HEIGHT = 120
export const SPRITE_CELL_SIZE = 100
export const SPRITE_COLUMNS = PIECE_ROLES.length
export const SPRITE_ROWS = PIECE_COLORS.length
export const SPRITE_VIEW_BOX = `0 0 ${SPRITE_COLUMNS * SPRITE_CELL_SIZE} ${SPRITE_ROWS * SPRITE_CELL_SIZE}`

export function pieceFileStem(color: PieceColor, role: PieceRole) {
  return `${color}-${role}`
}

export function pieceTitle(color: PieceColor, role: PieceRole) {
  return `${color[0]?.toUpperCase()}${color.slice(1)} ${role}`
}

export function pieceInventory() {
  return PIECE_COLORS.flatMap((color) =>
    PIECE_ROLES.map((role) => ({ color, role })),
  )
}
