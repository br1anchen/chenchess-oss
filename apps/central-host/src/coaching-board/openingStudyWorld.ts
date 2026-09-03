import { openingLineCatalog } from "./openingLineCatalog"
import type { OpeningLineRef } from "./openingLineRef"

/**
 * A small world: one tabiya, the few ideas that hold it together, and the
 * decisions a Player makes inside it.
 *
 * The catalog's `ideas` are prose to read. A world is the same knowledge in a
 * form the Player can be asked to produce — which square a piece belongs on,
 * which break the structure wants, what to do when the opponent leaves the
 * catalog. Nothing here is a line to recall; every card is one decision in one
 * position.
 *
 * Research: `docs/research/2026-08-30-opening-study-as-small-world-play.md`.
 */
export type OpeningStudyWorld = {
  /** The side the Player takes in this world. */
  side: "white" | "black"
  slots: readonly OpeningStudySlot[]
  /** The Player's own choice of pawn break, at the position `from` reaches. */
  pawnBreak: OpeningStudyBreakCard
  deviations: readonly OpeningStudyDeviation[]
  /** What a host agent checks a free-text plan against. */
  rubric: readonly string[]
}

/**
 * One piece of the tabiya and the squares this structure accepts for it.
 *
 * `accepts` holds more than one square when the structure genuinely allows a
 * choice — the Italian bishop lives on c4 until …Na5, then on b3. That is the
 * slot doing its job: a template stores a piece's role, not its coordinates.
 */
export type OpeningStudySlot = {
  piece: string
  /**
   * The ply of the catalog line that puts this piece on the board.
   *
   * The board browses to the ply before it while the card is open, so the
   * Player is asked to place a piece that is not sitting there already. A slot
   * naming a square the board is currently showing is not a question.
   */
  playedAtPly: number
  accepts: readonly string[]
  options: readonly string[]
  why: string
}

export type OpeningStudyBreakVerdict = "primary" | "situational" | "mistake"

export type OpeningStudyBreakCard = {
  /** Moves from the catalog line's end that reach the Player's own turn. */
  from: readonly string[]
  options: readonly OpeningStudyBreakOption[]
}

export type OpeningStudyBreakOption = {
  san: string
  verdict: OpeningStudyBreakVerdict
  why: string
}

/**
 * The opponent leaves the catalog and the Player answers from the plan.
 *
 * `from` ends with the opponent's move, so the Player is always to move at the
 * position it reaches. Every distractor is legal and plausible; one is usually
 * the right plan played at the wrong moment, which is the mistake the card
 * exists to catch.
 */
export type OpeningStudyDeviation = {
  from: readonly string[]
  answer: string
  principle: string
  distractors: readonly OpeningStudyDistractor[]
}

export type OpeningStudyDistractor = {
  san: string
  why: string
}

/** The opponent's move is the last one played to reach the card. */
export function deviationOpponentMove(
  deviation: OpeningStudyDeviation,
): string {
  const last = deviation.from.at(-1)
  if (!last) throw new Error("A deviation is reached by at least one move")
  return last
}

const italianGame = {
  side: "white",
  slots: [
    {
      piece: "King's knight",
      playedAtPly: 3,
      accepts: ["f3"],
      options: ["f3", "e2", "c3"],
      why: "The knight hits e5 and covers d4 from the start. It is also the piece …Bg4 pins, which is why the pin has to be answered before d3–d4.",
    },
    {
      piece: "Light-squared bishop",
      playedAtPly: 5,
      accepts: ["c4", "b3"],
      options: ["c4", "b3", "d3", "e2"],
      why: "The bishop's whole job is the a2–g8 diagonal. When Black plays …Na5 to trade it off, retreat to b3 rather than allow the swap.",
    },
  ],
  pawnBreak: {
    from: ["Bc5", "c3", "Nf6", "d3", "d6"],
    options: [
      {
        san: "d4",
        verdict: "primary",
        why: "The break the whole set-up was built for. c3 supports it and the centre opens while Black's king is still deciding.",
      },
      {
        san: "b4",
        verdict: "situational",
        why: "Gains queenside space and hits the c5 bishop, but it loosens c4 and only works once the centre is settled.",
      },
      {
        san: "g4",
        verdict: "mistake",
        why: "Grabs space on the side your own king is heading for, before castling, and abandons the slow centre plan the set-up asked for.",
      },
    ],
  },
  deviations: [
    {
      from: ["Bc5", "c3", "Nf6", "d3", "d6", "O-O", "Bg4"],
      answer: "h3",
      principle:
        "Black pins the knight that defends d4. Break the pin before you break the centre — the plan is unchanged, the order is not.",
      distractors: [
        {
          san: "d4",
          why: "The right plan at the wrong moment. The knight holding d4 is pinned, so the break loses its support.",
        },
        {
          san: "Qe2",
          why: "Passive. It unpins nothing and hands Black a free tempo to finish developing.",
        },
      ],
    },
    {
      from: ["Bc5", "c3", "Nf6", "d3", "d6", "O-O", "Na5"],
      answer: "Bb3",
      principle:
        "Black wants to trade your best piece. Save the bishop's diagonal; retreat rather than allow the exchange.",
      distractors: [
        {
          san: "Bxf7+",
          why: "A sacrifice with nothing behind it. The king walks to f7 and White has given up the bishop the plan depended on.",
        },
        {
          san: "Bd5",
          why: "Keeps the bishop but on a square Black challenges with …c6, losing the diagonal anyway.",
        },
      ],
    },
  ],
  rubric: [
    "Names d4 as the break the position is built toward",
    "Explains that c3 exists to support d4, not to develop",
    "Keeps the c4 bishop on the a2–g8 diagonal aimed at f7",
    "Castles before opening the centre",
  ],
} satisfies OpeningStudyWorld

