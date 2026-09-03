#!/usr/bin/env node

import { execFileSync } from "node:child_process"
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs"
import { tmpdir } from "node:os"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const designRoot = dirname(fileURLToPath(import.meta.url))
const repoRoot = resolve(designRoot, "../../..")
const assetRoot = join(repoRoot, "packages/ui/src/assets/brand")
const brandBoard = join(designRoot, "chenchess-brand-system-reference.jpg")
const applicationTarget = join(
  designRoot,
  "chenchess-workspace-application-target.jpg",
)
const workRoot = mkdtempSync(join(tmpdir(), "chenchess-brand-assets-"))

const magick = (...args) =>
  execFileSync(process.env.CHEN_CHESS_MAGICK ?? "magick", args, {
    stdio: "inherit",
  })

const toDataUri = (path) =>
  `data:image/webp;base64,${readFileSync(path).toString("base64")}`

function cropOpaque(source, geometry, output, extra = []) {
  magick(
    source,
    "-crop",
    geometry,
    "+repage",
    ...extra,
    "-colorspace",
    "sRGB",
    "-strip",
    "-quality",
    "92",
    output,
  )
}

function cropTransparent(source, geometry, output, width, height, extra = []) {
  magick(
    source,
    "-crop",
    geometry,
    "+repage",
    ...extra,
    "-alpha",
    "on",
    "-fuzz",
    "3%",
    "-fill",
    "none",
    "-draw",
    "alpha 0,0 floodfill",
    "-channel",
    "A",
    "-morphology",
    "Open",
    "Disk:1",
    "-morphology",
    "Close",
    "Disk:3",
    "+channel",
    "-resize",
    `${width}x${height}`,
    "-gravity",
    "center",
    "-background",
    "none",
    "-extent",
    `${width}x${height}`,
    "-strip",
    "-define",
    "webp:lossless=true",
    output,
  )
}

function writeRasterSvg({ destination, title, viewBox, raster, seal = false }) {
  const [, , width, height] = viewBox.split(" ")
  const sealAttribute = seal ? ' data-seal-codepoint="U+9673"' : ""
  writeFileSync(
    destination,
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${viewBox}" role="img" aria-labelledby="title"${sealAttribute}>
  <title id="title">${title}</title>
  <image width="${width}" height="${height}" preserveAspectRatio="xMidYMid meet" href="${toDataUri(raster)}"/>
</svg>
`,
  )
}

function writeCompactLogo(destination, mark, wordmark) {
  writeFileSync(
    destination,
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 360 420" role="img" aria-labelledby="title" data-seal-codepoint="U+9673">
  <title id="title">ChenChess compact logo</title>
  <image x="30" y="0" width="300" height="300" preserveAspectRatio="xMidYMid meet" href="${toDataUri(mark)}"/>
  <image x="25" y="310" width="310" height="96" preserveAspectRatio="xMidYMid meet" href="${toDataUri(wordmark)}"/>
</svg>
`,
  )
}

