import { expect, test } from "vitest"

import {
  fromGameImportId,
  fromCriticalMomentId,
  fromLearningResourceId,
  coreContract as coreFixture,
  events,
  decodeReviewSessionCoreContract,
  decodeReviewSessionEventEnvelope,
} from "@chenchess/coach-engine-sdk"
import type {
  GameReview,
  LearningResource,
  ReviewSessionCoreContract,
} from "@chenchess/coach-engine-sdk"

import {
  learningPathsForReviewMoment,
  openingTextForReviewMoment,
  publishedCommentForReviewMoment,
  curatedReviewMomentMarkers,
  reviewMomentMarkers,
  safeRenderingForReviewMoment,
  frozenSessionCommentFields,
  sessionCommentFields,
  waitingPlayerSelectedSession,
} from "./reviewMoments"
import { forkLearningMaterial } from "./learningMaterialTestFixtures"

test("splits published comment from first-open safe rendering", async () => {
  const core = await fixtureCore()
  const review = await fixtureGameReview()
  const comment = review.criticalMoments[0]?.comment
  if (!comment) throw new Error("fixture requires an admitted comment")
  expect(publishedCommentForReviewMoment(core, review)).toEqual(comment)
  const unpublished = structuredClone(review)
  const moment = unpublished.criticalMoments[0]
  if (!moment) throw new Error("fixture requires a Critical Moment")
  moment.comment = undefined
  expect(publishedCommentForReviewMoment(core, unpublished)).toBeNull()
  expect(safeRenderingForReviewMoment(core, unpublished)).toContain(
    "Good: c3 advanced the passed pawn to c3.",
  )
})

test("frozen session fields render the published comment without waiting", async () => {
  const core = await fixtureCore()
  const review = await fixtureGameReview()
  const comment = review.criticalMoments[0]?.comment
  if (!comment) throw new Error("fixture requires an admitted comment")

  const frozen = frozenSessionCommentFields(core, review)
  expect(frozen.commentPublished).toBe(true)
  expect(frozen.firstOpened).toBe(true)
  expect(frozen.firstOpenStartedAt).toBeNull()
  expect(frozen.openingText).toBe(comment.text)
  expect(frozen.comment).toEqual(comment)
})

test("frozen session fields wait for the engine when unpublished", async () => {
  const core = await fixtureCore()
  const review = structuredClone(await fixtureGameReview())
  const moment = review.criticalMoments[0]
  if (!moment) throw new Error("fixture requires a Critical Moment")
  moment.comment = undefined

  const frozen = frozenSessionCommentFields(core, review)
  // Not yet opened: the seed starts the authoring wait so the engine's
  // freshly authored comment lands instead of the client template freezing.
  expect(frozen.commentPublished).toBeNull()
  expect(frozen.firstOpened).toBe(false)
  expect(frozen.firstOpenStartedAt).toEqual(expect.any(Number))
  expect(frozen.openingText).toBeNull()
  // The deadline fallback still settles to the Safe Review Moment Rendering.
  expect(frozen.safeRendering).toContain(
    "Good: c3 advanced the passed pawn to c3.",
  )
})

test("first-open session fields wait, then publish or the full safe rendering", async () => {
  const core = await fixtureCore()
  const review = await fixtureGameReview()
  const comment = review.criticalMoments[0]?.comment
  if (!comment) throw new Error("fixture requires an admitted comment")

  const seeded = sessionCommentFields(core, review, undefined, true)
  expect(seeded.commentPublished).toBeNull()
  expect(seeded.firstOpened).toBe(false)
  expect(seeded.openingText).toBeNull()
  expect(seeded.firstOpenStartedAt).toEqual(expect.any(Number))

  const published = sessionCommentFields(core, review, {
    comment,
    commentPublished: true,
  })
  expect(published.commentPublished).toBe(true)
  expect(published.firstOpened).toBe(true)
  expect(published.firstOpenStartedAt).toBeNull()
  expect(published.openingText).toBe(comment.text)
  expect(published.safeRendering).toContain(
    "Good: c3 advanced the passed pawn to c3.",
  )

  const unpublishedReview = structuredClone(review)
  const moment = unpublishedReview.criticalMoments[0]
  if (!moment) throw new Error("fixture requires a Critical Moment")
  moment.comment = undefined
  const waiting = sessionCommentFields(core, unpublishedReview, undefined, true)
  expect(waiting.commentPublished).toBeNull()
  expect(waiting.firstOpened).toBe(false)
  expect(waiting.comment).toBeNull()
  expect(waiting.openingText).toBeNull()
  expect(waiting.firstOpenStartedAt).toEqual(expect.any(Number))
  expect(waiting.safeRendering).toContain(
    "Good: c3 advanced the passed pawn to c3.",
  )

  const openedSafe = sessionCommentFields(core, unpublishedReview, {
    comment: { text: waiting.safeRendering },
    commentPublished: false,
  })
  expect(openedSafe.commentPublished).toBe(false)
  expect(openedSafe.firstOpenStartedAt).toBeNull()
  expect(openedSafe.openingText).toBe(waiting.safeRendering)
})

