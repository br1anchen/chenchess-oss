import { expect, test } from "vitest"

import type { OperationCompletion } from "@chenchess/coach-engine-sdk"
import { fromAlternativeMoveId } from "@chenchess/coach-engine-sdk"

import {
  HOST_TURN_MAX_PRIOR_TURNS,
  hostTurnRefusalText,
  hostTurnStepLabels,
  HOST_TURN_MAX_PLAYER_MESSAGE_BYTES,
  priorHostTurns,
  shownLineLabel,
  type HostTurnEffects,
} from "./thread-state"

test("every HostTurn step label is product language and never a capability name", () => {
  const labels = Object.values(hostTurnStepLabels)
  expect(labels).toEqual([
    "Looking at another moment…",
    "Checking that line…",
    "Writing…",
  ])
  expect(labels.join(" ")).not.toMatch(
    /read_moment|list_moments|evaluate_line|learning_material|capability/i,
  )
})

test("HostTurnEffects matches the generated completion optional-nullable shape", () => {
  const completed: Extract<OperationCompletion, { kind: "hostTurnCompleted" }> =
    {
      kind: "hostTurnCompleted",
      answer: "The knight was hanging.",
      focusMoment: null,
      showLine: null,
    }
  const effects: HostTurnEffects = {
    focusMoment: completed.focusMoment,
    showLine: completed.showLine,
  }
  expect(effects.focusMoment).toBeNull()
  expect(effects.showLine).toBeNull()
})

test("priorHostTurns keeps the last four completed answer pairs", () => {
  const items = [1, 2, 3, 4, 5].flatMap((index) => [
    {
      kind: "playerMessage" as const,
      id: `p${index}`,
      text: `question ${index}`,
    },
    {
      kind: "coachAnswer" as const,
      id: `a${index}`,
      answer: `answer ${index}`,
      effects: {},
    },
  ])
  expect(priorHostTurns(items)).toEqual([
    { message: "question 2", answer: "answer 2" },
    { message: "question 3", answer: "answer 3" },
    { message: "question 4", answer: "answer 4" },
    { message: "question 5", answer: "answer 5" },
  ])
  expect(priorHostTurns(items)).toHaveLength(HOST_TURN_MAX_PRIOR_TURNS)
})

test("priorHostTurns skips refusals, unavailability, and rejections", () => {
  expect(
    priorHostTurns([
      { kind: "playerMessage", id: "p1", text: "off topic" },
      { kind: "refusal", id: "r1", reason: "notAboutThisReview" },
      { kind: "playerMessage", id: "p2", text: "too long" },
      { kind: "rejected", id: "x2", recovery: { kind: "correctInput" } },
      { kind: "playerMessage", id: "p3", text: "why this move?" },
      {
        kind: "coachAnswer",
        id: "a3",
        answer: "The knight was hanging.",
        effects: {},
      },
    ]),
  ).toEqual([{ message: "why this move?", answer: "The knight was hanging." }])
})

test("priorHostTurns drops pairs that would fail the Player-message gate", () => {
  const oversize = "x".repeat(HOST_TURN_MAX_PLAYER_MESSAGE_BYTES + 1)
  expect(
    priorHostTurns([
      { kind: "playerMessage", id: "p1", text: `dirty\u0007message` },
      {
        kind: "coachAnswer",
        id: "a1",
        answer: "The knight was hanging.",
        effects: {},
      },
      { kind: "playerMessage", id: "p2", text: "why this move?" },
      {
        kind: "coachAnswer",
        id: "a2",
        answer: `dirty\u0007answer`,
        effects: {},
      },
      { kind: "playerMessage", id: "p3", text: oversize },
      {
        kind: "coachAnswer",
        id: "a3",
        answer: "Still too large on the wire.",
        effects: {},
      },
      { kind: "playerMessage", id: "p4", text: "why this move?" },
      {
        kind: "coachAnswer",
        id: "a4",
        answer: "The knight was hanging.",
        effects: {},
      },
    ]),
  ).toEqual([{ message: "why this move?", answer: "The knight was hanging." }])
})

test("priorHostTurns pairs a question across an exploration system note", () => {
  expect(
    priorHostTurns([
      { kind: "playerMessage", id: "p1", text: "What if I play e4?" },
      {
        kind: "systemNote",
        id: "s1",
        text: "Stockfish evaluated e4 at +0.22. The real-game graph did not change.",
      },
      {
        kind: "coachAnswer",
        id: "a1",
        answer: "e4 occupies the center.",
        effects: {},
      },
      { kind: "playerMessage", id: "p2", text: "And the later moment?" },
    ]),
  ).toEqual([
    { message: "What if I play e4?", answer: "e4 occupies the center." },
  ])
})

test("refusal copy matches the engine-rendered HostTurn sentences", () => {
  expect(hostTurnRefusalText.notAboutThisReview).toContain("this reviewed game")
  expect(hostTurnRefusalText.notAboutChess).toBe(
    "I can only help with this chess review.",
  )
  expect(hostTurnRefusalText.unsafeRequest).toBe(
    "I cannot help with that request.",
  )
})

test("shownLineLabel names the board line without capability vocabulary", () => {
  expect(shownLineLabel({ kind: "engineBest" })).toBe("Engine line")
  expect(shownLineLabel({ kind: "playedMoveRefutation" })).toBe(
    "Played refutation",
  )
  expect(
    shownLineLabel({
      kind: "alternativeMove",
      alternativeMoveId: fromAlternativeMoveId("alternative-move:test"),
    }),
  ).toBe("Alternative branch")
})
