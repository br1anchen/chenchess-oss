import { Chess } from "chessops/chess"
import { parseFen } from "chessops/fen"
import { makeSan } from "chessops/san"
import { parseUci } from "chessops/util"

import type { AlternativeMoveResult } from "@chenchess/coach-engine-sdk"

/**
 * SAN (or a SAN-bearing heading such as `12… Nxd4`) that is safe to show a
 * Player. Construct only through this module so a raw UCI string cannot be
 * assigned at a Player-facing render site.
 */
export type PlayerVisibleSan = string & {
  readonly __playerVisibleSan: unique symbol
}

/** Neutral phrase when a legal SAN conversion is impossible. Never raw UCI. */
export const PLAYER_VISIBLE_MOVE_FALLBACK: PlayerVisibleSan =
  brandPlayerVisibleSan("this move")

export function playerVisibleSanFromLegalUci(
  sourceFen: string,
  uci: string,
): PlayerVisibleSan {
  const setup = parseFen(sourceFen)
  if (setup.isErr) return PLAYER_VISIBLE_MOVE_FALLBACK
  const position = Chess.fromSetup(setup.value)
  if (position.isErr) return PLAYER_VISIBLE_MOVE_FALLBACK
  const move = parseUci(uci)
  if (!move || !position.value.isLegal(move)) {
    return PLAYER_VISIBLE_MOVE_FALLBACK
  }
  return brandPlayerVisibleSan(makeSan(position.value, move))
}

export function playerVisibleSanLiteral(label: string): PlayerVisibleSan {
  if (containsRawUci(label)) return PLAYER_VISIBLE_MOVE_FALLBACK
  return brandPlayerVisibleSan(label)
}

export function sourceFenForAlternativeMove(
  alternative: AlternativeMoveResult,
  alternatives: readonly AlternativeMoveResult[],
  reviewMomentFen: string | null,
): string | null {
  const parentRef = alternative.parent
  if (parentRef.kind === "root") return reviewMomentFen
  const parent = alternatives.find(
    (candidate) => candidate.branchRef === parentRef.branchRef,
  )
  return parent?.resultingPosition.fen ?? null
}

export function playerVisibleAlternativeMove(
  alternative: AlternativeMoveResult,
  alternatives: readonly AlternativeMoveResult[],
  reviewMomentFen: string | null,
): PlayerVisibleSan {
  const sourceFen = sourceFenForAlternativeMove(
    alternative,
    alternatives,
    reviewMomentFen,
  )
  if (!sourceFen) return PLAYER_VISIBLE_MOVE_FALLBACK
  return playerVisibleSanFromLegalUci(sourceFen, alternative.moveUci)
}

export function playerVisibleStrongestReply(
  offered: Extract<
    AlternativeMoveResult["strongestReply"],
    { kind: "offered" }
  >,
  resultingFen: string,
): PlayerVisibleSan {
  return playerVisibleSanFromLegalUci(resultingFen, offered.uci)
}

/** Same shape the Grounding Gate uses for `contains_raw_uci`. */
export function containsRawUci(text: string): boolean {
  return text.split(/\s+/).map(trimNonAlphanumeric).some(isRawUci)
}

function isRawUci(token: string): boolean {
  return (
    (token.length === 4 || token.length === 5) &&
    isFile(token[0]) &&
    isRank(token[1]) &&
    isFile(token[2]) &&
    isRank(token[3]) &&
    (token.length === 4 || isPromotion(token[4]))
  )
}

function trimNonAlphanumeric(token: string): string {
  return token.replace(/^[^a-zA-Z0-9]+|[^a-zA-Z0-9]+$/g, "")
}

function isFile(character: string | undefined): boolean {
  return character !== undefined && character >= "a" && character <= "h"
}

function isRank(character: string | undefined): boolean {
  return character !== undefined && character >= "1" && character <= "8"
}

function isPromotion(character: string | undefined): boolean {
  return (
    character === "q" ||
    character === "r" ||
    character === "b" ||
    character === "n"
  )
}

function brandPlayerVisibleSan(label: string): PlayerVisibleSan {
  // SAFETY: TypeScript cannot construct a unique-symbol brand; makeSan or
  // containsRawUci already established that `label` is Player-visible.
  return label as PlayerVisibleSan
}
