import { Chess } from "chessops/chess"
import { parseFen } from "chessops/fen"
import { parseSan } from "chessops/san"
import { makeSquare, parseSquare } from "chessops/util"
import { expect, test } from "vitest"

import { openingCatalogRow } from "./openingLineCatalog"
import {
  deviationOpponentMove,
  openingStudyWorlds,
  type OpeningStudyWorld,
} from "./openingStudyWorld"

const INITIAL_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"

function positionAfter(path: string, from: readonly string[]) {
  const setup = parseFen(INITIAL_FEN).unwrap()
  const position = Chess.fromSetup(setup).unwrap()
  const line = path
    .replace(/\d+\.\s*/g, " ")
    .trim()
    .split(/\s+/)
    .filter(Boolean)
  for (const san of [...line, ...from]) {
    const move = parseSan(position, san)
    if (!move) throw new Error(`illegal move "${san}" in ${path}`)
    position.play(move)
  }
  return position
}

function positionFrom(sans: readonly string[]) {
  const setup = parseFen(INITIAL_FEN).unwrap()
  const position = Chess.fromSetup(setup).unwrap()
  for (const san of sans) {
    const move = parseSan(position, san)
    if (!move) throw new Error(`illegal move "${san}"`)
    position.play(move)
  }
  return position
}

/**
 * chessops is the oracle here: it replays every authored move independently of
 * the module under test, so a break blocked by its own knight or a reply that
 * is already on the board fails the suite rather than reaching a Player.
 */
test.each([...openingStudyWorlds])(
  "every authored move in %s is legal chess",
  (ref, world: OpeningStudyWorld) => {
    const row = openingCatalogRow(ref)
    if (!row) throw new Error("a world is keyed by a catalog row's own ref")

    const atBreak = positionAfter(row.path, world.pawnBreak.from)
    expect(atBreak.turn).toBe(world.side)
    expect(
      world.pawnBreak.options
        .filter((option) => !parseSan(atBreak, option.san))
        .map((option) => option.san),
    ).toEqual([])

    // `from` ends with the opponent's move, so the Player is always to move.
    expect(
      world.deviations
        .filter(
          (deviation) =>
            positionAfter(row.path, deviation.from).turn !== world.side,
        )
        .map(deviationOpponentMove),
    ).toEqual([])

    expect(
      world.deviations.flatMap((deviation) => {
        const at = positionAfter(row.path, deviation.from)
        return [
          { san: deviation.answer, why: deviation.principle },
          ...deviation.distractors,
        ]
          .filter((option) => !parseSan(at, option.san))
          .map((option) => option.san)
      }),
    ).toEqual([])
  },
)

test.each([...openingStudyWorlds])(
  "every slot in %s is answerable and names real squares",
  (_ref, world: OpeningStudyWorld) => {
    expect(
      world.slots
        .filter((slot) => slot.accepts.length === 0)
        .map((slot) => slot.piece),
    ).toEqual([])
    expect(
      world.slots.flatMap((slot) =>
        slot.options.filter((square) => parseSquare(square) === undefined),
      ),
    ).toEqual([])
    // A slot whose accepted square is missing from its options is a card the
    // Player cannot answer correctly.
    expect(
      world.slots.flatMap((slot) =>
        slot.accepts.filter((accepted) => !slot.options.includes(accepted)),
      ),
    ).toEqual([])
  },
)

/**
 * A slot is only a question while the piece is off the board, so the ply it
 * names has to be the ply that actually places it on an accepted square.
 */
test.each([...openingStudyWorlds])(
  "each slot in %s names the ply that places that piece",
  (ref, world: OpeningStudyWorld) => {
    const row = openingCatalogRow(ref)
    if (!row) throw new Error("a world is keyed by a catalog row's own ref")
    const line = row.path
      .replace(/\d+\.\s*/g, " ")
      .trim()
      .split(/\s+/)
      .filter(Boolean)
    expect(
      world.slots.flatMap((slot) => {
        const before = line.slice(0, slot.playedAtPly - 1)
        const position = positionFrom(before)
        const move = parseSan(position, line[slot.playedAtPly - 1] ?? "")
        if (!move || !("to" in move)) return [slot.piece]
        return slot.accepts.includes(makeSquare(move.to)) ? [] : [slot.piece]
      }),
    ).toEqual([])
  },
)

test.each([...openingStudyWorlds])(
  "no distractor in %s repeats the sound answer",
  (_ref, world: OpeningStudyWorld) => {
    expect(
      world.deviations
        .filter((deviation) =>
          deviation.distractors.some((one) => one.san === deviation.answer),
        )
        .map((deviation) => deviation.answer),
    ).toEqual([])
    expect(
      world.deviations
        .filter((deviation) => {
          const offered = deviation.distractors.map((one) => one.san)
          return new Set(offered).size !== offered.length
        })
        .map(deviationOpponentMove),
    ).toEqual([])
  },
)

test.each([...openingStudyWorlds])(
  "%s offers exactly one primary break",
  (_ref, world: OpeningStudyWorld) => {
    expect(
      world.pawnBreak.options
        .filter((option) => option.verdict === "primary")
        .map((option) => option.san),
    ).toHaveLength(1)
  },
)

test("worlds are keyed by refs the board can actually open", () => {
  expect(openingStudyWorlds.size).toBeGreaterThan(0)
  expect(
    [...openingStudyWorlds.keys()].filter((ref) => !openingCatalogRow(ref)),
  ).toEqual([])
})

/**
 * A slot card rewinds the board to the ply before its piece arrives, and the
 * board can stand on any ply from the first move on — so no slot may name the
 * first ply, which would ask from a position the board cannot reach.
 */
test.each([...openingStudyWorlds])(
  "every slot in %s is asked from a ply the board can stand on",
  (_ref, world: OpeningStudyWorld) => {
    expect(
      world.slots
        .filter((slot) => slot.playedAtPly < 2)
        .map((slot) => slot.piece),
    ).toEqual([])
  },
)
