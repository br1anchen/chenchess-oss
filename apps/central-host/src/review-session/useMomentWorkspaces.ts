import { useRef, useState } from "react"

import type {
  AlternativeMoveId,
  AlternativeMoveResult,
  BranchRef,
  CriticalMomentComment,
  GameReviewCriticalMoment,
  HostTurnShowLine,
  PositionInspection,
  ReviewMomentLearningMaterial,
  ReviewSessionCoreContract,
  GameImportId,
} from "@chenchess/coach-engine-sdk"

import type { WorkspaceThreadItem } from "./thread-state"

export type ActiveSession = {
  gameImportId: GameImportId
  core: ReviewSessionCoreContract
  criticalPly: number
  openingText: string | null
  comment: CriticalMomentComment | null
  commentPublished: boolean | null
  firstOpened: boolean
  firstOpenStartedAt: number | null
  safeRendering: string
  learningMaterial: ReviewMomentLearningMaterial
  nominatedMoment: GameReviewCriticalMoment | null
  nominatedClassification:
    | GameReviewCriticalMoment["classification"]["kind"]
    | null
  placeholder: boolean
}

export type BranchView =
  | {
      kind: "explored"
      result: AlternativeMoveResult
      inspection: PositionInspection
    }
  | {
      kind: "inspected"
      alternativeMoveId: AlternativeMoveId
      branchRef: BranchRef
      moveUci: string
      inspection: PositionInspection
    }

export function branchAlternativeMoveId(branch: BranchView): AlternativeMoveId {
  switch (branch.kind) {
    case "explored":
      return branch.result.alternativeMoveId
    case "inspected":
      return branch.alternativeMoveId
    default: {
      const _exhaustive: never = branch
      return _exhaustive
    }
  }
}

export function branchRefOf(branch: BranchView): BranchRef {
  switch (branch.kind) {
    case "explored":
      return branch.result.branchRef
    case "inspected":
      return branch.branchRef
    default: {
      const _exhaustive: never = branch
      return _exhaustive
    }
  }
}

export function exploredBranchResults(
  branches: readonly BranchView[],
): AlternativeMoveResult[] {
  return branches.flatMap((branch) =>
    branch.kind === "explored" ? [branch.result] : [],
  )
}

export type MomentWorkspace = {
  session: ActiveSession
  branches: BranchView[]
  activeBranchId: AlternativeMoveId | null
  shownLine: HostTurnShowLine | null
  messages: WorkspaceThreadItem[]
}

export function useMomentWorkspaces() {
  const [byPly, setByPlyState] = useState<ReadonlyMap<number, MomentWorkspace>>(
    () => new Map(),
  )
  const byPlyRef = useRef(byPly)
  const [activePly, setActivePly] = useState<number | null>(null)
  const active = activePly === null ? null : (byPly.get(activePly) ?? null)
  const workspaces: readonly MomentWorkspace[] = [...byPly.values()].sort(
    (left, right) => left.session.criticalPly - right.session.criticalPly,
  )

  function commit(next: ReadonlyMap<number, MomentWorkspace>) {
    byPlyRef.current = next
    setByPlyState(next)
  }

  function mutate(
    updater: (
      current: ReadonlyMap<number, MomentWorkspace>,
    ) => ReadonlyMap<number, MomentWorkspace>,
  ) {
    commit(updater(byPlyRef.current))
  }

  function activateAll(workspaces: readonly MomentWorkspace[]) {
    const first = workspaces[0]
    commit(
      new Map(
        workspaces.map((workspace) => [
          workspace.session.criticalPly,
          workspace,
        ]),
      ),
    )
    setActivePly(first?.session.criticalPly ?? null)
  }

  function open(ply: number): boolean {
    if (!byPlyRef.current.has(ply)) return false
    setActivePly(ply)
    return true
  }

  function get(ply: number): MomentWorkspace | undefined {
    return byPlyRef.current.get(ply)
  }

  function upsert(workspace: MomentWorkspace, activate = true) {
    const ply = workspace.session.criticalPly
    mutate((current) => {
      const existing = current.get(ply)
      const next = new Map(current)
      if (existing?.session.firstOpened) {
        next.set(ply, existing)
        return next
      }
      next.set(ply, {
        session: workspace.session,
        branches: existing?.branches ?? workspace.branches,
        activeBranchId: existing?.activeBranchId ?? workspace.activeBranchId,
        shownLine: existing?.shownLine ?? workspace.shownLine,
        messages: existing?.messages ?? workspace.messages,
      })
      return next
    })
    if (activate) setActivePly(ply)
  }

  function patch(
    ply: number,
    updater: (workspace: MomentWorkspace) => MomentWorkspace,
  ): MomentWorkspace | undefined {
    let nextWorkspace: MomentWorkspace | undefined
    mutate((current) => {
      const workspace = current.get(ply)
      if (!workspace) return current
      nextWorkspace = updater(workspace)
      const next = new Map(current)
      next.set(ply, nextWorkspace)
      return next
    })
    return nextWorkspace
  }

  function update(
    target: ActiveSession,
    updater: (workspace: MomentWorkspace) => MomentWorkspace,
  ) {
    mutate((current) => {
      const workspace = current.get(target.criticalPly)
      if (
        workspace?.session.gameImportId === target.gameImportId &&
        workspace.session.core.reviewMoment.momentId ===
          target.core.reviewMoment.momentId
      ) {
        const next = new Map(current)
        next.set(target.criticalPly, updater(workspace))
        return next
      }
      throw new Error("Moment updates require the retained Review Moment")
    })
  }

  function clear() {
    commit(new Map())
    setActivePly(null)
  }

  return {
    active,
    activateAll,
    clear,
    get,
    open,
    patch,
    upsert,
    update,
    workspaces,
  }
}
