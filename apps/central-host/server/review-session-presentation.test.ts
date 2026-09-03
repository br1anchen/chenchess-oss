import {
  decodeReviewSessionPresentation,
  decodeReviewSessionPresentationAddition,
  fromIdempotencyKey,
  type OperationCompletion,
} from "@chenchess/coach-engine-sdk"
import { beforeEach, describe, expect, test } from "vitest"

import {
  projectReviewSessionPresentation,
  projectReviewSessionPresentationAddition,
  resetReviewSessionPresentationCaches,
} from "./review-session-presentation"
import { completionFixture } from "./reviewCompletionFixtures"

const idempotencyKey = fromIdempotencyKey(
  "idempotency-key:fixture:presentation",
)

describe("Review Session presentation projection", () => {
  beforeEach(() => resetReviewSessionPresentationCaches())

  test("projects canonical session facts into one compact app-only contract", () => {
    const completion = resumedCompletion()
    const presentation = projectReviewSessionPresentation(
      completion,
      idempotencyKey,
    )
    const moment = presentation.moments[0]!
    const move = completion.importedGame.game.moves.find(
      ({ ply }) => ply === moment.ply,
    )!

    expect(decodeReviewSessionPresentation(presentation)).toBe(presentation)
    expect(presentation).toMatchObject({
      presentationRevision: 1,
      selectedMomentId: moment.momentId,
      gameImportId: completion.gameImportId,
      sessionRevision: 1,
      version: "v1",
    })
    expect(moment.board.positionRef).toBe(move.afterPositionRef)
    expect(moment.board.lastMove).toEqual({
      from: move.uci.slice(0, 2),
      to: move.uci.slice(2, 4),
    })
    expect(presentation.animation).toBeNull()
    expect(moment.arrows.map(({ label }) => label)).toEqual([
      "Engine",
      `Elo ${completion.importedGame.eloProfile.rating} player`,
    ])
    expect(moment.handoff).toEqual({
      momentId: moment.momentId,
      ply: moment.ply,
      idempotencyKey,
      selection: completion.reviewMoments[0]!.reviewMoment.selection,
      gameImportId: completion.gameImportId,
    })
    expect(moment.learningMaterial).toEqual(
      completion.reviewMoments[0]!.learningMaterial,
    )

    const serialized = JSON.stringify(presentation)
    expect(Buffer.byteLength(serialized)).toBeLessThan(100_000)
    expect(serialized).not.toMatch(
      /authoringContext|best reply|evidencePacket|firestore|importedGame|groundingLedger|providerPayload|stockfish/i,
    )
  })

  test("preserves Chess.com provenance in the compact presentation", () => {
    const completion = resumedCompletion()
    const lichess = completion.importedGame.provenance
    if (lichess.kind !== "lichess") {
      throw new Error("Generated fixture has no Lichess provenance")
    }
    completion.importedGame.provenance = {
      kind: "chessCom",
      canonicalGameId: lichess.canonicalGameId,
      canonicalUrl: "https://www.chess.com/game/computer/1403674481",
      fetchContractVersion: "chess-com-computer-game-callback/v1",
      capturedAt: lichess.capturedAt,
      responseDigest: lichess.responseDigest,
      pgnDigest: lichess.pgnDigest,
    }

    expect(
      projectReviewSessionPresentation(completion, idempotencyKey).source,
    ).toBe("chessCom")
  })

  test("fails closed when canonical moments cannot be joined", () => {
    const completion = resumedCompletion()
    expect(() =>
      projectReviewSessionPresentation(
        {
          ...completion,
          review: { ...completion.review, criticalMoments: [] },
        },
        idempotencyKey,
      ),
    ).toThrow(/has no canonical Game Review presentation/)
  })

  test("keeps coincident alternatives distinct without projecting continuation data", () => {
    const completion = resumedCompletion()
    const criticalMoment = completion.review.criticalMoments[0]!
    criticalMoment.human.mostLikelyMoveUci =
      criticalMoment.objective.bestMoveUci
    criticalMoment.objective.lines = {
      best: [],
      refutation: [
        { san: "e5", uci: "e7e5" },
        { san: "Nf3", uci: "g1f3" },
      ],
    }

    const presentation = projectReviewSessionPresentation(
      completion,
      idempotencyKey,
    )

    expect(presentation.animation).toBeNull()
    expect(presentation.moments[0]?.arrows).toMatchObject([
      { kind: "engineBest" },
      { kind: "maia" },
    ])
    expect(
      new Set(
        presentation.moments[0]?.arrows.map(({ from, to }) => `${from}:${to}`),
      ).size,
    ).toBe(1)
    expect(decodeReviewSessionPresentation(presentation)).toBe(presentation)
  })

  test("projects one bounded app-only addition for the exact opened moment", () => {
    const completion = openedCompletion()
    const addition = projectReviewSessionPresentationAddition(
      completion,
      idempotencyKey,
    )

    expect(decodeReviewSessionPresentationAddition(addition)).toBe(addition)
    expect(addition).toMatchObject({
      animation: null,
      moment: {
        authoringReadiness: "prepared",
        handoff: {
          momentId: completion.reviewMoment.reviewMoment.momentId,
          idempotencyKey,
          gameImportId: completion.gameImportId,
        },
        momentId: completion.reviewMoment.reviewMoment.momentId,
      },
      changedFields: ["animation", "moment", "selectedMomentId"],
      changedMomentIds: [completion.reviewMoment.reviewMoment.momentId],
      fullRefreshRequired: false,
      priorRevision: completion.revisionDelta.priorRevision,
      resultingRevision: completion.sessionRevision,
      gameImportId: completion.gameImportId,
      version: "v1",
    })
    expect(addition.moment.learningMaterial).toEqual(
      completion.criticalMoment.learningMaterial,
    )
    expect(JSON.stringify(addition)).not.toMatch(
      /authoringContext|evidencePacket|importedGame|groundingLedger/,
    )
    expect(Buffer.byteLength(JSON.stringify(addition))).toBeLessThan(25_000)
  })

  test("reuses stable session revisions while rebinding operation publication keys", () => {
    const completion = resumedCompletion()
    const firstKey = fromIdempotencyKey(
      "idempotency-key:fixture:presentation:first",
    )
    const secondKey = fromIdempotencyKey(
      "idempotency-key:fixture:presentation:second",
    )
    const first = projectReviewSessionPresentation(completion, firstKey)
    const equivalentRetry = structuredClone(completion)
    equivalentRetry.review.criticalMoments = []

    const second = projectReviewSessionPresentation(equivalentRetry, secondKey)

    expect(second).toEqual({
      ...first,
      moments: first.moments.map((moment) => ({
        ...moment,
        handoff: { ...moment.handoff, idempotencyKey: secondKey },
      })),
    })
    expect(first.moments[0]?.handoff.idempotencyKey).toBe(firstKey)
  })

  test("reuses stable opened-moment revisions with a fresh publication key", () => {
    const completion = openedCompletion()
    const firstKey = fromIdempotencyKey(
      "idempotency-key:fixture:addition:first",
    )
    const secondKey = fromIdempotencyKey(
      "idempotency-key:fixture:addition:second",
    )
    const first = projectReviewSessionPresentationAddition(completion, firstKey)
    const equivalentRetry = structuredClone(completion)
    equivalentRetry.criticalMoment.learningMaterial.tracks = []

    const second = projectReviewSessionPresentationAddition(
      equivalentRetry,
      secondKey,
    )

    expect(second).toEqual({
      ...first,
      moment: {
        ...first.moment,
        handoff: { ...first.moment.handoff, idempotencyKey: secondKey },
      },
    })
    expect(first.moment.handoff.idempotencyKey).toBe(firstKey)
  })
})

