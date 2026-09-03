import { openingLineRefFromPath, type OpeningLineRef } from "./openingLineRef"

export type OpeningLineIdeas = {
  pawnBreaks: string
  piecePlaces: string
  plan: string
}

export type OpeningCatalogRow = {
  eco: string
  ideas: OpeningLineIdeas
  name: string
  path: string
  ref: OpeningLineRef
}

/**
 * Signed Najdorf study, next-move branches, and stories. Find reads the
 * pinned catalog through Coach Engine's identification reader.
 */
const openingRows = [
  {
    eco: "B90",
    ideas: {
      pawnBreaks: "…e5, …e6, …b5",
      piecePlaces: "Knight on f6, bishop on e7 or g7, queen on c7",
      plan: "After …a6, prepare …e5 or …e6 and play on the queenside.",
    },
    name: "Sicilian Defense: Najdorf Variation",
    path: "1. e4 c5 2. Nf3 d6 3. d4 cxd4 4. Nxd4 Nf6 5. Nc3 a6",
  },
  {
    eco: "C00",
    ideas: {
      pawnBreaks: "…d5, …c5",
      piecePlaces: "Knight on f6, bishop on e7 or b4",
      plan: "Challenge e4 with …d5 and strike the center with …c5.",
    },
    name: "French Defense",
    path: "1. e4 e6",
  },
  {
    eco: "C41",
    ideas: {
      pawnBreaks: "…d5, …f5",
      piecePlaces: "Knights on f6 and d7, bishop on e7",
      plan: "Hold e5 and develop behind a solid …d6.",
    },
    name: "Philidor Defense",
    path: "1. e4 e5 2. Nf3 d6",
  },
  {
    eco: "C50",
    ideas: {
      pawnBreaks: "d4, or c3 then d4",
      piecePlaces: "Bishop on c4, knights on f3 and c3",
      plan: "Pressure f7 and prepare a central d4.",
    },
    name: "Italian Game",
    path: "1. e4 e5 2. Nf3 Nc6 3. Bc4",
  },
  {
    eco: "D00",
    ideas: {
      pawnBreaks: "c4, e4; …c5, …e5",
      piecePlaces: "Knights on f3 and c3; …f6 and …c6",
      plan: "Occupy the center with d-pawns and develop naturally.",
    },
    name: "Queen's Pawn Game",
    path: "1. d4 d5",
  },
  {
    eco: "E00",
    ideas: {
      pawnBreaks: "c4; …d5, …c5",
      piecePlaces: "Knight on f6, bishop on e7 or b4",
      plan: "White plays c4; Black prepares …d5 or …Bb4.",
    },
    name: "Queen's Pawn Opening",
    path: "1. d4 Nf6 2. c4 e6",
  },
] as const

export const openingLineCatalog: readonly OpeningCatalogRow[] = openingRows.map(
  (row) => ({
    ...row,
    ref: openingLineRefFromPath(row.eco, row.name, row.path),
  }),
)

export type PlayedOpening = {
  eco: string
  name: string
}

export function openingCatalogRow(
  ref: OpeningLineRef,
): OpeningCatalogRow | undefined {
  return openingLineCatalog.find((row) => row.ref === ref)
}
