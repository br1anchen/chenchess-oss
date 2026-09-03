/**
 * Spike generator: turn authored "small world" rows into validated board data.
 *
 * Every SAN move is replayed through chessops, so an illegal move or a
 * misspelled square fails the build instead of reaching the prototype.
 */
import { Chess } from "chessops/chess"
import { makeFen, parseFen } from "chessops/fen"
import { parseSan } from "chessops/san"
import { parseSquare, makeSquare } from "chessops/util"

type Slot = {
  /** Square the piece sits on in the tabiya. */
  square: string
  /** Squares that are acceptable homes for this piece in this structure. */
  accepts: string[]
  why: string
}

type Break = {
  san: string
  verdict: "primary" | "situational" | "mistake"
  why: string
}

type Deviation = {
  /** The learner's move first, when the tabiya has the learner to move. */
  after: string | null
  /** Opponent move that leaves the catalog. */
  san: string
  /** The principle that answers it — not a memorised reply. */
  principle: string
  /** A sound reply, for the coach to show after the learner commits. */
  answer: string
  /** Legal but wrong replies — each one a specific misunderstanding. */
  distractors: { san: string; why: string }[]
}

type World = {
  id: string
  eco: string
  name: string
  path: string
  side: "white" | "black"
  /** Played from the tabiya to reach the learner's own decision point. */
  decisionMove: string | null
  plan: string
  /** What a host agent would check a free-text plan answer against. */
  rubric: string[]
  slots: Slot[]
  breaks: Break[]
  deviations: Deviation[]
}

const worlds: World[] = [
  {
    id: "giuoco-piano",
    eco: "C53",
    name: "Italian Game: Giuoco Piano",
    path: "1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 4. c3 Nf6 5. d3 d6",
    side: "white",
    decisionMove: null,
    plan: "Build the big centre slowly. Tuck the king away, prepare d3–d4, and keep the light-squared bishop pointed at f7.",
    rubric: [
      "Names d4 as the break the position is built toward",
      "Explains that c3 exists to support d4, not to develop",
      "Keeps the c4 bishop on the a2–g8 diagonal aimed at f7",
      "Castles before opening the centre",
    ],
    slots: [
      {
        square: "c4",
        accepts: ["c4", "b3"],
        why: "The bishop's whole job is the a2–g8 diagonal. When Black plays …Na5 to trade it off, retreat to b3 rather than allow the swap.",
      },
      {
        square: "c3",
        accepts: ["c3"],
        why: "The c3 pawn is not developing — it is buying d4. Without it, d3–d4 never gets support.",
      },
      {
        square: "f3",
        accepts: ["f3"],
        why: "The king's knight holds e5 and covers d4. It is the piece …Bg4 pins, which is why the pin has to be answered before d3–d4.",
      },
    ],
    breaks: [
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
        why: "Grabs space on the side where your own king is heading, before castling, and abandons the slow centre plan the set-up asked for.",
      },
    ],
    deviations: [
      {
        after: "O-O",
        san: "Bg4",
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
        principle:
          "Black pins the knight that defends d4. Break the pin before you break the centre — the plan is unchanged, the order is not.",
        answer: "h3",
      },
      {
        after: "O-O",
        san: "Na5",
        distractors: [
          {
            san: "Bxf7+",
            why: "A sacrifice with nothing behind it. The king walks to f7 and White has given up the bishop the plan depended on.",
          },
          {
            san: "Bd5",
            why: "Keeps the bishop but on a square Black can challenge with …c6, losing the diagonal anyway.",
          },
        ],
        principle:
          "Black wants to trade your best piece. Save the bishop's diagonal; retreat rather than allow the exchange.",
        answer: "Bb3",
      },
      {
        after: "O-O",
        san: "Ng4",
        distractors: [
          {
            san: "h3",
            why: "Answering a threat that is not there. f2 is already covered twice, and h3 spends a tempo loosening your own king.",
          },
          {
            san: "Qe2",
            why: "Defends a square that needs no defending and blocks the bishop's own file.",
          },
        ],
        principle:
          "A one-piece raid at f2 with nothing behind it — castling already defended the square twice. Ignore it and let the wasted tempi pay for your centre.",
        answer: "d4",
      },
    ],
  },
  {
    id: "najdorf",
    eco: "B90",
    name: "Sicilian Defense: Najdorf Variation",
    path: "1. e4 c5 2. Nf3 d6 3. d4 cxd4 4. Nxd4 Nf6 5. Nc3 a6",
    side: "black",
    decisionMove: "Be3",
    plan: "…a6 takes b5 away from White's pieces first, then Black chooses a centre: …e5 to grab d4, or …e6 for a flexible Scheveningen shape.",
    rubric: [
      "Explains …a6 as prophylaxis against b5, not development",
      "Names …e5 and …e6 as the two centre choices",
      "Mentions the d5 hole as the price of …e5",
      "Connects the queenside plan to …b5 later",
    ],
    slots: [
      {
        square: "f6",
        accepts: ["f6"],
        why: "The f6 knight is the one piece that contests e4 immediately. Every Najdorf debate is about how White dislodges it.",
      },
      {
        square: "a6",
        accepts: ["a6"],
        why: "The move that names the variation. It is prophylaxis, not development — b5 is denied to White's knight and bishop before Black commits the centre.",
      },
    ],
    breaks: [
      {
        san: "e5",
        verdict: "primary",
        why: "Hits d4 and claims the centre at once, at the cost of a permanent hole on d5.",
      },
      {
        san: "e6",
        verdict: "situational",
        why: "The flexible choice. Keeps d5 covered and transposes toward Scheveningen structures.",
      },
      {
        san: "g6",
        verdict: "situational",
        why: "The Dragondorf. Playable, but it gives up on the …a6/…b5 queenside plan the last move prepared.",
      },
    ],
    deviations: [
      {
        after: null,
        san: "Bg5",
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
        principle:
          "White targets the f6 knight, the piece holding e4 in check. Answer the pressure on f6 before continuing with the queenside plan.",
        answer: "e6",
      },
      {
        after: null,
        san: "a4",
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
        principle:
          "White spends a move to stop …b5 — the very plan …a6 prepared. With the queenside slowed, take the centre instead.",
        answer: "e5",
      },
    ],
  },
]

