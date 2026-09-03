import type {
  DailyCoachingProvider,
  ImportedGameProvider,
} from "@chenchess/coach-engine-sdk"
import type {
  DigestCardIdea,
  DigestCardProps,
  DigestCardResource,
} from "@chenchess/ui"

type DigestCardSource = {
  coverageDate: string
  gameCount: number
  priorities: readonly DigestCardIdeaSource[]
  publishedAt: string
  timezone: string
}

type DigestCardIdeaSource = {
  purpose: "improvement" | "reinforcement"
  resources: readonly DigestCardResourceSource[]
  supportingGameCount: number
  title: string
}

type DigestCardResourceSource = {
  canonicalUrl?: string
  kind?:
    | "openingPuzzleStream"
    | "openingReference"
    | "practiceModule"
    | "puzzleStream"
  role: "learn" | "drill"
  title: string
}

export function countLabel(count: number, singular: string): string {
  return `${count} ${singular}${count === 1 ? "" : "s"}`
}

export function formatDate(value: string): string {
  return new Intl.DateTimeFormat(undefined, {
    day: "numeric",
    month: "short",
    timeZone: "UTC",
    weekday: "long",
    year: "numeric",
  }).format(new Date(`${value}T00:00:00Z`))
}

/** Local clock when Daily Coaching publishes. Matches Coach Engine's default
 * two-hour grace after local midnight. */
export const DIGEST_LOCAL_CLOCK = "02:00"

export function playingProfileCaption({
  enabled,
  status,
}: {
  enabled: boolean
  status: "connected" | "profileUnavailable"
}): string {
  if (!enabled) return "Daily Coaching off"
  if (status === "profileUnavailable") return "Profile unavailable"
  return `digest on ${DIGEST_LOCAL_CLOCK}`
}

export function formatPublishedAt(value: string, timezone: string): string {
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
    timeZone: timezone,
  }).format(new Date(value))
}

export function providerLabel(provider: DailyCoachingProvider): string {
  return provider === "lichess" ? "Lichess" : "Chess.com"
}

export function reviewedGameProviderLabel(
  provider: ImportedGameProvider,
): string {
  switch (provider) {
    case "chessCom":
      return "Chess.com"
    case "lichess":
      return "Lichess"
    case "pastedPgn":
      return "PGN"
    default: {
      const _exhaustive: never = provider
      return _exhaustive
    }
  }
}

export function providerPlaceholder(provider: DailyCoachingProvider): string {
  return provider === "lichess"
    ? "https://lichess.org/@/username"
    : "https://chess.com/member/username"
}

export function wordsFromCamelCase(value: string): string {
  const words = value.replace(/([a-z])([A-Z])/g, "$1 $2")
  return `${words.charAt(0).toUpperCase()}${words.slice(1)}`
}

export function presentDigestCard(
  digest: DigestCardSource,
): Pick<DigestCardProps, "gameCount" | "ideas" | "source" | "title"> {
  return {
    gameCount: digest.gameCount,
    ideas: digest.priorities.slice(0, 2).map(presentDigestIdea),
    source: `Published ${formatPublishedAt(digest.publishedAt, digest.timezone)}`,
    title: formatDate(digest.coverageDate),
  }
}

export function presentArchivedDigestCard(archived: {
  coverageDate: string
  gameCount: number
  learningPathCount: number
}): Pick<DigestCardProps, "eyebrow" | "title"> {
  return {
    eyebrow: formatDate(archived.coverageDate),
    title: `${countLabel(archived.gameCount, "game")} · ${countLabel(archived.learningPathCount, "path")}`,
  }
}

function presentDigestIdea(priority: DigestCardIdeaSource): DigestCardIdea {
  return {
    cluster: digestIdeaCluster(priority.resources),
    purpose: priority.purpose,
    resources: priority.resources
      .map(presentDigestResource)
      .filter((resource): resource is DigestCardResource => resource != null),
    title: priority.title,
  }
}

function digestIdeaCluster(
  resources: readonly DigestCardResourceSource[],
): NonNullable<DigestCardIdea["cluster"]> {
  for (const resource of resources) {
    switch (resource.kind) {
      case "openingPuzzleStream":
      case "openingReference":
        return "Opening Tactical Awareness"
      case "practiceModule":
      case "puzzleStream":
      case undefined:
        break
      default: {
        const _exhaustive: never = resource.kind
        return _exhaustive
      }
    }
  }
  return "Lichess Curriculum"
}

function presentDigestResource(
  resource: DigestCardResourceSource,
): DigestCardResource | undefined {
  if (!resource.canonicalUrl) return undefined
  return {
    href: resource.canonicalUrl,
    label: resource.title,
    role: resource.role,
  }
}
