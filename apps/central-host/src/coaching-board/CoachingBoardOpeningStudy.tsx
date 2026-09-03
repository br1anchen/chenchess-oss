import { useState } from "react"
import type { CanonicalGameMove } from "@chenchess/coach-engine-sdk"
import {
  List,
  ListItem,
  Text,
  VStack,
  WatercolorBadge,
  WatercolorButton,
  WatercolorCard,
  WatercolorTextarea,
} from "@chenchess/ui"

import { AskTheCoach } from "@/review-session/AskTheCoach"

import type { CoachingBoardStudy } from "./coachingBoardStudy"
import type { OpeningCatalogRow } from "./openingLineCatalog"
import type { OpeningLineRef } from "./openingLineRef"
import { openingNextMoves } from "./openingNextMoves"
import type {
  OpeningStudyTally,
  OpeningStudyVerdict,
} from "./openingStudySession"

/**
 * How the surface reaches the session the board holds.
 *
 * The session lives in the drive so every Coaching Board Snapshot carries it
 * (ADR 0063 defers the plan card to the host agent, which can only mark what
 * it can read). The card component renders the same projection the agent
 * reads — one read model, so the coach and the page ask the same words — and
 * commits answers through the Player's own drive, the way a browse does.
 */
export type OpeningStudyHandle = {
  answer: (answer: string) => void
  copyReferent: (referent: string) => Promise<void>
  restart: () => void
  study: CoachingBoardStudy
}

export function CoachingBoardOpeningStudy({
  currentRef,
  moves,
  onOpenLine,
  onSelectPly,
  row,
  study,
  viewedPly,
}: {
  currentRef: OpeningLineRef
  moves: readonly CanonicalGameMove[]
  onOpenLine: (openingLineRef: OpeningLineRef) => void
  onSelectPly: (ply: number) => void
  row: OpeningCatalogRow
  study: OpeningStudyHandle | null
  viewedPly: number
}) {
  const branches = openingNextMoves(currentRef, moves, viewedPly)
  return (
    <VStack gap={3} hAlign="stretch">
      <WatercolorCard headingLevel={2} title="Next moves">
        <List>
          {branches.map((branch) => (
            <ListItem
              key={`${branch.san}:${branch.openingLineRef}`}
              label={branch.label}
              onClick={() => {
                if (branch.onCurrentLine) onSelectPly(viewedPly + 1)
                else onOpenLine(branch.openingLineRef)
              }}
            />
          ))}
        </List>
      </WatercolorCard>
      {study ? (
        <OpeningStudyRun row={row} study={study} />
      ) : (
        <OpeningIdeas row={row} />
      )}
    </VStack>
  )
}

/**
 * The session runs in the page and nothing about it is written down. Leaving
 * the line takes the tray apart, which is the design rather than a limitation:
 * rebuilding the world next time is the practice.
 */
function OpeningStudyRun({
  row,
  study: { answer, copyReferent, restart, study },
}: {
  row: OpeningCatalogRow
  study: OpeningStudyHandle
}) {
  const [typedPlan, setTypedPlan] = useState("")
  const answered = study.answered.at(-1)

  // The verdict of the answer just given, above whatever comes next.
  const verdictNote = answered ? (
    <OpeningStudyVerdictNote
      copyReferent={copyReferent}
      verdict={answered.verdict}
    />
  ) : null

  if (!study.card) {
    return (
      <WatercolorCard headingLevel={2} title="Study complete">
        <VStack gap={2} hAlign="stretch">
          {verdictNote}
          <Text as="p" display="block" type="body">
            {openingStudyClosing(study.tally)}
          </Text>
          <OpeningIdeaLines row={row} />
          <WatercolorButton
            onClick={() => {
              setTypedPlan("")
              restart()
            }}
            variant="outline"
          >
            Build it again
          </WatercolorButton>
        </VStack>
      </WatercolorCard>
    )
  }

  const card = study.card
  return (
    <WatercolorCard headingLevel={2} title={card.title}>
      <VStack gap={2} hAlign="stretch">
        {verdictNote}
        <Text as="p" display="block" type="body">
          {card.prompt}
        </Text>
        {card.ask.kind === "freeText" ? (
          <>
            <WatercolorTextarea
              aria-label={card.prompt}
              onChange={(event) => setTypedPlan(event.target.value)}
              rows={4}
              value={typedPlan}
            />
            <WatercolorButton onClick={() => answer(typedPlan)}>
              Answer
            </WatercolorButton>
          </>
        ) : (
          <List>
            {card.ask.options.map((option) => (
              <ListItem
                key={option}
                label={option}
                onClick={() => answer(option)}
              />
            ))}
          </List>
        )}
      </VStack>
    </WatercolorCard>
  )
}

