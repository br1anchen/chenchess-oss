import {
  deviationOpponentMove,
  type OpeningStudyBreakVerdict,
  type OpeningStudyWorld,
} from "./openingStudyWorld"

/**
 * A study session, as a sequence of decisions over one small world.
 *
 * The order is the pedagogy: build the world before playing inside it, say
 * what you are trying to do before choosing a move, and only then meet an
 * opponent who leaves the catalog. Nothing here is durable — the session is
 * the container, and it comes apart when the Player leaves.
 */

/**
 * How a card is answered: pick one of the moves on offer, or write prose.
 *
 * The card carries this rather than the surface deriving it from the kind, so
 * a view branches once on how to ask instead of once per kind.
 */
export type OpeningStudyAsk =
  | { kind: "choice"; options: readonly string[] }
  | { kind: "freeText" }

/** What every card carries, whatever it asks. */
type OpeningStudyCardFrame = {
  ask: OpeningStudyAsk
  /**
   * The ply the board shows while this card is open. A slot card stands one
   * ply before its piece arrives, so the Player is never asked to place a
   * piece already sitting on the answer; every other card studies the
   * finished tabiya.
   */
  viewedPly: number
}

export type OpeningStudyCard = OpeningStudyCardFrame &
  (
    | { kind: "slot"; piece: string; accepts: readonly string[]; why: string }
    | { kind: "plan"; rubric: readonly string[] }
    | { kind: "break"; options: readonly OpeningStudyBreakOption[] }
    | {
        kind: "deviation"
        answer: string
        opponent: string
        options: readonly OpeningStudyMoveOption[]
        principle: string
      }
  )

export type OpeningStudyBreakOption = {
  san: string
  verdict: OpeningStudyBreakVerdict
  why: string
}

export type OpeningStudyMoveOption = {
  san: string
  why: string
}

/**
 * `ungraded` is not a gap in this module. A free-text plan is exactly the
 * answer a board cannot mark — the input channel only carries moves — so the
 * card carries its rubric and defers to the host agent rather than pretending
 * to a verdict it cannot reach.
 */
export type OpeningStudyVerdict =
  | { kind: "correct"; why: string }
  | { kind: "acceptable"; why: string }
  | { kind: "incorrect"; why: string }
  | { kind: "ungraded"; rubric: readonly string[] }

export type OpeningStudyAnswer = {
  answer: string
  verdict: OpeningStudyVerdict
}

export type OpeningStudySession = {
  answers: readonly OpeningStudyAnswer[]
}

/**
 * The cards of one world, in order, each already knowing where the board
 * stands for it and how the Player answers it.
 */
export function openingStudyCards(
  world: OpeningStudyWorld,
  lineEndPly: number,
): readonly OpeningStudyCard[] {
  const slots = world.slots.map(
    (slot): OpeningStudyCard => ({
      kind: "slot",
      ask: { kind: "choice", options: slot.options },
      viewedPly: slot.playedAtPly - 1,
      piece: slot.piece,
      accepts: slot.accepts,
      why: slot.why,
    }),
  )
  const plan: OpeningStudyCard = {
    kind: "plan",
    ask: { kind: "freeText" },
    viewedPly: lineEndPly,
    rubric: world.rubric,
  }
  const breaks: OpeningStudyCard = {
    kind: "break",
    ask: { kind: "choice", options: world.pawnBreak.options.map(sanOf) },
    viewedPly: lineEndPly,
    options: world.pawnBreak.options,
  }
  const deviations = world.deviations.map((deviation): OpeningStudyCard => {
    const options = sortedByMove([
      { san: deviation.answer, why: deviation.principle },
      ...deviation.distractors,
    ])
    return {
      kind: "deviation",
      ask: { kind: "choice", options: options.map(sanOf) },
      viewedPly: lineEndPly,
      answer: deviation.answer,
      opponent: deviationOpponentMove(deviation),
      options,
      principle: deviation.principle,
    }
  })
  return [...slots, plan, breaks, ...deviations]
}

