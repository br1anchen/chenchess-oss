import { type ReactNode, useEffect, useMemo, useRef, useState } from "react"

import type {
  AlternativeMoveId,
  AlternativeMoveResult,
  CanonicalGameMove,
  GameImportId,
  HostTurnPriorTurn,
  HostTurnShowLine,
  ImportedGame,
  GameReview,
  EngineEvaluation,
  GameReviewCriticalMoment,
  PositionInspection,
  PositionSnapshot,
  ReviewSessionCoreContract,
  ReviewSessionMoment,
  Square,
} from "@chenchess/coach-engine-sdk"
import {
  PLAYER_VISIBLE_MOVE_FALLBACK,
  criticalMomentComparisonArrows,
  engineMoveArrow,
  occurrenceMoveLabel,
  playerVisibleAlternativeMove,
  playerVisibleSanFromLegalUci,
  playerVisibleSanLiteral,
  playerVisibleStrongestReply,
  type PlayerVisibleSan,
} from "@chenchess/review-projection"
import { Button, WatercolorNotice, type BoardArrow } from "@chenchess/ui"
import { WatercolorOverlay } from "@/overlay/WatercolorOverlay"

import {
  BoardWorkspace,
  ReviewBranchControls,
  ReviewMoveControls,
  boardMaxPly,
  reviewSessionEvaluationGraph,
  type ExploredBranchLabel,
} from "./BoardWorkspace"
import { boardArrowsFrom } from "./boardArrows"
import { ReviewGameHeaderInfo } from "./ReviewGameHeaderInfo"
import { AccountSettings } from "./AccountSettings"
import { ReviewSessionShell } from "./ReviewSessionShell"
import { ReviewSessionView } from "./ReviewSessionView"
import { EmptyReviewSession } from "./EmptyReviewSession"
import { createIdempotencyKey } from "./client"
import {
  browseBoardAtPly,
  bestMoveUciFromCore,
  type BrowseBoardPosition,
  type EvaluationPoint,
  evaluationFromCore,
  evaluationPoint,
  legalDestinations,
  moveLabel,
  type PromotionRole,
  promotionRequired,
  reviewSideOrientation,
  uciForDestination,
} from "./model"
import {
  type ActiveOperation,
  type CancellableOperation,
  type OperationLane,
  useReviewSessionCommands,
} from "./useReviewSessionCommands"
import {
  type ActiveSession,
  type BranchView,
  type MomentWorkspace,
  branchAlternativeMoveId,
  branchRefOf,
  exploredBranchResults,
  useMomentWorkspaces,
} from "./useMomentWorkspaces"
import {
  replaceGameReviewPath,
  replaceViewedPly,
} from "@/game-review/gameReviewRoute"
import {
  learningPathsForReviewMoment,
  type MomentLearningPath,
  type NominatedMarkerSource,
  type ReviewMomentMarker,
  curatedReviewMomentMarkers,
  frozenSessionCommentFields,
  publishedCommentForReviewMoment,
  sessionCommentFields,
  waitingPlayerSelectedSession,
} from "./reviewMoments"
import { useReviewRetentionPreference } from "./useReviewRetentionPreference"
import { useReviewFeedback } from "./useReviewFeedback"
import { useLearningPathFeedback } from "./useLearningPathFeedback"
import {
  hostTurnStepLabels,
  priorHostTurns,
  shownLineLabel,
  type ComposerState,
  type HostTurnStepDisplayLabel,
  type WorkspaceThreadDraft,
  type WorkspaceThreadItem,
} from "./thread-state"

type FetchAccessToken = (options: {
  forceRefreshToken: boolean
}) => Promise<string | null>

type ReviewSessionWorkspaceProps = {
  fetchAccessToken: FetchAccessToken
  initialGameImportId: GameImportId
  initialPly: number | null
  onUnavailableGameImport: () => void
  reauthenticate: (password: string) => Promise<void>
  signOut: () => Promise<void>
}

type PendingPromotion = {
  from: Square
  to: Square
}

type InspectionLane = Extract<
  OperationLane,
  "navigation" | "alternative" | "hostTurn"
>

type ActiveLanes = Partial<Record<OperationLane, ActiveOperation>>

function nominatedMarkerSources(
  workspaces: readonly MomentWorkspace[],
): NominatedMarkerSource[] {
  return workspaces.flatMap((candidate) => {
    if (
      candidate.session.core.reviewMoment.selection.kind !==
      "playerSelectedMoment"
    ) {
      return []
    }
    const occurrence = candidate.session.core.reviewMoment
    return [
      {
        ply: candidate.session.criticalPly,
        facts: candidate.session.nominatedMoment,
        placeholder: candidate.session.placeholder,
        classificationKind: candidate.session.nominatedClassification,
        moveLabel: occurrenceMoveLabel(occurrence),
      },
    ]
  })
}

function reviewSessionBindings(
  workspace: MomentWorkspace | null,
  review: GameReview | null,
  workspaces: readonly MomentWorkspace[],
  momentlessReviewId: GameImportId | null,
) {
  const session = workspace?.session ?? null
  const branches = workspace?.branches ?? []
  const activeBranchId = workspace?.activeBranchId ?? null
  const messages = workspace?.messages ?? []
  const gameImportId = session?.gameImportId ?? momentlessReviewId
  const momentMarkers = review
    ? curatedReviewMomentMarkers(review, nominatedMarkerSources(workspaces))
    : []
  const activeBranch =
    branches.find(
      (branch) => branchAlternativeMoveId(branch) === activeBranchId,
    ) ?? null
  return {
    session,
    branches,
    activeBranchId,
    messages,
    gameImportId,
    momentMarkers,
    activeBranch,
  }
}

function reviewBoardBindings(
  activeBranch: BranchView | null,
  session: ActiveSession | null,
) {
  const position =
    activeBranch?.inspection.positionSnapshot ??
    session?.core.positionSnapshot ??
    null
  const evaluation =
    activeBranch?.inspection.evaluation ??
    (session ? evaluationFromCore(session.core) : null)
  return { position, evaluation }
}

function reviewBusyBindings(active: ActiveLanes) {
  const navigationBusy = active.navigation !== undefined
  const hostTurnBusy = active.hostTurn !== undefined
  const alternativeBusy = active.alternative !== undefined
  const composerBusy = navigationBusy || hostTurnBusy || alternativeBusy
  const navigationDisabled = alternativeBusy || hostTurnBusy
  const boardInteractionBusy = navigationBusy || alternativeBusy || hostTurnBusy
  const conversationBusy =
    active.navigation?.label ?? active.hostTurn?.label ?? null
  return {
    navigationBusy,
    alternativeBusy,
    composerBusy,
    navigationDisabled,
    boardInteractionBusy,
    conversationBusy,
  }
}

function reviewBrowseBindings(
  session: ActiveSession | null,
  viewedPly: number | null,
  activeBranch: BranchView | null,
  position: PositionSnapshot | null,
  evaluation: EngineEvaluation | null,
  review: GameReview | null,
) {
  const boardPly = viewedPly ?? session?.criticalPly ?? null
  const browsing =
    session !== null &&
    boardPly !== null &&
    boardPly !== session.criticalPly &&
    activeBranch === null
  const boardPosition =
    browsing && session
      ? browseBoardAtPly(session.core.importedGame.game.moves, boardPly)
      : position
  const boardEvaluation = browsing
    ? (review?.evaluationTimeline.find((point) => point.ply === boardPly)
        ?.evaluation ?? evaluation)
    : evaluation
  return { boardPly, browsing, boardPosition, boardEvaluation }
}