function build(world: World) {
  const setup = parseFen(
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
  ).unwrap()
  const pos = Chess.fromSetup(setup).unwrap()
  const sans = world.path
    .replace(/\d+\.\s*/g, " ")
    .trim()
    .split(/\s+/)
    .filter(Boolean)

  const moves: { san: string; ply: number; fen: string }[] = []
  for (const [index, san] of sans.entries()) {
    const move = parseSan(pos, san)
    if (!move)
      throw new Error(`${world.id}: illegal SAN "${san}" at ply ${index + 1}`)
    pos.play(move)
    moves.push({ san, ply: index + 1, fen: makeFen(pos.toSetup()) })
  }
  const tabiyaFen = makeFen(pos.toSetup())

  // A slot is only meaningful if the tabiya actually has a piece on that
  // square, and every accepted square must be a real square.
  const board = pos.board
  for (const slot of world.slots) {
    const sq = parseSquare(slot.square)
    if (sq === undefined)
      throw new Error(`${world.id}: bad slot square ${slot.square}`)
    if (!board.get(sq)) {
      throw new Error(`${world.id}: slot ${slot.square} is empty in the tabiya`)
    }
    for (const accept of slot.accepts) {
      if (parseSquare(accept) === undefined) {
        throw new Error(`${world.id}: bad accepted square ${accept}`)
      }
    }
  }

  // Breaks are the learner's own choice, so they are legal from the
  // learner's decision point rather than from the tabiya itself.
  const decision = pos.clone()
  if (world.decisionMove) {
    const lead = parseSan(decision, world.decisionMove)
    if (!lead)
      throw new Error(
        `${world.id}: illegal decisionMove "${world.decisionMove}"`,
      )
    decision.play(lead)
  }
  const decisionFen = makeFen(decision.toSetup())
  if ((decision.turn === "white") !== (world.side === "white")) {
    throw new Error(`${world.id}: decision point is not the learner's move`)
  }
  const breaks = world.breaks.map((brk) => {
    const probe = decision.clone()
    const move = parseSan(probe, brk.san)
    if (!move) {
      throw new Error(
        `${world.id}: illegal break "${brk.san}" from decision point`,
      )
    }
    probe.play(move)
    return { ...brk, fen: makeFen(probe.toSetup()) }
  })
  // A deviation is the opponent's move, so it roots where the opponent is to
  // move: the tabiya itself, after the learner's move when the learner is up.
  // A deviation is the opponent's move, so it roots where the opponent is to
  // move: the tabiya itself, after the learner's move when the learner is up.
  const deviations = world.deviations.map((dev) => {
    const probe = pos.clone()
    if (dev.after) {
      const lead = parseSan(probe, dev.after)
      if (!lead) throw new Error(`${world.id}: illegal lead-in "${dev.after}"`)
      probe.play(lead)
    }
    const rootFen = makeFen(probe.toSetup())
    const move = parseSan(probe, dev.san)
    if (!move) throw new Error(`${world.id}: illegal deviation "${dev.san}"`)
    probe.play(move)
    const askFen = makeFen(probe.toSetup())

    // Every offered reply, sound or wrong, must be legal from the position the
    // learner is actually shown.
    for (const distractor of dev.distractors) {
      if (!parseSan(probe, distractor.san)) {
        throw new Error(
          `${world.id}: illegal distractor "${distractor.san}" after ${dev.san}`,
        )
      }
    }
    const reply = parseSan(probe, dev.answer)
    if (!reply) {
      throw new Error(
        `${world.id}: illegal answer "${dev.answer}" after ${dev.san}`,
      )
    }
    probe.play(reply)
    const answerFen = makeFen(probe.toSetup())
    if (answerFen === askFen) {
      throw new Error(
        `${world.id}: answer "${dev.answer}" did not change the board`,
      )
    }
    return { ...dev, rootFen, askFen, answerFen }
  })

  const pieces: { square: string; role: string; color: string }[] = []
  for (const [sq, piece] of board) {
    pieces.push({
      square: makeSquare(sq),
      role: piece.role,
      color: piece.color,
    })
  }

  return { ...world, breaks, decisionFen, deviations, moves, tabiyaFen, pieces }
}

const built = worlds.map(build)
console.log(JSON.stringify(built, null, 2))
