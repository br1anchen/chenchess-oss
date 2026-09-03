/** Canonical Player-facing hosted Language Layer training and retention claim. */

export const languageLayerPrivacyHeading = "Hosted coaching notes"

/**
 * Coach Turns also send PRIOR_TURN (the previous coach note on that
 * Alternative Move) and the Coaching Profile Projection's track_keys.
 * Both are ChenChess-generated. track_keys are Player-derived learning
 * labels, not raw Player text. The notice names that bundle as "a short note
 * ChenChess writes about what you are working on" so the privacy page covers
 * the profile line without listing prompt fields.
 */
/**
 * The claim, paragraph by paragraph, so a surface can lay it out without
 * hand-copying it. `languageLayerPrivacyNotice` is these joined: the governed
 * wording has exactly one source.
 */
export const languageLayerPrivacyParagraphs = [
  "When you review a game on the web, ChenChess may send a small set of chess facts, your current message, and a short note ChenChess writes about what you are working on to an outside model, so it can write the reply you read. That request does not include your account, your name, game links, or your other games.",
  "Those inputs and replies are not used to train a model. The provider keeps them only when its automated checks flag a request for review, and only for as long as that review takes. We are not telling you nothing is stored — we are telling you a safety check is the one reason anything is kept. If the provider's terms stop matching the ones we agreed to, ChenChess stops sending anything and writes your notes itself instead.",
  'This is separate from "Help improve coaching," which keeps a pseudonymized copy inside ChenChess so we can improve coaching for everyone. Turning that setting off stops new copies. It does not change how these notes are written.',
] as const

export const languageLayerPrivacyNotice =
  languageLayerPrivacyParagraphs.join(" ")

export const languageLayerPrivacyCompanion =
  'These notes are written by an outside model that sees only a few chess facts, your current message, and a short note ChenChess writes about what you are working on. Those inputs are not used for training, and are kept only if an automated safety check flags a request, and only briefly. That is separate from "Help improve coaching," which keeps a copy inside ChenChess.'
