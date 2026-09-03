import {
  useEffect,
  useRef,
  useState,
  type FocusEvent,
  type KeyboardEvent,
  type ReactNode,
} from "react"
import * as stylex from "@stylexjs/stylex"
import { Icon } from "@chenchess/ui/astryx"

import type {
  LearningPathFeedbackState,
  LearningPathRef,
  LearningPathVote,
} from "@chenchess/coach-engine-sdk"
import {
  Banner,
  ChatMessage,
  ChatMessageList,
  HStack,
  Text,
  VStack,
  WatercolorButton,
  WatercolorCard,
  WatercolorChatBubble,
  WatercolorChatComposer,
} from "@chenchess/ui"
import { brandAssets } from "@chenchess/ui/assets"
import { LearningPathCards } from "@chenchess/ui/review/learning-paths"

import { sharedLimits } from "@chenchess/shared-assets"
import { useCompactLayout } from "@/useCompactLayout"
import { reviewSessionShellStyles } from "./ReviewSessionShell.styles"
import type { MomentLearningPath } from "./reviewMoments"
import { recoveryMessage } from "./model"
import { hostTurnRefusalText, type WorkspaceThreadItem } from "./thread-state"
import { useAuthoringClock } from "./useAuthoringClock"
import { unavailableReasonMessage } from "./useReviewSessionCommands"
import type {
  ReviewFeedbackState,
  ReviewFeedbackVote,
} from "./useReviewFeedback"

const noPendingLearningPaths: ReadonlySet<LearningPathRef> = new Set()
const noLearningPathFeedback: Partial<
  Record<LearningPathRef, LearningPathFeedbackState>
> = {}
const noLearningPathFeedbackFailures: Partial<Record<LearningPathRef, string>> =
  {}

/** Matches LanguageLayerAdmissionConfig.comment_authoring_deadline. */
export const COMMENT_AUTHORING_DEADLINE_SECONDS =
  sharedLimits.commentAuthoringDeadlineSeconds

export type ConversationMessageAuthor = "player" | "coach" | "system"

export type ConversationPanelProps = {
  openingText: string | null
  comment?: { text: string } | null
  commentPublished?: boolean | null
  firstOpenStartedAt?: number | null
  safeRendering?: string
  messages: readonly WorkspaceThreadItem[]
  busyLabel: string | null
  inputDisabled: boolean
  failure: string | null
  learningPathsAriaLabel?: string
  learningPaths: readonly MomentLearningPath[]
  learningPathFeedback?: Partial<
    Record<LearningPathRef, LearningPathFeedbackState>
  >
  learningPathFeedbackFailures?: Partial<Record<LearningPathRef, string>>
  learningPathFeedbackPending?: ReadonlySet<LearningPathRef>
  learningPathFeedbackVotePending?: ReadonlySet<LearningPathRef>
  onMessage: (text: string) => void
  onLearningPathVote?: (
    learningPathRef: LearningPathRef,
    vote: LearningPathVote | null,
  ) => void
  reviewFeedback?: ReviewFeedbackControls
  onCancel?: () => void
  onAuthoringDeadline?: () => void
  /** Compact only: the floating button starts a discussion of the walked
   * position instead of opening the composer. The one button stays put; what
   * it starts follows where the board is. */
  onDiscussPosition?: () => void
  /** The board is walked to an unlisted position: this neutral standing
   * prompt replaces the opening commentary, so the previous Critical
   * Moment's coaching never speaks for a position it does not describe.
   * Sending from here nominates the position first. */
  browsingNote?: string
}

function conversationPanelProps(props: ConversationPanelProps) {
  return {
    commentPublished: props.commentPublished ?? null,
    firstOpenStartedAt: props.firstOpenStartedAt ?? null,
    learningPathsAriaLabel:
      props.learningPathsAriaLabel ?? "Learning plan for this moment",
    learningPathFeedback: props.learningPathFeedback ?? noLearningPathFeedback,
    learningPathFeedbackFailures:
      props.learningPathFeedbackFailures ?? noLearningPathFeedbackFailures,
    learningPathFeedbackPending:
      props.learningPathFeedbackPending ?? noPendingLearningPaths,
    learningPathFeedbackVotePending:
      props.learningPathFeedbackVotePending ?? noPendingLearningPaths,
  }
}

