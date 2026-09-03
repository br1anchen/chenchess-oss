// @vitest-environment jsdom

import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import {
  decodePositionSnapshot,
  fromAlternativeMoveId,
  fromBranchRef,
  fromPositionRef,
  fromSquare,
  positionSnapshot,
  type AlternativeMoveResult,
  type PositionSnapshot,
} from "@chenchess/coach-engine-sdk"
import {
  containsRawUci,
  PLAYER_VISIBLE_MOVE_FALLBACK,
  playerVisibleSanFromLegalUci,
  playerVisibleSanLiteral,
  playerVisibleStrongestReply,
} from "@chenchess/review-projection"

import {
  BoardWorkspace,
  ReviewBranchControls,
  ReviewMoveControls,
} from "./BoardWorkspace"

const START_POSITION_FEN =
  "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"

const AFTER_E4_FEN =
  "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1"

const NXD4_SOURCE_FEN =
  "r1bqkbnr/pppp1ppp/2n5/8/3pP3/5N2/PPP2PPP/RNBQKB1R w KQkq - 0 4"

afterEach(cleanup)

// The branch controls are where a Player reads an Alternative Move, so the
// SAN-not-UCI guarantee is asserted against them rather than through the
// board that composes them.
test("Player-facing Alternative Move labels are SAN, never raw UCI", async () => {
  const branch = await alternativeMove({
    moveUci: "f3d4",
    resultingFen: NXD4_SOURCE_FEN,
  })
  const label = playerVisibleSanFromLegalUci(NXD4_SOURCE_FEN, "f3d4")

  const { container } = render(
    <ReviewBranchControls
      branch={branch}
      exploredBranches={[
        {
          alternativeMoveId: branch.alternativeMoveId,
          label,
          selectedMove: branch.evaluation.selectedMove,
        },
      ]}
      interactionDisabled={false}
      onSelectBranch={() => undefined}
    />,
  )

  expect(screen.getByRole("button", { name: /Nxd4 · \+0.00/ })).toBeTruthy()
  expect(containsRawUci(container.textContent ?? "")).toBe(false)
  expect(screen.queryByText("f3d4")).toBeNull()
})

test("the branch line pairs Exit branch with the SAN Best move preview", async () => {
  const branch = await alternativeMove({
    moveUci: "e2e4",
    strongestReplyUci: "e7e5",
    resultingFen: AFTER_E4_FEN,
  })

  render(
    <ReviewMoveControls
      alternativeBusy={false}
      branch={branch}
      maxPly={3}
      momentMarkers={[]}
      moves={fixtureMoves()}
      navigationDisabled={false}
      onExitBranch={() => undefined}
      onNavigate={() => undefined}
      onStrongestReply={() => undefined}
      strongestReplyLabel={playerVisibleStrongestReply(
        { kind: "offered", uci: "e7e5" },
        AFTER_E4_FEN,
      )}
      viewedPly={1}
    />,
  )

  const navigation = screen.getByRole("group", { name: "Position navigation" })
  expect(
    within(navigation).getByRole("button", { name: "Exit branch" }),
  ).toBeTruthy()
  expect(
    within(navigation).getByRole("button", { name: "Best move: e5" }),
  ).toBeTruthy()
  expect(screen.queryByText(/e7e5/)).toBeNull()
})

test("promotion role buttons honor interactionDisabled", async () => {
  const position = await fixturePosition()
  render(
    <BoardWorkspace
      alternativeBusy={false}
      branch={null}
      heading={playerVisibleSanFromLegalUci(START_POSITION_FEN, "e2e4")}
      criticalPly={1}
      destinations={[]}
      evaluation={null}
      evaluationPoints={[]}
      interactionDisabled
      momentMarkers={[]}
      navigationDisabled={false}
      onExitBranch={() => undefined}
      onNavigate={() => undefined}
      onPromote={() => undefined}
      onSquare={() => undefined}
      orientation="white"
      position={position}
      promotion={{ from: fromSquare("e7"), to: fromSquare("e8") }}
      selectedSquare={null}
      viewedPly={1}
    />,
  )

  for (const name of ["Queen", "Rook", "Bishop", "Knight"] as const) {
    expect(screen.getByRole("button", { name })).toHaveProperty(
      "disabled",
      true,
    )
  }
})

