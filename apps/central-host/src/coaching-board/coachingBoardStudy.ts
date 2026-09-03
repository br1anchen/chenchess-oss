import {
  answerOpeningStudyCard,
  openingStudyCardCopy,
  openingStudyCards,
  openingStudyCurrentCard,
  openingStudyTally,
  startOpeningStudy,
  type OpeningStudyAnswer,
  type OpeningStudyCard,
  type OpeningStudyCardCopy,
  type OpeningStudySession,
  type OpeningStudyTally,
} from "./openingStudySession"
import type { OpeningStudyWorld } from "./openingStudyWorld"

/**
 * The opening study session as the board holds it.
 *
 * ADR 0063 runs the session in page state and defers the one card a board
 * cannot grade — the plan, in the Player's own words — to the host agent,
 * "against the snapshot it already holds". For that to be true the session
 * has to be *in* the snapshot, which means it lives beside the position in
 * the drive rather than inside the card component: a Player answering a card
 * is a change of the board like a browse, with a revision the agent can see
 * it arrived at. Nothing here is durable; the state comes apart with the
 * board, as the ADR intends.
 */
export type CoachingBoardStudyState = {
  cards: readonly OpeningStudyCard[]
  /** Where the finished session stands, and where every non-slot card is
   * asked from. */
  lineEndPly: number
  session: OpeningStudySession
  side: OpeningStudyWorld["side"]
}

export function openingStudyState(
  world: OpeningStudyWorld,
  lineEndPly: number,
): CoachingBoardStudyState {
  return {
    cards: openingStudyCards(world, lineEndPly),
    lineEndPly,
    session: startOpeningStudy(),
    side: world.side,
  }
}

export function studyAnswered(
  state: CoachingBoardStudyState,
  answer: string,
): CoachingBoardStudyState {
  return {
    ...state,
    session: answerOpeningStudyCard(state.session, state.cards, answer),
  }
}

export function studyRestarted(
  state: CoachingBoardStudyState,
): CoachingBoardStudyState {
  return { ...state, session: startOpeningStudy() }
}

function studyCurrentCard(
  state: CoachingBoardStudyState,
): OpeningStudyCard | undefined {
  return openingStudyCurrentCard(state.session, state.cards)
}

/**
 * The ply the session wants the board on: the current card's, or the end of
 * the line once every card is answered.
 */
export function studyViewedPly(state: CoachingBoardStudyState): number {
  return studyCurrentCard(state)?.viewedPly ?? state.lineEndPly
}

/** A card as the agent reads it: the authored card plus the exact words the
 * Player is reading, so the coach and the page ask the same question. */
export type CoachingBoardStudyCard = OpeningStudyCard &
  OpeningStudyCardCopy & {
    /** One-based position in the session, so "card 2 of 6" needs no counting. */
    position: number
  }

export type CoachingBoardStudyAnswer = OpeningStudyAnswer & {
  card: CoachingBoardStudyCard
}

/**
 * What the snapshot says about the study session on an opening board.
 *
 * Every answer rides with the card it answered and the verdict the page gave,
 * so the agent can discuss a wrong slot without re-deriving which card it
 * was, and can find the one `ungraded` plan it is asked to mark. `card` is
 * null once the session is finished and the world has come apart.
 */
export type CoachingBoardStudy = {
  answered: readonly CoachingBoardStudyAnswer[]
  card: CoachingBoardStudyCard | null
  cardCount: number
  side: OpeningStudyWorld["side"]
  tally: OpeningStudyTally
}

export function coachingBoardStudy(
  state: CoachingBoardStudyState | null,
): CoachingBoardStudy | null {
  if (!state) return null
  const cards = state.cards.map(
    (card, index): CoachingBoardStudyCard => ({
      ...card,
      ...openingStudyCardCopy(card, state.side),
      position: index + 1,
    }),
  )
  const answered = state.session.answers.flatMap((answer, index) => {
    const card = cards[index]
    return card ? [{ ...answer, card }] : []
  })
  return {
    answered,
    card: cards[state.session.answers.length] ?? null,
    cardCount: cards.length,
    side: state.side,
    tally: openingStudyTally(state.session),
  }
}
