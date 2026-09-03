import { createHash } from "node:crypto"
import { readFile } from "node:fs/promises"
import { resolve } from "node:path"

import {
  PIECE_COLORS,
  PIECE_ROLES,
  PIECE_VIEW_BOX,
  pieceInventory,
  type PieceColor,
  type PieceRole,
} from "./chess-pieces"

export const VECTORIZE_LOCK_NAME = "vectorize.lock.json"
export const VECTORIZE_ALPHA_CUTOFF = 40
export const VECTORIZE_COLOR_PRECISION = 6
export const VECTORIZE_GRADIENT_STEP = 16
export const VECTORIZE_FILTER_SPECKLE = 3
export const VECTORIZE_PATH_PRECISION = 2

const SHA256_PREFIX = "sha256:"
const SHA256_HEX = /^[0-9a-f]{64}$/
const VECTORIZE_LOCK_KEYS = [
  "alphaCutoff",
  "sources",
  "svgo",
  "viewBox",
  "vtracer",
] as const
const SVGO_KEYS = ["cleanupIds", "keepRoleAttr", "preset"] as const
const VTRACER_KEYS = [
  "colormode",
  "color_precision",
  "filter_speckle",
  "gradient_step",
  "hierarchical",
  "mode",
  "path_precision",
] as const

export type VectorizeSourceName =
  `${(typeof PIECE_COLORS)[number]}-${(typeof PIECE_ROLES)[number]}.webp`
export type SourceWebpDigest = `sha256:${string}`
export type VectorizeLockSources = {
  [Name in VectorizeSourceName]: SourceWebpDigest
}
export type VectorizeLock = {
  alphaCutoff: typeof VECTORIZE_ALPHA_CUTOFF
  sources: VectorizeLockSources
  svgo: {
    cleanupIds: false
    keepRoleAttr: true
    preset: "default"
  }
  viewBox: typeof PIECE_VIEW_BOX
  vtracer: {
    colormode: "color"
    color_precision: typeof VECTORIZE_COLOR_PRECISION
    filter_speckle: typeof VECTORIZE_FILTER_SPECKLE
    gradient_step: typeof VECTORIZE_GRADIENT_STEP
    hierarchical: "stacked"
    mode: "spline"
    path_precision: typeof VECTORIZE_PATH_PRECISION
  }
}

export function sourceWebpName(
  color: PieceColor,
  role: PieceRole,
): VectorizeSourceName {
  return `${color}-${role}.webp`
}

export function digestSourceBytes(bytes: Buffer): SourceWebpDigest {
  return `${SHA256_PREFIX}${createHash("sha256").update(bytes).digest("hex")}`
}

export function vectorizeLockFromSourceDigests(
  sources: VectorizeLockSources,
): VectorizeLock {
  return {
    alphaCutoff: VECTORIZE_ALPHA_CUTOFF,
    sources,
    svgo: {
      cleanupIds: false,
      keepRoleAttr: true,
      preset: "default",
    },
    viewBox: PIECE_VIEW_BOX,
    vtracer: {
      colormode: "color",
      color_precision: VECTORIZE_COLOR_PRECISION,
      filter_speckle: VECTORIZE_FILTER_SPECKLE,
      gradient_step: VECTORIZE_GRADIENT_STEP,
      hierarchical: "stacked",
      mode: "spline",
      path_precision: VECTORIZE_PATH_PRECISION,
    },
  }
}

export async function readSourceWebpDigests(sourceDirectory: string) {
  // SAFETY: the inventory loop writes every VectorizeSourceName before return.
  const sources = {} as unknown as VectorizeLockSources
  for (const { color, role } of pieceInventory()) {
    const name = sourceWebpName(color, role)
    const path = resolve(sourceDirectory, name)
    try {
      sources[name] = digestSourceBytes(await readFile(path))
    } catch {
      throw new Error(
        `Missing ${path}. Run node docs/design/brand/regenerate-assets.mjs`,
      )
    }
  }
  return sources
}

export function parseVectorizeLock(value: unknown): VectorizeLock {
  const lock = parseKeyedObject(
    value,
    VECTORIZE_LOCK_KEYS,
    "vectorize.lock.json",
  )
  const alphaCutoff = Object.getOwnPropertyDescriptor(
    lock,
    "alphaCutoff",
  )?.value
  const viewBox = Object.getOwnPropertyDescriptor(lock, "viewBox")?.value
  if (alphaCutoff !== VECTORIZE_ALPHA_CUTOFF) {
    throw invalid(
      `vectorize.lock.json alphaCutoff must be ${String(VECTORIZE_ALPHA_CUTOFF)}`,
    )
  }
  if (viewBox !== PIECE_VIEW_BOX) {
    throw invalid(`vectorize.lock.json viewBox must be ${PIECE_VIEW_BOX}`)
  }
  return {
    alphaCutoff: VECTORIZE_ALPHA_CUTOFF,
    sources: parseSources(
      Object.getOwnPropertyDescriptor(lock, "sources")?.value,
    ),
    svgo: parseSvgo(Object.getOwnPropertyDescriptor(lock, "svgo")?.value),
    viewBox: PIECE_VIEW_BOX,
    vtracer: parseVtracer(
      Object.getOwnPropertyDescriptor(lock, "vtracer")?.value,
    ),
  }
}