function generateIdentity() {
  const primary = join(workRoot, "primary.webp")
  const wordmark = join(workRoot, "wordmark.webp")
  const mark = join(workRoot, "mark.webp")
  const markSmall = join(workRoot, "mark-small.webp")
  const monochrome = join(workRoot, "mark-monochrome.webp")
  const lightIcon = join(workRoot, "app-icon-light.webp")
  const darkIcon = join(workRoot, "app-icon-dark.webp")

  cropOpaque(applicationTarget, "500x120+20+7", primary)
  cropOpaque(applicationTarget, "310x96+190+25", wordmark)
  cropOpaque(brandBoard, "420x420+56+28", mark)
  magick(
    mark,
    "-resize",
    "64x64!",
    "-strip",
    "-define",
    "webp:lossless=true",
    markSmall,
  )
  cropOpaque(brandBoard, "420x420+56+28", monochrome, [
    "-colorspace",
    "Gray",
    "-colorspace",
    "sRGB",
  ])
  cropOpaque(brandBoard, "190x190+24+665", lightIcon)
  cropOpaque(brandBoard, "190x190+230+665", darkIcon)

  writeRasterSvg({
    destination: join(assetRoot, "logos/primary-horizontal.svg"),
    title: "ChenChess",
    viewBox: "0 0 500 120",
    raster: primary,
    seal: true,
  })
  writeCompactLogo(join(assetRoot, "logos/compact-stacked.svg"), mark, wordmark)
  writeRasterSvg({
    destination: join(assetRoot, "logos/mark.svg"),
    title: "ChenChess knight mark",
    viewBox: "0 0 420 420",
    raster: mark,
    seal: true,
  })
  writeRasterSvg({
    destination: join(assetRoot, "logos/mark-small.svg"),
    title: "ChenChess small knight mark",
    viewBox: "0 0 64 64",
    raster: markSmall,
  })
  writeRasterSvg({
    destination: join(assetRoot, "logos/monochrome.svg"),
    title: "ChenChess monochrome mark",
    viewBox: "0 0 256 256",
    raster: monochrome,
    seal: true,
  })
  writeRasterSvg({
    destination: join(assetRoot, "app-icons/app-icon-light.svg"),
    title: "ChenChess app icon on rice paper",
    viewBox: "0 0 512 512",
    raster: lightIcon,
    seal: true,
  })
  writeRasterSvg({
    destination: join(assetRoot, "app-icons/app-icon-dark.svg"),
    title: "ChenChess app icon on ink navy",
    viewBox: "0 0 512 512",
    raster: darkIcon,
    seal: true,
  })

  for (const [name, source] of [
    ["light", lightIcon],
    ["dark", darkIcon],
  ]) {
    for (const size of [180, 512]) {
      magick(
        source,
        "-resize",
        `${size}x${size}!`,
        "-strip",
        join(assetRoot, `app-icons/app-icon-${name}-${size}.png`),
      )
    }
  }
}

function generateValueIcons() {
  const icons = [
    {
      name: "see",
      title: "See what happened",
      geometry: "90x100+475+510",
    },
    {
      name: "understand",
      title: "Understand why it happened",
      geometry: "90x100+475+590",
    },
    {
      name: "improve",
      title: "Improve your game",
      geometry: "90x100+475+675",
    },
    {
      name: "enjoy",
      title: "Enjoy the journey",
      geometry: "90x100+475+760",
    },
  ]

  for (const icon of icons) {
    const raster = join(workRoot, `icon-${icon.name}.webp`)
    cropTransparent(brandBoard, icon.geometry, raster, 64, 64)
    writeRasterSvg({
      destination: join(assetRoot, `icons/${icon.name}.svg`),
      title: icon.title,
      viewBox: "0 0 64 64",
      raster,
    })
  }
}

function generatePieces() {
  const columns = {
    king: 90,
    queen: 270,
    bishop: 450,
    knight: 630,
    rook: 810,
    pawn: 990,
  }
  const rows = {
    white: { y: 880, height: 190 },
    black: { y: 1080, height: 174 },
  }

  for (const [color, row] of Object.entries(rows)) {
    for (const [role, x] of Object.entries(columns)) {
      const raster = join(workRoot, `${color}-${role}.webp`)
      const normalizeCrop =
        color === "white" && (role === "bishop" || role === "knight")
          ? [
              "-crop",
              "180x150+0+40",
              "+repage",
              "-background",
              "#F7F2E8",
              "-gravity",
              "south",
              "-extent",
              "180x190",
            ]
          : color === "black"
            ? [
                "-background",
                "#F7F2E8",
                "-gravity",
                "south",
                "-extent",
                "180x190",
              ]
            : []
      cropTransparent(
        brandBoard,
        `180x${row.height}+${x}+${row.y}`,
        raster,
        100,
        120,
        normalizeCrop,
      )
      mkdirSync(join(assetRoot, "chess-pieces/source"), { recursive: true })
      writeFileSync(
        join(assetRoot, `chess-pieces/source/${color}-${role}.webp`),
        readFileSync(raster),
      )
    }
  }
}

mkdirSync(assetRoot, { recursive: true })
try {
  generateIdentity()
  generateValueIcons()
  generatePieces()
  process.stdout.write(
    "wrote chess-piece WebP sources; run bun run --cwd packages/ui vectorize:chess-pieces\n",
  )
} finally {
  rmSync(workRoot, { recursive: true, force: true })
}