function conversationCommentModel(
  openingText: string | null,
  comment: { text: string } | null | undefined,
  commentPublished: boolean | null,
  firstOpenStartedAt: number | null,
  safeRendering: string | undefined,
) {
  const unpublishedFallback =
    comment?.text?.trim() ||
    safeRendering?.trim() ||
    (commentPublished === false ? (openingText ?? "") : "")
  const publishedText =
    commentPublished === true
      ? comment?.text.trim() || openingText?.trim() || ""
      : ""
  const unpublishedSettled =
    commentPublished === false && firstOpenStartedAt === null
      ? unpublishedFallback
      : ""
  return { unpublishedFallback, publishedText, unpublishedSettled }
}

function conversationWaiting(
  frozenBody: string | null,
  publishedText: string,
  unpublishedSettled: string,
  firstOpenStartedAt: number | null,
) {
  return (
    frozenBody === null &&
    publishedText.length === 0 &&
    unpublishedSettled.length === 0 &&
    firstOpenStartedAt !== null
  )
}

function conversationCommentText(
  frozenBody: string | null,
  waiting: boolean,
  openingText: string | null,
) {
  return frozenBody ?? (waiting ? "" : (openingText ?? ""))
}

export function ConversationPanel(props: ConversationPanelProps) {
  const {
    openingText,
    comment,
    safeRendering,
    messages,
    busyLabel,
    inputDisabled,
    failure,
    learningPaths,
    onMessage,
    onLearningPathVote,
    reviewFeedback,
    onCancel,
    onAuthoringDeadline,
    onDiscussPosition,
    browsingNote,
  } = props
  const {
    commentPublished,
    firstOpenStartedAt,
    learningPathsAriaLabel,
    learningPathFeedback,
    learningPathFeedbackFailures,
    learningPathFeedbackPending,
    learningPathFeedbackVotePending,
  } = conversationPanelProps(props)
  const [draft, setDraft] = useState("")
  const { unpublishedFallback, publishedText, unpublishedSettled } =
    conversationCommentModel(
      openingText,
      comment,
      commentPublished,
      firstOpenStartedAt,
      safeRendering,
    )
  const [frozenBody, setFrozenBody] = useState<string | null>(null)
  const waiting = conversationWaiting(
    frozenBody,
    publishedText,
    unpublishedSettled,
    firstOpenStartedAt,
  )
  const remaining = useAuthoringClock(
    COMMENT_AUTHORING_DEADLINE_SECONDS,
    waiting,
  )
  const deadlineNotified = useRef(false)

  useEffect(() => {
    if (frozenBody) return
    if (publishedText) {
      setFrozenBody(publishedText)
      return
    }
    if (unpublishedSettled) setFrozenBody(unpublishedSettled)
  }, [frozenBody, publishedText, unpublishedSettled])

  useEffect(() => {
    if (!waiting || remaining !== 0 || deadlineNotified.current) return
    const fallback = unpublishedFallback
    if (!fallback) return
    deadlineNotified.current = true
    setFrozenBody(fallback)
    onAuthoringDeadline?.()
  }, [onAuthoringDeadline, remaining, unpublishedFallback, waiting])

  const commentText = conversationCommentText(frozenBody, waiting, openingText)
  const composerDisabled = inputDisabled
  const compact = useCompactLayout()
  const [composerOpen, setComposerOpen] = useState(false)
  const composerSheet = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!composerOpen) return
    composerSheet.current?.querySelector("textarea")?.focus()
  }, [composerOpen])

  function send() {
    const message = draft.trim()
    if (composerDisabled || !message) return
    onMessage(message)
    setDraft("")
    setComposerOpen(false)
  }

  /** Touching away from an empty sheet folds it back to the button; a typed
   * draft keeps the input open so re-reading the thread cannot lose it. */
  function sheetFocusOut(event: FocusEvent<HTMLDivElement>) {
    // SAFETY: React types relatedTarget as Element | null, and Node.contains
    // accepts exactly that plus null; the cast only widens Element to Node.
    if (composerSheet.current?.contains(event.relatedTarget as Node)) return
    if (draft.trim().length === 0) setComposerOpen(false)
  }
  function composerKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault()
      send()
    }
  }

  return (
    <WatercolorCard
      aria-label="Coaching conversation"
      className="chen-review-session-conversation"
      frame={false}
      padding="compact"
      xstyle={reviewSessionShellStyles.conversation}
    >
      <VStack
        gap={3}
        hAlign="stretch"
        xstyle={reviewSessionShellStyles.conversationBody}
      >
        <VStack
          aria-live="polite"
          gap={2}
          hAlign="stretch"
          xstyle={reviewSessionShellStyles.conversationThread}
        >
          <ChatMessageList gap={3}>
            {browsingNote ? (
              <ChatMessage
                avatar={<CoachAvatar />}
                sender="assistant"
                xstyle={reviewSessionShellStyles.chatMessage}
              >
                <WatercolorChatBubble
                  name={
                    <MessageNameLine avatar>
                      <Text type="label">Coach</Text>
                    </MessageNameLine>
                  }
                  tone="coach"
                  width="100%"
                >
                  <Text as="p" display="block" type="body">
                    {browsingNote}
                  </Text>
                </WatercolorChatBubble>
              </ChatMessage>
            ) : (
              <CoachOpeningMessage
                commentText={commentText}
                learningPathFeedback={learningPathFeedback}
                learningPathFeedbackFailures={learningPathFeedbackFailures}
                learningPathFeedbackPending={learningPathFeedbackPending}
                learningPathFeedbackVotePending={
                  learningPathFeedbackVotePending
                }
                learningPaths={learningPaths}
                learningPathsAriaLabel={learningPathsAriaLabel}
                onLearningPathVote={onLearningPathVote}
                reviewFeedback={reviewFeedback}
                waiting={waiting}
              />
            )}
            {messages.map((message) => (
              <ThreadMessage
                fullWidth={compact}
                key={message.id}
                message={message}
              />
            ))}
          </ChatMessageList>

          {failure ? (
            <Banner description={failure} status="error" title="Game review" />
          ) : null}

          <ConversationBusyStatus
            busyLabel={busyLabel}
            onCancel={onCancel}
            waiting={browsingNote ? false : waiting}
          />
        </VStack>

        {compact ? (
          composerOpen ? (
            <div
              className="chen-review-composer-sheet"
              onBlur={sheetFocusOut}
              ref={composerSheet}
              {...stylex.props(reviewSessionShellStyles.composerSheet)}
            >
              <ConversationComposer
                composerDisabled={composerDisabled}
                draft={draft}
                onChange={setDraft}
                onKeyDown={composerKeyDown}
                onSend={send}
              />
            </div>
          ) : (
            <WatercolorButton
              aria-label={
                onDiscussPosition
                  ? "Discuss this position"
                  : "Message the coach"
              }
              className="chen-review-composer-fab"
              onClick={() =>
                onDiscussPosition ? onDiscussPosition() : setComposerOpen(true)
              }
              size="icon"
              type="button"
              variant="primary"
              xstyle={reviewSessionShellStyles.composerFab}
            >
              <Icon icon="messageCircle" size="md" />
            </WatercolorButton>
          )
        ) : (
          <VStack
            hAlign="stretch"
            xstyle={reviewSessionShellStyles.conversationComposer}
          >
            <ConversationComposer
              composerDisabled={composerDisabled}
              draft={draft}
              onChange={setDraft}
              onKeyDown={composerKeyDown}
              onSend={send}
            />
          </VStack>
        )}
      </VStack>
    </WatercolorCard>
  )
}