function reviewFeedbackTargetId(session: ActiveSession | null) {
  return session
    ? `${session.gameImportId}:${session.core.reviewMoment.momentId}:${session.criticalPly}`
    : ""
}

function cancellableLane(
  operation: ActiveOperation | undefined,
  kind: CancellableOperation["kind"],
): CancellableOperation | null {
  return operation?.kind === kind ? operation : null
}

export function ReviewSessionWorkspace({
  fetchAccessToken,
  initialGameImportId,
  initialPly,
  onUnavailableGameImport,
  reauthenticate,
  signOut,
}: ReviewSessionWorkspaceProps) {
  const [snapshot, setSnapshot] = useState<ImportedGame | null>(null)
  const [review, setReview] = useState<GameReview | null>(null)
  const [momentlessReviewId, setMomentlessReviewId] =
    useState<GameImportId | null>(null)
  const [selectedSquare, setSelectedSquare] = useState<Square | null>(null)
  const [pendingPromotion, setPendingPromotion] =
    useState<PendingPromotion | null>(null)
  const [evaluationPoints, setEvaluationPoints] = useState<EvaluationPoint[]>(
    [],
  )
  // The address is the board's position, so a reload and a copied link both
  // land on the ply the Player was looking at. Held in a ref as well because
  // opening the first moment sets its own ply, and on first paint the address
  // is the more specific answer; later opens have no address to defer to.
  const addressedPly = useRef(initialPly)
  const [viewedPly, setViewedPly] = useState<number | null>(
    () => addressedPly.current,
  )
  useEffect(() => {
    replaceViewedPly(viewedPly)
  }, [viewedPly])
  const messageIdentity = useRef(0)
  const coachRequestVersion = useRef(0)
  const openedGameReview = useRef<GameImportId | null>(null)
  const { active, failure, invalidate, run, runIndependent, setFailure } =
    useReviewSessionCommands(fetchAccessToken, onUnavailableGameImport)
  const retention = useReviewRetentionPreference(fetchAccessToken)
  const reviewFeedback = useReviewFeedback(fetchAccessToken)
  const momentWorkspaces = useMomentWorkspaces()
  const [playerSelectedWaiting, setPlayerSelectedWaiting] = useState(false)
  const [accountSettingsOpen, setAccountSettingsOpen] = useState(false)
  const engineOpenPlyRef = useRef<number | null>(null)
  /** In-flight Engine opens by moment id, so a background open from moment
   * selection and an awaited open from an interaction share one command. */
  const momentOpens = useRef(new Map<string, Promise<boolean>>())
  const workspace = momentWorkspaces.active
  const {
    session,
    branches,
    activeBranchId,
    messages,
    gameImportId,
    momentMarkers,
    activeBranch,
  } = reviewSessionBindings(
    workspace,
    review,
    momentWorkspaces.workspaces,
    momentlessReviewId,
  )
  const { position, evaluation } = reviewBoardBindings(activeBranch, session)
  const destinations = useMemo(
    () =>
      position && selectedSquare
        ? legalDestinations(position, selectedSquare)
        : [],
    [position, selectedSquare],
  )
  const {
    navigationBusy,
    alternativeBusy,
    composerBusy,
    navigationDisabled,
    boardInteractionBusy,
    conversationBusy,
  } = reviewBusyBindings(active)
  const { boardPly, browsing, boardPosition, boardEvaluation } =
    reviewBrowseBindings(
      session,
      viewedPly,
      activeBranch,
      position,
      evaluation,
      review,
    )
  // Last-intent: the Review Moment picker stays usable while an open is in
  // flight so a later selection can supersede. HostTurn and exploration still
  // block navigation.
  const learningPaths = useMemo(
    () =>
      session
        ? learningPathsForReviewMoment(
            session.learningMaterial,
            session.core.reviewMoment.momentId,
          )
        : [],
    [session],
  )
  const learningPathRefs = useMemo(
    () => learningPaths.map((path) => path.learningPathRef),
    [learningPaths],
  )
  const learningPathFeedback = useLearningPathFeedback(
    session?.gameImportId ?? null,
    learningPathRefs,
    runIndependent,
  )
  /** One Coach comment is one Review Moment at one ply, and so is one vote. */
  const reviewFeedbackTarget = reviewFeedbackTargetId(session)

  useEffect(() => {
    if (
      !initialGameImportId ||
      openedGameReview.current === initialGameImportId
    ) {
      return
    }
    openedGameReview.current = initialGameImportId
    void startSession(initialGameImportId, "import").then((opened) => {
      // A transient failure must not spend the one addressed open: clearing
      // the guard lets Try again (or a later navigation) reopen it.
      if (!opened && openedGameReview.current === initialGameImportId) {
        openedGameReview.current = null
      }
    })
    // startSession is re-created every render; the ref guard above already
    // makes this open once per addressed Game Review.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialGameImportId, run])

  /** Whether the session actually started; a false lets the caller reopen
   * the same Game Review after a transient failure. */
  async function startSession(
    gameImportId: GameImportId,
    lane: Extract<OperationLane, "import" | "navigation">,
    replacePath = true,
  ): Promise<boolean> {
    const started = await run(
      lane,
      {
        kind: "startReviewSession",
        gameImportId,
      },
      "Opening the review…",
    )
    if (started?.kind !== "reviewSessionStarted") return false
    if (replacePath) replaceGameReviewPath(started.gameImportId)
    setReview(started.review)
    setSnapshot(started.importedGame)
    // The measured real-game graph is the review's own timeline. Opening by
    // address is the only entry now, so seeding it here is what keeps the
    // graph whole rather than sparse with the moments that happen to carry an
    // evaluation.
    setEvaluationPoints(
      started.review.evaluationTimeline.map((point) =>
        evaluationPoint(point.ply, point.evaluation),
      ),
    )
    const prepared = preparedMoments(started.reviewMoments)
    if (prepared.length !== started.reviewMoments.length) {
      setFailure("The review did not finish loading. Try again.")
      return true
    }
    if (prepared.length === 0) {
      momentWorkspaces.clear()
      setMomentlessReviewId(started.gameImportId)
      return true
    }
    activatePreparedSession(started.gameImportId, prepared, started.review)
    return true
  }

  /** Which ply the Engine currently holds open. Read only from handlers and
   * awaited flows, so it is a ref: no render depends on it. */
  function recordEngineOpenPly(ply: number | null) {
    engineOpenPlyRef.current = ply
  }

  function activatePreparedSession(
    gameImportId: GameImportId,
    moments: readonly PreparedReviewMoment[],
    startedReview: GameReview,
  ) {
    const first = moments[0]
    if (!first) return
    coachRequestVersion.current += 1
    invalidate()
    setMomentlessReviewId(null)
    recordEngineOpenPly(null)
    momentOpens.current.clear()
    momentWorkspaces.activateAll(
      moments.map(({ core, learningMaterial, classificationKind }) => ({
        session: {
          gameImportId,
          core,
          criticalPly: core.reviewMoment.ply,
          ...frozenSessionCommentFields(core, startedReview),
          learningMaterial,
          nominatedMoment: null,
          nominatedClassification: classificationKind,
          placeholder: false,
        },
        branches: [],
        activeBranchId: null,
        shownLine: null,
        messages: [],
      })),
    )
    setViewedPly(addressedPly.current ?? first.core.reviewMoment.ply)
    addressedPly.current = null
    setSelectedSquare(null)
    setPendingPromotion(null)
    // The first moment opens right away so the Engine authors its comment —
    // the seed above starts the wait the panel shows meanwhile.
    if (!publishedCommentForReviewMoment(first.core, startedReview)) {
      void openPreparedMoment(gameImportId, first.core, startedReview)
    }
    const points = moments.flatMap(({ core }) => {
      const evaluation = evaluationFromCore(core)
      return evaluation
        ? [evaluationPoint(core.reviewMoment.ply, evaluation)]
        : []
    })
    setEvaluationPoints((current) => [
      ...current.filter(
        (candidate) => !points.some((point) => point.ply === candidate.ply),
      ),
      ...points,
    ])
  }

  /**
   * Open a prepared moment on the Engine and land its authored comment.
   *
   * Runs on the `coach` lane so a background open (moment selection) never
   * locks navigation or the composer. Concurrent callers for the same moment
   * share one in-flight open. Mirroring the MCP host glue, an authored but
   * unpublished comment is published back so the Review Annotation Store
   * keeps the same text the thread shows.
   */
  function openPreparedMoment(
    targetGameImportId: GameImportId,
    core: ReviewSessionCoreContract,
    reviewForComment: GameReview,
  ): Promise<boolean> {
    const momentKey = String(core.reviewMoment.momentId)
    const inFlight = momentOpens.current.get(momentKey)
    if (inFlight) return inFlight
    const opening = runPreparedMomentOpen(
      targetGameImportId,
      core,
      reviewForComment,
    ).finally(() => momentOpens.current.delete(momentKey))
    momentOpens.current.set(momentKey, opening)
    return opening
  }

  async function runPreparedMomentOpen(
    targetGameImportId: GameImportId,
    core: ReviewSessionCoreContract,
    reviewForComment: GameReview,
  ) {
    const opened = await run(
      "coach",
      {
        kind: "openReviewMoment",
        idempotencyKey: createIdempotencyKey(),
        gameImportId: targetGameImportId,
        selection: core.reviewMoment.selection,
      },
      "Opening the moment…",
    )
    if (opened?.kind !== "reviewMomentOpened") return false
    recordEngineOpenPly(core.reviewMoment.ply)
    let openedFields = opened
    if (opened.comment && !opened.commentPublished && opened.authoringContext) {
      const published = await run(
        "coach",
        {
          kind: "publishReviewMomentComment",
          gameImportId: targetGameImportId,
          groundingLedger: opened.authoringContext.requiredGroundingLedger,
          idempotencyKey: createIdempotencyKey(),
          reviewMomentId: opened.reviewMoment.reviewMoment.momentId,
          text: opened.comment.text,
        },
        "Writing the coaching note…",
      )
      if (published?.kind === "reviewMomentCommentPublished") {
        openedFields = {
          ...opened,
          comment: published.comment,
          commentPublished: true,
        }
      }
    }
    momentWorkspaces.patch(core.reviewMoment.ply, (workspace) => {
      if (workspace.session.firstOpened) return workspace
      return {
        ...workspace,
        session: {
          ...workspace.session,
          ...sessionCommentFields(
            openedFields.reviewMoment,
            reviewForComment,
            openedFields,
          ),
          core: openedFields.reviewMoment,
          learningMaterial: openedFields.criticalMoment.learningMaterial,
          nominatedMoment:
            openedFields.reviewMoment.reviewMoment.selection.kind ===
            "playerSelectedMoment"
              ? openedFields.criticalMoment
              : workspace.session.nominatedMoment,
          nominatedClassification:
            openedFields.criticalMoment.classification.kind,
          placeholder: false,
        },
      }
    })
    return true
  }

  function beginNavigation() {
    coachRequestVersion.current += 1
    invalidate()
    setFailure(null)
    setSelectedSquare(null)
    setPendingPromotion(null)
  }

  function browse(ply: number) {
    if (!review || navigationDisabled) return
    setSelectedSquare(null)
    setPendingPromotion(null)
    setViewedPly(ply)
  }

  async function selectMoment(ply: number) {
    if (!review || !gameImportId || navigationDisabled) return
    const existing = momentWorkspaces.workspaces.find(
      (workspace) => workspace.session.criticalPly === ply,
    )
    if (!existing) {
      browse(ply)
      return
    }
    if (
      session &&
      ply === session.criticalPly &&
      !activeBranch &&
      engineOpenPlyRef.current === ply
    ) {
      setSelectedSquare(null)
      setPendingPromotion(null)
      setViewedPly(ply)
      return
    }
    momentWorkspaces.open(ply)
    setSelectedSquare(null)
    setPendingPromotion(null)
    setViewedPly(ply)
    if (!existing.session.firstOpened && !existing.session.placeholder) {
      void openPreparedMoment(gameImportId, existing.session.core, review)
    }
  }

  async function ensureMomentOpen(target: ActiveSession) {
    if (engineOpenPlyRef.current === target.criticalPly) return true
    if (!review) return false
    return openPreparedMoment(target.gameImportId, target.core, review)
  }

  async function nominate(ply: number): Promise<ActiveSession | null> {
    if (!review || !gameImportId || navigationDisabled) return null
    if (
      momentMarkers.some((marker) => marker.ply === ply) ||
      momentWorkspaces.workspaces.some(
        (workspace) => workspace.session.criticalPly === ply,
      )
    ) {
      await selectMoment(ply)
      return (
        momentWorkspaces.workspaces.find(
          (workspace) => workspace.session.criticalPly === ply,
        )?.session ?? null
      )
    }
    beginNavigation()
    setViewedPly(ply)
    const previousPly = session?.criticalPly ?? null
    if (session) {
      momentWorkspaces.upsert({
        session: waitingPlayerSelectedSession(session, review, ply),
        branches: [],
        activeBranchId: null,
        shownLine: null,
        messages: [],
      })
    } else {
      setPlayerSelectedWaiting(true)
    }
    const opened = await run(
      "navigation",
      {
        kind: "openReviewMoment",
        idempotencyKey: createIdempotencyKey(),
        gameImportId,
        selection: { kind: "playerSelectedMoment", ply },
      },
      "Opening the moment…",
    )
    setPlayerSelectedWaiting(false)
    if (opened?.kind !== "reviewMomentOpened") {
      if (previousPly != null) {
        momentWorkspaces.open(previousPly)
        setViewedPly(previousPly)
      }
      return null
    }
    const core = opened.reviewMoment
    const nominatedMoment: GameReviewCriticalMoment = opened.criticalMoment
    recordEngineOpenPly(core.reviewMoment.ply)
    const openedSession: ActiveSession = {
      gameImportId,
      core,
      criticalPly: core.reviewMoment.ply,
      ...sessionCommentFields(core, review, opened),
      learningMaterial: opened.criticalMoment.learningMaterial,
      nominatedMoment,
      nominatedClassification: nominatedMoment.classification.kind,
      placeholder: false,
    }
    momentWorkspaces.upsert({
      session: openedSession,
      branches: [],
      activeBranchId: null,
      shownLine: null,
      messages: [],
    })
    const openedEvaluation = evaluationFromCore(core)
    if (openedEvaluation) {
      setEvaluationPoints((current) => [
        ...current.filter((point) => point.ply !== core.reviewMoment.ply),
        evaluationPoint(core.reviewMoment.ply, openedEvaluation),
      ])
    }
    return openedSession
  }

  function selectSquare(square: Square) {
    if (!position || boardInteractionBusy) return
    setPendingPromotion(null)
    if (selectedSquare && destinations.includes(square)) {
      const from = selectedSquare
      setSelectedSquare(null)
      if (promotionRequired(position, from, square)) {
        setPendingPromotion({ from, to: square })
      } else {
        void exploreMove(uciForDestination(position, from, square))
      }
      return
    }
    const piece = position.occupied.find(
      (entry) => entry.square === square,
    )?.piece
    setSelectedSquare(piece?.color === position.sideToMove ? square : null)
  }

  function promote(role: PromotionRole) {
    if (!position || !pendingPromotion) {
      throw new Error("Promotion controls require an active promotion move")
    }
    const { from, to } = pendingPromotion
    setPendingPromotion(null)
    void exploreMove(uciForDestination(position, from, to, role))
  }

  async function exploreMove(uci: string) {
    if (!session || !position || boardInteractionBusy) return
    if (!(await ensureMomentOpen(session))) return
    const key = createIdempotencyKey()
    const sourceBranchId = activeBranchId
    const explored = await run(
      "alternative",
      {
        kind: "exploreAlternativeMove",
        gameImportId: session.gameImportId,
        reviewMomentId: session.core.reviewMoment.momentId,
        parent: activeBranch
          ? { kind: "move", branchRef: branchRefOf(activeBranch) }
          : {
              kind: "root",
              positionRef: session.core.positionSnapshot.positionRef,
            },
        sourcePositionRef: position.positionRef,
        moveInput: { kind: "uci", uci },
        idempotencyKey: key,
      },
      `Evaluating ${playerVisibleSanFromLegalUci(position.fen, uci)}…`,
    )
    if (explored?.kind !== "alternativeMoveEvaluated") return

    const inspected = await loadInspection(
      "alternative",
      session,
      explored.alternativeMove.alternativeMoveId,
    )
    if (!inspected) return
    const branch = {
      kind: "explored" as const,
      result: explored.alternativeMove,
      inspection: inspected,
    }
    // Exploring is the Player's own browsing, not a coaching turn: the branch
    // list and the board already carry the evaluation, so narrating each one
    // into the thread only buries the coaching in bookkeeping.
    momentWorkspaces.update(session, (current) => ({
      ...current,
      branches: [...current.branches, branch],
      activeBranchId:
        current.activeBranchId === sourceBranchId
          ? branch.result.alternativeMoveId
          : current.activeBranchId,
    }))
  }

  function settleUnpublished(ply: number) {
    momentWorkspaces.patch(ply, (workspace) => {
      if (workspace.session.firstOpened) return workspace
      return {
        ...workspace,
        session: {
          ...workspace.session,
          commentPublished: false,
          firstOpened: true,
          firstOpenStartedAt: null,
          /* Whatever the engine already sent outranks the local rendering:
             the engine authors against intent this browser may not hold, so
             replacing its prose on a deadline can only lose ground. */
          openingText:
            workspace.session.openingText || workspace.session.safeRendering,
        },
      }
    })
  }

  function sendMessage(text: string) {
    if (composerBusy) return
    if (
      browsing &&
      boardPly !== null &&
      !momentMarkers.some((marker) => marker.ply === boardPly)
    ) {
      void discussThenAsk(boardPly, text)
      return
    }
    if (!session) return
    const priorTurns = priorHostTurns(messages)
    appendThreadItem(session, {
      kind: "playerMessage",
      text,
    })
    void startHostTurn(session, text, priorTurns)
  }

  /** A message typed at a walked, unlisted position: nominate it as a
   * Player-Selected Moment, then ask the question inside the opened thread. */
  async function discussThenAsk(ply: number, text: string) {
    const opened = await nominate(ply)
    if (!opened) {
      // The open failed and its failure banner is up; keep the typed words in
      // the thread so they are not silently lost.
      if (session) {
        appendThreadItem(session, {
          kind: "systemNote",
          text: `Opening that position did not complete, so this was not sent: “${text}”. Walk back to it and try again.`,
        })
      }
      return
    }
    appendThreadItem(opened, { kind: "playerMessage", text })
    void startHostTurn(opened, text, [])
  }

  async function startHostTurn(
    targetSession: ActiveSession,
    message: string,
    priorTurns: HostTurnPriorTurn[],
  ) {
    if (!(await ensureMomentOpen(targetSession))) return
    if (engineOpenPlyRef.current !== targetSession.criticalPly) return
    const requestVersion = coachRequestVersion.current + 1
    coachRequestVersion.current = requestVersion
    const completed = await run(
      "hostTurn",
      {
        kind: "startHostTurn",
        gameImportId: targetSession.gameImportId,
        message,
        priorTurns,
        idempotencyKey: createIdempotencyKey(),
      },
      hostTurnStepLabels.writing,
    )
    if (coachRequestVersion.current !== requestVersion) return
    if (completed?.kind === "hostTurnCompleted") {
      const effects = {
        focusMoment: completed.focusMoment,
        showLine: completed.showLine,
      }
      appendThreadItem(targetSession, {
        kind: "coachAnswer",
        answer: completed.answer,
        effects,
      })
      await applyHostTurnEffects(
        targetSession.criticalPly,
        effects.focusMoment,
        effects.showLine,
      )
      const focusedPly =
        effects.focusMoment != null && effects.focusMoment > 0
          ? effects.focusMoment
          : null
      if (focusedPly != null && focusedPly !== targetSession.criticalPly) {
        momentWorkspaces.patch(focusedPly, (workspace) => ({
          ...workspace,
          messages: [
            ...workspace.messages,
            threadItemWithId({ kind: "playerMessage", text: message }),
            threadItemWithId({
              kind: "coachAnswer",
              answer: completed.answer,
              effects,
            }),
          ],
        }))
      }
      return
    }
    if (completed?.kind === "hostTurnRefused") {
      appendThreadItem(targetSession, {
        kind: "refusal",
        reason: completed.reason,
      })
      return
    }
    if (completed?.kind === "unavailable") {
      appendThreadItem(targetSession, {
        kind: "unavailable",
        reason: completed.reason,
      })
      return
    }
    if (completed?.kind === "rejected") {
      appendThreadItem(targetSession, {
        kind: "rejected",
        recovery: completed.recovery,
      })
    }
  }

  async function applyHostTurnEffects(
    sourcePly: number,
    focusMoment: number | null | undefined,
    showLine: HostTurnShowLine | null | undefined,
  ) {
    await applyShownLine(sourcePly, showLine)
    if (focusMoment != null && focusMoment > 0) {
      const existing = momentWorkspaces.workspaces.find(
        (workspace) => workspace.session.criticalPly === focusMoment,
      )
      if (existing) {
        await selectMoment(focusMoment)
        await ensureMomentOpen(existing.session)
      } else browse(focusMoment)
    }
  }

  async function applyShownLine(
    ply: number,
    showLine: HostTurnShowLine | null | undefined,
  ) {
    if (!showLine) {
      momentWorkspaces.patch(ply, (current) => ({
        ...current,
        shownLine: null,
      }))
      return
    }
    const workspace = momentWorkspaces.get(ply)
    if (!workspace) return
    setSelectedSquare(null)
    setPendingPromotion(null)
    switch (showLine.kind) {
      case "alternativeMove": {
        const inspection = await loadInspection(
          "hostTurn",
          workspace.session,
          showLine.alternativeMoveId,
        )
        if (!inspection) return
        momentWorkspaces.patch(ply, (current) => {
          const existing = current.branches.some(
            (branch) =>
              branchAlternativeMoveId(branch) === showLine.alternativeMoveId,
          )
          if (existing) {
            return {
              ...current,
              branches: current.branches.map((branch) =>
                branchAlternativeMoveId(branch) === showLine.alternativeMoveId
                  ? { ...branch, inspection }
                  : branch,
              ),
              activeBranchId: showLine.alternativeMoveId,
              shownLine: showLine,
            }
          }
          const created = branchViewFromInspection(
            showLine.alternativeMoveId,
            inspection,
          )
          if (!created) return current
          return {
            ...current,
            branches: [...current.branches, created],
            activeBranchId: showLine.alternativeMoveId,
            shownLine: showLine,
          }
        })
        return
      }
      case "engineBest":
      case "playedMoveRefutation": {
        const inspection = await loadInspection(
          "hostTurn",
          workspace.session,
          null,
        )
        if (!inspection) return
        momentWorkspaces.patch(ply, (current) => ({
          ...current,
          session: sessionWithInspection(current.session, inspection),
          shownLine: showLine,
        }))
        return
      }
      default: {
        const _exhaustive: never = showLine
        return _exhaustive
      }
    }
  }

  async function loadInspection(
    lane: InspectionLane,
    targetSession: ActiveSession,
    alternativeMoveId: AlternativeMoveId | null,
  ): Promise<PositionInspection | null> {
    const inspected = await run(
      lane,
      {
        kind: "inspectPosition",
        gameImportId: targetSession.gameImportId,
        reviewMomentId: targetSession.core.reviewMoment.momentId,
        target: alternativeMoveId
          ? { kind: "alternativeMove", alternativeMoveId }
          : { kind: "reviewedMove" },
      },
      "Refreshing the engine lines…",
    )
    return inspected?.kind === "positionInspected" ? inspected.inspection : null
  }

  async function refreshInspection(
    lane: InspectionLane,
    targetSession: ActiveSession,
    alternativeMoveId: AlternativeMoveId | null,
  ): Promise<PositionInspection | null> {
    const inspection = await loadInspection(
      lane,
      targetSession,
      alternativeMoveId,
    )
    if (!inspection) return null
    momentWorkspaces.update(targetSession, (current) => ({
      ...current,
      branches: alternativeMoveId
        ? current.branches.map((branch) =>
            branchAlternativeMoveId(branch) === alternativeMoveId
              ? { ...branch, inspection }
              : branch,
          )
        : current.branches,
      session: alternativeMoveId
        ? current.session
        : sessionWithInspection(current.session, inspection),
    }))
    return inspection
  }

  async function selectBranch(
    targetSession: ActiveSession,
    alternativeMoveId: AlternativeMoveId,
  ) {
    if (!(await ensureMomentOpen(targetSession))) return
    momentWorkspaces.update(targetSession, (current) => ({
      ...current,
      activeBranchId: alternativeMoveId,
      shownLine: null,
    }))
    setSelectedSquare(null)
    setPendingPromotion(null)
    await refreshInspection("navigation", targetSession, alternativeMoveId)
  }

  async function cancel(
    gameImportId: GameImportId,
    operation: CancellableOperation,
  ) {
    await run(
      "control",
      {
        kind: "cancelOperation",
        gameImportId,
        operationId: operation.operationId,
        idempotencyKey: operation.key,
      },
      "Cancelling…",
    )
  }

  function appendThreadItem(
    targetSession: ActiveSession,
    item: WorkspaceThreadDraft,
  ) {
    momentWorkspaces.update(targetSession, (current) => ({
      ...current,
      messages: [...current.messages, threadItemWithId(item)],
    }))
  }

  function threadItemWithId(item: WorkspaceThreadDraft): WorkspaceThreadItem {
    messageIdentity.current += 1
    const id = `message-${messageIdentity.current}`
    switch (item.kind) {
      case "playerMessage":
        return { kind: "playerMessage", id, text: item.text }
      case "coachAnswer":
        return {
          kind: "coachAnswer",
          id,
          answer: item.answer,
          effects: item.effects,
        }
      case "unavailable":
        return { kind: "unavailable", id, reason: item.reason }
      case "refusal":
        return { kind: "refusal", id, reason: item.reason }
      case "rejected":
        return { kind: "rejected", id, recovery: item.recovery }
      case "systemNote":
        return { kind: "systemNote", id, text: item.text }
      default: {
        const _exhaustive: never = item
        return _exhaustive
      }
    }
  }

  function changeGame() {
    coachRequestVersion.current += 1
    invalidate()
    setSnapshot(null)
    setReview(null)
    setMomentlessReviewId(null)
    momentWorkspaces.clear()
    setViewedPly(null)
    recordEngineOpenPly(null)
    momentOpens.current.clear()
    setEvaluationPoints([])
    setPendingPromotion(null)
    setFailure(null)
  }

  async function handleSignOut() {
    changeGame()
    await signOut()
  }

  const accountSettings = (
    <AccountSettings
      fetchAccessToken={fetchAccessToken}
      onDeleted={handleSignOut}
      reauthenticate={reauthenticate}
      retention={retention}
    />
  )
  const phase = reviewSessionPhase({
    failure,
    momentlessReviewId,
    navigationDisabled,
    onAccountSettings: () => setAccountSettingsOpen(true),
    onNavigate: (ply) => void nominate(ply),
    onRetryOpen: initialGameImportId
      ? () => {
          openedGameReview.current = initialGameImportId
          setFailure(null)
          void startSession(initialGameImportId, "import").then((opened) => {
            if (!opened && openedGameReview.current === initialGameImportId) {
              openedGameReview.current = null
            }
          })
        }
      : undefined,
    playerSelectedWaiting,
    position,
    review,
    session,
    signOut: handleSignOut,
    snapshot,
  })
  const sessionView =
    phase.kind === "gate" ? (
      phase.view
    ) : (
      <ActiveReviewSessionView
        active={active}
        activeBranch={activeBranch}
        alternativeBusy={alternativeBusy}
        branches={branches}
        composerBusy={composerBusy}
        conversationBusy={conversationBusy}
        criticalMoment={sessionCriticalMomentFacts(phase.review, phase.session)}
        browsing={browsing}
        destinations={browsing ? [] : destinations}
        evaluation={boardEvaluation}
        evaluationPoints={evaluationPoints}
        failure={failure}
        learningPathFeedback={learningPathFeedback}
        learningPaths={learningPaths}
        messages={messages}
        momentMarkers={momentMarkers}
        momentWorkspaces={momentWorkspaces}
        navigationBusy={navigationBusy}
        navigationDisabled={navigationDisabled}
        onAccountSettings={() => setAccountSettingsOpen(true)}
        onCancel={cancel}
        onClearBoardSelection={() => {
          setSelectedSquare(null)
          setPendingPromotion(null)
        }}
        onExploreMove={exploreMove}
        onNavigate={browse}
        onNominate={
          boardPly !== null &&
          !momentMarkers.some((marker) => marker.ply === boardPly)
            ? nominate
            : undefined
        }
        onPromote={promote}
        onSelectBranch={(id) => void selectBranch(phase.session, id)}
        onSelectMoment={(ply) => void selectMoment(ply)}
        onSelectSquare={selectSquare}
        onSendMessage={sendMessage}
        onSettleUnpublished={() => settleUnpublished(phase.session.criticalPly)}
        pendingPromotion={pendingPromotion}
        signOut={handleSignOut}
        position={boardPosition ?? phase.position}
        reviewFeedback={reviewFeedback}
        reviewFeedbackTarget={reviewFeedbackTarget}
        selectedSquare={browsing ? null : selectedSquare}
        session={phase.session}
        shownLine={workspaceShownLine(workspace)}
        viewedPly={boardPly ?? phase.session.criticalPly}
      />
    )
  return (
    <>
      {accountSettingsOpen ? (
        <WatercolorOverlay
          onOpenChange={setAccountSettingsOpen}
          open
          title="Account settings"
        >
          {accountSettings}
        </WatercolorOverlay>
      ) : null}
      {sessionView}
    </>
  )
}

function workspaceShownLine(workspace: MomentWorkspace | null) {
  return workspace?.shownLine ?? null
}

/** The Game Review entry behind the active session's Critical Moment. */
function sessionCriticalMomentFacts(
  review: GameReview,
  session: ActiveSession,
): GameReviewCriticalMoment | null {
  return (
    session.nominatedMoment ??
    review.criticalMoments.find(
      (moment) =>
        moment.criticalMomentId === session.core.reviewMoment.momentId,
    ) ??
    null
  )
}

function comparisonBoardArrows(
  moment: GameReviewCriticalMoment,
  elo: number,
): BoardArrow[] {
  return boardArrowsFrom(criticalMomentComparisonArrows(moment, elo))
}

/** The engine's strongest reply from an explored branch position, as the
 * board arrow the Best move button previews. */
function engineReplyArrow(uci: string): BoardArrow[] {
  return boardArrowsFrom([engineMoveArrow(uci)])
}

type ReviewSessionPhase =
  | { kind: "gate"; view: ReactNode }
  | {
      kind: "active"
      position: PositionSnapshot
      review: GameReview
      session: ActiveSession
    }

function reviewSessionPhase({
  failure,
  momentlessReviewId,
  navigationDisabled,
  onAccountSettings,
  onNavigate,
  onRetryOpen,
  playerSelectedWaiting,
  position,
  review,
  session,
  signOut,
  snapshot,
}: {
  failure: string | null
  momentlessReviewId: GameImportId | null
  navigationDisabled: boolean
  onAccountSettings: () => void
  onNavigate: (ply: number) => void
  /** Reopens the addressed Game Review after a failed deep-link open. */
  onRetryOpen?: () => void
  playerSelectedWaiting: boolean
  position: PositionSnapshot | null
  review: GameReview | null
  session: ActiveSession | null
  signOut: () => Promise<void>
  snapshot: ImportedGame | null
}): ReviewSessionPhase {
  if (review && session && position) {
    return { kind: "active", position, review, session }
  }
  if (momentlessReviewId && snapshot && review) {
    return {
      kind: "gate",
      view: (
        <EmptyReviewSession
          disabled={navigationDisabled}
          onAccountSettings={onAccountSettings}
          onOpen={onNavigate}
          review={review}
          signOut={signOut}
          snapshot={snapshot}
          waiting={playerSelectedWaiting}
        />
      ),
    }
  }
  return {
    kind: "gate",
    view: (
      <ReviewSessionShell
        board={
          failure && onRetryOpen ? (
            <WatercolorNotice glyph="…" heading="Game review">
              This Review Session did not open.{" "}
              <Button
                label="Try again"
                onClick={onRetryOpen}
                size="sm"
                type="button"
                variant="secondary"
              />
            </WatercolorNotice>
          ) : (
            <WatercolorNotice glyph="…" heading="Game review">
              Opening this Review Session…
            </WatercolorNotice>
          )
        }
        failure={failure}
        onAccountSettings={onAccountSettings}
        signOut={signOut}
        title="Game review"
      />
    ),
  }
}

function reviewSessionBoardModel(
  activeBranch: BranchView | null,
  branches: readonly BranchView[],
  session: ActiveSession,
  workspaceShownLine: HostTurnShowLine | null,
  momentMarkers: readonly ReviewMomentMarker[],
  viewedPly: number,
) {
  const exploredResult =
    activeBranch?.kind === "explored" ? activeBranch.result : null
  const inspectedMoveUci =
    activeBranch?.kind === "inspected" ? activeBranch.moveUci : null
  const shownLineLabelText = workspaceShownLine
    ? shownLineLabel(workspaceShownLine)
    : null
  const shownLineMove =
    workspaceShownLine?.kind === "engineBest"
      ? bestMoveUciFromCore(session.core)
      : inspectedMoveUci
  const strongestReplyLabel =
    exploredResult && exploredResult.strongestReply.kind === "offered"
      ? playerVisibleStrongestReply(
          exploredResult.strongestReply,
          exploredResult.resultingPosition.fen,
        )
      : null
  const importedGame = session.core.importedGame
  const heading = playerFacingBoardHeading({
    branch: exploredResult,
    branches: exploredBranchResults(branches),
    inspectedMoveUci,
    currentMove: importedGame.game.moves.find((move) => move.ply === viewedPly),
    frozenMoveLabel: momentMarkers.find((marker) => marker.ply === viewedPly)
      ?.moveLabel,
    reviewMomentFen: session.core.positionSnapshot.fen,
  })
  return {
    exploredResult,
    heading,
    importedGame,
    shownLineLabelText,
    shownLineMove,
    strongestReplyLabel,
  }
}

function ActiveReviewSessionView({
  active,
  activeBranch,
  alternativeBusy,
  branches,
  browsing,
  composerBusy,
  conversationBusy,
  criticalMoment,
  destinations,
  evaluation,
  evaluationPoints,
  failure,
  learningPathFeedback,
  learningPaths,
  messages,
  momentMarkers,
  momentWorkspaces,
  navigationBusy,
  navigationDisabled,
  onAccountSettings,
  onCancel,
  onClearBoardSelection,
  onExploreMove,
  onNavigate,
  onNominate,
  onPromote,
  onSelectBranch,
  onSelectMoment,
  onSelectSquare,
  onSendMessage,
  onSettleUnpublished,
  pendingPromotion,
  position,
  reviewFeedback,
  reviewFeedbackTarget,
  selectedSquare,
  session,
  shownLine,
  signOut,
  viewedPly,
}: {
  active: ActiveLanes
  activeBranch: BranchView | null
  alternativeBusy: boolean
  branches: readonly BranchView[]
  browsing: boolean
  composerBusy: boolean
  conversationBusy: string | null
  criticalMoment: GameReviewCriticalMoment | null
  destinations: Square[]
  evaluation: EngineEvaluation | null
  evaluationPoints: EvaluationPoint[]
  failure: string | null
  learningPathFeedback: ReturnType<typeof useLearningPathFeedback>
  learningPaths: readonly MomentLearningPath[]
  messages: MomentWorkspace["messages"]
  momentMarkers: readonly ReviewMomentMarker[]
  momentWorkspaces: ReturnType<typeof useMomentWorkspaces>
  navigationBusy: boolean
  navigationDisabled: boolean
  onAccountSettings: () => void
  onCancel: (
    gameImportId: GameImportId,
    operation: CancellableOperation,
  ) => Promise<void>
  onClearBoardSelection: () => void
  onExploreMove: (uci: string) => void
  onNavigate: (ply: number) => void
  onNominate?: (ply: number) => void
  onPromote: (role: PromotionRole) => void
  onSelectBranch: (alternativeMoveId: AlternativeMoveId) => void
  onSelectMoment: (ply: number) => void
  onSelectSquare: (square: Square) => void
  onSendMessage: (text: string) => void
  onSettleUnpublished: () => void
  pendingPromotion: PendingPromotion | null
  position: PositionSnapshot | BrowseBoardPosition
  reviewFeedback: ReturnType<typeof useReviewFeedback>
  reviewFeedbackTarget: string
  selectedSquare: Square | null
  session: ActiveSession
  shownLine: HostTurnShowLine | null
  signOut: () => Promise<void>
  viewedPly: number
}) {
  const cancellableAlternative = cancellableLane(
    active.alternative,
    "alternative",
  )
  const cancellableHostTurn = cancellableLane(active.hostTurn, "hostTurn")
  const { composer, pendingLabel } = workspaceComposer(
    active.hostTurn?.label,
    conversationBusy,
  )
  const board = reviewSessionBoardModel(
    activeBranch,
    branches,
    session,
    shownLine,
    momentMarkers,
    viewedPly,
  )
  return (
    <ReviewSessionView
      onAccountSettings={onAccountSettings}
      signOut={signOut}
      meta={<ReviewGameHeaderInfo importedGame={board.importedGame} />}
      gameInfo={
        <ReviewBranchControls
          branch={board.exploredResult}
          exploredBranches={exploredBranchLabels(
            exploredBranchResults(branches),
            session.core.positionSnapshot.fen,
          )}
          interactionDisabled={browsing || navigationBusy || navigationDisabled}
          onSelectBranch={onSelectBranch}
        />
      }
      evaluationGraph={reviewSessionEvaluationGraph({
        activePly: session.criticalPly,
        disabled: navigationDisabled,
        evaluationPoints,
        maxPly: boardMaxPly(
          session.core.importedGame.game.moves,
          evaluationPoints,
          session.criticalPly,
          viewedPly,
        ),
        momentMarkers,
        onSelect: onSelectMoment,
      })}
      board={
        <ReviewSessionBoardPane
          activeBranch={activeBranch}
          alternativeBusy={alternativeBusy}
          branches={branches}
          browsing={browsing}
          cancellableAlternative={cancellableAlternative}
          criticalMoment={criticalMoment}
          destinations={destinations}
          evaluation={evaluation}
          evaluationPoints={evaluationPoints}
          momentMarkers={momentMarkers}
          momentWorkspaces={momentWorkspaces}
          navigationBusy={navigationBusy}
          navigationDisabled={navigationDisabled}
          onCancel={onCancel}
          onClearBoardSelection={onClearBoardSelection}
          onNavigate={onNavigate}
          onPromote={onPromote}
          onSelectSquare={onSelectSquare}
          pendingPromotion={pendingPromotion}
          position={position}
          selectedSquare={selectedSquare}
          session={session}
          shownLine={shownLine}
          viewedPly={viewedPly}
        />
      }
      moveControls={
        <ReviewMoveControls
          alternativeBusy={alternativeBusy}
          branch={board.exploredResult}
          maxPly={boardMaxPly(
            board.importedGame.game.moves,
            evaluationPoints,
            session.criticalPly,
            viewedPly,
          )}
          momentMarkers={momentMarkers}
          moves={board.importedGame.game.moves}
          navigationDisabled={navigationDisabled}
          onCancel={
            cancellableAlternative
              ? () =>
                  void onCancel(session.gameImportId, cancellableAlternative)
              : undefined
          }
          onExitBranch={() => {
            momentWorkspaces.update(session, (current) => ({
              ...current,
              activeBranchId: null,
              branches: [],
              shownLine: null,
            }))
            onClearBoardSelection()
          }}
          onNavigate={onSelectMoment}
          onStrongestReply={onExploreMove}
          strongestReplyLabel={board.strongestReplyLabel}
          viewedPly={viewedPly}
        />
      }
      conversationKey={`${session.gameImportId}:${session.core.reviewMoment.momentId}:${session.criticalPly}`}
      conversation={{
        browsingNote:
          browsing && !momentMarkers.some((marker) => marker.ply === viewedPly)
            ? "Discuss this position?"
            : undefined,
        comment: session.comment,
        commentPublished: session.commentPublished,
        composer,
        composerLocked: composerBusy,
        failure,
        firstOpenStartedAt: session.firstOpenStartedAt,
        learningPathFeedback: learningPathFeedback.feedback,
        learningPathFeedbackFailures: learningPathFeedback.failures,
        learningPathFeedbackPending: learningPathFeedback.pending,
        learningPathFeedbackVotePending: learningPathFeedback.votePending,
        learningPaths,
        messages,
        onAuthoringDeadline: onSettleUnpublished,
        onCancel: cancellableHostTurn
          ? () => void onCancel(session.gameImportId, cancellableHostTurn)
          : undefined,
        onLearningPathVote: (learningPathRef, vote) =>
          void learningPathFeedback.updateVote(learningPathRef, vote),
        onDiscussPosition:
          !activeBranch && onNominate ? () => onNominate(viewedPly) : undefined,
        onMessage: onSendMessage,
        openingText: session.openingText,
        pendingLabel,
        reviewFeedback: {
          ...reviewFeedback.stateFor(reviewFeedbackTarget),
          onVote: (vote) =>
            void reviewFeedback.submit(reviewFeedbackTarget, vote),
        },
        safeRendering: session.safeRendering,
      }}
      eyebrow="Game review"
      title="Game review"
      failure={failure}
      momentMarkers={momentMarkers}
      momentNavigationDisabled={navigationDisabled}
      onSelectMoment={onSelectMoment}
      sessionPly={session.criticalPly}
      viewedPly={viewedPly}
    />
  )
}

function workspaceComposer(
  hostTurnLabel: string | undefined,
  navigationLabel: string | null,
) {
  if (isHostTurnStepLabel(hostTurnLabel)) {
    return {
      composer: {
        kind: "hostTurn",
        draft: "",
        progress: { label: hostTurnLabel },
      } satisfies ComposerState,
      pendingLabel: null,
    }
  }
  return {
    composer: { kind: "idle", draft: "" } satisfies ComposerState,
    pendingLabel: navigationLabel,
  }
}

function isHostTurnStepLabel(
  label: string | undefined,
): label is HostTurnStepDisplayLabel {
  return (
    label === hostTurnStepLabels.lookingAtAnotherMoment ||
    label === hostTurnStepLabels.checkingThatLine ||
    label === hostTurnStepLabels.writing
  )
}

function ReviewSessionBoardPane({
  activeBranch,
  alternativeBusy,
  branches,
  browsing,
  cancellableAlternative,
  criticalMoment,
  destinations,
  evaluation,
  evaluationPoints,
  momentMarkers,
  momentWorkspaces,
  navigationBusy,
  navigationDisabled,
  onCancel,
  onClearBoardSelection,
  onNavigate,
  onPromote,
  onSelectSquare,
  pendingPromotion,
  position,
  selectedSquare,
  session,
  shownLine,
  viewedPly,
}: {
  activeBranch: BranchView | null
  alternativeBusy: boolean
  branches: readonly BranchView[]
  browsing: boolean
  cancellableAlternative: CancellableOperation | null
  criticalMoment: GameReviewCriticalMoment | null
  destinations: Square[]
  evaluation: EngineEvaluation | null
  evaluationPoints: EvaluationPoint[]
  momentMarkers: readonly ReviewMomentMarker[]
  momentWorkspaces: ReturnType<typeof useMomentWorkspaces>
  navigationBusy: boolean
  navigationDisabled: boolean
  onCancel: (
    gameImportId: GameImportId,
    operation: CancellableOperation,
  ) => Promise<void>
  onClearBoardSelection: () => void
  onNavigate: (ply: number) => void
  onPromote: (role: PromotionRole) => void
  onSelectSquare: (square: Square) => void
  pendingPromotion: PendingPromotion | null
  position: PositionSnapshot | BrowseBoardPosition
  selectedSquare: Square | null
  session: ActiveSession
  shownLine: HostTurnShowLine | null
  viewedPly: number
}) {
  const board = reviewSessionBoardModel(
    activeBranch,
    branches,
    session,
    shownLine,
    momentMarkers,
    viewedPly,
  )
  // The engine/Maia comparison belongs to the Critical Moment position
  // itself — walking the game, a branch, or a shown line leaves it off.
  const comparisonArrows =
    criticalMoment &&
    !activeBranch &&
    !shownLine &&
    viewedPly === session.criticalPly
      ? comparisonBoardArrows(
          criticalMoment,
          session.core.importedGame.eloProfile.rating,
        )
      : undefined
  // An explored branch draws the engine's best move from its position, so
  // entering a line immediately shows what the Player is up against.
  const branchReplyArrows =
    !shownLine && board.exploredResult?.strongestReply.kind === "offered"
      ? engineReplyArrow(board.exploredResult.strongestReply.uci)
      : undefined
  return (
    <BoardWorkspace
      alternativeBusy={alternativeBusy}
      arrows={comparisonArrows ?? branchReplyArrows}
      branch={board.exploredResult}
      criticalPly={session.criticalPly}
      destinations={destinations}
      evaluation={evaluation}
      evaluationPoints={evaluationPoints}
      heading={board.heading}
      importedGame={board.importedGame}
      interactionDisabled={browsing || navigationBusy || navigationDisabled}
      momentMarkers={momentMarkers}
      navigationDisabled={navigationDisabled}
      orientation={reviewSideOrientation(board.importedGame.reviewSide)}
      onCancel={
        cancellableAlternative
          ? () => void onCancel(session.gameImportId, cancellableAlternative)
          : undefined
      }
      onExitBranch={() => {
        momentWorkspaces.update(session, (current) => ({
          ...current,
          activeBranchId: null,
          branches: [],
          shownLine: null,
        }))
        onClearBoardSelection()
      }}
      onNavigate={onNavigate}
      onPromote={onPromote}
      onSquare={onSelectSquare}
      position={position}
      promotion={pendingPromotion}
      selectedSquare={selectedSquare}
      shownLineLabel={board.shownLineLabelText}
      shownLineMove={board.shownLineMove}
      showMoveControls={false}
      showPositionCaption
      viewedPly={viewedPly}
    />
  )
}

function branchViewFromInspection(
  alternativeMoveId: AlternativeMoveId,
  inspection: PositionInspection,
) {
  const target = inspection.context.target
  if (target.kind !== "alternativeMove") return null
  return {
    kind: "inspected" as const,
    alternativeMoveId,
    branchRef: target.branchRef,
    moveUci: target.uci,
    inspection,
  }
}

function sessionWithInspection(
  session: ActiveSession,
  inspection: PositionInspection,
): ActiveSession {
  return {
    ...session,
    core: {
      ...session.core,
      evidencePacket: inspection.evidencePacket,
      positionSnapshot: inspection.positionSnapshot,
    },
  }
}

type PreparedReviewMoment = {
  core: ReviewSessionCoreContract
  learningMaterial: ReviewSessionMoment["learningMaterial"]
  classificationKind: ReviewSessionMoment["classificationKind"]
}

function preparedMoments(
  moments: readonly ReviewSessionMoment[],
): PreparedReviewMoment[] {
  return moments.flatMap((moment) =>
    moment.authoring.kind === "prepared"
      ? [
          {
            core: moment.authoring.core,
            learningMaterial: moment.learningMaterial,
            classificationKind: moment.classificationKind,
          },
        ]
      : [],
  )
}

function exploredBranchLabels(
  branches: readonly AlternativeMoveResult[],
  reviewMomentFen: string,
): ExploredBranchLabel[] {
  return branches.map((candidate) => ({
    alternativeMoveId: candidate.alternativeMoveId,
    label: playerVisibleAlternativeMove(candidate, branches, reviewMomentFen),
    selectedMove: candidate.evaluation.selectedMove,
  }))
}

function playerFacingBoardHeading(spec: {
  branch: AlternativeMoveResult | null
  branches: readonly AlternativeMoveResult[]
  inspectedMoveUci: string | null
  currentMove: CanonicalGameMove | undefined
  frozenMoveLabel: string | undefined
  reviewMomentFen: string
}): PlayerVisibleSan {
  if (spec.branch) {
    return playerVisibleAlternativeMove(
      spec.branch,
      spec.branches,
      spec.reviewMomentFen,
    )
  }
  if (spec.inspectedMoveUci) {
    return playerVisibleSanFromLegalUci(
      spec.reviewMomentFen,
      spec.inspectedMoveUci,
    )
  }
  if (spec.currentMove) return moveLabel(spec.currentMove)
  if (spec.frozenMoveLabel) return playerVisibleSanLiteral(spec.frozenMoveLabel)
  return PLAYER_VISIBLE_MOVE_FALLBACK
}