/**
 * What the Player pastes in front of "mark my plan" (#530's referent, one
 * card over). The plan itself is not repeated: the coach reads it from the
 * board, where the page put it with its rubric.
 */
const OPENING_STUDY_PLAN_REFERENT =
  "About the plan I wrote in the opening study:"

function OpeningStudyVerdictNote({
  copyReferent,
  verdict,
}: {
  copyReferent: (referent: string) => Promise<void>
  verdict: OpeningStudyVerdict
}) {
  if (verdict.kind === "ungraded") {
    return (
      <VStack gap={1} hAlign="stretch">
        <WatercolorBadge tone="info">For your coach to mark</WatercolorBadge>
        <Text as="p" display="block" type="body">
          A board can only grade which move. Your plan is on the board for the
          coach to read against the position — it looks for:
        </Text>
        <List>
          {verdict.rubric.map((line) => (
            <ListItem key={line} label={line} />
          ))}
        </List>
        <AskTheCoach
          copyReferent={copyReferent}
          label="Ask the coach to mark my plan"
          referent={OPENING_STUDY_PLAN_REFERENT}
        />
      </VStack>
    )
  }
  return (
    <VStack gap={1} hAlign="stretch">
      <WatercolorBadge tone={verdictTone[verdict.kind]}>
        {verdictLabel[verdict.kind]}
      </WatercolorBadge>
      <Text as="p" display="block" type="body">
        {verdict.why}
      </Text>
    </VStack>
  )
}

const verdictTone = {
  acceptable: "info",
  correct: "success",
  incorrect: "danger",
} as const

const verdictLabel = {
  acceptable: "Playable",
  correct: "Yes",
  incorrect: "Not that",
} as const

/**
 * What the session says about itself once the last card is answered. The
 * coach-marked sentence is gated on the tally rather than asserting the one
 * plan card today's worlds happen to author.
 */
function openingStudyClosing(tally: OpeningStudyTally): string {
  const marked = `${tally.right} of ${tally.graded} decisions.`
  const coached =
    tally.ungraded > 0
      ? " What you wrote in your own words went to your coach, not to a score."
      : ""
  return `${marked}${coached} Nothing was saved — no deck, no interval, nothing due.`
}

function OpeningIdeas({ row }: { row: OpeningCatalogRow }) {
  return (
    <WatercolorCard headingLevel={2} title="Ideas">
      <OpeningIdeaLines row={row} />
    </WatercolorCard>
  )
}

function OpeningIdeaLines({ row }: { row: OpeningCatalogRow }) {
  return (
    <VStack gap={2} hAlign="stretch">
      <OpeningIdeaLine label="Plan" text={row.ideas.plan} />
      <OpeningIdeaLine label="Pawn breaks" text={row.ideas.pawnBreaks} />
      <OpeningIdeaLine label="Piece places" text={row.ideas.piecePlaces} />
    </VStack>
  )
}

function OpeningIdeaLine({ label, text }: { label: string; text: string }) {
  return (
    <VStack gap={0} hAlign="stretch">
      <Text type="label">{label}</Text>
      <Text as="p" display="block" type="body">
        {text}
      </Text>
    </VStack>
  )
}