/** Inert feedback wiring, for a host that renders the thread statically. */
export const noLearningPathFeedbackWiring = {
  learningPathFeedback: noLearningPathFeedback,
  learningPathFeedbackFailures: noLearningPathFeedbackFailures,
  learningPathFeedbackPending: noPendingLearningPaths,
  learningPathFeedbackVotePending: noPendingLearningPaths,
} as const

/**
 * The coach's opening message for the focused Review Moment: comment plus
 * Learning Paths. Exported so the landing showcase renders the same thread
 * the shipped Review Session does, instead of a hand-built approximation.
 * The feedback wiring stays required: a caller that has it must not be able
 * to drop it silently, and one that does not passes
 * `noLearningPathFeedbackWiring`.
 */
export function CoachOpeningMessage({
  commentText,
  learningPathFeedback,
  learningPathFeedbackFailures,
  learningPathFeedbackPending,
  learningPathFeedbackVotePending,
  learningPaths,
  learningPathsAriaLabel,
  onLearningPathVote,
  reviewFeedback,
  waiting,
}: {
  commentText: string
  learningPathFeedback: Partial<
    Record<LearningPathRef, LearningPathFeedbackState>
  >
  learningPathFeedbackFailures: Partial<Record<LearningPathRef, string>>
  learningPathFeedbackPending: ReadonlySet<LearningPathRef>
  learningPathFeedbackVotePending: ReadonlySet<LearningPathRef>
  learningPaths: readonly MomentLearningPath[]
  learningPathsAriaLabel: string
  onLearningPathVote?: (
    learningPathRef: LearningPathRef,
    vote: LearningPathVote | null,
  ) => void
  reviewFeedback?: ReviewFeedbackControls
  waiting: boolean
}) {
  return (
    <ChatMessage
      avatar={<CoachAvatar />}
      sender="assistant"
      xstyle={reviewSessionShellStyles.chatMessage}
    >
      <WatercolorChatBubble
        name={
          <MessageNameLine avatar>
            <Text type="label">Coach</Text>
            {waiting || !reviewFeedback ? null : (
              <ReviewFeedbackMetadata controls={reviewFeedback} />
            )}
          </MessageNameLine>
        }
        tone="coach"
        width="100%"
      >
        {waiting ? (
          <AuthoringNote />
        ) : (
          <Text as="p" display="block" type="body">
            {commentText}
          </Text>
        )}
        {learningPaths.length > 0 ? (
          <LearningPathCards
            ariaLabel={learningPathsAriaLabel}
            currentVote={(learningPathRef) =>
              learningPathFeedback[learningPathRef]?.currentVote ?? null
            }
            failure={(learningPathRef) =>
              learningPathFeedbackFailures[learningPathRef]
            }
            disabled={(learningPathRef) =>
              learningPathFeedbackPending.has(learningPathRef)
            }
            onVote={onLearningPathVote}
            paths={learningPaths}
            tone="bamboo"
            pending={(learningPathRef) =>
              learningPathFeedbackVotePending.has(learningPathRef)
            }
          />
        ) : null}
      </WatercolorChatBubble>
    </ChatMessage>
  )
}

