import * as stylex from "@stylexjs/stylex"

import { Heading, HStack, Text } from "../astryx"
import { WatercolorButton, WatercolorCard } from "../components/watercolor"
import { Icon } from "../icons"
import { learningStyles } from "./LearningPathCards.styles"

/** Keeps a structural class hook alongside the compiled StyleX classes. */
function craft(
  hook: string,
  ...styles: ReadonlyArray<object | false | null | undefined>
) {
  // SAFETY: every argument is compiled StyleX from LearningPathCards.styles.ts;
  // the published prop types cannot express the authored style objects.
  const applied = stylex.props(...(styles as never[]))
  return {
    ...applied,
    className: [hook, applied.className].filter(Boolean).join(" "),
  }
}

import type {
  LearningPathPresentation,
  LearningPathResourcePresentation,
  LearningPathVotePresentation,
} from "./learningPathProjection"

export function LearningPathCards<
  PathRef extends string,
  Resource extends LearningPathResourcePresentation,
>({
  ariaLabel = "Learning plan for this moment",
  currentVote,
  density = "default",
  disabled,
  failure,
  frame = true,
  onVote,
  paths,
  pending,
  tone = "mist",
  xstyle,
}: {
  ariaLabel?: string
  /** `compact` is the Coach App widget, where the plan shares the card with
   * a board. */
  density?: "default" | "compact"
  /** `bamboo` is the digest reading: a green ink frame and splash bubbles
   * for the concept lesson and pattern-drilling links. */
  tone?: "bamboo" | "mist"
  currentVote?: (
    learningPathRef: PathRef,
  ) => LearningPathVotePresentation | null
  disabled?: (learningPathRef: PathRef) => boolean
  failure?: (learningPathRef: PathRef) => string | undefined
  /** Digest surfaces drop the ink frame because the parent card already
   * wears one. Coach App, landing, and Game Review keep it. */
  frame?: boolean
  onVote?: (
    learningPathRef: PathRef,
    vote: LearningPathVotePresentation | null,
  ) => void
  paths: readonly LearningPathPresentation<PathRef, Resource>[]
  /** Host sizing — how the surface seats the plan in its own layout. */
  xstyle?: object
  pending?: (learningPathRef: PathRef) => boolean
}) {
  return (
    <section
      aria-label={ariaLabel}
      {...craft("chen-learning-paths", learningStyles.plan, xstyle)}
    >
      {paths.map((path) => (
        <LearningPathCard
          currentVote={currentVote?.(path.learningPathRef) ?? null}
          density={density}
          tone={tone}
          disabled={
            disabled?.(path.learningPathRef) ??
            pending?.(path.learningPathRef) ??
            false
          }
          failure={failure?.(path.learningPathRef)}
          frame={frame}
          key={path.id}
          onVote={onVote}
          path={path}
          pending={pending?.(path.learningPathRef) ?? false}
        />
      ))}
    </section>
  )
}

function LearningPathCard<
  PathRef extends string,
  Resource extends LearningPathResourcePresentation,
>({
  currentVote,
  density,
  disabled,
  failure,
  frame,
  onVote,
  path,
  pending,
  tone,
}: {
  currentVote: LearningPathVotePresentation | null
  density: "default" | "compact"
  tone: "bamboo" | "mist"
  disabled: boolean
  frame: boolean
  failure: string | undefined
  onVote:
    | ((
        learningPathRef: PathRef,
        vote: LearningPathVotePresentation | null,
      ) => void)
    | undefined
  path: LearningPathPresentation<PathRef, Resource>
  pending: boolean
}) {
  return (
    <WatercolorCard
      className="chen-learning-path"
      frame={frame}
      padding="compact"
      tone={tone}
      xstyle={[
        learningStyles.card,
        density === "compact" && learningStyles.cardCompact,
      ]}
    >
      {/* Still the card's header landmark — `as` keeps the element while the
          layout comes from the stack. */}
      <HStack as="header" xstyle={learningStyles.header}>
        <Heading level={3} xstyle={learningStyles.idea}>
          <Text
            className="chen-learning-eyebrow"
            type="label"
            xstyle={learningStyles.eyebrow}
          >
            {path.purpose === "missing" ? "Missing idea" : "Idea reinforced"}
          </Text>
          {" : "}
          {path.idea}
        </Heading>
        {onVote ? (
          <LearningPathFeedback
            currentVote={currentVote}
            disabled={disabled}
            failure={failure}
            learningPathRef={path.learningPathRef}
            onVote={onVote}
            pending={pending}
          />
        ) : null}
      </HStack>
      <ul {...stylex.props(learningStyles.stages)}>
        <LearningStage
          label="Concept lesson"
          resources={path.conceptLessons}
          tone={tone}
        />
        <LearningStage
          label="Pattern drilling"
          resources={path.patternDrills}
          tone={tone}
        />
      </ul>
    </WatercolorCard>
  )
}

type LearningPathFeedbackWrite =
  | { kind: "failed"; message: string }
  | { kind: "idle" }
  | { kind: "inFlight" }
  | { kind: "recorded" }

