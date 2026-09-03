import {
  fromCriticalMomentId,
  type CriticalMomentComment,
  type CriticalMomentId,
  type CriticalMomentIntentAuthoringContext,
  type GameImportId,
  type GameReview,
  type GameReviewCriticalMoment,
  type LearningPathRef,
  type LearningResource,
  type ReviewMomentLearningMaterial,
  type ReviewSessionCoreContract,
} from "@chenchess/coach-engine-sdk"
import {
  projectLearningPaths,
  type LearningPathPresentation,
} from "@chenchess/ui/review/learning-path-projection"
import {
  isNeutralPlayerSelectedClassification,
  toneFromClassificationKind,
} from "@chenchess/review-projection"

export type ReviewMomentMarker = {
  ply: number
  moveLabel: string
  label: string
  glyph: string
  tone: "improvement" | "positive" | "selected"
  countsInTotal?: boolean
}

export type MomentLearningPath = LearningPathPresentation<
  LearningPathRef,
  LearningResource
>

export function learningPathsForReviewMoment(
  material: ReviewMomentLearningMaterial,
  criticalMomentId: CriticalMomentId,
): MomentLearningPath[] {
  return projectLearningPaths<LearningPathRef, LearningResource>(
    material,
    criticalMomentId,
  )
}

export type NominatedMarkerSource = {
  ply: number
  facts: GameReviewCriticalMoment | null
  placeholder?: boolean
  classificationKind?: GameReviewCriticalMoment["classification"]["kind"] | null
  moveLabel?: string
}

export function reviewMomentMarkers(
  review: GameReview,
  cores: readonly ReviewSessionCoreContract[],
): ReviewMomentMarker[] {
  return curatedReviewMomentMarkers(
    review,
    cores.map((core) => ({
      ply: core.reviewMoment.ply,
      facts:
        review.criticalMoments.find(
          (moment) => moment.criticalMomentId === core.reviewMoment.momentId,
        ) ?? null,
    })),
  )
}

export function curatedReviewMomentMarkers(
  review: GameReview,
  nominated: readonly NominatedMarkerSource[],
): ReviewMomentMarker[] {
  const frozen = frozenReviewMomentMarkers(review)
  const frozenPlys = new Set(frozen.map((marker) => marker.ply))
  const extras = nominated.flatMap((source) => {
    if (frozenPlys.has(source.ply)) return []
    if (source.placeholder) return []
    const kind =
      source.facts?.classification.kind ?? source.classificationKind ?? null
    if (isNeutralPlayerSelectedClassification(kind, true)) return []
    if (source.facts) {
      return [
        presentNominatedWorkspaceMoment({
          ...markerFromCriticalMoment(source.facts),
          countsInTotal: false,
        }),
      ]
    }
    const moveLabel = source.moveLabel
    if (!kind || !moveLabel) return []
    return [
      presentNominatedWorkspaceMoment(
        markerFromClassificationKind(source.ply, kind, moveLabel),
      ),
    ]
  })
  return [...frozen, ...extras].sort((left, right) => left.ply - right.ply)
}

export function frozenReviewMomentMarkers(
  review: GameReview,
): ReviewMomentMarker[] {
  return review.criticalMoments.map(markerFromCriticalMoment)
}

function presentNominatedWorkspaceMoment(
  moment: ReviewMomentMarker,
): ReviewMomentMarker {
  return {
    ...moment,
    glyph: "◦",
    tone: "selected",
  }
}

