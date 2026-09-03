import type {
  BoardSquare,
  WorkspaceAction,
  WorkspacePresentation,
} from "./contracts"
import { piecesFromFen } from "./board/fen"

function boardSnapshot(
  fen: string,
  lastMove: WorkspacePresentation["board"]["lastMove"],
) {
  return Object.freeze({
    fen,
    pieces: piecesFromFen(fen),
    lastMove,
  })
}

const momentBoardSnapshots = {
  "moment-6": boardSnapshot(
    "rnb1kb1r/ppp1pppp/5n2/3q4/3P4/8/PPP2PPP/RNBQKBNR w KQkq - 0 4",
    { from: "d8", to: "d5" },
  ),
  "moment-12": boardSnapshot(
    "rn2kb1r/ppp2ppp/4pn2/3q4/3P2b1/5N2/PPP1BPPP/RNBQ1RK1 w kq - 2 7",
    { from: "f5", to: "g4" },
  ),
  "moment-17": boardSnapshot(
    "r3kb1r/pppn1ppp/3qpn2/8/2PP2b1/2N2N1P/PP2BPP1/R1BQ1RK1 b kq - 0 9",
    { from: "h2", to: "h3" },
  ),
  "moment-23": boardSnapshot(
    "r1b1k2r/pp1nbppp/2pqpn2/8/2PP4/2NQ1N1P/PP3PP1/R1B1R1K1 w kq - 0 13",
    { from: "c7", to: "c6" },
  ),
}

export const workspaceFixture: WorkspacePresentation = {
  playerName: "Brian",
  sessionLabel: "Review Session · Scandinavian Defense",
  importSetup: {
    source: "lichess",
    sourceLabel: "lichess.org/Synthet1 · Black",
    reviewSide: "black",
    eloLabel: "Imported profile · 1450",
    status: "complete",
    recovery: null,
  },
  moments: [
    {
      id: "moment-6",
      ply: 6,
      moveLabel: "3… Qxd5",
      kind: "automatic",
      tone: "critical",
      title: "Early queen exposure",
      summary: "The queen recapture gave White a useful developing tempo.",
    },
    {
      id: "moment-12",
      ply: 12,
      moveLabel: "6… Bg4",
      kind: "automatic",
      tone: "positive",
      title: "Useful pin",
      summary: "A calm developing move that made castling easier.",
    },
    {
      id: "moment-17",
      ply: 17,
      moveLabel: "9. h3",
      kind: "playerSelected",
      tone: "selected",
      title: "Player-selected position",
      summary: "A manually opened moment for closer inspection.",
    },
    {
      id: "moment-23",
      ply: 23,
      moveLabel: "12… c6",
      kind: "automatic",
      tone: "quiet",
      title: "Evidence limited",
      summary:
        "The most common choices at your rating were unavailable; objective evidence remains visible.",
    },
  ],
  activeMomentId: "moment-6",
  board: {
    id: "fixture-board",
    ...momentBoardSnapshots["moment-6"],
    orientation: "white",
    selectedSquare: "d4",
    legalDestinations: ["d5"],
    checkSquare: null,
    promotion: null,
    disabled: false,
    announcement: "Black queen moved from d8 to d5.",
  },
  comment: {
    eyebrow: "Critical Moment",
    heading: "Develop before bringing the queen back out",
    body: "After 3…Qxd5, Nc3 gains a tempo while White develops. Keeping the queen flexible makes the next few moves easier to coordinate.",
    status: "admitted",
  },
  alternatives: [
    {
      evaluation: "+0.18; best move Nf6",
      id: "alternative-nf6",
      san: "Nf6",
      label: "Develop with tempo",
      selected: false,
      status: "complete",
      detail: "Develops a piece and asks White to defend e4.",
      strongestReply: "d4d5",
    },
    {
      evaluation: null,
      id: "alternative-c6",
      san: "c6",
      label: "Prepare the recapture",
      selected: true,
      status: "active",
      detail: "Coach analysis is comparing the quieter Scandinavian structure.",
      strongestReply: null,
    },
    {
      evaluation: null,
      id: "alternative-qd6",
      san: "Qd6",
      label: "Cancelled line",
      selected: false,
      status: "cancelled",
      detail: "Cancelled before new evidence was committed.",
      strongestReply: null,
    },
  ],
  retention: {
    available: true,
    enabled: true,
    disclosureRequired: true,
    description:
      "Help improve coaching by retaining admitted, identity-scrubbed review artifacts for up to 12 months.",
    resolving: false,
  },
  statusMessage: "Review Moment 1 of 4. Canonical comment admitted.",
}

const momentScenarios = {
  "moment-6": {
    board: momentBoardSnapshots["moment-6"],
    comment: workspaceFixture.comment,
  },
  "moment-12": {
    board: momentBoardSnapshots["moment-12"],
    comment: {
      eyebrow: "Strong Choice",
      heading: "The pin supports smooth development",
      body: "6…Bg4 develops with purpose, makes the center easier to watch, and keeps castling available.",
      status: "admitted",
    } as const,
  },
  "moment-17": {
    board: momentBoardSnapshots["moment-17"],
    comment: {
      eyebrow: "Player-Selected Moment",
      heading: "Inspect the bishop before deciding",
      body: "This moment was opened by the Player. It is available for inspection but is not presented as an admitted automatic comment.",
      status: "draft",
    } as const,
  },
  "moment-23": {
    board: momentBoardSnapshots["moment-23"],
    comment: {
      eyebrow: "Evidence-Limited Moment",
      heading: "Objective comparison remains available",
      body: "The interface keeps uncertainty visible and does not invent a plan when provider evidence is unavailable.",
      status: "unavailable",
    } as const,
  },
}

