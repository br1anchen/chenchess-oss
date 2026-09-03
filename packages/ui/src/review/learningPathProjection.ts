/**
 * The pure Learning Path projection, split from `LearningPathCards.tsx` so
 * data pipelines (the landing fixture generator runs under plain `bun`, no
 * StyleX compiler) can project without importing component code.
 */

export type LearningPathVotePresentation = "thumbsDown" | "thumbsUp"

export type LearningPathResourcePresentation = {
  canonicalUrl: string
  resourceId: string
  role: "drill" | "learn"
  title: string
}

type LearningTrackPresentation<
  PathRef extends string,
  Resource extends LearningPathResourcePresentation,
> = {
  key:
    | { concept: string; kind: "curriculum" }
    | { kind: "opening"; resourceMappingId: string }
  resources: readonly Resource[]
  support: readonly {
    criticalMomentId: string
    learningPathRef: PathRef
    purpose: "improvement" | "reinforcement"
  }[]
}

export type LearningPathPresentation<
  PathRef extends string = string,
  Resource extends LearningPathResourcePresentation =
    LearningPathResourcePresentation,
> = {
  cluster: "Opening Tactical Awareness" | "Lichess Curriculum"
  conceptLessons: readonly Resource[]
  idea: string
  id: string
  learningPathRef: PathRef
  patternDrills: readonly Resource[]
  purpose: "missing" | "reinforced"
}

export function projectLearningPaths<
  PathRef extends string,
  Resource extends LearningPathResourcePresentation,
>(
  material: {
    tracks: readonly LearningTrackPresentation<PathRef, Resource>[]
  },
  criticalMomentId: string,
): LearningPathPresentation<PathRef, Resource>[] {
  return material.tracks.map((track) => {
    const localSupport = track.support.filter(
      (support) => support.criticalMomentId === criticalMomentId,
    )
    if (localSupport.length !== 1) {
      throw new Error("Learning Paths require exactly one local support")
    }
    return {
      ...learningIdea(track),
      conceptLessons: deduplicatedResources(track, "learn"),
      id: learningTrackId(track),
      learningPathRef: localSupport[0]!.learningPathRef,
      patternDrills: deduplicatedResources(track, "drill"),
      purpose:
        localSupport[0]!.purpose === "improvement" ? "missing" : "reinforced",
    }
  })
}

function learningIdea<
  PathRef extends string,
  Resource extends LearningPathResourcePresentation,
>(
  track: LearningTrackPresentation<PathRef, Resource>,
): Pick<LearningPathPresentation, "cluster" | "idea"> {
  if (track.key.kind === "curriculum") {
    const reference =
      track.resources.find(({ role }) => role === "learn") ?? track.resources[0]
    return {
      cluster: "Lichess Curriculum",
      idea: reference?.title ?? track.key.concept,
    }
  }
  const reference = track.resources.find(({ role }) => role === "learn")
  return {
    cluster: "Opening Tactical Awareness",
    idea: reference?.title ?? "Opening idea",
  }
}

function deduplicatedResources<
  PathRef extends string,
  Resource extends LearningPathResourcePresentation,
>(
  track: LearningTrackPresentation<PathRef, Resource>,
  role: LearningPathResourcePresentation["role"],
) {
  const resources = new Map<string, Resource>()
  for (const resource of track.resources) {
    if (resource.role === role) resources.set(resource.resourceId, resource)
  }
  return [...resources.values()]
}

function learningTrackId<
  PathRef extends string,
  Resource extends LearningPathResourcePresentation,
>(track: LearningTrackPresentation<PathRef, Resource>) {
  if (track.key.kind === "curriculum") return `curriculum:${track.key.concept}`
  return `opening:${track.key.resourceMappingId}`
}