function markerFromCriticalMoment(
  moment: GameReviewCriticalMoment,
): ReviewMomentMarker {
  const moveLabel = `${moment.moveNumber}${moment.side === "black" ? "…" : "."} ${moment.playedSan}`
  if (moment.classification.kind === "positiveHighlight") {
    return {
      glyph: moment.classification.grade === "great" ? "!!" : "!",
      label: "Positive highlight",
      moveLabel,
      ply: moment.ply,
      tone: "positive",
    }
  }
  if (moment.classification.kind === "improvementOpportunity") {
    const missingIdea = learningPathsForReviewMoment(
      moment.learningMaterial,
      moment.criticalMomentId,
    ).find(({ purpose }) => purpose === "missing")
    return {
      glyph: missingIdea ? "◇" : "↗",
      label: missingIdea
        ? `Missing idea · ${missingIdea.idea}`
        : "Improvement opportunity",
      moveLabel,
      ply: moment.ply,
      tone: "improvement",
    }
  }
  return {
    glyph: "•",
    label: "Neutral moment",
    moveLabel,
    ply: moment.ply,
    tone: "selected",
  }
}

function markerFromClassificationKind(
  ply: number,
  kind: GameReviewCriticalMoment["classification"]["kind"],
  moveLabel: string,
): ReviewMomentMarker {
  const tone = toneFromClassificationKind(kind)
  switch (kind) {
    case "positiveHighlight":
      return {
        countsInTotal: false,
        glyph: "!",
        label: "Positive highlight",
        moveLabel,
        ply,
        tone,
      }
    case "improvementOpportunity":
      return {
        countsInTotal: false,
        glyph: "↗",
        label: "Improvement opportunity",
        moveLabel,
        ply,
        tone,
      }
    case "neutral":
      return {
        countsInTotal: false,
        glyph: "•",
        label: "Neutral moment",
        moveLabel,
        ply,
        tone,
      }
    default: {
      const exhaustive: never = kind
      return exhaustive
    }
  }
}

export function publishedCommentForReviewMoment(
  core: ReviewSessionCoreContract,
  review: GameReview,
  canonicalComment?: CriticalMomentComment | null,
): CriticalMomentComment | null {
  if (canonicalComment) return canonicalComment
  return (
    review.criticalMoments.find(
      (moment) => moment.criticalMomentId === core.reviewMoment.momentId,
    )?.comment ?? null
  )
}

export function safeRenderingForReviewMoment(
  core: ReviewSessionCoreContract,
  review: GameReview,
  intent?: CriticalMomentIntentAuthoringContext | null,
): string {
  const reviewedMoment = review.criticalMoments.find(
    (moment) => moment.criticalMomentId === core.reviewMoment.momentId,
  )
  if (reviewedMoment) return factualReviewText(reviewedMoment, intent)
  if (core.reviewMoment.selection.kind === "pipelineCriticalMoment") {
    throw new Error(
      "Automatic Review Moments require matching Game Review facts",
    )
  }
  throw new Error("Player-selected Review Moments require a canonical comment")
}

export function openingTextForReviewMoment(
  core: ReviewSessionCoreContract,
  review: GameReview,
  canonicalComment?: CriticalMomentComment | null,
  commentPublished?: boolean,
): string {
  if (commentPublished === false) {
    const unpublished = canonicalComment?.text.trim() ?? ""
    return unpublished || safeRenderingForReviewMoment(core, review)
  }
  if (canonicalComment) return canonicalComment.text
  if (commentPublished === true) {
    const published = publishedCommentForReviewMoment(core, review)
    if (published) return published.text
  }
  return ""
}

/**
 * The Review Engine's Safe Review Moment Rendering, re-derived here.
 *
 * The Review Engine's own `safely_rendered_comment` is the authority: it
 * renders the same sentences in the same order and inserts the intent
 * sentence second. This copy exists because the browser seeds a thread from
 * the frozen Game Review before any Review Moment opens, and it must not
 * read thinner than the text the engine would have sent.
 */
/**
 * What the coach said about a frozen Critical Moment: the Language Layer's
 * prose when it authored any, the Review Engine's safe rendering otherwise.
 *
 * The session-shaped readers above take a live `ReviewSessionCoreContract`;
 * a surface holding only the frozen Review reads it here rather than reaching
 * for the safe rendering directly.
 */
