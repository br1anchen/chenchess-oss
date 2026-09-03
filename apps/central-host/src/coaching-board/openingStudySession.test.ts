import { expect, test } from "vitest"

import {
  answerOpeningStudyCard,
  gradeOpeningStudyCard,
  openingStudyCards,
  openingStudyCurrentCard,
  openingStudyTally,
  startOpeningStudy,
  type OpeningStudyCard,
} from "./openingStudySession"
import { openingStudyWorlds, type OpeningStudyWorld } from "./openingStudyWorld"

/** Any ply works: the cards only carry it back out as `viewedPly`. */
const LINE_END_PLY = 5

function anyWorld(): OpeningStudyWorld {
  const world = [...openingStudyWorlds.values()][0]
  if (!world) throw new Error("the study catalog authors at least one world")
  return world
}

function playThrough(
  cards: readonly OpeningStudyCard[],
  answerFor: (card: OpeningStudyCard) => string,
) {
  let session = startOpeningStudy()
  for (const card of cards) {
    session = answerOpeningStudyCard(session, cards, answerFor(card))
  }
  return session
}

function soundAnswer(card: OpeningStudyCard): string {
  if (card.kind === "slot") return card.accepts[0] ?? ""
  if (card.kind === "plan") return "A plan, in the Player's own words."
  if (card.kind === "break") {
    return card.options.find((one) => one.verdict === "primary")?.san ?? ""
  }
  return card.answer
}

test("the card order is the pedagogy: build, then say, then choose, then off book", () => {
  const world = anyWorld()
  const kinds = openingStudyCards(world, LINE_END_PLY).map((card) => card.kind)
  expect(kinds).toEqual([
    ...world.slots.map(() => "slot"),
    "plan",
    "break",
    ...world.deviations.map(() => "deviation"),
  ])
})

test("every card kind the session can produce is one of the four", () => {
  const kinds = new Set(
    [...openingStudyWorlds.values()].flatMap((world) =>
      openingStudyCards(world, LINE_END_PLY).map((card) => card.kind),
    ),
  )
  expect([...kinds].sort()).toEqual(["break", "deviation", "plan", "slot"])
})

test("a slot stands before its piece arrives; every other card studies the tabiya", () => {
  const world = anyWorld()
  const cards = openingStudyCards(world, LINE_END_PLY)
  const slots = cards.filter((card) => card.kind === "slot")
  expect(slots).toHaveLength(world.slots.length)
  for (const [index, slot] of slots.entries()) {
    // Asking where a piece belongs while it sits on the answer is not a
    // question, so the card is one ply short of the move that places it.
    expect(slot.viewedPly).toBe((world.slots[index]?.playedAtPly ?? 0) - 1)
  }
  for (const card of cards.filter((one) => one.kind !== "slot")) {
    expect(card.viewedPly).toBe(LINE_END_PLY)
  }
})

test("only the plan is answered in prose, and every move offered can be graded", () => {
  const problems: string[] = []
  for (const world of openingStudyWorlds.values()) {
    for (const card of openingStudyCards(world, LINE_END_PLY)) {
      const wantsProse = card.kind === "plan"
      if ((card.ask.kind === "freeText") !== wantsProse) {
        problems.push(`a ${card.kind} card asks for ${card.ask.kind}`)
      }
      if (card.ask.kind !== "choice") continue
      if (card.ask.options.length < 2) {
        problems.push(`a ${card.kind} card offers no real choice`)
      }
      for (const option of card.ask.options) {
        // A move on offer must be one the card knows: nothing may fall
        // through to the "not one of these" rejection.
        const verdict = gradeOpeningStudyCard(card, option)
        if (verdict.kind === "ungraded" || /not one of the/.test(verdict.why)) {
          problems.push(`a ${card.kind} card cannot grade ${option}`)
        }
      }
    }
  }
  expect(problems).toEqual([])
})

test("a free-text plan is returned ungraded with its rubric, not marked", () => {
  const world = anyWorld()
  const plan = openingStudyCards(world, LINE_END_PLY).find(
    (card) => card.kind === "plan",
  )
  if (plan?.kind !== "plan")
    throw new Error("a world always offers a plan card")
  expect(gradeOpeningStudyCard(plan, "anything the Player types")).toEqual({
    kind: "ungraded",
    rubric: world.rubric,
  })
})