/** One thread turn, rendered the way the shipped Review Session renders it.
 * Exported for the landing showcase — same reason as `CoachOpeningMessage`. */
export function ThreadMessage({
  fullWidth = false,
  message,
}: {
  fullWidth?: boolean
  message: WorkspaceThreadItem
}) {
  const rendered = renderedThreadMessage(message)
  return (
    <ChatMessage
      avatar={rendered.author === "coach" ? <CoachAvatar /> : undefined}
      sender={messageSender(rendered.author)}
      xstyle={
        rendered.author === "player"
          ? undefined
          : reviewSessionShellStyles.chatMessage
      }
    >
      <WatercolorChatBubble
        name={
          rendered.author === "player" ? (
            messageName(rendered.author)
          ) : (
            <MessageNameLine avatar={rendered.author === "coach"}>
              {messageName(rendered.author)}
            </MessageNameLine>
          )
        }
        tone={messageTone(rendered.author)}
        width={fullWidth ? "100%" : undefined}
      >
        {paragraphs(rendered.text).map((paragraph, index) => (
          <Text
            as="p"
            display="block"
            key={`${message.id}:${index}`}
            type="body"
          >
            {paragraph}
          </Text>
        ))}
      </WatercolorChatBubble>
    </ChatMessage>
  )
}