const najdorf = {
  side: "black",
  slots: [
    {
      piece: "King's knight",
      playedAtPly: 8,
      accepts: ["f6"],
      options: ["f6", "e7", "d7"],
      why: "The f6 knight is the one piece contesting e4 immediately. Every Najdorf debate is about how White dislodges it.",
    },
    {
      piece: "a-pawn",
      playedAtPly: 10,
      accepts: ["a6"],
      options: ["a6", "a5", "a7"],
      why: "The move that names the variation. It is prophylaxis, not development — b5 is denied to White's knight and bishop before Black commits the centre.",
    },
  ],
  pawnBreak: {
    from: ["Be3"],
    options: [
      {
        san: "e5",
        verdict: "primary",
        why: "Hits d4 and claims the centre at once, at the cost of a permanent hole on d5.",
      },
      {
        san: "e6",
        verdict: "situational",
        why: "The flexible choice. Keeps d5 covered and heads for Scheveningen structures.",
      },
      {
        san: "g6",
        verdict: "situational",
        why: "The Dragondorf. Playable, but it gives up on the …a6/…b5 queenside plan the last move prepared.",
      },
    ],
  },
  deviations: [
    {
      from: ["Bg5"],
      answer: "e6",
      principle:
        "White targets the f6 knight, the piece holding e4 in check. Answer the pressure on f6 before continuing with the queenside plan.",
      distractors: [
        {
          san: "h6",
          why: "Kicking before deciding the centre. After Bh4 the pawn on h6 is a target in every line Black later wants.",
        },
        {
          san: "Nbd7",
          why: "Develops, but leaves f6 pinned against the queen with the centre still unresolved.",
        },
      ],
    },
    {
      from: ["a4"],
      answer: "e5",
      principle:
        "White spends a move to stop …b5 — the very plan …a6 prepared. With the queenside slowed, take the centre instead.",
      distractors: [
        {
          san: "b5",
          why: "The move a4 was played to stop. It simply drops a pawn to axb5.",
        },
        {
          san: "Nc6",
          why: "Blocks the c-pawn in a Sicilian and abandons the queenside plan without taking the centre in return.",
        },
      ],
    },
  ],
  rubric: [
    "Explains …a6 as prophylaxis against b5, not development",
    "Names …e5 and …e6 as the two centre choices",
    "Mentions the d5 hole as the price of …e5",
    "Connects the queenside plan to …b5 later",
  ],
} satisfies OpeningStudyWorld

const worldsByName: ReadonlyMap<string, OpeningStudyWorld> = new Map<
  string,
  OpeningStudyWorld
>([
  ["Italian Game", italianGame],
  ["Sicilian Defense: Najdorf Variation", najdorf],
])

/**
 * Worlds are addressed by the same `OpeningLineRef` the board opens on, minted
 * from the catalog row's path so the two cannot drift apart.
 */
export const openingStudyWorlds: ReadonlyMap<
  OpeningLineRef,
  OpeningStudyWorld
> = new Map(
  openingLineCatalog.flatMap((row) => {
    const world = worldsByName.get(row.name)
    return world ? [[row.ref, world] as const] : []
  }),
)

export function openingStudyWorld(
  ref: OpeningLineRef,
): OpeningStudyWorld | undefined {
  return openingStudyWorlds.get(ref)
}