test("the tally counts only what this surface can mark", () => {
  const world = anyWorld()
  const cards = openingStudyCards(world, LINE_END_PLY)
  const session = playThrough(cards, soundAnswer)
  const tally = openingStudyTally(session)
  // Every card was answered, but the plan is not among the graded ones.
  expect(session.answers).toHaveLength(cards.length)
  expect(tally.graded).toBe(cards.length - 1)
  expect(tally.right).toBe(tally.graded)
})

test("a situational break is acceptable rather than correct", () => {
  const world = [...openingStudyWorlds.values()].find((one) =>
    one.pawnBreak.options.some((option) => option.verdict === "situational"),
  )
  if (!world) throw new Error("a world authors a situational break")
  const card = openingStudyCards(world, LINE_END_PLY).find(
    (one) => one.kind === "break",
  )
  if (card?.kind !== "break")
    throw new Error("a world always offers a break card")
  const situational = card.options.find(
    (option) => option.verdict === "situational",
  )
  if (!situational)
    throw new Error("this world was chosen for its situational break")
  expect(gradeOpeningStudyCard(card, situational.san).kind).toBe("acceptable")
})

test("a mistaken break and a wrong reply are both incorrect, and say why", () => {
  const world = [...openingStudyWorlds.values()].find((one) =>
    one.pawnBreak.options.some((option) => option.verdict === "mistake"),
  )
  if (!world) throw new Error("a world authors a break that is a mistake")
  const cards = openingStudyCards(world, LINE_END_PLY)

  const brk = cards.find((card) => card.kind === "break")
  if (brk?.kind !== "break")
    throw new Error("a world always offers a break card")
  const mistake = brk.options.find((option) => option.verdict === "mistake")
  if (!mistake) throw new Error("this world was chosen for its mistaken break")
  expect(gradeOpeningStudyCard(brk, mistake.san)).toEqual({
    kind: "incorrect",
    why: mistake.why,
  })

  const deviation = cards.find((card) => card.kind === "deviation")
  if (deviation?.kind !== "deviation")
    throw new Error("a world offers a deviation")
  const wrong = deviation.options.find((one) => one.san !== deviation.answer)
  if (!wrong) throw new Error("a deviation offers at least one distractor")
  expect(gradeOpeningStudyCard(deviation, wrong.san)).toEqual({
    kind: "incorrect",
    why: wrong.why,
  })
})

test("an answer that is not on offer is rejected rather than silently accepted", () => {
  const world = anyWorld()
  const cards = openingStudyCards(world, LINE_END_PLY)
  const brk = cards.find((card) => card.kind === "break")
  if (brk?.kind !== "break")
    throw new Error("a world always offers a break card")
  expect(gradeOpeningStudyCard(brk, "Qh5").kind).toBe("incorrect")

  const slot = cards.find((card) => card.kind === "slot")
  if (slot?.kind !== "slot")
    throw new Error("a world always offers a slot card")
  expect(gradeOpeningStudyCard(slot, "h8").kind).toBe("incorrect")
})

test("answering past the last card changes nothing", () => {
  const world = anyWorld()
  const cards = openingStudyCards(world, LINE_END_PLY)
  const finished = playThrough(cards, soundAnswer)
  expect(openingStudyCurrentCard(finished, cards)).toBeUndefined()
  expect(answerOpeningStudyCard(finished, cards, "d4")).toBe(finished)
})

test("the session advances one card at a time and keeps what came before", () => {
  const world = anyWorld()
  const cards = openingStudyCards(world, LINE_END_PLY)
  const first = cards[0]
  if (!first) throw new Error("a world produces cards")
  const session = answerOpeningStudyCard(
    startOpeningStudy(),
    cards,
    soundAnswer(first),
  )
  expect(session.answers).toHaveLength(1)
  expect(openingStudyCurrentCard(session, cards)).toBe(cards[1])
  expect(startOpeningStudy().answers).toHaveLength(0)
})