test("player-selected ply starts wait fields before an opened comment exists", async () => {
  const core = await fixtureCore()
  const review = await fixtureGameReview()
  const material = review.criticalMoments[0]?.learningMaterial
  if (!material) throw new Error("fixture requires Review Moment material")
  const started = waitingPlayerSelectedSession(
    {
      gameImportId: fromGameImportId("game-import:test:web"),
      core,
      learningMaterial: material,
    },
    review,
    3,
  )
  expect(started.criticalPly).toBe(3)
  expect(started.core.reviewMoment.selection).toEqual({
    kind: "playerSelectedMoment",
    ply: 3,
  })
  expect(started.firstOpened).toBe(false)
  expect(started.commentPublished).toBeNull()
  expect(started.openingText).toBeNull()
  expect(started.firstOpenStartedAt).toEqual(expect.any(Number))
  expect(started.learningMaterial.tracks).toEqual([])
})

test("renders an admitted Automatic comment as prose-only authority", async () => {
  const core = await fixtureCore()
  const review = await fixtureGameReview()
  const comment = review.criticalMoments[0]?.comment
  if (!comment) throw new Error("fixture requires an admitted comment")

  expect(openingTextForReviewMoment(core, review)).toBe("")
  expect(openingTextForReviewMoment(core, review, comment, true)).toBe(
    comment.text,
  )
})

test("opens a real Automatic Review Moment before an admitted comment exists", async () => {
  const core = await fixtureCore()
  const review = await fixtureGameReview()
  const moment = review.criticalMoments[0]
  if (!moment) throw new Error("fixture requires a Critical Moment")
  moment.comment = undefined

  expect(openingTextForReviewMoment(core, review)).toBe("")
  const opening = openingTextForReviewMoment(core, review, undefined, false)
  expect(opening).toContain("Good: c3 advanced the passed pawn to c3.")
  expect(opening).toContain(
    "After c3, the evaluation is 0.0 — Roughly balanced.",
  )
  expect(opening).not.toContain("analyzed 0.0")
  expect(opening).not.toMatch(/\b[a-h][1-8][a-h][1-8][qrbn]?\b/)
})

test("renders a canonical Player-selected opening", async () => {
  const core = await fixtureCore()
  const review = await fixtureGameReview()
  review.criticalMoments = []
  core.reviewMoment.selection = {
    kind: "playerSelectedMoment",
    ply: core.reviewMoment.ply,
  }
  const comment = {
    text: "Neutral: e4. This move is outside your Review Side. Verified observation: White played e4 at ply 1.",
  }

  expect(openingTextForReviewMoment(core, review, comment)).toBe(comment.text)
})

test("projects typed tracks as categorized learning paths", async () => {
  const review = await fixtureGameReview()
  const moment = review.criticalMoments[0]
  if (!moment) throw new Error("fixture requires a Critical Moment")
  const fixtureMaterial = forkLearningMaterial(
    moment.criticalMomentId,
    moment.ply,
  )
  const fixtureSupport = fixtureMaterial.tracks[0]?.support[0]
  if (!fixtureSupport)
    throw new Error("fixture requires Learning Track support")
  const support = { ...fixtureSupport, purpose: "improvement" as const }
  const resource: LearningResource = {
    resourceId: fromLearningResourceId("lichess:practice:Qj281y1p"),
    role: "learn" as const,
    kind: "practiceModule" as const,
    title: "The Fork",
    canonicalUrl:
      "https://lichess.org/practice/fundamental-tactics/the-fork/Qj281y1p",
  }
  const hangingPiece: LearningResource = {
    resourceId: fromLearningResourceId("lichess:puzzles:hangingPiece"),
    role: "drill" as const,
    kind: "puzzleStream" as const,
    title: "Hanging piece",
    canonicalUrl: "https://lichess.org/training/hangingPiece",
  }
  moment.learningMaterial.tracks = [
    {
      key: { kind: "curriculum", concept: "fork" },
      support: [support],
      resources: [resource, resource],
    },
    {
      key: { kind: "curriculum", concept: "hangingPiece" },
      support: [support],
      resources: [hangingPiece],
    },
  ]

  expect(
    learningPathsForReviewMoment(
      moment.learningMaterial,
      moment.criticalMomentId,
    ),
  ).toEqual([
    {
      cluster: "Lichess Curriculum",
      conceptLessons: [resource],
      idea: "The Fork",
      id: "curriculum:fork",
      learningPathRef: support.learningPathRef,
      patternDrills: [],
      purpose: "missing",
    },
    {
      cluster: "Lichess Curriculum",
      conceptLessons: [],
      idea: "Hanging piece",
      id: "curriculum:hangingPiece",
      learningPathRef: support.learningPathRef,
      patternDrills: [hangingPiece],
      purpose: "missing",
    },
  ])
})