export function frozenMomentText(
  moment: GameReview["criticalMoments"][number],
): string {
  return moment.comment?.text ?? factualReviewText(moment)
}

function factualReviewText(
  moment: GameReview["criticalMoments"][number],
  intent?: CriticalMomentIntentAuthoringContext | null,
): string {
  const sentences = safeSentences(moment)
  if (intent && moment.classification.kind !== "neutral") {
    sentences.splice(1, 0, intentSentence(moment, intent))
  }
  return sentences.join(" ")
}

function safeSentences(
  moment: GameReview["criticalMoments"][number],
): string[] {
  const takeaway = teachingTakeaway(moment)
  switch (moment.classification.kind) {
    case "positiveHighlight": {
      const achievement = moment.classification.qualification.achievements[0]
      if (!achievement) {
        throw new Error("Positive Highlights require a concrete achievement")
      }
      const grade = moment.classification.grade === "great" ? "Great" : "Good"
      const sentences = [
        `${grade}: ${moment.playedSan} ${achievementText(achievement)}.`,
        difficultyText(moment.classification),
        playedOutcomeText(moment),
      ]
      if (takeaway) sentences.push(takeaway)
      return sentences
    }
    case "improvementOpportunity": {
      const correction = moment.classification.correction
      const sentences = [
        `Improvement: After ${moment.playedSan}, the evaluation is ${moment.display.playedEvaluation.score} — ${moment.display.playedEvaluation.label}; ${consequenceText(moment.residualOutcome.classification)}.`,
        correctionText(moment, correction),
        `Before committing here, calculate ${correction.betterMoveSan} first.`,
      ]
      if (takeaway) sentences.push(takeaway)
      return sentences
    }
    case "neutral": {
      const reasons = moment.classification.reasons
        .map(neutralReasonText)
        .join(" and ")
      return [
        `Neutral: ${moment.playedSan}.`,
        `This move is neutral because ${reasons}.`,
        playedOutcomeText(moment),
      ]
    }
  }
}

/** Mirrors `pawn_units_text`: material in words, never a figure. */
function pawnUnitsText(pawnUnits: number): string {
  switch (pawnUnits) {
    case 1:
      return "a pawn"
    case 2:
      return "two pawns"
    case 3:
      return "three pawns"
    case 4:
      return "four pawns"
    case 5:
      return "five pawns"
    case 6:
      return "six pawns"
    case 7:
      return "seven pawns"
    case 8:
      return "eight pawns"
    default:
      return "a decisive amount"
  }
}

/** Mirrors `safe_line_opening`: a line's first three moves with an ellipsis
 * standing for the rest — never the full transcript. */
function lineOpening(san: readonly string[]): string {
  return san.length <= 3 ? san.join(" ") : `${san.slice(0, 3).join(" ")} …`
}

/** Mirrors `safe_intent_sentence`: one plan, stated as a guess, never asserted. */
function intentSentence(
  moment: GameReview["criticalMoments"][number],
  intent: CriticalMomentIntentAuthoringContext,
): string {
  const enrichment = intent.enrichment
  if (!enrichment) {
    return `My best guess is that ${moment.playedSan} may have been aiming to improve the position.`
  }
  const plan = lineOpening(enrichment.projectedPlanSan)
  const counterplay = lineOpening(enrichment.objectiveCounterplaySan)
  if (moment.classification.kind === "positiveHighlight") {
    return `My best guess is that ${moment.playedSan} may have been aiming for ${plan}; ${counterplay} is the strongest defense, while the move's achievement still stands.`
  }
  return `My best guess is that ${moment.playedSan} may have been aiming for ${plan}, but ${counterplay} may disrupt that plan.`
}

function difficultyText(
  classification: Extract<
    GameReview["criticalMoments"][number]["classification"],
    { kind: "positiveHighlight" }
  >,
): string {
  const eloRelative = classification.qualification.reasons.some(
    (reason) => reason.lane === "eloRelative",
  )
  if (!eloRelative) return "This required precise play."
  return classification.grade === "great"
    ? "This was especially difficult to find for players at your rating."
    : "This was a notable find for players at your rating."
}