function ConversationBusyStatus({
  busyLabel,
  onCancel,
  waiting,
}: {
  busyLabel: string | null
  onCancel?: () => void
  waiting: boolean
}) {
  if (!busyLabel || waiting) return null
  return (
    <HStack gap={2} role="status" vAlign="center" wrap="wrap">
      <Icon icon="loader" size="sm" />
      <Text type="supporting">{busyLabel}</Text>
      {onCancel ? (
        <WatercolorButton
          onClick={onCancel}
          size="sm"
          type="button"
          variant="danger"
        >
          <Icon icon="square" size="sm" />
          Cancel
        </WatercolorButton>
      ) : null}
    </HStack>
  )
}

function ConversationComposer({
  composerDisabled,
  draft,
  onChange,
  onKeyDown,
  onSend,
}: {
  composerDisabled: boolean
  draft: string
  onChange: (value: string) => void
  onKeyDown: (event: KeyboardEvent<HTMLTextAreaElement>) => void
  onSend: () => void
}) {
  return (
    <WatercolorChatComposer
      disabled={composerDisabled}
      onChange={onChange}
      onKeyDown={onKeyDown}
      onSend={onSend}
      placeholder="Describe your plan or ask a follow-up…"
      value={draft}
    />
  )
}

function messageSender(
  author: ConversationMessageAuthor,
): "user" | "assistant" | "system" {
  switch (author) {
    case "player":
      return "user"
    case "system":
      return "system"
    case "coach":
      return "assistant"
  }
}

function messageName(author: ConversationMessageAuthor) {
  switch (author) {
    case "player":
      return "You"
    case "system":
      return "Game review"
    case "coach":
      return "Coach"
  }
}

function messageTone(
  author: ConversationMessageAuthor,
): "coach" | "player" | "system" {
  switch (author) {
    case "player":
      return "player"
    case "system":
      return "system"
    case "coach":
      return "coach"
  }
}

function CoachAvatar() {
  return (
    <img
      alt=""
      aria-hidden="true"
      height="32"
      src={brandAssets.appIcons.primary}
      width="32"
      {...stylex.props(reviewSessionShellStyles.chatSlotAvatar)}
    />
  )
}

/** The message header line. On the stack the avatar joins it as a flex
 * sibling — avatar, name and votes center on one row. */
function MessageNameLine({
  avatar = false,
  children,
}: {
  avatar?: boolean
  children: ReactNode
}) {
  return (
    <HStack
      gap={2}
      vAlign="center"
      wrap="wrap"
      xstyle={reviewSessionShellStyles.chatName}
    >
      {avatar ? (
        <img
          alt=""
          aria-hidden="true"
          height="24"
          src={brandAssets.appIcons.primary}
          width="24"
          {...stylex.props(reviewSessionShellStyles.chatNameAvatar)}
        />
      ) : null}
      {children}
    </HStack>
  )
}

/**
 * The feedback prompt, rendered in the coach bubble's metadata slot so it
 * stays visually attached to the answer it rates.
 */
function ReviewFeedbackMetadata({
  controls,
}: {
  controls: ReviewFeedbackControls
}) {
  return (
    <VStack gap={1} hAlign="start">
      <ReviewFeedbackVote controls={controls} />
      {controls.failure ? (
        <Text role="alert" type="supporting">
          {controls.failure}
        </Text>
      ) : null}
    </VStack>
  )
}