function resumedCompletion() {
  return completionFixture("reviewSessionStarted")
}

function openedCompletion(): Extract<
  OperationCompletion,
  { kind: "reviewMomentOpened" }
> {
  const resumed = resumedCompletion()
  const admitted = resumed.reviewMoments[0]
  if (!admitted || admitted.authoring.kind !== "prepared") {
    throw new Error("Generated fixtures have no prepared Review Moment")
  }
  const criticalMoment = resumed.review.criticalMoments.find(
    ({ criticalMomentId }) =>
      criticalMomentId === admitted.reviewMoment.momentId,
  )
  if (!criticalMoment) {
    throw new Error("Generated fixtures have no Game Review entry to open")
  }
  return {
    authoringContext: null,
    comment: null,
    commentPublished: false,
    criticalMoment,
    decisionExplanationRef: criticalMoment.decisionExplanationRef,
    kind: "reviewMomentOpened",
    reviewMoment: admitted.authoring.core,
    revisionDelta: {
      changedMomentIds: [admitted.reviewMoment.momentId],
      fullRefreshRequired: false,
      priorRevision: resumed.sessionRevision,
      resultingRevision: resumed.sessionRevision + 1,
    },
    gameImportId: resumed.gameImportId,
    sessionRevision: resumed.sessionRevision + 1,
  }
}
