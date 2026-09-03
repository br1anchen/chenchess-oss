import type { CSSProperties } from "react"

import pieceSprite from "./assets/brand/chess-pieces/sprite.svg?url"

export function chessPieceSpriteBackground(): CSSProperties {
  return { backgroundImage: `url("${pieceSprite}")` }
}
