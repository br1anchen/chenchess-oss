import { expect, test } from "vitest"

import {
  decodePositionSnapshot,
  fromAlternativeMoveId,
  fromBranchRef,
  fromPositionRef,
  positionSnapshot,
  type AlternativeMoveResult,
} from "@chenchess/coach-engine-sdk"

import { projectAlternativeMove } from "./alternative-move.js"
import {
  containsRawUci,
  PLAYER_VISIBLE_MOVE_FALLBACK,
  playerVisibleAlternativeMove,
  playerVisibleSanFromLegalUci,
  playerVisibleSanLiteral,
  playerVisibleStrongestReply,
} from "./player-visible-san.js"

const START_POSITION_FEN =
  "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"

const AFTER_E4_FEN =
  "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1"

const NXD4_SOURCE_FEN =
  "r1bqkbnr/pppp1ppp/2n5/8/3pP3/5N2/PPP2PPP/RNBQKB1R w KQkq - 0 4"

test("converts a legal UCI Alternative Move to SAN", () => {
  expect(playerVisibleSanFromLegalUci(START_POSITION_FEN, "e2e4")).toBe("e4")
  expect(playerVisibleSanFromLegalUci(NXD4_SOURCE_FEN, "f3d4")).toBe("Nxd4")
})

test("refuses to brand a raw UCI string as Player-visible", () => {
  expect(playerVisibleSanLiteral("f3d4")).toBe(PLAYER_VISIBLE_MOVE_FALLBACK)
  expect(playerVisibleSanLiteral("12… f3d4")).toBe(PLAYER_VISIBLE_MOVE_FALLBACK)
  expect(playerVisibleSanLiteral("12… Nxd4")).toBe("12… Nxd4")
})

test("detects raw UCI with the Grounding Gate token shape", () => {
  expect(containsRawUci("The heading is f3d4")).toBe(true)
  expect(containsRawUci("Preview strongest reply e7e5")).toBe(true)
  expect(containsRawUci("Nxd4")).toBe(false)
  expect(containsRawUci("12… Nxd4")).toBe(false)
  expect(containsRawUci("O-O-O")).toBe(false)
})

test("renders an Alternative Move and its strongest reply from source Positions", async () => {
  const alternative = await alternativeMove({
    moveUci: "e2e4",
    resultingFen: AFTER_E4_FEN,
    strongestReply: { kind: "offered", uci: "e7e5" },
  })

  expect(
    playerVisibleAlternativeMove(
      alternative,
      [alternative],
      START_POSITION_FEN,
    ),
  ).toBe("e4")
  expect(
    playerVisibleStrongestReply({ kind: "offered", uci: "e7e5" }, AFTER_E4_FEN),
  ).toBe("e5")
})

test("falls back to a neutral phrase instead of throwing or showing UCI", async () => {
  const alternative = await alternativeMove({
    moveUci: "e2e4",
    resultingFen: AFTER_E4_FEN,
    strongestReply: { kind: "terminal" },
  })
  expect(playerVisibleSanFromLegalUci(START_POSITION_FEN, "f3d4")).toBe(
    PLAYER_VISIBLE_MOVE_FALLBACK,
  )
  expect(playerVisibleSanFromLegalUci("not-a-fen", "e2e4")).toBe(
    PLAYER_VISIBLE_MOVE_FALLBACK,
  )
  expect(playerVisibleAlternativeMove(alternative, [alternative], null)).toBe(
    PLAYER_VISIBLE_MOVE_FALLBACK,
  )
})

test("pins SAN beside UCI on the model-safe Alternative Move", async () => {
  const alternative = await alternativeMove({
    bestMoveUci: "g1f3",
    moveUci: "e2e4",
    resultingFen: AFTER_E4_FEN,
    strongestReply: { kind: "offered", uci: "e7e5" },
  })

  const projected = projectAlternativeMove(alternative, START_POSITION_FEN)
  expect(projected.moveSan).toBe("e4")
  expect(projected.moveUci).toBe("e2e4")
  expect(projected.bestMoveSan).toBe("Nf3")
  expect(projected.strongestReply).toEqual({
    kind: "offered",
    san: "e5",
    uci: "e7e5",
  })
  expect(containsRawUci(projected.moveSan)).toBe(false)
  expect(containsRawUci(projected.bestMoveSan)).toBe(false)
  expect(
    projected.strongestReply.kind === "offered" &&
      containsRawUci(projected.strongestReply.san),
  ).toBe(false)
})

test("does not make UCI the only spelling when source FEN is missing", async () => {
  const alternative = await alternativeMove({
    moveUci: "e2e4",
    resultingFen: AFTER_E4_FEN,
    strongestReply: { kind: "offered", uci: "e7e5" },
  })

  const projected = projectAlternativeMove(alternative)
  expect(projected.moveSan).toBe(PLAYER_VISIBLE_MOVE_FALLBACK)
  expect(projected.moveUci).toBe("e2e4")
  expect(projected.strongestReply).toEqual({
    kind: "offered",
    san: "e5",
    uci: "e7e5",
  })
})

async function alternativeMove(spec: {
  bestMoveUci?: string
  moveUci: string
  resultingFen: string
  strongestReply: AlternativeMoveResult["strongestReply"]
}): Promise<AlternativeMoveResult> {
  const evaluation = {
    kind: "centipawns" as const,
    perspective: "white" as const,
    value: 22,
  }
  const resultingPosition = await decodePositionSnapshot(
    structuredClone(positionSnapshot),
  )
  return {
    alternativeMoveId: fromAlternativeMoveId("alternative-move:web:e4"),
    branchRef: fromBranchRef("branch:web:e4"),
    evaluation: {
      bestMove: evaluation,
      bestMoveUci: spec.bestMoveUci ?? spec.moveUci,
      comparison: { kind: "centipawns", value: 0 },
      selectedMove: evaluation,
    },
    moveUci: spec.moveUci,
    parent: {
      kind: "root",
      positionRef: fromPositionRef(
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      ),
    },
    resultingPosition: {
      ...resultingPosition,
      fen: spec.resultingFen,
      sideToMove: "black",
    },
    sourcePositionRef: fromPositionRef(
      "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ),
    strongestReply: spec.strongestReply,
  }
}
