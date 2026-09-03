import { spawnSync } from "node:child_process"
import { mkdir, readFile, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"

import { optimize } from "svgo"

import {
  PIECE_COLORS,
  PIECE_ROLES,
  PIECE_VIEW_BOX,
  SPRITE_CELL_SIZE,
  pieceFileStem,
  pieceTitle,
} from "./chess-pieces"
import {
  pieceInnerMarkup,
  spriteMarkupFromPieces,
} from "./generate-piece-sprite"
import {
  VECTORIZE_ALPHA_CUTOFF,
  VECTORIZE_COLOR_PRECISION,
  VECTORIZE_FILTER_SPECKLE,
  VECTORIZE_GRADIENT_STEP,
  VECTORIZE_LOCK_NAME,
  VECTORIZE_PATH_PRECISION,
  formatVectorizeLock,
  readSourceWebpDigests,
  vectorizeLockFromSourceDigests,
} from "./vectorize-lock"

const packageRoot = fileURLToPath(new URL("..", import.meta.url))
const pieceDirectory = resolve(packageRoot, "src/assets/brand/chess-pieces")
const sourceDirectory = resolve(pieceDirectory, "source")
const spritePath = resolve(pieceDirectory, "sprite.svg")
const lockPath = resolve(pieceDirectory, VECTORIZE_LOCK_NAME)

function requiredCommand(name: string, fallbacks: readonly string[] = []) {
  const cargoHome = process.env.CARGO_HOME ?? "/usr/local/cargo"
  const candidates = [
    name,
    ...fallbacks,
    resolve(cargoHome, "bin", name),
    resolve(process.env.HOME ?? "", ".cargo/bin", name),
  ]
  for (const candidate of candidates) {
    const result = spawnSync(candidate, ["-version"], { encoding: "utf8" })
    if (result.status === 0 || result.status === 1) return candidate
    const help = spawnSync(candidate, ["--help"], { encoding: "utf8" })
    if (help.status === 0 || help.status === 1) return candidate
  }
  throw new Error(
    `Missing ${name}. Install it with cargo install vtracer before running bun run vectorize:chess-pieces.`,
  )
}

function run(command: string, args: readonly string[]) {
  const result = spawnSync(command, args, { encoding: "utf8" })
  if (result.status !== 0) {
    throw new Error(
      `${command} ${args.join(" ")} failed: ${result.stderr || result.stdout}`,
    )
  }
}

function wrapTracedSvg(traced: string, title: string) {
  const inner = traced
    .replace(/^[\s\S]*?<svg\b[^>]*>/i, "")
    .replace(/<\/svg>\s*$/i, "")
    .trim()
  if (!/<path\b/.test(inner)) {
    throw new Error(`${title} VTracer output has no paths`)
  }
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${PIECE_VIEW_BOX}" role="img">
  <title>${title}</title>
  ${inner}
</svg>
`
}

function minifyPieceSvg(source: string, label: string) {
  const minified = optimize(source, {
    multipass: true,
    plugins: [
      {
        name: "preset-default",
        params: {
          overrides: {
            cleanupIds: false,
            removeUnknownsAndDefaults: {
              keepRoleAttr: true,
            },
          },
        },
      },
    ],
  }).data
  if (
    !minified.includes(`viewBox="${PIECE_VIEW_BOX}"`) ||
    !/<path\b/.test(minified)
  ) {
    throw new Error(`${label} SVGO output is not a controlled vector piece`)
  }
  return minified.endsWith("\n") ? minified : `${minified}\n`
}

async function vectorizePieces() {
  const ffmpeg = requiredCommand("ffmpeg")
  const vtracer = requiredCommand("vtracer")
  const sourceDigests = await readSourceWebpDigests(sourceDirectory)
  const workRoot = resolve(
    tmpdir(),
    `chenchess-piece-vectorize-${String(process.pid)}`,
  )
  await mkdir(workRoot, { recursive: true })
  const pieces = []
  for (const [row, color] of PIECE_COLORS.entries()) {
    for (const [column, role] of PIECE_ROLES.entries()) {
      const stem = pieceFileStem(color, role)
      const webpPath = resolve(sourceDirectory, `${stem}.webp`)
      const pngPath = resolve(workRoot, `${stem}.png`)
      const tracedPath = resolve(workRoot, `${stem}.svg`)
      const outputPath = resolve(pieceDirectory, `${stem}.svg`)
      run(ffmpeg, [
        "-y",
        "-i",
        webpPath,
        "-vf",
        `format=rgba,geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='if(lt(alpha(X,Y),${String(VECTORIZE_ALPHA_CUTOFF)}),0,255)'`,
        pngPath,
      ])
      run(vtracer, [
        "--input",
        pngPath,
        "--output",
        tracedPath,
        "--colormode",
        "color",
        "--hierarchical",
        "stacked",
        "--mode",
        "spline",
        "--filter_speckle",
        String(VECTORIZE_FILTER_SPECKLE),
        "--color_precision",
        String(VECTORIZE_COLOR_PRECISION),
        "--gradient_step",
        String(VECTORIZE_GRADIENT_STEP),
        "--path_precision",
        String(VECTORIZE_PATH_PRECISION),
      ])
      const minified = minifyPieceSvg(
        wrapTracedSvg(
          await readFile(tracedPath, "utf8"),
          pieceTitle(color, role),
        ),
        stem,
      )
      await writeFile(outputPath, minified)
      pieces.push({
        inner: pieceInnerMarkup(minified, stem),
        x: column * SPRITE_CELL_SIZE,
        y: row * SPRITE_CELL_SIZE,
      })
      process.stdout.write(`vectorized ${outputPath}\n`)
    }
  }
  await mkdir(dirname(spritePath), { recursive: true })
  await writeFile(spritePath, spriteMarkupFromPieces(pieces))
  await writeFile(
    lockPath,
    formatVectorizeLock(vectorizeLockFromSourceDigests(sourceDigests)),
  )
  process.stdout.write(`generated ${spritePath}\n`)
  process.stdout.write(`wrote ${lockPath}\n`)
}

if (import.meta.main) {
  await vectorizePieces()
}
