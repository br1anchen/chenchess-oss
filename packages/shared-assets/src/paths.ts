import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

const packageRoot = join(dirname(fileURLToPath(import.meta.url)), "..")

export const sharedAssetsRoot = packageRoot

export const canonicalGameDir = join(packageRoot, "fixtures/Synthet1")

export const canonicalGamePgnPath = join(canonicalGameDir, "lichess-export.pgn")

export const canonicalGameRawPgnPath = join(
  canonicalGameDir,
  "lichess-export.raw.pgn",
)

export const canonicalGameRecordingPath = join(
  canonicalGameDir,
  "review-session-provider-recording.json",
)

export const canonicalGameBaselinePath = join(
  canonicalGameDir,
  "provider-recordings/full-game.baseline.json",
)