test("keeps its own navigation above the chessboard for non-session surfaces", async () => {
  const position = await fixturePosition()
  render(
    <BoardWorkspace
      alternativeBusy={false}
      branch={null}
      heading={playerVisibleSanFromLegalUci(START_POSITION_FEN, "e2e4")}
      criticalPly={8}
      destinations={[]}
      evaluation={null}
      evaluationPoints={[]}
      interactionDisabled={false}
      momentMarkers={[
        {
          glyph: "↗",
          label: "Improvement opportunity",
          moveLabel: "4. Nxd4",
          ply: 8,
          tone: "improvement",
        },
      ]}
      navigationDisabled={false}
      onExitBranch={() => undefined}
      onNavigate={() => undefined}
      onPromote={() => undefined}
      onSquare={() => undefined}
      orientation="white"
      position={position}
      promotion={null}
      selectedSquare={null}
      viewedPly={4}
    />,
  )

  const navigation = screen.getByLabelText("Position navigation")
  const chessboard = screen.getByLabelText("Chess position")
  expect(
    navigation.compareDocumentPosition(chessboard) &
      Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy()
  expect(screen.queryByRole("button", { name: "Discuss" })).toBeNull()
})

test("illegal or missing FEN keeps the Review Session mounted", async () => {
  const position = await fixturePosition()
  const heading = playerVisibleSanFromLegalUci("not-a-fen", "e2e4")

  const { container } = render(
    <BoardWorkspace
      alternativeBusy={false}
      branch={null}
      heading={heading}
      criticalPly={1}
      destinations={[]}
      evaluation={null}
      evaluationPoints={[]}
      interactionDisabled={false}
      momentMarkers={[]}
      navigationDisabled={false}
      onExitBranch={() => undefined}
      onNavigate={() => undefined}
      onPromote={() => undefined}
      onSquare={() => undefined}
      orientation="white"
      position={position}
      promotion={null}
      selectedSquare={null}
      showPositionCaption
      viewedPly={1}
    />,
  )

  expect(heading).toBe(PLAYER_VISIBLE_MOVE_FALLBACK)
  expect(
    screen.getByRole("heading", { level: 2, name: "this move" }),
  ).toBeTruthy()
  expect(containsRawUci(container.textContent ?? "")).toBe(false)
  expect(screen.queryByText(/e2e4/)).toBeNull()
  expect(playerVisibleSanLiteral("e2e4")).toBe(PLAYER_VISIBLE_MOVE_FALLBACK)
})

test("the viewed move's chip reads selected in the move sequence", () => {
  render(
    <ReviewMoveControls
      alternativeBusy={false}
      branch={null}
      maxPly={3}
      momentMarkers={[]}
      moves={fixtureMoves()}
      navigationDisabled={false}
      onExitBranch={() => undefined}
      onNavigate={() => undefined}
      viewedPly={2}
    />,
  )

  const picker = screen.getByLabelText("Full game move list")
  const viewed = within(picker).getByRole("button", { name: "1… e5" })
  const other = within(picker).getByRole("button", { name: "1. e4" })
  expect(viewed.getAttribute("aria-current")).toBe("step")
  expect(other.getAttribute("aria-current")).toBeNull()
  expect(viewed.className).not.toBe(other.className)
})

test("the move sequence opens scrolled to the viewed move's chip", () => {
  const scrollTo = vi.fn()
  const originalScrollTo = HTMLElement.prototype.scrollTo
  try {
    HTMLElement.prototype.scrollTo = scrollTo
    render(
      <ReviewMoveControls
        alternativeBusy={false}
        branch={null}
        maxPly={3}
        momentMarkers={[]}
        moves={fixtureMoves()}
        navigationDisabled={false}
        onExitBranch={() => undefined}
        onNavigate={() => undefined}
        viewedPly={3}
      />,
    )

    expect(scrollTo).toHaveBeenCalledTimes(1)
  } finally {
    HTMLElement.prototype.scrollTo = originalScrollTo
  }
})

// The referent is prose the Player pastes in front of a question, so it is
// pinned as the Player reads it — the caption's kind and move — and against
// the SAN-not-UCI oracle, since a raw UCI would be the one thing the coach
// could not settle against the board the Player sees.
test("Ask about this position copies a referent naming the shown position", async () => {
  const position = await fixturePosition()
  const branch = await alternativeMove({
    moveUci: "f3d4",
    resultingFen: NXD4_SOURCE_FEN,
  })
  const copied: string[] = []
  render(
    <BoardWorkspace
      alternativeBusy={false}
      branch={branch}
      copyPositionReferent={async (referent) => {
        copied.push(referent)
      }}
      criticalPly={7}
      destinations={[]}
      evaluation={null}
      evaluationPoints={[]}
      heading={playerVisibleSanFromLegalUci(NXD4_SOURCE_FEN, "f3d4")}
      interactionDisabled={false}
      momentMarkers={[]}
      navigationDisabled={false}
      onExitBranch={() => undefined}
      onNavigate={() => undefined}
      onPromote={() => undefined}
      onSquare={() => undefined}
      orientation="white"
      position={position}
      promotion={null}
      selectedSquare={null}
      viewedPly={7}
    />,
  )

  fireEvent.click(
    screen.getByRole("button", { name: "Ask about this position" }),
  )

  expect(
    await screen.findByText("Copied. Paste it into the chat, then ask."),
  ).toBeTruthy()
  expect(copied).toEqual([
    "About the position on my Coaching Board (alternative branch, after Nxd4):",
  ])
  expect(containsRawUci(copied[0] ?? "")).toBe(false)
})

test("a clipboard the page cannot write leaves the sentence on screen to copy by hand", async () => {
  const position = await fixturePosition()
  render(
    <BoardWorkspace
      alternativeBusy={false}
      branch={null}
      copyPositionReferent={() => Promise.reject(new Error("NotAllowedError"))}
      criticalPly={1}
      destinations={[]}
      evaluation={null}
      evaluationPoints={[]}
      heading={playerVisibleSanFromLegalUci(START_POSITION_FEN, "e2e4")}
      interactionDisabled={false}
      momentMarkers={[]}
      navigationDisabled={false}
      onExitBranch={() => undefined}
      onNavigate={() => undefined}
      onPromote={() => undefined}
      onSquare={() => undefined}
      orientation="white"
      position={position}
      promotion={null}
      selectedSquare={null}
      viewedPly={1}
    />,
  )

  fireEvent.click(
    screen.getByRole("button", { name: "Ask about this position" }),
  )

  expect(
    await screen.findByText(
      /Paste this into the chat: About the position on my Coaching Board \(before e4\):/,
    ),
  ).toBeTruthy()
})

test("a surface whose chat is on the page gets no Ask about this position", async () => {
  const position = await fixturePosition()
  render(
    <BoardWorkspace
      alternativeBusy={false}
      branch={null}
      criticalPly={1}
      destinations={[]}
      evaluation={null}
      evaluationPoints={[]}
      heading={playerVisibleSanFromLegalUci(START_POSITION_FEN, "e2e4")}
      interactionDisabled={false}
      momentMarkers={[]}
      navigationDisabled={false}
      onExitBranch={() => undefined}
      onNavigate={() => undefined}
      onPromote={() => undefined}
      onSquare={() => undefined}
      orientation="white"
      position={position}
      promotion={null}
      selectedSquare={null}
      viewedPly={1}
    />,
  )

  expect(
    screen.queryByRole("button", { name: "Ask about this position" }),
  ).toBeNull()
})

function fixtureMoves() {
  return (
    [
      { moveNumber: 1, ply: 1, san: "e4", side: "white", uci: "e2e4" },
      { moveNumber: 1, ply: 2, san: "e5", side: "black", uci: "e7e5" },
      { moveNumber: 2, ply: 3, san: "Nf3", side: "white", uci: "g1f3" },
    ] as const
  ).map((move) => ({
    ...move,
    beforePositionRef: fromPositionRef("position:before"),
    afterPositionRef: fromPositionRef("position:after"),
  }))
}

async function fixturePosition(): Promise<PositionSnapshot> {
  return decodePositionSnapshot(structuredClone(positionSnapshot))
}

async function alternativeMove(spec: {
  moveUci: string
  strongestReplyUci?: string
  resultingFen?: string
}): Promise<AlternativeMoveResult> {
  const evaluation = {
    kind: "centipawns" as const,
    perspective: "white" as const,
    value: 0,
  }
  const resultingPosition = await fixturePosition()
  return {
    alternativeMoveId: fromAlternativeMoveId("alternative-move:web:test"),
    branchRef: fromBranchRef("branch:web:test"),
    evaluation: {
      bestMove: evaluation,
      bestMoveUci: spec.moveUci,
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
      fen: spec.resultingFen ?? AFTER_E4_FEN,
    },
    sourcePositionRef: fromPositionRef(
      "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ),
    strongestReply: spec.strongestReplyUci
      ? { kind: "offered", uci: spec.strongestReplyUci }
      : { kind: "terminal" },
  }
}