function learningPathFeedbackWrite({
  currentVote,
  failure,
  pending,
}: {
  currentVote: LearningPathVotePresentation | null
  failure: string | undefined
  pending: boolean
}): LearningPathFeedbackWrite {
  if (pending) return { kind: "inFlight" }
  if (failure) return { kind: "failed", message: failure }
  if (currentVote) return { kind: "recorded" }
  return { kind: "idle" }
}

type LearningPathFeedbackPromptWrite = Exclude<
  LearningPathFeedbackWrite,
  { kind: "failed" }
>

type LearningPathFeedbackPrompt =
  | { role: "status"; text: string }
  | { text: string }

function learningPathFeedbackPromptWrite(
  write: LearningPathFeedbackWrite,
  currentVote: LearningPathVotePresentation | null,
): LearningPathFeedbackPromptWrite {
  if (write.kind !== "failed") return write
  return currentVote ? { kind: "recorded" } : { kind: "idle" }
}

function learningPathFeedbackPrompt(
  write: LearningPathFeedbackPromptWrite,
): LearningPathFeedbackPrompt {
  switch (write.kind) {
    case "idle":
      return { text: "Relevant?" }
    case "inFlight":
      return { role: "status", text: "Saving…" }
    case "recorded":
      return { role: "status", text: "Recorded" }
    default: {
      const exhaustive: never = write
      return exhaustive
    }
  }
}

function LearningPathFeedback<PathRef extends string>({
  currentVote,
  disabled,
  failure,
  learningPathRef,
  onVote,
  pending,
}: {
  currentVote: LearningPathVotePresentation | null
  disabled: boolean
  failure: string | undefined
  learningPathRef: PathRef
  onVote: (
    learningPathRef: PathRef,
    vote: LearningPathVotePresentation | null,
  ) => void
  pending: boolean
}) {
  const choose = (vote: LearningPathVotePresentation) => {
    onVote(learningPathRef, currentVote === vote ? null : vote)
  }
  const write = learningPathFeedbackWrite({ currentVote, failure, pending })
  const prompt = learningPathFeedbackPrompt(
    learningPathFeedbackPromptWrite(write, currentVote),
  )
  return (
    <>
      <HStack
        className="chen-learning-feedback"
        data-learning-feedback=""
        xstyle={learningStyles.feedback}
      >
        <Text
          as="p"
          display="block"
          role={"role" in prompt ? prompt.role : undefined}
          xstyle={learningStyles.feedbackPrompt}
        >
          {prompt.text}
        </Text>
        <HStack
          aria-label="Learning path relevance"
          role="group"
          xstyle={learningStyles.feedbackGroup}
        >
          <WatercolorButton
            aria-label="Relevant"
            aria-pressed={currentVote === "thumbsUp"}
            className="chen-learning-feedback-button"
            disabled={disabled}
            onClick={() => choose("thumbsUp")}
            size="icon"
            type="button"
            variant="quiet"
            xstyle={[
              learningStyles.feedbackButton,
              currentVote === "thumbsUp" &&
                learningStyles.feedbackButtonPressed,
            ]}
          >
            <Icon icon="thumbsUp" size="sm" />
          </WatercolorButton>
          <WatercolorButton
            aria-label="Not relevant"
            aria-pressed={currentVote === "thumbsDown"}
            className="chen-learning-feedback-button"
            disabled={disabled}
            onClick={() => choose("thumbsDown")}
            size="icon"
            type="button"
            variant="quiet"
            xstyle={[
              learningStyles.feedbackButton,
              currentVote === "thumbsDown" &&
                learningStyles.feedbackButtonPressed,
            ]}
          >
            <Icon icon="thumbsDown" size="sm" />
          </WatercolorButton>
        </HStack>
      </HStack>
      {write.kind === "failed" ? (
        <Text
          as="p"
          className="chen-learning-feedback-alert"
          display="block"
          role="alert"
          xstyle={learningStyles.feedbackAlert}
        >
          {write.message}
        </Text>
      ) : null}
    </>
  )
}

function LearningStage<Resource extends LearningPathResourcePresentation>({
  label,
  resources,
  tone,
}: {
  label: "Concept lesson" | "Pattern drilling"
  resources: readonly Resource[]
  tone: "bamboo" | "mist"
}) {
  if (resources.length === 0) return null
  const digestBubble = tone === "bamboo"
  return (
    <>
      {resources.map((resource) => (
        <li
          key={resource.resourceId}
          {...stylex.props(learningStyles.stageItem)}
        >
          <WatercolorCard
            className="chen-learning-stage"
            frame={digestBubble}
            padding="compact"
            splash={digestBubble}
            tone={digestBubble ? "bamboo" : "paper"}
            xstyle={learningStyles.stage}
          >
            <a
              aria-label={`${label}: ${resource.title}`}
              href={resource.canonicalUrl}
              rel="noreferrer"
              target="_blank"
              {...craft("chen-learning-stage-link", learningStyles.stageLink)}
            >
              <strong {...stylex.props(learningStyles.stageLabel)}>
                {label}
              </strong>
              <Text xstyle={learningStyles.stageTitle}>{resource.title}</Text>
            </a>
          </WatercolorCard>
        </li>
      ))}
    </>
  )
}
