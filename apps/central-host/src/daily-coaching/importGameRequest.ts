import type {
  GameInputSource,
  RequestedEloProfile,
  ReviewSide,
} from "@chenchess/coach-engine-sdk"
import { parseChessComInput, parseLichessInput } from "@chenchess/ui"

import { parseEloRating } from "@/review-session/model"
import { extractCompletedPgn } from "@/review-session/reviewRequest"

/** The form field an invalid import request is refused on. */
export type ImportGameField = "elo" | "reviewSide" | "source"

/** What the dashboard's import form holds, exactly as the Player typed it. */
export type ImportGameFields = {
  elo: string
  reviewSide: ReviewSide
  source: string
}

export type ParsedImportGameRequest =
  | { field: ImportGameField; kind: "invalid"; message: string }
  | {
      kind: "ready"
      eloProfile: RequestedEloProfile
      reviewSide: ReviewSide
      source: GameInputSource
    }

/**
 * Three exact fields to one `importGame` command.
 *
 * The dashboard asks for the Review Side and Elo directly, so unlike the
 * conversational surface there is nothing to infer from prose. What is left is
 * the agreement with the Engine: it resolves Both sides only for a pasted PGN,
 * and it will not read an Elo out of metadata when neither side is the Review
 * Side. Refusing those here costs the Player a round trip instead of a typed
 * rejection.
 *
 * A side-qualified Lichess URL is not one of those refusals. `/black` on the URL
 * with White in the control is a Game the Engine imports as White — the qualifier
 * preselects the side and the control stays authoritative — so the card adopts
 * it through `preselectedReviewSide` rather than the parser calling it invalid.
 */
export function parseImportGameRequest({
  elo,
  reviewSide,
  source,
}: ImportGameFields): ParsedImportGameRequest {
  const trimmed = source.trim()
  if (!trimmed) {
    return {
      field: "source",
      kind: "invalid",
      message: "Paste a Chess.com or Lichess game URL, or a full PGN.",
    }
  }

  const enteredElo = elo.trim()
  const rating = enteredElo ? parseEloRating(enteredElo) : null
  if (enteredElo && !rating) {
    return {
      field: "elo",
      kind: "invalid",
      message: "Elo must be a whole number between 100 and 3500.",
    }
  }
  const eloProfile: RequestedEloProfile = rating
    ? { kind: "playerProvided", rating }
    : { kind: "fromImportedMetadata" }

  if (/^https?:\/\//i.test(trimmed)) {
    return parseGameUrl(trimmed, reviewSide, eloProfile)
  }

  const pgn = extractCompletedPgn(trimmed)
  if (!pgn) {
    return {
      field: "source",
      kind: "invalid",
      message:
        "Paste one completed game URL, or the game's full PGN including its result.",
    }
  }
  if (reviewSide === "both" && !rating) {
    return {
      field: "elo",
      kind: "invalid",
      message: "Reviewing both sides needs an Elo to coach at.",
    }
  }
  return {
    eloProfile,
    kind: "ready",
    reviewSide,
    source: { kind: "pastedPgn", pgn },
  }
}

function parseGameUrl(
  url: string,
  reviewSide: ReviewSide,
  eloProfile: RequestedEloProfile,
): ParsedImportGameRequest {
  // Host first, protocol second: an `http://lichess.org/…` URL is a Lichess game
  // the Player mistyped, and `parseLichessInput` says so exactly. Reading the
  // protocol first would answer a Lichess URL with "unsupported host".
  if (isHost(url, "www.chess.com")) {
    const parsed = parseChessComInput(url)
    if (parsed.kind === "invalid") {
      return { field: "source", kind: "invalid", message: parsed.message }
    }
    if (reviewSide === "both") {
      return {
        field: "reviewSide",
        kind: "invalid",
        message: "A Chess.com game is reviewed as White or as Black.",
      }
    }
    return {
      eloProfile,
      kind: "ready",
      reviewSide,
      source: { kind: "chessComUrl", url: parsed.url },
    }
  }

  if (isHost(url, "lichess.org")) {
    const parsed = parseLichessInput(url)
    if (parsed.kind === "invalid") {
      return { field: "source", kind: "invalid", message: parsed.message }
    }
    if (reviewSide === "both") {
      return {
        field: "reviewSide",
        kind: "invalid",
        message: "A Lichess game is reviewed as White or as Black.",
      }
    }
    return {
      eloProfile,
      kind: "ready",
      reviewSide,
      source: { kind: "lichessUrl", url: parsed.url },
    }
  }

  return {
    field: "source",
    kind: "invalid",
    message:
      "Only Chess.com and Lichess game URLs can be imported. Paste the game's PGN instead.",
  }
}

/**
 * The Review Side a side-qualified Lichess URL preselects, if the source is one.
 *
 * The card adopts this when the source changes and leaves the control
 * authoritative afterwards, which is what CONTEXT means by preselects: pasting
 * the `/black` link a Player shares after playing Black should open Black's
 * review, without making the control's own value a contradiction to refuse.
 */
export function preselectedReviewSide(source: string): ReviewSide | null {
  const trimmed = source.trim()
  if (!isHost(trimmed, "lichess.org")) return null
  const parsed = parseLichessInput(trimmed)
  return parsed.kind === "qualified" ? parsed.side : null
}

function isHost(url: string, host: string): boolean {
  return new RegExp(`^https?://${host.replaceAll(".", "\\.")}/`, "i").test(url)
}
