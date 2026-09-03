import { describe, expect, test } from "vitest"

import { reduceWorkspaceFixture, workspaceFixture } from "./fixtures"

describe("fixture workspace contract", () => {
  test("covers automatic, Player-Selected, uncertain, and unavailable moments", () => {
    expect(workspaceFixture.moments.map((moment) => moment.kind)).toContain(
      "automatic",
    )
    expect(workspaceFixture.moments.map((moment) => moment.kind)).toContain(
      "playerSelected",
    )
    const unavailable = reduceWorkspaceFixture(workspaceFixture, {
      type: "momentSelected",
      momentId: "moment-23",
    })
    expect(unavailable.comment.status).toBe("unavailable")
    expect(
      workspaceFixture.moments.find((moment) => moment.id === "moment-23")
        ?.summary,
    ).toBe(
      "The most common choices at your rating were unavailable; objective evidence remains visible.",
    )
  })

  test("Player-visible fixture copy never names the Human Move Model", () => {
    const banned = /human-likely|human likely|human model|move model|\bmaia\b/i
    for (const moment of workspaceFixture.moments) {
      const selected = reduceWorkspaceFixture(workspaceFixture, {
        type: "momentSelected",
        momentId: moment.id,
      })
      expect(moment.title).not.toMatch(banned)
      expect(moment.summary).not.toMatch(banned)
      expect(selected.comment.eyebrow).not.toMatch(banned)
      expect(selected.comment.heading).not.toMatch(banned)
      expect(selected.comment.body).not.toMatch(banned)
    }
  })

  test("keeps legality and canonical position ownership in the controller", () => {
    const illegal = reduceWorkspaceFixture(workspaceFixture, {
      type: "boardSquareSelected",
      square: "a1",
    })
    expect(illegal.board.legalDestinations).toEqual([])

    const moved = reduceWorkspaceFixture(workspaceFixture, {
      type: "boardMoveRequested",
      move: { from: "d4", to: "d5" },
    })
    expect(moved.board.pieces).toBe(workspaceFixture.board.pieces)
    expect(moved.board.lastMove).toEqual({ from: "d4", to: "d5" })
  })

  test("selects the canonical board snapshot for every Review Moment", () => {
    const snapshots = workspaceFixture.moments.map((moment) =>
      reduceWorkspaceFixture(workspaceFixture, {
        type: "momentSelected",
        momentId: moment.id,
      }),
    )

    expect(new Set(snapshots.map((snapshot) => snapshot.board.fen)).size).toBe(
      workspaceFixture.moments.length,
    )
    expect(snapshots[2]?.board.lastMove).toEqual({ from: "h2", to: "h3" })
    expect(snapshots[3]?.board.lastMove).toEqual({ from: "c7", to: "c6" })
  })

  test("completes the deterministic fixture import synchronously", () => {
    const changed = reduceWorkspaceFixture(workspaceFixture, {
      type: "importSourceChanged",
      source: "pgn",
    })
    expect(changed.importSetup.status).toBe("ready")

    const imported = reduceWorkspaceFixture(changed, {
      type: "importRequested",
    })
    expect(imported.importSetup.status).toBe("complete")
    expect(imported.statusMessage).toBe(
      "Fixture game imported and ready for review.",
    )
  })

  test("cancels active work and changes retention synchronously", () => {
    const cancelled = reduceWorkspaceFixture(workspaceFixture, {
      type: "activeWorkCancelled",
    })
    expect(
      cancelled.alternatives.find((move) => move.id === "alternative-c6")
        ?.status,
    ).toBe("cancelled")

    const disabled = reduceWorkspaceFixture(cancelled, {
      type: "retentionChanged",
      enabled: false,
    })
    expect(disabled.retention).toMatchObject({
      enabled: false,
      disclosureRequired: false,
    })
  })

  test("selects an alternative without starting or cancelling work", () => {
    const selected = reduceWorkspaceFixture(workspaceFixture, {
      type: "alternativeSelected",
      alternativeId: "alternative-qd6",
    })

    expect(
      selected.alternatives.filter(
        (alternative) => alternative.status === "active",
      ),
    ).toHaveLength(1)
    expect(
      selected.alternatives.find(
        (alternative) => alternative.id === "alternative-c6",
      )?.status,
    ).toBe("active")
    expect(
      selected.alternatives.find(
        (alternative) => alternative.id === "alternative-qd6",
      ),
    ).toMatchObject({ selected: true, status: "cancelled" })
  })
})