function teachingTakeaway(
  moment: GameReview["criticalMoments"][number],
): string | null {
  const theme = moment.teaching.themes[0]
  if (theme) {
    switch (theme) {
      case "forcedMateConversion":
        return "Takeaway: convert a forced mate with forcing moves."
      case "passedPawnPromotion":
        return "Takeaway: advance passed pawns with promotion in mind."
      case "queenExchange":
        return "Takeaway: consider a queen exchange when it improves the resulting position."
    }
  }
  const principle = moment.teaching.openingPrinciples[0]
  if (principle === "occupyTheCenter") {
    return "Takeaway: fight for the center early."
  }
  return null
}

function neutralReasonText(
  reason: Extract<
    GameReview["criticalMoments"][number]["classification"],
    { kind: "neutral" }
  >["reasons"][number],
): string {
  switch (reason) {
    case "mechanicallyForcedMove":
      return "it was mechanically forced"
    case "soundWithoutConcreteAchievement":
      return "it was sound without a concrete achievement"
    case "belowImprovementThreshold":
      return "it stayed below the improvement threshold"
    case "nonInstructionalTerminalOutcome":
      return "the terminal outcome did not add an instructional point"
  }
}

function achievementText(
  achievement: Extract<
    GameReview["criticalMoments"][number]["classification"],
    {
      kind: "positiveHighlight"
    }
  >["qualification"]["achievements"][number],
): string {
  switch (achievement.kind) {
    case "completedCheckmate":
      return "completed checkmate"
    case "capturedPiece":
      return `captured the ${achievement.role} on ${achievement.square}`
    case "advancedPassedPawn":
      return `advanced the passed pawn to ${achievement.toSquare}`
    case "tacticalPayoff":
      switch (achievement.payoff.kind) {
        case "mate":
          return "secured a mating payoff"
        case "promotion":
          return "secured a promotion payoff"
        case "winsMaterialOutright":
          return `won a ${achievement.payoff.role}`
        case "winsMaterialNet":
          return `won a ${achievement.payoff.role} and came out ${pawnUnitsText(
            achievement.payoff.netPawnUnits,
          )} ahead`
        case "queenExchange":
          return "secured a queen exchange"
      }
  }
}

function playedOutcomeText(
  moment: GameReview["criticalMoments"][number],
): string {
  if (moment.playedMoveOutcome.kind === "analyzed") {
    return `After ${moment.playedSan}, the evaluation is ${moment.display.playedEvaluation.score} — ${moment.display.playedEvaluation.label}.`
  }
  return `After ${moment.playedSan}, ${terminalOutcomeText(moment.playedMoveOutcome.outcome)}.`
}

function correctionText(
  moment: GameReview["criticalMoments"][number],
  correction: Extract<
    GameReview["criticalMoments"][number]["classification"],
    {
      kind: "improvementOpportunity"
    }
  >["correction"],
): string {
  if (correction.outcome.kind === "improvedAnalyzed") {
    return `The better move was ${correction.betterMoveSan}, leaving the evaluation at ${moment.display.bestEvaluation.score} — ${moment.display.bestEvaluation.label}.`
  }
  return `The better move was ${correction.betterMoveSan}, avoiding the recorded outcome where ${terminalOutcomeText(correction.outcome.avoided)}.`
}

function terminalOutcomeText(
  outcome: Extract<
    GameReview["criticalMoments"][number]["playedMoveOutcome"],
    {
      kind: "terminal"
    }
  >["outcome"],
): string {
  switch (outcome.kind) {
    case "checkmate":
      return `${
        outcome.winner === "white" ? "White" : "Black"
      } delivered checkmate`
    case "stalemate":
      return "the game ended in stalemate"
    case "insufficientMaterial":
      return "the game ended because there was insufficient material"
  }
}

