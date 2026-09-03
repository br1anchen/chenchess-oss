/**
 * One Player move on the game Coaching Board, evaluated on its own.
 *
 * The board used to send the Player's drag through `evaluate_player_line`,
 * which is built for an agent proposing a line it has never walked: it
 * re-establishes the moment root and re-submits every ply of the line on every
 * call. That is right for that caller and wrong for this one — a drag cost
 * `k + 4` sequential round trips at the k-th move, and the already-walked
 * plies only deduplicated once they reached the engine.
 *
 * The Player's drag is a different operation. The board already holds the
 * branch it is standing on, so the new move needs one command parented to that
 * branch. `evaluate_player_line` keeps its contract exactly as signed; this
 * module is only the Player's own path.
 *
 * The idempotency key is deliberately still minted over the whole move path
 * from the moment root, so a move reached incrementally and the same move
 * reached by an agent re-walking the line remain one logical write on the
 * engine.
 */

import {
  fromCriticalMomentId,
  fromGameImportId,
  mintIdempotencyKey,
  mintOperationId,
  type AlternativeMoveResult,
  type BranchParent,
  type CriticalMomentId,
  type IdempotencyKey,
  type MoveInput,
  type OperationId,
  type PositionRef,
  type ReviewSessionEventEnvelope,
} from "@chenchess/coach-engine-sdk"

import { openAddressedReviewMomentCommand } from "../../server/board/review-moment-command"
import type {
  PlayerLineCommandExecute,
  PlayerLineCommandOutcome,
  PlayerLineKeyMint,
} from "../../server/board/player-line-evaluate"

import type { CoachingBoardExplorationBranch } from "./coachingBoardSnapshot"

type GameExplorationRoot = {
  positionRef: PositionRef
  reviewMomentId: CriticalMomentId
}

/** Where a move is played from: the branch it extends, and the engine's
 * reference for the position it is played in. */
type GameExplorationSource = {
  parent: BranchParent
  positionRef: PositionRef
}

/** The engine identity of one evaluation: what deduplicates a replay, and
 * what names the operation to cancel. */
export type GameExplorationIdentity = {
  idempotencyKey: IdempotencyKey
  operationId: OperationId
}

/**
 * The identity for one move on a path from the moment root.
 *
 * Minted from the whole path so a move reached incrementally and the same move
 * reached by an agent re-walking the line remain one logical write, and minted
 * in exactly one place so the command the board sends and the operation it can
 * later cancel can never drift into naming different operations.
 */
export function explorationIdentity({
  gameImportId,
  keys,
  movePath,
  reviewMomentId,
}: {
  gameImportId: string
  keys: PlayerLineKeyMint
  movePath: readonly MoveInput[]
  reviewMomentId: CriticalMomentId
}): GameExplorationIdentity {
  const values = [gameImportId, fromCriticalMomentId(reviewMomentId), movePath]
  return {
    idempotencyKey: keys.idempotency("move-path", values),
    operationId: keys.options("explore", values).operationId,
  }
}

/**
 * Where the Player's next move is played from.
 *
 * A branch the board is standing on parents the new move directly; with no
 * branch the move hangs off the moment root.
 */
export function explorationSource(
  root: GameExplorationRoot,
  activeBranch: CoachingBoardExplorationBranch | null,
): GameExplorationSource {
  if (!activeBranch) {
    return {
      parent: { kind: "root", positionRef: root.positionRef },
      positionRef: root.positionRef,
    }
  }
  return {
    parent: { kind: "move", branchRef: activeBranch.branchRef },
    positionRef: activeBranch.resultingPosition.positionRef,
  }
}

/**
 * A step that reached the engine, or the engine outcome that stopped it.
 *
 * The refusal carries the Coach Engine outcome verbatim rather than a second
 * vocabulary restating it. There is one place that turns an outcome into
 * Player prose (`gameExplorationRefusalNotice`) and one that decides whether
 * it invalidates the open session, so a new outcome kind reaches both by
 * failing to compile rather than by silently mapping onto a catch-all.
 */
type GameExplorationStep<T> =
  | { kind: "ready"; value: T }
  | { kind: "refused"; outcome: PlayerLineCommandOutcome }

/**
 * Whether a refusal means this page can no longer trust the moment root it
 * holds.
 *
 * A domain answer — illegal move, spent allowance, engine deadline — leaves
 * the session exactly as it was. A command that never returned an answer, or
 * returned one for an operation nobody asked for, leaves this process unsure
 * the session still exists, so the next move re-opens the moment rather than
 * parenting onto one only this page believes in.
 */
export function refusalInvalidatesSession(outcome: PlayerLineCommandOutcome) {
  switch (outcome.kind) {
    case "deadlineReached":
    case "explorationExhausted":
    case "illegalMove":
      return false
    case "completed":
    case "failed":
    case "idempotencyKeyMismatch":
      return true
    default: {
      const _exhaustive: never = outcome
      return _exhaustive
    }
  }
}

/**
 * The moment roots this board has opened, and whether it has a session.
 *
 * Reading a root is the only part of exploring that genuinely needs the engine
 * more than once per position: it resolves which Review Moment a ply belongs
 * to and the `PositionRef` the first branch hangs from. Held here so the
 * Player pays it on the first move at a position and never again while the
 * board stays open, and so the caching rule is testable without a component.
 */