function scenarioForMoment(momentId: string) {
  switch (momentId) {
    case "moment-6":
    case "moment-12":
    case "moment-17":
    case "moment-23":
      return momentScenarios[momentId]
    default:
      return undefined
  }
}

export function reduceWorkspaceFixture(
  current: WorkspacePresentation,
  action: WorkspaceAction,
): WorkspacePresentation {
  switch (action.type) {
    case "momentSelected":
      return reduceMomentSelected(current, action)
    case "boardSquareSelected":
      return reduceBoardSquareSelected(current, action)
    case "boardMoveRequested":
      return {
        ...current,
        board: {
          ...current.board,
          selectedSquare: null,
          legalDestinations: [],
          lastMove: action.move,
          announcement: `Move requested from ${action.move.from} to ${action.move.to}.`,
        },
        statusMessage: `Move requested: ${action.move.from}–${action.move.to}.`,
      }
    case "promotionRequested":
      return {
        ...current,
        statusMessage: `Promotion handed off as ${action.role} for ${action.move.from}–${action.move.to}.`,
      }
    case "alternativeSelected":
      return {
        ...current,
        alternatives: current.alternatives.map((alternative) => ({
          ...alternative,
          selected: alternative.id === action.alternativeId,
        })),
      }
    case "strongestReplySelected":
      return {
        ...current,
        statusMessage: "Strongest reply selected as an optional continuation.",
      }
    case "alternativeDiscussionRequested":
      return {
        ...current,
        statusMessage: "Alternative Move handed to chat.",
      }
    case "activeWorkCancelled":
      return {
        ...current,
        alternatives: current.alternatives.map((alternative) => ({
          ...alternative,
          status:
            alternative.status === "active" ? "cancelled" : alternative.status,
        })),
        statusMessage:
          "Active coaching work cancelled. Focus returned to alternatives.",
      }
    case "retentionChanged":
      return reduceRetentionChanged(current, action)
    case "retentionDisclosureAcknowledged":
      return {
        ...current,
        retention: { ...current.retention, disclosureRequired: false },
        statusMessage: "Retention disclosure acknowledged.",
      }
    case "importSourceChanged":
      return reduceImportSourceChanged(current, action)
    case "importRequested":
      return {
        ...current,
        importSetup: {
          ...current.importSetup,
          status: "complete",
          recovery: null,
        },
        statusMessage: "Fixture game imported and ready for review.",
      }
    case "signOutRequested":
      return { ...current, statusMessage: "Sign-out requested by the host." }
    default: {
      const _exhaustive: never = action
      return _exhaustive
    }
  }
}

function reduceMomentSelected(
  current: WorkspacePresentation,
  action: Extract<WorkspaceAction, { type: "momentSelected" }>,
): WorkspacePresentation {
  const moment = current.moments.find(
    (candidate) => candidate.id === action.momentId,
  )
  const scenario = scenarioForMoment(action.momentId)
  if (!moment || !scenario) return current
  return {
    ...current,
    activeMomentId: moment.id,
    comment: scenario.comment,
    board: {
      ...current.board,
      ...scenario.board,
      selectedSquare: null,
      legalDestinations: [],
      announcement: `${moment.moveLabel}: ${moment.title}.`,
    },
    statusMessage: `${moment.kind === "automatic" ? "Automatic" : "Player-Selected"} Review Moment at ply ${moment.ply}.`,
  }
}

function reduceBoardSquareSelected(
  current: WorkspacePresentation,
  action: Extract<WorkspaceAction, { type: "boardSquareSelected" }>,
): WorkspacePresentation {
  const selected =
    current.board.selectedSquare === action.square ? null : action.square
  return {
    ...current,
    board: {
      ...current.board,
      selectedSquare: selected,
      legalDestinations: selected === "d4" ? ["d5"] : [],
      announcement: selected
        ? `${selected} selected.`
        : "Board selection cleared.",
    },
    statusMessage: selected
      ? `${selected} selected.`
      : "Board selection cleared.",
  }
}

function reduceRetentionChanged(
  current: WorkspacePresentation,
  action: Extract<WorkspaceAction, { type: "retentionChanged" }>,
): WorkspacePresentation {
  return {
    ...current,
    retention: {
      ...current.retention,
      enabled: action.enabled,
      disclosureRequired: false,
      resolving: false,
    },
    statusMessage: action.enabled
      ? "Help improve coaching is enabled."
      : "Help improve coaching is disabled.",
  }
}

function reduceImportSourceChanged(
  current: WorkspacePresentation,
  action: Extract<WorkspaceAction, { type: "importSourceChanged" }>,
): WorkspacePresentation {
  return {
    ...current,
    importSetup: {
      ...current.importSetup,
      source: action.source,
      status: "ready",
      sourceLabel:
        action.source === "lichess"
          ? "lichess.org/Synthet1 · Black"
          : action.source === "chessCom"
            ? "chess.com/game/computer/1403674481 · White"
            : "Pasted PGN · 42 moves",
    },
    statusMessage:
      action.source === "lichess"
        ? "Lichess URL selected."
        : action.source === "chessCom"
          ? "Chess.com Game URL selected."
          : "Pasted PGN selected.",
  }
}

export function squareIsLegalDestination(
  model: WorkspacePresentation,
  square: BoardSquare,
) {
  return model.board.legalDestinations.includes(square)
}