function consequenceText(
  classification: GameReview["criticalMoments"][number]["residualOutcome"]["classification"],
): string {
  switch (classification) {
    case "missedForcedMate":
      return "the forced mate was missed"
    case "advantageKept":
      return "the advantage was kept"
    case "standingKept":
      return "the position's standing was kept"
    case "advantageReduced":
      return "the advantage was reduced"
    case "advantageLost":
      return "the advantage was lost"
    case "nowWorse":
      return "the position became unfavorable"
  }
}

export type SessionCommentFields = {
  comment: CriticalMomentComment | null
  commentPublished: boolean | null
  firstOpened: boolean
  firstOpenStartedAt: number | null
  openingText: string | null
  safeRendering: string
}

/**
 * Seed a Review Session from the frozen Game Review. A published comment
 * renders immediately; a moment without one starts the authoring wait so the
 * engine's freshly authored comment — intent hypothesis and all — lands via
 * openPreparedMoment's first-open patch instead of the client's re-derived
 * template freezing first. The deadline fallback still settles to the Safe
 * Review Moment Rendering when the engine does not answer.
 */
export function frozenSessionCommentFields(
  core: ReviewSessionCoreContract,
  review: GameReview,
): SessionCommentFields {
  const comment = publishedCommentForReviewMoment(core, review)
  if (comment != null) {
    return sessionCommentFields(core, review, {
      comment,
      commentPublished: true,
    })
  }
  return sessionCommentFields(core, review, undefined, true)
}

export function sessionCommentFields(
  core: ReviewSessionCoreContract,
  review: GameReview,
  opened?: {
    comment?: CriticalMomentComment | null
    commentPublished?: boolean
    /** Only the intent is read; the rest of the engine's context is not this function's business. */
    authoringContext?: {
      intent?: CriticalMomentIntentAuthoringContext | null
    } | null
  },
  startWait = false,
): SessionCommentFields {
  const reviewedMoment = review.criticalMoments.find(
    (moment) => moment.criticalMomentId === core.reviewMoment.momentId,
  )
  // The engine discloses the intent it authored against only while the moment
  // is open, so the rendering carries it exactly when the engine's would.
  const safeRendering = reviewedMoment
    ? factualReviewText(reviewedMoment, opened?.authoringContext?.intent)
    : (opened?.comment?.text ?? "")
  if (opened) {
    const comment = opened.comment ?? null
    const commentPublished = opened.commentPublished ?? Boolean(comment)
    return {
      comment,
      commentPublished,
      firstOpened: true,
      firstOpenStartedAt: null,
      openingText: commentPublished
        ? (comment?.text ?? "")
        : (comment?.text ?? safeRendering),
      safeRendering,
    }
  }
  return {
    comment: null,
    commentPublished: null,
    firstOpened: false,
    firstOpenStartedAt: startWait ? Date.now() : null,
    openingText: null,
    safeRendering,
  }
}

/** Placeholder session so a player-selected ply can wait before openReviewMoment returns. */
export function waitingPlayerSelectedSession(
  current: {
    gameImportId: GameImportId
    core: ReviewSessionCoreContract
    learningMaterial: ReviewMomentLearningMaterial
  },
  review: GameReview,
  ply: number,
) {
  const momentId = fromCriticalMomentId(
    `review-moment:pending:${current.core.reviewMoment.gameRef}:${ply}`,
  )
  const core: ReviewSessionCoreContract = {
    ...current.core,
    reviewMoment: {
      ...current.core.reviewMoment,
      momentId,
      ply,
      selection: { kind: "playerSelectedMoment", ply },
    },
  }
  return {
    gameImportId: current.gameImportId,
    core,
    criticalPly: ply,
    ...sessionCommentFields(core, review, undefined, true),
    learningMaterial: {
      ...current.learningMaterial,
      tracks: [],
    },
    nominatedMoment: null,
    nominatedClassification: null,
    placeholder: true,
  }
}