test("projects curriculum tracks without misclassifying them as openings", async () => {
  const review = await fixtureGameReview()
  const moment = review.criticalMoments[0]
  if (!moment) throw new Error("fixture requires a Critical Moment")
  const material = forkLearningMaterial(moment.criticalMomentId, moment.ply)
  const support = material.tracks[0]?.support[0]
  if (!support) throw new Error("fixture requires Learning Track support")
  const resource: LearningResource = {
    resourceId: fromLearningResourceId("lichess:practice:defensiveMove"),
    role: "learn",
    kind: "practiceModule",
    title: "Defensive move",
    canonicalUrl: "https://lichess.org/practice/defensive-move",
  }
  material.tracks = [
    {
      key: { kind: "curriculum", concept: "defensiveMove" },
      support: [{ ...support, purpose: "improvement" }],
      resources: [resource],
    },
  ]

  expect(
    learningPathsForReviewMoment(material, moment.criticalMomentId),
  ).toEqual([
    {
      cluster: "Lichess Curriculum",
      conceptLessons: [resource],
      idea: "Defensive move",
      id: "curriculum:defensiveMove",
      learningPathRef: support.learningPathRef,
      patternDrills: [],
      purpose: "missing",
    },
  ])
})

test("projects no paths for neutral Player-selected material", async () => {
  const neutral = (await fixtureGameReview()).criticalMoments[0]
    ?.learningMaterial
  if (!neutral) throw new Error("fixture requires Review Moment material")
  neutral.tracks = []

  expect(
    learningPathsForReviewMoment(
      neutral,
      fromCriticalMomentId("critical-moment:missing"),
    ),
  ).toEqual([])
})

test("derives missing versus reinforced from the active moment support", async () => {
  const review = await fixtureGameReview()
  const moment = review.criticalMoments[0]
  if (!moment) throw new Error("fixture requires a Critical Moment")
  const material = forkLearningMaterial(moment.criticalMomentId, moment.ply)
  const track = material.tracks[0]
  const currentSupport = track?.support[0]
  if (!track || !currentSupport) {
    throw new Error("fixture requires Fork support")
  }
  track.support.push({
    ...currentSupport,
    criticalMomentId: fromCriticalMomentId(
      "critical-moment:another-improvement",
    ),
    purpose: "improvement",
  })

  expect(
    learningPathsForReviewMoment(material, moment.criticalMomentId)[0]?.purpose,
  ).toBe("reinforced")
})

test("labels an evidence-backed improvement by its missing idea", async () => {
  const review = await fixtureGameReview()
  const moment = review.criticalMoments[0]
  if (!moment) throw new Error("fixture requires a Critical Moment")
  moment.learningMaterial = forkLearningMaterial(
    moment.criticalMomentId,
    moment.ply,
  )
  const fixtureSupport = moment.learningMaterial.tracks[0]?.support[0]
  if (!fixtureSupport)
    throw new Error("fixture requires Learning Track support")
  moment.classification = {
    kind: "improvementOpportunity",
    correction: {
      betterMoveSan: "Nc7+",
      betterMoveUci: "b5c7",
      outcome: {
        kind: "improvedAnalyzed",
        betterEvaluation: moment.objective.bestEvaluation,
      },
    },
  }
  moment.learningMaterial.tracks[0] = {
    ...moment.learningMaterial.tracks[0]!,
    key: { kind: "curriculum", concept: "fork" },
    support: [{ ...fixtureSupport, purpose: "improvement" }],
  }
  const core = await fixtureCore()

  const markers = reviewMomentMarkers(review, [core])
  expect(markers.find((marker) => marker.ply === moment.ply)).toMatchObject({
    glyph: "◇",
    label: "Missing idea · The Fork",
    tone: "improvement",
  })
})

test("walk-only extras do not grow the curated selector", async () => {
  const review = await fixtureGameReview()
  const frozen = review.criticalMoments.length
  expect(curatedReviewMomentMarkers(review, [])).toHaveLength(frozen)
  expect(
    curatedReviewMomentMarkers(review, [
      {
        ply: 21,
        facts: {
          ...review.criticalMoments[0]!,
          ply: 21,
          classification: {
            kind: "neutral",
            reasons: ["belowImprovementThreshold"],
          },
        },
      },
    ]),
  ).toHaveLength(frozen)
})

test("a waiting placeholder does not join the curated selector", async () => {
  const review = await fixtureGameReview()
  const frozen = review.criticalMoments.length
  expect(
    curatedReviewMomentMarkers(review, [
      { ply: 21, facts: null, placeholder: true },
    ]),
  ).toHaveLength(frozen)
})