export function explorationRoots() {
  const byPly = new Map<number, GameExplorationRoot>()
  let sessionStarted = false

  return {
    /** A session the engine no longer holds invalidates every root read from
     * it, so the next move re-opens rather than parenting onto a moment this
     * process is alone in believing exists. */
    forget() {
      byPly.clear()
      sessionStarted = false
    },

    async ensure(input: {
      execute: PlayerLineCommandExecute
      gameImportId: string
      keys: PlayerLineKeyMint
      ply: number
    }): Promise<GameExplorationStep<GameExplorationRoot>> {
      const held = byPly.get(input.ply)
      if (held) return { kind: "ready", value: held }
      const prepared = await openExplorationRoot({ ...input, sessionStarted })
      if (prepared.kind === "ready") {
        sessionStarted = true
        byPly.set(input.ply, prepared.value)
      }
      return prepared
    },
  }
}

/**
 * Open the moment the Player is exploring from, and read its root Position.
 *
 * `startReviewSession` is per Game rather than per moment, so it is skipped
 * once the board has one. Written as four plain steps rather than a generic
 * runner: narrowing each completion against a literal is what makes a renamed
 * completion kind fail to compile here.
 */
async function openExplorationRoot({
  execute,
  gameImportId,
  keys,
  ply,
  sessionStarted,
}: {
  execute: PlayerLineCommandExecute
  gameImportId: string
  keys: PlayerLineKeyMint
  ply: number
  sessionStarted: boolean
}): Promise<GameExplorationStep<GameExplorationRoot>> {
  const moment = { kind: "ply", ply } as const
  const resolved = await execute(
    openAddressedReviewMomentCommand(gameImportId, moment),
    keys.options("resolve", [gameImportId, moment]),
  )
  const opened = completionOf(resolved)
  if (opened?.kind !== "addressedReviewMomentOpened") {
    return { kind: "refused", outcome: resolved }
  }
  const { detail } = opened

  if (!sessionStarted) {
    const started = await execute(
      {
        gameImportId: fromGameImportId(gameImportId),
        kind: "startReviewSession",
      },
      keys.options("start", [gameImportId]),
    )
    if (completionOf(started)?.kind !== "reviewSessionStarted") {
      return { kind: "refused", outcome: started }
    }
  }

  const selection = {
    kind: "playerSelectedMoment" as const,
    ply: detail.ply,
  }
  const momentOpened = await execute(
    {
      gameImportId: fromGameImportId(gameImportId),
      idempotencyKey: keys.idempotency("moment-open", [
        gameImportId,
        selection,
      ]),
      kind: "openReviewMoment",
      selection,
    },
    keys.options("open", [gameImportId, selection]),
  )
  if (completionOf(momentOpened)?.kind !== "reviewMomentOpened") {
    return { kind: "refused", outcome: momentOpened }
  }

  const inspected = await execute(
    {
      gameImportId: fromGameImportId(gameImportId),
      kind: "inspectPosition",
      reviewMomentId: detail.reviewMomentId,
      target: { kind: "reviewedMove" },
    },
    keys.options("inspect", [gameImportId, detail.reviewMomentId]),
  )
  const inspection = completionOf(inspected)
  if (inspection?.kind !== "positionInspected") {
    return { kind: "refused", outcome: inspected }
  }

  return {
    kind: "ready",
    value: {
      positionRef: inspection.inspection.positionSnapshot.positionRef,
      reviewMomentId: detail.reviewMomentId,
    },
  }
}

/**
 * Evaluate one move from the branch the board is standing on.
 *
 * `movePath` is the whole path from the moment root, including this move, so
 * the engine's own deduplication is unchanged. `parent` is the branch the
 * Player is on, so no earlier ply is re-sent.
 */
export async function exploreGameMove({
  execute,
  gameImportId,
  identity,
  keys,
  move,
  observe,
  reviewMomentId,
  source,
}: {
  execute: PlayerLineCommandExecute
  gameImportId: string
  identity: GameExplorationIdentity
  keys: PlayerLineKeyMint
  move: MoveInput
  observe: (envelope: ReviewSessionEventEnvelope) => void
  reviewMomentId: CriticalMomentId
  source: GameExplorationSource
}): Promise<GameExplorationStep<AlternativeMoveResult>> {
  const command = {
    gameImportId: fromGameImportId(gameImportId),
    idempotencyKey: identity.idempotencyKey,
    kind: "exploreAlternativeMove" as const,
    moveInput: move,
    parent: source.parent,
    reviewMomentId,
    sourcePositionRef: source.positionRef,
  }
  let explored = await execute(command, {
    onEvent: observe,
    operationId: identity.operationId,
    signal: keys.signal,
  })

  if (explored.kind === "idempotencyKeyMismatch") {
    // Both identities are minted from the move path, so replaying a drag
    // reuses the engine's own answer. A move the Player cancelled — or one the
    // engine interrupted — leaves that key bound to an operation that never
    // settled, and only a fresh identity retries it. Without this, cancelling
    // a move makes replaying the same move permanently impossible.
    const retry = freshExplorationIdentity()
    explored = await execute(
      { ...command, idempotencyKey: retry.idempotencyKey },
      {
        onEvent: observe,
        operationId: retry.operationId,
        signal: keys.signal,
      },
    )
  }

  const completion = completionOf(explored)
  if (completion?.kind !== "alternativeMoveEvaluated") {
    return { kind: "refused", outcome: explored }
  }
  return { kind: "ready", value: completion.alternativeMove }
}

function freshExplorationIdentity() {
  const identity = crypto.randomUUID()
  return {
    idempotencyKey: mintIdempotencyKey("web", `explore-retry:${identity}`),
    operationId: mintOperationId("web", `explore-retry:${identity}`),
  }
}

/**
 * The completion a command produced, or nothing if it did not complete.
 *
 * Callers compare `kind` against a literal so TypeScript narrows the union at
 * the call site; a completion kind renamed in the contract then fails to
 * compile there rather than silently never matching.
 */
function completionOf(outcome: PlayerLineCommandOutcome) {
  return outcome.kind === "completed" ? outcome.completion : null
}