function AuthoringNote() {
  return (
    <HStack
      aria-live="polite"
      data-comment-wait="bounded"
      gap={2}
      role="status"
      vAlign="center"
    >
      <Icon icon="loader" size="sm" />
    </HStack>
  )
}

export type ReviewFeedbackControls = ReviewFeedbackState & {
  onVote: (vote: ReviewFeedbackVote) => void
}

type ReviewFeedbackPrompt = { text: string }

function reviewFeedbackPrompt(
  controls: ReviewFeedbackControls,
): ReviewFeedbackPrompt {
  if (controls.pending) return { text: "Saving…" }
  if (controls.vote) return { text: "Recorded" }
  return { text: "Helpful?" }
}

function ReviewFeedbackVote({
  controls,
}: {
  controls: ReviewFeedbackControls
}) {
  const prompt = reviewFeedbackPrompt(controls)

  return (
    <HStack gap={2} vAlign="center">
      <Text type="supporting">{prompt.text}</Text>
      <HStack aria-label="Review feedback" gap={1} role="group">
        {/* Quiet ink like the learning-path "Relevant?" thumbs: a pressed vote
            darkens to primary ink, an open one stays washed out. */}
        <WatercolorButton
          aria-label="Helpful"
          aria-pressed={controls.vote === "thumbsUp"}
          disabled={controls.pending}
          onClick={() => controls.onVote("thumbsUp")}
          size="icon"
          variant="quiet"
          xstyle={[
            reviewFeedbackStyles.voteButton,
            controls.vote === "thumbsUp" &&
              reviewFeedbackStyles.voteButtonPressed,
          ]}
        >
          <Icon icon="thumbsUp" size="sm" />
        </WatercolorButton>
        <WatercolorButton
          aria-label="Not helpful"
          aria-pressed={controls.vote === "thumbsDown"}
          disabled={controls.pending}
          onClick={() => controls.onVote("thumbsDown")}
          size="icon"
          variant="quiet"
          xstyle={[
            reviewFeedbackStyles.voteButton,
            controls.vote === "thumbsDown" &&
              reviewFeedbackStyles.voteButtonPressed,
          ]}
        >
          <Icon icon="thumbsDown" size="sm" />
        </WatercolorButton>
      </HStack>
    </HStack>
  )
}

/** Matches the learning-path feedback thumbs in LearningPathCards.styles.ts. */
const reviewFeedbackStyles = stylex.create({
  voteButton: {
    width: "1.75rem",
    height: "1.75rem",
    color: "var(--color-text-disabled)",
  },
  voteButtonPressed: {
    color: "var(--color-text-primary)",
  },
})

type RenderedThreadMessage = {
  author: ConversationMessageAuthor
  text: string
}

function renderedThreadMessage(
  item: WorkspaceThreadItem,
): RenderedThreadMessage {
  switch (item.kind) {
    case "playerMessage":
      return { author: "player", text: item.text }
    case "coachAnswer":
      return { author: "coach", text: item.answer }
    case "unavailable":
      return {
        author: "system",
        text: unavailableReasonMessage(item.reason),
      }
    case "refusal":
      return { author: "coach", text: hostTurnRefusalText[item.reason] }
    case "rejected":
      return { author: "system", text: recoveryMessage(item.recovery) }
    case "systemNote":
      return { author: "system", text: item.text }
    default: {
      const _exhaustive: never = item
      return _exhaustive
    }
  }
}

/**
 * Coach answers arrive as prose with blank-line paragraph breaks. Rendering the
 * whole answer in one `<p>` collapses those breaks into a wall of text.
 */
function paragraphs(text: string) {
  const blocks = text
    .split(/\n{2,}/)
    .map((block) => block.trim())
    .filter((block) => block.length > 0)
  return blocks.length > 0 ? blocks : [text]
}
