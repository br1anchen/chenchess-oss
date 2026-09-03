import { mkdir, readFile, writeFile } from "node:fs/promises"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"

import {
  PIECE_COLORS,
  PIECE_ROLES,
  PIECE_VIEW_BOX,
  SPRITE_CELL_SIZE,
  SPRITE_VIEW_BOX,
  pieceFileStem,
} from "./chess-pieces"
import { assertVectorizeLockMatches } from "./vectorize-lock"

const packageRoot = fileURLToPath(new URL("..", import.meta.url))
const pieceDirectory = resolve(packageRoot, "src/assets/brand/chess-pieces")
const outputPath = resolve(pieceDirectory, "sprite.svg")

export function spriteMarkupFromPieces(
  pieces: ReadonlyArray<{ inner: string; x: number; y: number }>,
) {
  const images = pieces.map(
    ({ inner, x, y }) =>
      `<svg x="${String(x)}" y="${String(y)}" width="${String(SPRITE_CELL_SIZE)}" height="${String(SPRITE_CELL_SIZE)}" viewBox="${PIECE_VIEW_BOX}" preserveAspectRatio="xMidYMid meet">${inner}</svg>`,
  )
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${SPRITE_VIEW_BOX}">${images.join("")}</svg>\n`
}

export function pieceInnerMarkup(source: string, label: string) {
  if (source.includes("data:image/webp")) {
    throw new Error(
      `${label} still wraps a WebP raster; run bun run vectorize:chess-pieces`,
    )
  }
  if (!source.includes(`viewBox="${PIECE_VIEW_BOX}"`)) {
    throw new Error(`${label} must use viewBox ${PIECE_VIEW_BOX}`)
  }
  const match = source.match(/<svg\b[^>]*>([\s\S]*)<\/svg>\s*$/)
  const inner = match?.[1]
    ?.replace(/<title\b[^>]*>[\s\S]*?<\/title>/g, "")
    .trim()
  if (!inner || !/<path\b/.test(inner)) {
    throw new Error(`${label} is not a vector SVG`)
  }
  return inner
}

async function generatePieceSprite() {
  const pieces = []
  for (const [row, color] of PIECE_COLORS.entries()) {
    for (const [column, role] of PIECE_ROLES.entries()) {
      const label = pieceFileStem(color, role)
      const source = await readFile(
        resolve(pieceDirectory, `${label}.svg`),
        "utf8",
      )
      pieces.push({
        inner: pieceInnerMarkup(source, label),
        x: column * SPRITE_CELL_SIZE,
        y: row * SPRITE_CELL_SIZE,
      })
    }
  }
  return spriteMarkupFromPieces(pieces)
}

if (import.meta.main) {
  const sprite = await generatePieceSprite()
  if (process.argv.includes("--check")) {
    await assertVectorizeLockMatches(pieceDirectory)
    const current = await readFile(outputPath, "utf8").catch(() => "")
    if (current !== sprite) {
      throw new Error(
        "Piece sprite is stale; run bun run generate:piece-sprite from packages/ui",
      )
    }
    process.stdout.write("verified generated UI piece sprite\n")
  } else {
    await mkdir(dirname(outputPath), { recursive: true })
    await writeFile(outputPath, sprite)
    process.stdout.write(`generated ${outputPath}\n`)
  }
}