export function formatVectorizeLock(lock: VectorizeLock) {
  return `${JSON.stringify(lock, null, 2)}\n`
}

export async function assertVectorizeLockMatches(pieceDirectory: string) {
  const sourceDirectory = resolve(pieceDirectory, "source")
  const lockPath = resolve(pieceDirectory, VECTORIZE_LOCK_NAME)
  let text: string
  try {
    text = await readFile(lockPath, "utf8")
  } catch {
    throw new Error(
      `Missing ${lockPath}. Run bun run vectorize:chess-pieces from packages/ui`,
    )
  }
  let value: unknown
  try {
    value = JSON.parse(text) as unknown
  } catch (cause) {
    throw new Error("Invalid vectorize.lock.json: malformed JSON", { cause })
  }
  const lock = parseVectorizeLock(value)
  const expected = vectorizeLockFromSourceDigests(
    await readSourceWebpDigests(sourceDirectory),
  )
  if (formatVectorizeLock(lock) !== formatVectorizeLock(expected)) {
    throw new Error(
      "Piece source lock is stale; run bun run vectorize:chess-pieces from packages/ui",
    )
  }
}

function parseSources(value: unknown): VectorizeLockSources {
  const expectedNames = pieceInventory().map(({ color, role }) =>
    sourceWebpName(color, role),
  )
  const record = parseKeyedObject(
    value,
    expectedNames,
    "vectorize.lock.json sources",
  )
  // SAFETY: the inventory loop writes every VectorizeSourceName before return.
  const sources = {} as unknown as VectorizeLockSources
  for (const { color, role } of pieceInventory()) {
    const name = sourceWebpName(color, role)
    sources[name] = parseDigest(
      Object.getOwnPropertyDescriptor(record, name)?.value,
      name,
    )
  }
  return sources
}

function parseSvgo(value: unknown): VectorizeLock["svgo"] {
  const record = parseKeyedObject(value, SVGO_KEYS, "vectorize.lock.json svgo")
  if (
    Object.getOwnPropertyDescriptor(record, "cleanupIds")?.value !== false ||
    Object.getOwnPropertyDescriptor(record, "keepRoleAttr")?.value !== true ||
    Object.getOwnPropertyDescriptor(record, "preset")?.value !== "default"
  ) {
    throw invalid(
      "vectorize.lock.json svgo must match the committed SVGO preset",
    )
  }
  return {
    cleanupIds: false,
    keepRoleAttr: true,
    preset: "default",
  }
}

function parseVtracer(value: unknown): VectorizeLock["vtracer"] {
  const record = parseKeyedObject(
    value,
    VTRACER_KEYS,
    "vectorize.lock.json vtracer",
  )
  if (
    Object.getOwnPropertyDescriptor(record, "colormode")?.value !== "color" ||
    Object.getOwnPropertyDescriptor(record, "color_precision")?.value !==
      VECTORIZE_COLOR_PRECISION ||
    Object.getOwnPropertyDescriptor(record, "filter_speckle")?.value !==
      VECTORIZE_FILTER_SPECKLE ||
    Object.getOwnPropertyDescriptor(record, "gradient_step")?.value !==
      VECTORIZE_GRADIENT_STEP ||
    Object.getOwnPropertyDescriptor(record, "hierarchical")?.value !==
      "stacked" ||
    Object.getOwnPropertyDescriptor(record, "mode")?.value !== "spline" ||
    Object.getOwnPropertyDescriptor(record, "path_precision")?.value !==
      VECTORIZE_PATH_PRECISION
  ) {
    throw invalid(
      "vectorize.lock.json vtracer must match the committed VTracer settings",
    )
  }
  return {
    colormode: "color",
    color_precision: VECTORIZE_COLOR_PRECISION,
    filter_speckle: VECTORIZE_FILTER_SPECKLE,
    gradient_step: VECTORIZE_GRADIENT_STEP,
    hierarchical: "stacked",
    mode: "spline",
    path_precision: VECTORIZE_PATH_PRECISION,
  }
}

function parseDigest(value: unknown, name: string): SourceWebpDigest {
  if (typeof value !== "string" || !value.startsWith(SHA256_PREFIX)) {
    throw invalid(`vectorize.lock.json sources.${name} must be a sha256 digest`)
  }
  const hex = value.slice(SHA256_PREFIX.length)
  if (!SHA256_HEX.test(hex)) {
    throw invalid(`vectorize.lock.json sources.${name} must be a sha256 digest`)
  }
  return `${SHA256_PREFIX}${hex}`
}

function parseIsPlainObject(value: unknown): value is object {
  return typeof value === "object" && value !== null && !Array.isArray(value)
}

function parseKeyedObject(
  value: unknown,
  keys: readonly string[],
  label: string,
): object {
  if (!parseIsPlainObject(value)) {
    throw invalid(`${label} must be an object`)
  }
  const expectedKeys = new Set(keys)
  const missing = keys.filter((key) => !Object.hasOwn(value, key))
  const unexpected = Object.keys(value).filter((key) => !expectedKeys.has(key))
  if (missing.length > 0) {
    throw invalid(`${label} is missing ${missing.join(", ")}`)
  }
  if (unexpected.length > 0) {
    throw invalid(`${label} has unexpected ${unexpected.join(", ")}`)
  }
  return value
}

function invalid(message: string) {
  return new Error(`Invalid ${message}`)
}
