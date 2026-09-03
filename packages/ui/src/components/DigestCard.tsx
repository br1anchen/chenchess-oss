import * as stylex from "@stylexjs/stylex"
import type { MouseEvent, ReactNode } from "react"

import { LearningPathCards } from "../review/LearningPathCards"
import type { LearningPathPresentation } from "../review/learningPathProjection"
import { digestStyles } from "./DigestCard.styles"
import { WatercolorBadge, WatercolorCard } from "./watercolor"

/** Keeps a structural class hook alongside the compiled StyleX classes. */
function craft(
  hook: string,
  ...styles: ReadonlyArray<object | false | null | undefined>
) {
  // SAFETY: every argument is compiled StyleX from DigestCard.styles.ts; the
  // published prop types cannot express the authored style objects.
  const applied = stylex.props(...(styles as never[]))
  return {
    ...applied,
    className: [hook, applied.className].filter(Boolean).join(" "),
  }
}

export type DigestCardResource = {
  href: string
  label: string
  role: "drill" | "learn"
}

export type DigestCardIdea = {
  cluster?: "Lichess Curriculum" | "Opening Tactical Awareness"
  purpose: "improvement" | "reinforcement"
  resources?: readonly DigestCardResource[]
  title: string
}

export type DigestCardAppearance = "detail" | "featured" | "list"

export type DigestCardProps = {
  appearance?: DigestCardAppearance
  /** Extra craft for the coverage title, for hosts with a narrower column
   * than the dashboard's featured slot. */
  titleXstyle?: object
  children?: ReactNode
  className?: string
  eyebrow?: string
  gameCount?: number
  /** The coach's read of the coverage the card names, and the homework that
   * closes it. Composed by the layer that knows what the digest covers —
   * `digestCoachVoice` builds both — because only that layer can say
   * "yesterday" without lying about an archived digest. */
  summary?: string
  homework?: string
  href?: string
  ideas?: readonly DigestCardIdea[]
  onSelect?: () => void
  seal?: boolean
  selected?: boolean
  source?: string
  title: string
}

const MAX_IDEAS = 2

export function DigestCard({
  appearance = "featured",
  children,
  className,
  eyebrow,
  gameCount,
  homework,
  href,
  ideas = [],
  onSelect,
  seal = false,
  selected = false,
  source,
  summary,
  title,
  titleXstyle,
}: DigestCardProps) {
  const rows = ideas.slice(0, MAX_IDEAS)
  const interactive = appearance === "list" && Boolean(href || onSelect)
  return (
    <WatercolorCard
      aria-current={selected ? "true" : undefined}
      className={[
        "chen-digest-card",
        digestAppearanceClass(appearance),
        interactive ? "chen-digest-card-interactive" : undefined,
        className,
      ]
        .filter(Boolean)
        .join(" ")}
      data-digest-appearance={appearance}
      data-watercolor-surface="digest"
      eyebrow={eyebrow}
      headingLevel={appearance === "list" ? 3 : 1}
      meta={digestMeta({ appearance, gameCount, source })}
      padding={appearance === "list" ? "compact" : "comfortable"}
      seal={seal}
      title={title}
      titleXstyle={[
        digestStyles.title,
        appearance === "list"
          ? digestStyles.titleSmall
          : digestStyles.titleLarge,
        titleXstyle,
      ]}
      tone="paper"
      xstyle={selected ? digestStyles.selected : undefined}
    >
      {interactive ? digestHit({ eyebrow, href, onSelect, title }) : null}
      {summary ? (
        <p {...craft("chen-digest-card__summary", digestStyles.summary)}>
          {summary}
        </p>
      ) : null}
      {appearance === "list" || rows.length === 0 ? null : (
        <section
          aria-labelledby="chen-digest-priorities"
          {...craft("chen-digest-card__priorities", digestStyles.priorities)}
        >
          <h2
            id="chen-digest-priorities"
            {...craft(
              "chen-digest-card__priorities-title",
              digestStyles.prioritiesTitle,
            )}
          >
            Today’s priorities
          </h2>
          <LearningPathCards
            ariaLabel="Today’s priorities"
            frame={false}
            paths={priorityLearningPaths(rows)}
            tone="bamboo"
          />
          {homework ? (
            <p {...craft("chen-digest-card__homework", digestStyles.homework)}>
              {homework}
            </p>
          ) : null}
        </section>
      )}
      {children ? (
        <div {...craft("chen-digest-card__games", digestStyles.games)}>
          {children}
        </div>
      ) : null}
    </WatercolorCard>
  )
}

function digestMeta({
  appearance,
  gameCount,
  source,
}: Pick<DigestCardProps, "appearance" | "gameCount" | "source">): ReactNode {
  const showCount = appearance !== "list" && gameCount != null
  if (!source && !showCount) return undefined
  return (
    <>
      {source ? (
        <span {...craft("chen-digest-card__source", digestStyles.source)}>
          {source}
        </span>
      ) : null}
      {showCount && gameCount != null ? (
        <WatercolorBadge tone="info">
          {gameCountLabel(gameCount)}
        </WatercolorBadge>
      ) : null}
    </>
  )
}

function gameCountLabel(gameCount: number): string {
  return `${gameCount} ${gameCount === 1 ? "game" : "games"}`
}

function digestAppearanceClass(appearance: DigestCardAppearance): string {
  switch (appearance) {
    case "detail":
      return "chen-digest-card--detail"
    case "featured":
      return "chen-digest-card--featured"
    case "list":
      return "chen-digest-card--list"
    default: {
      const _exhaustive: never = appearance
      return _exhaustive
    }
  }
}

function digestHit({
  eyebrow,
  href,
  onSelect,
  title,
}: Pick<
  DigestCardProps,
  "eyebrow" | "href" | "onSelect" | "title"
>): ReactNode {
  const name = eyebrow ? `${eyebrow} ${title}` : title
  if (href) {
    return (
      <a
        aria-label={name}
        href={href}
        onClick={onSelect ? digestHitClick(onSelect) : undefined}
        {...craft("chen-digest-card__hit", digestStyles.hit)}
      />
    )
  }
  if (onSelect) {
    return (
      <button
        aria-label={name}
        onClick={onSelect}
        type="button"
        {...craft("chen-digest-card__hit", digestStyles.hit)}
      />
    )
  }
  return null
}

function digestHitClick(onSelect: () => void) {
  return (event: MouseEvent<HTMLAnchorElement>) => {
    if (
      event.defaultPrevented ||
      event.button !== 0 ||
      event.metaKey ||
      event.altKey ||
      event.ctrlKey ||
      event.shiftKey
    ) {
      return
    }
    onSelect()
  }
}

function priorityLearningPaths(
  ideas: readonly DigestCardIdea[],
): LearningPathPresentation[] {
  return ideas.map((idea, index) => ({
    cluster: idea.cluster ?? "Lichess Curriculum",
    conceptLessons: ideaResources(idea, "learn"),
    idea: idea.title,
    id: `${idea.purpose}:${idea.title}:${index}`,
    learningPathRef: `${idea.purpose}:${idea.title}:${index}`,
    patternDrills: ideaResources(idea, "drill"),
    purpose: idea.purpose === "improvement" ? "missing" : "reinforced",
  }))
}

function ideaResources(idea: DigestCardIdea, role: DigestCardResource["role"]) {
  return (idea.resources ?? [])
    .filter((resource) => resource.role === role)
    .map((resource, resourceIndex) => ({
      canonicalUrl: resource.href,
      resourceId: `${idea.title}:${role}:${resourceIndex}`,
      role,
      title: resource.label,
    }))
}
