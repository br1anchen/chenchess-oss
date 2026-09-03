/**
 * The projection from what the Engine answers into what a surface renders.
 *
 * It lives beside the contract rather than inside the MCP server because two
 * surfaces read the same review: the widget, which reads these projections as
 * MCP resources, and the web, which fetches the Engine directly and projects in
 * the browser. One function, one rendering — so the two mounts cannot disagree
 * about a board, an arrow, or an evaluation, whatever carried the bytes.
 *
 * Everything here is pure: the contract and chessops, no transport and no I/O.
 */
export { projectGameReviewSnapshot } from "./game-review-snapshot.js"
export {
  type ComparisonBoardArrow,
  criticalMomentComparisonArrows,
  engineMoveArrow,
  presentationComparisonArrows,
} from "./comparison-arrows.js"
export {
  type AlternativeMoveChatTarget,
  type ModelAlternativeMove,
  type ModelStrongestReply,
  projectAlternativeMove,
} from "./alternative-move.js"
export {
  containsRawUci,
  PLAYER_VISIBLE_MOVE_FALLBACK,
  playerVisibleAlternativeMove,
  playerVisibleSanFromLegalUci,
  playerVisibleSanLiteral,
  playerVisibleStrongestReply,
  type PlayerVisibleSan,
} from "./player-visible-san.js"
export {
  canonicalLinesFrom,
  canonicalMovesFromFen,
  canonicalMomentLines,
  type CanonicalMomentLine,
} from "./move-sequence-lines.js"
export { projectSequenceMoves } from "./move-sequence-presentation.js"
export {
  boardOrientation,
  boardSourceMoment,
  type BoardSourceMoment,
  classificationLabel,
  isNeutralPlayerSelectedClassification,
  momentGlyph,
  momentSummary,
  momentTone,
  moveLabel,
  occurrenceMoveLabel,
  reviewMomentToneFromClassificationKind,
  toneFromClassificationKind,
  projectArrows,
  projectBoardFacts,
} from "./review-moment-board.js"
export {
  projectMoveSequenceSnapshot,
  projectPlayerLineSequenceSnapshot,
  projectReviewMomentSnapshot,
} from "./review-moment-snapshot.js"
export {
  canonicalUciPattern,
  decodePlayerLineSequenceSnapshot,
  decodeRenderedSequenceSnapshot,
  isUciLine,
  type PlayerLineSequenceSnapshot,
  type RenderedSequenceSnapshot,
} from "./rendered-sequence-snapshot.js"
export { presentationPiecesFromFen } from "./review-session-presentation-pieces.js"
export {
  projectReviewSessionPresentation,
  projectReviewSessionPresentationAddition,
} from "./review-session-presentation.js"