export function gradeOpeningStudyCard(
  card: OpeningStudyCard,
  answer: string,
): OpeningStudyVerdict {
  switch (card.kind) {
    case "plan":
      return { kind: "ungraded", rubric: card.rubric }
    case "slot":
      return card.accepts.includes(answer)
        ? { kind: "correct", why: card.why }
        : { kind: "incorrect", why: card.why }
    case "break": {
      const chosen = card.options.find((option) => option.san === answer)
      if (!chosen)
        return {
          kind: "incorrect",
          why: "That is not one of the breaks on offer.",
        }
      if (chosen.verdict === "primary")
        return { kind: "correct", why: chosen.why }
      if (chosen.verdict === "situational")
        return { kind: "acceptable", why: chosen.why }
      return { kind: "incorrect", why: chosen.why }
    }
    case "deviation": {
      const chosen = card.options.find((option) => option.san === answer)
      if (!chosen)
        return {
          kind: "incorrect",
          why: "That is not one of the replies on offer.",
        }
      return answer === card.answer
        ? { kind: "correct", why: chosen.why }
        : { kind: "incorrect", why: chosen.why }
    }
  }
}

export function startOpeningStudy(): OpeningStudySession {
  return { answers: [] }
}

export function answerOpeningStudyCard(
  session: OpeningStudySession,
  cards: readonly OpeningStudyCard[],
  answer: string,
): OpeningStudySession {
  const card = openingStudyCurrentCard(session, cards)
  if (!card) return session
  return {
    answers: [
      ...session.answers,
      { answer, verdict: gradeOpeningStudyCard(card, answer) },
    ],
  }
}

export function openingStudyCurrentCard(
  session: OpeningStudySession,
  cards: readonly OpeningStudyCard[],
): OpeningStudyCard | undefined {
  return cards[session.answers.length]
}

/**
 * What the session can say about itself at the end. `ungraded` is carried
 * rather than assumed, so the closing summary never asserts a count of
 * coach-marked answers that a world did not author.
 */
export type OpeningStudyTally = {
  graded: number
  right: number
  ungraded: number
}

export function openingStudyTally(
  session: OpeningStudySession,
): OpeningStudyTally {
  const graded = session.answers.filter(
    (entry) => entry.verdict.kind !== "ungraded",
  )
  return {
    graded: graded.length,
    right: graded.filter((entry) => entry.verdict.kind === "correct").length,
    ungraded: session.answers.length - graded.length,
  }
}

/** What the Player reads on one card. */
export type OpeningStudyCardCopy = { prompt: string; title: string }

/**
 * Everything the Player reads on a card, in one place. Adding a card kind
 * fails to compile here until it is written, which is the point of keeping
 * the copy in a single exhaustive dispatch rather than one chain per field.
 *
 * Lives with the session rather than the surface because the snapshot quotes
 * it too: the coach and the page must be asking the Player the same words.
 */
export function openingStudyCardCopy(
  card: OpeningStudyCard,
  side: "black" | "white",
): OpeningStudyCardCopy {
  const mover = side === "white" ? "White" : "Black"
  const opponent = side === "white" ? "Black" : "White"
  switch (card.kind) {
    case "slot":
      return {
        prompt: `Where does the ${card.piece.toLowerCase()} belong in this structure?`,
        title: "Build the world",
      }
    case "plan":
      return {
        prompt: "In your own words: what is your side trying to do here?",
        title: "Say the plan",
      }
    case "break":
      return {
        prompt: `${mover} to move. Which pawn break is this set-up built for?`,
        title: "Choose the break",
      }
    case "deviation":
      return {
        prompt: `${card.opponent} is not in any line you studied. Answer from the plan, not from a line — what now?`,
        title: `Off book — ${opponent} plays ${card.opponent}`,
      }
  }
}

function sanOf(option: { san: string }): string {
  return option.san
}

function sortedByMove(
  options: readonly OpeningStudyMoveOption[],
): readonly OpeningStudyMoveOption[] {
  return [...options].sort((left, right) => left.san.localeCompare(right.san))
}
