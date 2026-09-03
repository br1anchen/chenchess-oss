import { useState } from "react"
import type { Square } from "@chenchess/coach-engine-sdk"

import {
  legalDestinations,
  promotionRequired,
  uciForDestination,
  type BrowseBoardPosition,
  type PromotionRole,
} from "@/review-session/model"

type PromotionMove = { from: Square; to: Square }

/**
 * The Player's own move on the Coaching Board.
 *
 * Selection is click-to-select, then click-to-move, the same two steps the
 * Review Session board takes. A move that needs a promotion piece waits for
 * one before it reaches the engine.
 */
export function useBoardExploration({
  explore,
  exploring,
  position,
}: {
  explore: (uci: string) => void
  exploring: boolean
  position: BrowseBoardPosition
}) {
  const [selectedSquare, setSelectedSquare] = useState<Square | null>(null)
  const [promotion, setPromotion] = useState<PromotionMove | null>(null)
  const destinations = selectedSquare
    ? legalDestinations(position, selectedSquare)
    : []

  function selectSquare(square: Square) {
    if (exploring) return
    setPromotion(null)
    if (selectedSquare && destinations.includes(square)) {
      const from = selectedSquare
      setSelectedSquare(null)
      if (promotionRequired(position, from, square)) {
        setPromotion({ from, to: square })
      } else {
        explore(uciForDestination(position, from, square))
      }
      return
    }
    const piece = position.occupied.find(
      (entry) => entry.square === square,
    )?.piece
    setSelectedSquare(piece?.color === position.sideToMove ? square : null)
  }

  function promote(role: PromotionRole) {
    if (!promotion) {
      throw new Error("Promotion controls require a waiting promotion move")
    }
    const { from, to } = promotion
    setPromotion(null)
    explore(uciForDestination(position, from, to, role))
  }

  function clearSelection() {
    setSelectedSquare(null)
    setPromotion(null)
  }

  return {
    clearSelection,
    destinations,
    promote,
    promotion,
    selectedSquare,
    selectSquare,
  }
}