test("a resumed nomination without facts does not invent a generic marker", async () => {
  const review = await fixtureGameReview()
  const frozen = review.criticalMoments.length
  expect(
    curatedReviewMomentMarkers(review, [{ ply: 21, facts: null }]),
  ).toHaveLength(frozen)
})

test("a resumed nomination with classification kind renders that classification", async () => {
  const review = await fixtureGameReview()
  const frozen = review.criticalMoments.length
  const markers = curatedReviewMomentMarkers(review, [
    {
      ply: 21,
      facts: null,
      classificationKind: "improvementOpportunity",
      moveLabel: "11. Nf3",
    },
  ])
  expect(markers).toHaveLength(frozen + 1)
  expect(markers.find((marker) => marker.ply === 21)).toMatchObject({
    countsInTotal: false,
    glyph: "◦",
    label: "Improvement opportunity",
    moveLabel: "11. Nf3",
    ply: 21,
    tone: "selected",
  })
})

test("a nominated highlight joins the curated selector with its classification", async () => {
  const review = await fixtureGameReview()
  const frozen = review.criticalMoments.length
  const source = review.criticalMoments.find(
    (moment) => moment.classification.kind === "positiveHighlight",
  )
  if (!source) throw new Error("fixture requires a Positive Highlight")
  const highlight = {
    ...source,
    ply: 26,
    playedSan: "Nxd4",
  }
  const markers = curatedReviewMomentMarkers(review, [
    { ply: 26, facts: highlight },
  ])
  expect(markers).toHaveLength(frozen + 1)
  expect(markers.find((marker) => marker.ply === 26)).toMatchObject({
    countsInTotal: false,
    glyph: "◦",
    ply: 26,
    label: "Positive highlight",
    tone: "selected",
  })
})

test("a resumed nomination without SAN does not invent a ply label", async () => {
  const review = await fixtureGameReview()
  const frozen = review.criticalMoments.length
  expect(
    curatedReviewMomentMarkers(review, [
      { ply: 21, facts: null, classificationKind: "improvementOpportunity" },
    ]),
  ).toHaveLength(frozen)
})

async function fixtureCore(): Promise<ReviewSessionCoreContract> {
  return decodeReviewSessionCoreContract(structuredClone(coreFixture))
}

async function fixtureGameReview(): Promise<GameReview> {
  for (const fixture of await Promise.all(
    events.map(decodeReviewSessionEventEnvelope),
  )) {
    if (
      fixture.event.kind === "completed" &&
      fixture.event.result.kind === "gameImported"
    ) {
      return structuredClone(fixture.event.result.review)
    }
  }
  throw new Error("generated fixtures must contain a Game Review")
}

test("safe rendering carries the intent sentence the engine would have authored", async () => {
  const core = await fixtureCore()
  const review = await fixtureGameReview()
  const unpublished = structuredClone(review)
  const moment = unpublished.criticalMoments[0]
  if (!moment) throw new Error("fixture requires a Critical Moment")
  moment.comment = undefined

  const withoutIntent = safeRenderingForReviewMoment(core, unpublished)
  expect(withoutIntent).not.toContain("My best guess")

  const withIntent = safeRenderingForReviewMoment(core, unpublished, {
    enrichment: {
      projectedPlanSan: ["d5"],
      objectiveCounterplaySan: ["Nc3"],
    },
    instructions: intentInstructions(),
  })
  expect(withIntent).toContain(
    `My best guess is that ${moment.playedSan} may have been aiming for d5`,
  )
  expect(withIntent).toContain("Nc3")
  // The engine inserts intent second, after the classification opener.
  const sentences = withIntent.split(". ")
  expect(sentences[1]).toContain("My best guess")
  expect(sentences[0]).toBe(withoutIntent.split(". ")[0])
})

test("opened session fields render intent from the engine's authoring context", async () => {
  const core = await fixtureCore()
  const review = await fixtureGameReview()
  const unpublished = structuredClone(review)
  const moment = unpublished.criticalMoments[0]
  if (!moment) throw new Error("fixture requires a Critical Moment")
  moment.comment = undefined

  const opened = sessionCommentFields(core, unpublished, {
    comment: null,
    commentPublished: false,
    authoringContext: {
      intent: {
        enrichment: {
          projectedPlanSan: ["d5"],
          objectiveCounterplaySan: ["Nc3"],
        },
        instructions: intentInstructions(),
      },
    },
  })
  expect(opened.safeRendering).toContain("My best guess")
  expect(opened.openingText).toBe(opened.safeRendering)
})

function intentInstructions() {
  return {
    hypothesis: "State one plan as explicitly uncertain.",
    counterplay: "Name the reply that disrupts it.",
  }
}
